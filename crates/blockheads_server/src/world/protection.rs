//! Build-protection test: may a player modify the cell at a position?
//! Implemented from the behavioural spec (decision table), not from earlier code.

use super::misc::deps;
use super::state::World;
use super::tiles::macro_pos_for_world_pos;
use crate::foundation::nsstring::safe_case_insensitive_compare;
use crate::objects::handle::ObjRef;
use crate::plist::value::PlistValue;
use crate::types::structs::IntPair;

/// Default half extent of an ownership sign when `w` / `h` are absent.
const DEFAULT_HALF_EXTENT: i32 = 15;
/// Edge length of a macro block in cells.
const MACRO_SPAN: i32 = 32;

/// Outcome of scanning the signs around a position.
enum SignScan {
    /// A covering sign belongs to the asking player.
    Own,
    /// Covering sign(s) exist, none owned by the player.
    Foreign,
    /// No sign covers the position.
    None,
}

/// Wrap a horizontal difference into the world's seam-aware range.
fn wrap_delta(d: i32, width_cells: i32) -> i32 {
    let half = width_cells / 2;
    if d >= half {
        d.wrapping_sub(width_cells)
    } else if d < half.wrapping_neg() {
        d.wrapping_add(width_cells)
    } else {
        d
    }
}

/// Integer field of a sign dictionary, `default` when the key is missing.
fn field(sign: &PlistValue, key: &str, default: i32) -> i32 {
    sign.object_for_key(key).map_or(default, |v| v.int_value())
}

impl World {
    /// Does any sign in the neighbourhood of `pos` cover it, and who owns it?
    fn scan_signs(&self, pos: IntPair, owner_id: &str) -> SignScan {
        let width_cells = self.world_width_macro.wrapping_mul(MACRO_SPAN);
        let mut foreign = false;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let probe = IntPair { x: pos.x.wrapping_add(dx * MACRO_SPAN), y: pos.y.wrapping_add(dy * MACRO_SPAN) };
                let mp = macro_pos_for_world_pos(probe, self);
                let key = mp.y.wrapping_mul(self.world_width_macro).wrapping_add(mp.x).to_string();
                // Copy out so no RefCell borrow outlives the lookup.
                let signs: Vec<PlistValue> =
                    match self.ownership_sign_positions.borrow().as_ref().and_then(|t| t.get(&key)) {
                        Some(PlistValue::Array(a)) => a.items.clone(),
                        _ => continue,
                    };
                for sign in &signs {
                    let reach_x = field(sign, "w", DEFAULT_HALF_EXTENT);
                    let d = wrap_delta(pos.x.wrapping_sub(field(sign, "x", 0)), width_cells);
                    if d < reach_x.wrapping_neg() || d > reach_x {
                        continue;
                    }
                    let cy = field(sign, "y", 0);
                    let reach_y = field(sign, "h", DEFAULT_HALF_EXTENT);
                    if pos.y < cy.wrapping_sub(reach_y) || pos.y > cy.wrapping_add(reach_y) {
                        continue;
                    }
                    if let Some(owner) = sign.object_for_key("o") {
                        if safe_case_insensitive_compare(owner.as_str(), Some(owner_id)) == 0 {
                            return SignScan::Own;
                        }
                    }
                    foreign = true;
                }
            }
        }
        if foreign {
            SignScan::Foreign
        } else {
            SignScan::None
        }
    }

    /// May-not-modify test for the player `owner_id` at `tile_pos`; `true` means protected.
    pub fn tile_is_protected_at_pos_against_client(&self, tile_pos: IntPair, blockhead_owner_id: Option<&str>) -> bool {
        let rules = self.custom_rules.get();
        let rules_on = rules.custom_rules_enabled != 0;
        let blanket = if rules_on && rules.world_modifcation == 2 {
            true
        } else {
            let admin = if self.server().is_some() {
                deps().player_is_admin_with_id(blockhead_owner_id)
            } else {
                self.is_admin.get()
            };
            if admin {
                return false;
            }
            rules_on && rules.world_modifcation == 1
        };
        if self.ownership_sign_positions.borrow().is_none() {
            return blanket;
        }
        let Some(owner_id) = blockhead_owner_id else {
            return blanket;
        };
        match self.scan_signs(tile_pos, owner_id) {
            SignScan::Own => false,
            SignScan::Foreign => true,
            SignScan::None => blanket,
        }
    }

    /// Same test, identifying the player through a blockhead's local net id.
    pub fn tile_is_protected_at_pos_against_blockhead(&self, tile_pos: IntPair, blockhead: &ObjRef) -> bool {
        let id = match blockhead.as_blockhead() {
            Some(b) => b.local_net_id(),
            None => deps().bh_local_net_id(blockhead),
        };
        self.tile_is_protected_at_pos_against_client(tile_pos, id.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::gsdict::GsDict;
    use crate::plist::value::PlistDict;
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::BHServer;
    use std::rc::Rc;

    fn sign(x: i64, y: i64, w: Option<i64>, h: Option<i64>, o: Option<&str>) -> PlistValue {
        let mut pairs: Vec<(String, PlistValue)> =
            vec![("x".into(), PlistValue::int(x)), ("y".into(), PlistValue::int(y))];
        if let Some(w) = w {
            pairs.push(("w".into(), PlistValue::int(w)));
        }
        if let Some(h) = h {
            pairs.push(("h".into(), PlistValue::int(h)));
        }
        if let Some(o) = o {
            pairs.push(("o".into(), PlistValue::string(o)));
        }
        PlistValue::dict(GsDict::from_pairs(pairs))
    }

    fn world_with_signs(signs: Vec<(IntPair, PlistValue)>, server: bool) -> Rc<World> {
        let w = World::alloc(32);
        if server {
            *w.server.borrow_mut() = Some(Rc::new(BHServer { api: None }));
        }
        let mut d = PlistDict { map: GsDict::new(), tag: Default::default() };
        let mut by_idx: std::collections::BTreeMap<i32, Vec<PlistValue>> = Default::default();
        for (p, s) in signs {
            let mp = macro_pos_for_world_pos(p, &w);
            by_idx.entry(mp.y * 32 + mp.x).or_default().push(s);
        }
        for (k, v) in by_idx {
            d.map.insert(format!("{}", k), PlistValue::array(v));
        }
        *w.ownership_sign_positions.borrow_mut() = Some(d);
        w
    }

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// T3: truth table over (enabled, worldModifcation) x (admin / non-admin / none) x signs.
    #[test]
    fn protection_truth_table() {
        let rec = testkit::install();
        rec.admin.borrow_mut().push("admin".into());
        // no signs, no custom rules: not protected for anyone
        let w = World::alloc(32);
        *w.server.borrow_mut() = Some(Rc::new(BHServer { api: None }));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(5, 5), Some("bob")));
        for (wm, admin_expect, user_expect) in [(0i8, false, false), (1, false, true), (2, true, true)] {
            let mut cr = w.custom_rules.get();
            cr.custom_rules_enabled = 1;
            cr.world_modifcation = wm;
            w.custom_rules.set(cr);
            // signs table nil: result = blanket
            assert_eq!(
                w.tile_is_protected_at_pos_against_client(ip(5, 5), Some("admin")),
                admin_expect,
                "admin wm={wm}"
            );
            assert_eq!(w.tile_is_protected_at_pos_against_client(ip(5, 5), Some("bob")), user_expect, "bob wm={wm}");
            assert_eq!(w.tile_is_protected_at_pos_against_client(ip(5, 5), None), user_expect, "none wm={wm}");
        }
        // disabled custom rules ignore worldModifcation
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 0;
        w.custom_rules.set(cr);
        assert!(!w.tile_is_protected_at_pos_against_client(ip(5, 5), Some("bob")));

        // signs: foreign sign covering, own sign (case-insensitive), sign 32 tiles away
        let w = world_with_signs(
            vec![
                (ip(100, 100), sign(100, 100, None, None, Some("Alice"))),
                (ip(300, 100), sign(300, 100, Some(3), Some(2), None)),
            ],
            true,
        );
        assert!(w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("bob")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("ALICE")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("admin")));
        // default half extents are 15 in both axes: 116 is outside, 115 inside
        assert!(w.tile_is_protected_at_pos_against_client(ip(115, 115), Some("bob")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(116, 100), Some("bob")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(100, 116), Some("bob")));
        // custom w/h and no owner => protected for everybody non-admin
        assert!(w.tile_is_protected_at_pos_against_client(ip(303, 102), Some("bob")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(304, 100), Some("bob")));
        // nil owner id never scans
        assert!(!w.tile_is_protected_at_pos_against_client(ip(110, 90), None));
        // own sign returns false even when worldModifcation == 2 (blanket)
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 2;
        w.custom_rules.set(cr);
        assert!(!w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("alice")));
        assert!(w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("bob")));
        assert!(w.tile_is_protected_at_pos_against_client(ip(110, 90), Some("admin")));
        install_deps(Rc::new(NullDeps));
    }

    /// x wraps across the world edge.
    #[test]
    fn protection_wraps_x() {
        let _rec = testkit::install();
        let w = world_with_signs(vec![(ip(2, 100), sign(2, 100, None, None, Some("al")))], true);
        // 1022 is 4 tiles west of the sign across the seam (world is 1024 wide)
        assert!(w.tile_is_protected_at_pos_against_client(ip(1022, 100), Some("bob")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(1005, 100), Some("bob")));
        install_deps(Rc::new(NullDeps));
    }

    /// Single-player branch (server == nil): isAdmin decides.
    #[test]
    fn protection_no_server_uses_is_admin() {
        let _rec = testkit::install();
        let w = World::alloc(32);
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 1;
        w.custom_rules.set(cr);
        assert!(w.tile_is_protected_at_pos_against_client(ip(1, 1), Some("x")));
        w.is_admin.set(true);
        assert!(!w.tile_is_protected_at_pos_against_client(ip(1, 1), Some("x")));
        install_deps(Rc::new(NullDeps));
    }

    /// Spec table: wm=1 with signs, ownerless query, ownerless sign, own+foreign overlap, negative seam.
    #[test]
    fn protection_sign_decision_table() {
        let rec = testkit::install();
        rec.admin.borrow_mut().push("admin".into());
        let w = world_with_signs(
            vec![
                (ip(100, 100), sign(100, 100, None, None, Some("al"))),
                (ip(100, 100), sign(100, 100, None, None, Some("bo"))),
                (ip(500, 100), sign(500, 100, None, None, None)),
                (ip(1020, 100), sign(1020, 100, None, None, Some("eve"))),
            ],
            true,
        );
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 1;
        w.custom_rules.set(cr);
        // own sign beats blanket even with a foreign one overlapping
        assert!(!w.tile_is_protected_at_pos_against_client(ip(100, 100), Some("BO")));
        assert!(w.tile_is_protected_at_pos_against_client(ip(100, 100), Some("cy")));
        // no sign here: blanket applies; ownerless query ignores signs
        assert!(w.tile_is_protected_at_pos_against_client(ip(700, 100), Some("al")));
        assert!(w.tile_is_protected_at_pos_against_client(ip(100, 100), None));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(100, 100), Some("admin")));
        // ownerless sign covers and protects
        assert!(w.tile_is_protected_at_pos_against_client(ip(500, 100), Some("al")));
        // seam, sign near the east edge, query just past the west edge
        assert!(w.tile_is_protected_at_pos_against_client(ip(3, 100), Some("al")));
        assert!(!w.tile_is_protected_at_pos_against_client(ip(1020, 100), Some("eve")));
        // rules off, no covering sign: open
        cr.custom_rules_enabled = 0;
        w.custom_rules.set(cr);
        assert!(!w.tile_is_protected_at_pos_against_client(ip(700, 100), Some("al")));
        install_deps(Rc::new(NullDeps));
    }

    struct MockObj {
        base: std::cell::RefCell<crate::objects::base::DynamicObjectBase>,
        net_id: Option<String>,
        is_bh: bool,
    }
    impl crate::objects::base::DynamicObject for MockObj {
        fn base(&self) -> &std::cell::RefCell<crate::objects::base::DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static crate::objects::class_info::ClassInfo {
            &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn crate::objects::base::BlockheadApi> {
            if self.is_bh {
                Some(self)
            } else {
                None
            }
        }
    }
    impl crate::objects::base::BlockheadApi for MockObj {
        fn client_id(&self) -> Option<String> {
            self.net_id.clone()
        }
        fn local_net_id(&self) -> Option<String> {
            self.net_id.clone()
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            false
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            false
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {}
    }
    fn mock(is_bh: bool, id: &str) -> ObjRef {
        crate::objects::handle::alloc_obj(|w| MockObj {
            base: std::cell::RefCell::new(crate::objects::base::DynamicObjectBase::alloc_zeroed_with(w)),
            net_id: Some(id.to_string()),
            is_bh,
        })
    }

    /// Blockhead entry point forwards the local net id (or the deps fallback).
    #[test]
    fn protection_against_blockhead() {
        let _rec = testkit::install();
        let w = world_with_signs(vec![(ip(100, 100), sign(100, 100, None, None, Some("al")))], true);
        assert!(!w.tile_is_protected_at_pos_against_blockhead(ip(100, 100), &mock(true, "AL")));
        assert!(w.tile_is_protected_at_pos_against_blockhead(ip(100, 100), &mock(true, "bo")));
        // not a blockhead: id comes from deps (recorder answers none) -> signs not consulted
        assert!(!w.tile_is_protected_at_pos_against_blockhead(ip(100, 100), &mock(false, "al")));
        // the stand-in itself answers its small interface
        let m = mock(true, "zed");
        let _ = (m.class_info(), m.base().borrow().unique_id);
        let bh = m.as_blockhead().unwrap();
        assert_eq!(bh.client_id().as_deref(), Some("zed"));
        assert!(!bh.is_client_blockhead_being_controlled_by_server());
        assert!(!bh.is_interacting_with_any_interaction_object());
        bh.set_interaction_object(None);
        bh.stop_interacting();
        assert!(m.as_any().is::<MockObj>());
        install_deps(Rc::new(NullDeps));
    }
}
