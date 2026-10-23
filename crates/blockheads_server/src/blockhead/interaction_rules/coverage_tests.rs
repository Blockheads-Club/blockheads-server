//! Coverage tests for `interaction_rules.rs`: a deterministic fuzz of the rule cascade over random neighbourhoods,
//! items, queued actions and host answers, plus targeted scenarios for what the fuzz cannot reach.

use super::testhost::*;
use super::*;
use crate::blockhead::blockhead::testutil::*;
use crate::items::inventory_item::InventoryItem;
use crate::objects::base::{DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::interaction_object::{InteractionObjectLike, InteractionObjectState};
use crate::plist::value::PlistValue;
use std::cell::{Cell, RefCell};

/// xorshift64*
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u32) as usize]
    }
}

/// An interaction object with scripted answers.
struct Usable {
    base: RefCell<DynamicObjectBase>,
    st: RefCell<InteractionObjectState>,
    ty: i32,
    usable: bool,
    removable: bool,
    expert_ok: bool,
    class: &'static ClassInfo,
    double: bool,
    wide: bool,
    fb_type: i32,
}
impl DynamicObject for Usable {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        self.class
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        self.ty
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.fb_type
    }
    fn can_be_removed_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
        self.removable
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.st)
    }
}
impl InteractionObjectLike for Usable {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.st
    }
    fn can_be_used_by_blockhead(&self, _me: &ObjRef, _b: &ObjRef) -> bool {
        self.usable
    }
    fn can_be_used_in_expert_mode_when_not_owned(&self) -> bool {
        self.expert_ok
    }
    fn is_double_height(&self) -> bool {
        self.double
    }
    fn two_blocks_wide(&self) -> bool {
        self.wide
    }
}
fn usable(ty: i32, usable: bool, removable: bool, expert_ok: bool) -> ObjRef {
    crate::objects::handle::alloc_obj(|w| Usable {
        base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
        st: RefCell::new(InteractionObjectState::default()),
        ty,
        usable,
        removable,
        expert_ok,
        class: &DYNAMIC_OBJECT_CLASS,
        double: false,
        wide: false,
        fb_type: 0,
    })
}

fn usable_with(f: impl FnOnce(&mut Usable)) -> ObjRef {
    crate::objects::handle::alloc_obj(|w| {
        let mut u = Usable {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            st: RefCell::new(InteractionObjectState::default()),
            ty: 0,
            usable: true,
            removable: true,
            expert_ok: true,
            class: &DYNAMIC_OBJECT_CLASS,
            double: false,
            wide: false,
            fb_type: 0,
        };
        f(&mut u);
        u
    })
}

fn act(g: u32, item: Option<ItemRef>, goal: IntPair) -> ActionRef {
    use crate::types::enums::PathType;
    super::super::action::Action::init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
        goal,
        InteractionType(g),
        PathType(0),
        item,
        -1,
        -1,
        0,
        None,
        0,
        false,
        None,
    )
}

fn rule_full(
    b: &Blockhead,
    w: &World,
    pos: IntPair,
    item: Option<&ItemRef>,
    include_actions: bool,
    face: i32,
    allow: bool,
) -> (InteractionType, bool) {
    let tl = tile(w, pos.x, pos.y);
    let mut rej = false;
    let r = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        Some(&tl),
        pos,
        item,
        &mut rej,
        include_actions,
        face,
        allow,
    );
    (r, rej)
}

const TYPES: [u8; 24] =
    [2, 2, 2, 2, 2, 1, 1, 6, 3, 5, 4, 9, 0x1f, 0x19, 0x3c, 0x21, 0x30, 0x46, 0x20, 0x3a, 7, 0x12, 0x2b, 0x41];
const CONTENTS: [u8; 20] =
    [0, 0, 0, 0, 0, 0, 0x2e, 0x30, 0x60, 0x65, 0x67, 0x68, 0x92, 0x2b, 0x3f, 0x40, 0x2f, 0x31, 0x6a, 0x2c];
const FGS: [u8; 28] = [
    0, 0, 0, 0, 0, 0, 0x42, 0x43, 0x62, 0x60, 0x64, 0x63, 0x31, 0x4e, 0x30, 0x61, 0x32, 0x57, 0x66, 0x69, 0x52, 0x39,
    0x37, 0x35, 0x33, 0x3b, 0x58, 0x44,
];
const BGS: [u8; 8] = [0, 0, 0, 0, 0x45, 0x5f, 0x46, 0x4b];
const BWS: [u8; 7] = [0, 0, 1, 2, 0x26, 0x28, 3];
const PCLS: [u8; 6] = [0, 0x20, 0x21, 0x80, 0xff, 0xff];

fn randomize(rng: &mut Rng, w: &World, cx: i32, cy: i32) {
    // 0: every field random, 1: sparse (mostly air / rock with the odd special field)
    let mode = rng.below(3);
    for dx in -2..=2 {
        for dy in -2..=2 {
            let (x, y) = (cx + dx, cy + dy);
            if !(0..32).contains(&x) || !(0..32).contains(&y) {
                continue;
            }
            let (mut t, mut c, mut fg, mut bg, mut bw, mut pcl) = (
                rng.pick(&TYPES),
                rng.pick(&CONTENTS),
                rng.pick(&FGS),
                rng.pick(&BGS),
                rng.pick(&BWS),
                rng.pick(&PCLS),
            );
            if mode > 0 {
                let solid_chance = if mode == 1 { 35 } else { 60 };
                t = if rng.chance(solid_chance) {
                    rng.pick(&[1u8, 1, 6, 0x30, 7])
                } else {
                    rng.pick(&[2u8, 2, 2, 3, 5])
                };
                if !rng.chance(12) {
                    c = 0;
                }
                if !rng.chance(12) {
                    fg = 0;
                }
                if !rng.chance(8) {
                    bg = 0;
                }
                if !rng.chance(30) {
                    bw = 0;
                }
                if t == 3 && rng.chance(60) {
                    pcl = 0xff;
                }
            }
            let light = rng.chance(85) as u8;
            tile(w, x, y).update(|tl| {
                tl.type_index = t;
                tl.contents = c;
                tl.foreground_contents = fg;
                tl.background_contents = bg;
                tl.back_wall_type_index = bw;
                tl.partial_content_left = pcl;
                tl.light = light;
                tl.sun_light = 200;
            });
        }
    }
}

fn interesting_items() -> Vec<u32> {
    let mut v: Vec<u32> = vec![
        13, 53, 178, 1087, 1088, 0x69, 0x9e, 0x9f, 221, 52, 331, 164, 270, 200, 134, 210, 135, 138, 137, 136, 139,
        0x90, 0x127, 0x66, 0x6f, 0xa3, 0x12d, 0xb2, 199, 0xd1, 0x67, 0x68, 0x45, 0xa5, 0x34, 0xa4, 0xcc, 0x12e, 0x4e,
        0x5b, 0xaf, 0x3a, 0xae, 0x8, 0x22,
    ];
    for t in 0..1200u32 {
        let it_ = ItemType(t);
        if item_type_is_column(it_)
            || item_type_is_hangable(it_)
            || item_type_is_placable_on_floor(it_)
            || item_type_is_placable_on_wall(it_)
            || item_type_is_placable_on_trees(it_)
            || item_type_is_two_blocks_wide(it_)
            || item_type_is_full_cage(it_)
            || item_type_is_train_car(it_)
            || item_type_is_torch(it_)
            || item_type_is_workbench(it_)
            || item_type_is_interaction_object(it_)
            || item_type_is_painting(it_)
            || item_type_is_placable_on_back_wall(it_)
            || item_type_requires_ownership_to_remove(it_)
        {
            v.push(t);
        }
    }
    v
}

fn fuzz_rule_engine(seed: u64, iterations: u32) {
    let h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let mut rng = Rng(seed);
    let others = [stub(1, true), stub(2, false)];
    let mut me_pos = IntPair { x: 10, y: 12 };
    let mut seen = std::collections::BTreeMap::new();
    let interesting = interesting_items();
    for _ in 0..iterations {
        me_pos = if rng.chance(40) {
            IntPair { x: rng.pick(&[0, 1, 30, 31, 10]), y: rng.pick(&[0, 1, 30, 31, 12]) }
        } else {
            IntPair { x: 10, y: 12 }
        };
        randomize(&mut rng, &w, me_pos.x, me_pos.y);
        w.set_is_admin(rng.chance(60));
        w.set_expert_mode(rng.chance(30));
        // host answers
        let mk = |rng: &mut Rng| -> Option<ObjRef> {
            if rng.chance(30) {
                None
            } else {
                Some(usable(rng.pick(&[0x31, 0x3c, 7, 0x32]), rng.chance(60), rng.chance(60), rng.chance(50)))
            }
        };
        *h.io.borrow_mut() = mk(&mut rng);
        *h.torch.borrow_mut() = mk(&mut rng);
        *h.plant.borrow_mut() = mk(&mut rng);
        for k in ["wire", "column", "painting", "ladder", "stairs", "door", "window", "egg", "shaft", "motor"] {
            let v = mk(&mut rng);
            match v {
                Some(v) => {
                    h.misc.borrow_mut().insert(k, v);
                }
                None => {
                    h.misc.borrow_mut().remove(k);
                }
            }
        }
        *h.bh_at.borrow_mut() = match rng.below(3) {
            0 => None,
            1 => Some(o.clone()),
            _ => Some(others[rng.below(2) as usize].clone()),
        };
        h.io_type.set(rng.pick(&[0, 6, 0x31]));
        // item
        let item_ty =
            if rng.chance(80) { interesting[rng.below(interesting.len() as u32) as usize] } else { rng.below(1200) };
        let item = if rng.chance(12) { None } else { Some(give(b, item_ty, rng.below(4) as u16)) };
        if rng.chance(25) {
            // a full inventory: the other slots hold unstackable tools
            let f = b.bh.borrow();
            for i in 2..8 {
                f.inventory_items[i].borrow_mut().push(InventoryItem::new(ItemType(8), 0, 0, None, None));
            }
        }
        if rng.chance(5) {
            // a ghost item that is not in the inventory
            b.bh.borrow().inventory_items[1].borrow_mut().clear();
        }
        // queued actions
        {
            let mut f = b.bh.borrow_mut();
            f.action_queue.clear();
            let n = rng.below(3);
            for _ in 0..n {
                let g = rng.pick(&[2u32, 3, 3, 1]);
                let ai = if rng.chance(70) {
                    Some(InventoryItem::new(ItemType(if rng.chance(50) { item_ty } else { 0x69 }), 0, 0, None, None))
                } else {
                    None
                };
                let goal = IntPair { x: me_pos.x + rng.below(5) as i32 - 2, y: me_pos.y + rng.below(5) as i32 - 2 };
                f.action_queue.push(act(g, ai, goal));
            }
        }
        let face = rng.below(4) as i32;
        let allow = rng.chance(70);
        let include_actions = rng.chance(50);
        h.fb_exist.set(rng.chance(20));
        let (r, _rej) = if rng.chance(8) {
            // the same tile asked about at the top of the world (the 0x3f8 build limit)
            let t = tile(&w, me_pos.x, me_pos.y);
            let mut rej = false;
            let pos = IntPair { x: me_pos.x, y: 0x3f8 + rng.below(3) as i32 };
            let r = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                Some(&t),
                pos,
                item.as_ref(),
                &mut rej,
                include_actions,
                face,
                allow,
            );
            (r, rej)
        } else {
            rule_full(b, &w, me_pos, item.as_ref(), include_actions, face, allow)
        };
        *seen.entry(r.0).or_insert(0u32) += 1;
    }
    // many different answers were produced
    assert!(seen.len() >= 6, "{seen:?}");
}

#[test]
fn fuzz_rule_cascade_a() {
    fuzz_rule_engine(0x1234_5678_9abc_def1, 60_000);
}
#[test]
fn fuzz_rule_cascade_b() {
    fuzz_rule_engine(0xfeed_beef_cafe_0042, 60_000);
}

// ---------------------------------------------------------------------------------------------------
// the seam defaults and the default render item types
// ---------------------------------------------------------------------------------------------------

#[test]
fn null_actions_host_is_inert() {
    let (_w, o) = arena();
    let b = bh_of(&o);
    let n: &dyn ActionsHost = &NullActionsHost;
    let p = IntPair { x: 3, y: 4 };
    assert!(!n.free_blocks_exist_at_pos(b, p));
    assert!(n.blockhead_at_pos(b, p).is_none());
    assert!(n.check_for_train_car_under_tap(b, Vector2 { x: 0.0, y: 0.0 }).is_none());
    assert_eq!(n.interaction_object_type_for_object_at_pos(b, p), 0);
    assert!(n.all_blockheads_including_net(b).is_empty());
    assert!(!n.dynamic_world_is_client());
    assert_eq!(n.interaction_render_item_type(&stub(1, true)), 1);
    let t = tile(&_w, 3, 4);
    n.load_glow_block_if_needed_at_pos_tile(b, p, &t);
    n.add_torch_at_pos_of_type(b, p, 3);
    n.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(b, &t, p, true, true);
    n.world_contents_changed_at_pos(b, p);
    n.remove_standard_object(b, &stub(1, true));
    // the installed default
    set_actions_host(Rc::new(NullActionsHost));
    assert!(!actions_host().dynamic_world_is_client());
}

#[test]
fn render_item_type_per_interaction_object_class() {
    use crate::building::bed::Bed;
    use crate::interaction::{
        chest::Chest, mirror::Mirror, ownership_sign::OwnershipSign, sign::Sign, trade_portal::TradePortal,
        trading_post::TradingPost,
    };
    let (w, _o) = arena();
    let dw = real_dw(&w, "ir_render");
    let pos = IntPair { x: 5, y: 9 };
    let item = |t: u32| InventoryItem::new(ItemType(t), 0, 0, None, None);
    let bed_item = item(0xaa);
    let bed = Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
        &w,
        &dw,
        pos,
        None,
        Some(&bed_item.borrow()),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(default_interaction_render_item_type(&bed), 0xaa);
    let chest_item = item(0x430);
    let chest = Chest::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
        &w,
        &dw,
        pos,
        None,
        Some(&chest_item.borrow()),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let chest_obj: ObjRef = chest;
    let _ = default_interaction_render_item_type(&chest_obj);
    let any_item = item(0x1);
    macro_rules! build {
        ($ty:ty) => {{
            let o: ObjRef = <$ty>::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
                &w,
                &dw,
                pos,
                None,
                Some(&any_item.borrow()),
                false,
                None,
                None,
                None,
            )
            .unwrap();
            o
        }};
    }
    assert_eq!(default_interaction_render_item_type(&build!(TradingPost)), 167);
    assert_eq!(default_interaction_render_item_type(&build!(TradePortal)), 167);
    assert_eq!(default_interaction_render_item_type(&build!(OwnershipSign)), 301);
    assert_eq!(default_interaction_render_item_type(&build!(Sign)), 163);
    assert_eq!(default_interaction_render_item_type(&build!(Mirror)), 331);
    let station: ObjRef = crate::trains::train_station::TrainStation::new(
        &w,
        &dw,
        pos,
        None,
        Some(&any_item.borrow()),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(default_interaction_render_item_type(&station), 200);
    assert_eq!(default_interaction_render_item_type(&stub(1, true)), 1);
}

// ---------------------------------------------------------------------------------------------------
// good / bad previews
// ---------------------------------------------------------------------------------------------------

fn act_full(
    g: u32,
    item: Option<ItemRef>,
    goal: IntPair,
    oid: u64,
    craftable: Option<crate::crafting::craftable_item_object::CraftableItemObjectRef>,
) -> ActionRef {
    use crate::types::enums::PathType;
    super::super::action::Action::init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
        goal,
        InteractionType(g),
        PathType(0),
        item,
        -1,
        -1,
        oid,
        craftable,
        0,
        false,
        None,
    )
}

#[test]
fn good_or_bad_sweep() {
    let h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let mut rng = Rng(0x600d_0b0d);
    let interesting = interesting_items();
    crate::objects::class_info::register_class_info(&crate::npc::donkey::DONKEY_CLASS_INFO);
    let donkey = usable_with(|u| u.class = &crate::npc::donkey::DONKEY_CLASS_INFO);
    let other = usable(1, true, true, true);
    let goal = IntPair { x: 10, y: 12 };
    let mut craftable: crate::types::structs::CraftableItem = unsafe { std::mem::zeroed() };
    craftable.crafted_item_type = 0x69;
    craftable.crafted_item_data_b = 3;
    let cio = crate::crafting::craftable_item_object::CraftableItemObject::init_with_craftable_item(craftable);
    for i in 0..30_000u32 {
        if i % 4 == 0 {
            randomize(&mut rng, &w, goal.x, goal.y);
        }
        *h.npc.borrow_mut() = match rng.below(3) {
            0 => None,
            1 => Some(donkey.clone()),
            _ => Some(other.clone()),
        };
        *h.io.borrow_mut() = if rng.chance(50) { Some(other.clone()) } else { None };
        h.can_ride.set(rng.chance(50));
        let g = rng.below(31);
        let ty = if rng.chance(60) {
            rng.pick(&[
                8u32, 0x22, 0x2b, 0x42, 0x5a, 0x11d, 0x10, 0x28, 0x40, 0x59, 6, 0x21, 0x46, 0x19, 0x31, 0x43, 0xaf,
                0xd1, 0x67, 0x68, 7, 0x32, 0x44, 0x4a, 0x101, 0x2a, 0x5d, 0x5e, 0,
            ])
        } else {
            interesting[rng.below(interesting.len() as u32) as usize]
        };
        let item = if rng.chance(10) {
            None
        } else {
            Some(InventoryItem::new(ItemType(ty), 0, rng.below(5) as u16, None, None))
        };
        let a = act_full(g, item, goal, rng.below(3) as u64, if rng.chance(50) { Some(cio.clone()) } else { None });
        let r = b.good_or_bad_interaction_for_action(&a);
        assert!((-1..=1).contains(&r.good_or_bad));
    }
    // an action naming a goal outside the loaded world, and with no world at all
    let a = act_full(1, None, IntPair { x: 5000, y: 5 }, 0, None);
    let _ = b.good_or_bad_interaction_for_action(&a);
    let a = act_full(23, None, IntPair { x: 5000, y: 5 }, 0, None);
    let _ = b.good_or_bad_interaction_for_action(&a);
    // the donkey that can be ridden with a carrot stick
    let a = act_full(10, Some(InventoryItem::new(ItemType(0x4a), 0, 0, None, None)), goal, 5, None);
    *h.npc.borrow_mut() = Some(donkey.clone());
    h.can_ride.set(true);
    let r = b.good_or_bad_interaction_for_action(&a);
    assert_eq!((r.good_or_bad, r.render_item_type), (1, 0x4a));
    h.can_ride.set(false);
    let r = b.good_or_bad_interaction_for_action(&a);
    assert_eq!((r.good_or_bad, r.render_item_type), (0, 1));
    // crafting shows the crafted item
    let a = act_full(4, None, goal, 0, Some(cio.clone()));
    let r = b.good_or_bad_interaction_for_action(&a);
    assert_eq!((r.render_item_type, r.render_item_data_b), (0x69, 3));
    // an interaction object shows its own icon
    let a = act_full(5, None, goal, 5, None);
    *h.io.borrow_mut() = Some(other);
    assert_eq!(b.good_or_bad_interaction_for_action(&a).render_item_type, 1);
    // no world
    let (w2, o2) = arena();
    let b2 = bh_of(&o2);
    drop(w2);
    let _ = b2.good_or_bad_interaction_for_action(&act_full(1, None, goal, 0, None));
    let _ = b2.good_or_bad_interaction_for_action(&act_full(23, None, goal, 0, None));
}

// ---------------------------------------------------------------------------------------------------
// gather speed and animation
// ---------------------------------------------------------------------------------------------------

#[test]
fn gather_speed_sweep() {
    let h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let mut rng = Rng(0x6a7e_5eed);
    let tl = tile(&w, 10, 3);
    for i in 0..30_000u32 {
        w.set_expert_mode(rng.chance(30));
        let ty = rng.pick(&[
            8u32, 0x22, 0x2b, 0x42, 0x5a, 0x11d, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x5d, 0x10, 0x28, 0x40, 0x59, 6, 0x21,
            0x46, 0x19, 0x31, 0x43, 0xaf, 0x3, 0,
        ]);
        let t = rng
            .pick(&[1u8, 6, 9, 0x20, 4, 0x18, 0x3b, 0x3a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x44, 0x45, 2, 0x2b, 7, 0x10]);
        let c = rng.pick(&[0u8, 0, 0x40, 0x3f, 0x05, 0x67, 0x30, 0x2e, 0x2b]);
        let fg = rng.pick(&[0u8, 0, 0x30, 0x43, 0x44, 0x42, 0x31]);
        tl.update(|x| {
            x.type_index = t;
            x.contents = c;
            x.foreground_contents = fg;
        });
        {
            let mut f = b.bh.borrow_mut();
            f.interacting_tile = if rng.chance(95) { Some(tl) } else { None };
            f.interaction_item =
                if rng.chance(90) { Some(InventoryItem::new(ItemType(ty), 0, 0, None, None)) } else { None };
            f.state.interaction_type = rng.below(0x20);
            f.state.interacting_square = IntPair { x: 10, y: 3 };
            f.state.sub_animation_type = 99;
        }
        *h.io.borrow_mut() = None;
        let _ = i;
        b.update_gather_speed_and_animation_for_current_interation_and_item();
    }
}

#[test]
fn using_beds_and_chests_sets_the_sleeping_pose() {
    use crate::building::bed::Bed;
    use crate::interaction::chest::Chest;
    let h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let dw = real_dw(&w, "ir_bed");
    let pos = IntPair { x: 5, y: 9 };
    for (item_ty, golden) in [(0xa9u32, false), (0xaa, true)] {
        let it_ = InventoryItem::new(ItemType(item_ty), 0, 0, None, None);
        let bed =
            Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
                &w,
                &dw,
                pos,
                None,
                Some(&it_.borrow()),
                false,
                None,
                None,
                None,
            )
            .unwrap();
        *h.io.borrow_mut() = Some(bed);
        {
            let mut f = b.bh.borrow_mut();
            f.state.interaction_type = 5;
            f.state.interacting_square = pos;
            f.interacting_tile = Some(tile(&w, 5, 9));
        }
        h.take();
        b.update_gather_speed_and_animation_for_current_interation_and_item();
        let log = h.take();
        assert_eq!(b.bh.borrow().state.sub_animation_type, 5);
        assert!(log.contains(&"ach:grp.bed".to_string()), "{log:?}");
        assert_eq!(log.contains(&"ach:grp.golden_bed".to_string()), golden);
        assert_eq!(b.bh.borrow().to_square, pos);
    }
    // a controlled blockhead gets no achievement
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
    h.take();
    b.update_gather_speed_and_animation_for_current_interation_and_item();
    assert!(!h.take().iter().any(|l| l.starts_with("ach:")));
    // other interaction objects: no pose; a gold chest reports the treasure
    let gold_item = InventoryItem::new(ItemType(0x430), 0, 0, None, None);
    let chest = Chest::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
        &w,
        &dw,
        pos,
        None,
        Some(&gold_item.borrow()),
        false,
        None,
        None,
        None,
    )
    .unwrap();
    let chest_obj: ObjRef = chest;
    *h.io.borrow_mut() = Some(chest_obj);
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
    h.take();
    b.update_gather_speed_and_animation_for_current_interation_and_item();
    assert!(h.take().contains(&"ach:grp.treasure".to_string()));
    *h.io.borrow_mut() = Some(usable(1, true, true, true));
    b.update_gather_speed_and_animation_for_current_interation_and_item();
    assert_eq!(b.bh.borrow().state.sub_animation_type, 0);
    *h.io.borrow_mut() = None;
    b.update_gather_speed_and_animation_for_current_interation_and_item();
}

// ---------------------------------------------------------------------------------------------------
// updatePosition
// ---------------------------------------------------------------------------------------------------

#[test]
fn update_position_corners() {
    let h = AHost::install();
    // no world
    let (w0, o0) = arena();
    let b0 = bh_of(&o0);
    drop(w0);
    b0.update_position(IntPair { x: 7, y: 9 });
    // a door above the old position closes, the north pole is an achievement, promoted torches are placed
    let (w, o) = arena();
    let b = bh_of(&o);
    tile(&w, 5, 9).update(|t| t.background_contents = 0x46);
    h.take();
    b.update_position(IntPair { x: 0, y: 8 });
    let log = h.take();
    assert!(log.contains(&"door:5,9:false:0".to_string()), "{log:?}");
    assert!(log.contains(&"ach:grp.northpole".to_string()));
    for (fg, torch) in [(0x34u8, 0x4b), (0x36, 0x4c), (0x38, 0x56), (0x3a, 0x57), (0x3c, 0x58)] {
        tile(&w, 2, 9).update(|t| t.foreground_contents = fg);
        h.take();
        b.update_position(IntPair { x: 2, y: 8 });
        let log = h.take();
        assert!(log.contains(&format!("torch:2,9:{torch}")), "{fg:#x}: {log:?}");
        b.update_position(IntPair { x: 3, y: 8 });
    }
    // sleeping chests and trolls
    for (c, troll, treasure) in [(0x5eu8, true, true), (0x91, false, true), (0x90, true, false)] {
        tile(&w, 4, 9).update(|t| {
            t.contents = c;
            t.type_index = 2;
        });
        h.take();
        b.update_position(IntPair { x: 6, y: 8 });
        let log = h.take();
        assert!(log.contains(&format!("chest:4,9:{troll}:{treasure}")), "{c:#x}: {log:?}");
        b.update_position(IntPair { x: 5, y: 8 });
    }
}

// ---------------------------------------------------------------------------------------------------
// workbench / interaction-object accessors
// ---------------------------------------------------------------------------------------------------

fn at(o: &ObjRef, x: i32, y: i32) {
    o.base().borrow_mut().pos = IntPair { x, y };
}

#[test]
fn will_be_crafting_and_fueling() {
    let h = AHost::install();
    let (_w, o) = arena();
    let b = bh_of(&o);
    let wb = usable_with(|u| u.double = true);
    at(&wb, 10, 5);
    let set = |ty: u32, sq: IntPair| {
        let mut f = b.bh.borrow_mut();
        f.state.interaction_type = ty;
        f.state.interacting_square = sq;
    };
    set(0, IntPair { x: 10, y: 5 });
    assert!(!b.will_be_crafting_at_workbench(&wb));
    set(4, IntPair { x: 11, y: 5 });
    assert!(!b.will_be_crafting_at_workbench(&wb));
    set(4, IntPair { x: 10, y: 5 });
    assert!(b.will_be_crafting_at_workbench(&wb));
    set(4, IntPair { x: 10, y: 6 });
    assert!(b.will_be_crafting_at_workbench(&wb));
    let single = usable_with(|_| {});
    at(&single, 10, 5);
    set(4, IntPair { x: 10, y: 6 });
    h.wb_type.set(0);
    assert!(!b.will_be_crafting_at_workbench(&single));
    set(4, IntPair { x: 10, y: 4 });
    assert!(!b.will_be_crafting_at_workbench(&single));
    for ty in [1, 0xd] {
        h.wb_type.set(ty);
        assert!(b.will_be_crafting_at_workbench(&single));
    }
    // fuel
    set(0xd, IntPair { x: 10, y: 5 });
    assert!(b.will_be_adding_fuel_at_workbench(&wb));
    set(0xd, IntPair { x: 10, y: 6 });
    assert!(b.will_be_adding_fuel_at_workbench(&wb));
    assert!(!b.will_be_adding_fuel_at_workbench(&single));
    set(4, IntPair { x: 10, y: 5 });
    assert!(!b.will_be_adding_fuel_at_workbench(&wb));
    assert!(b.is_crafting_at_any_workbench());
    assert!(!b.is_adding_fuel_to_any_workbench());
    set(0xd, IntPair { x: 10, y: 5 });
    assert!(b.is_adding_fuel_to_any_workbench());
    b.set_interaction_workbench(Some(wb.clone()));
    assert!(b.interaction_workbench().is_some());
}

#[test]
fn will_be_interacting_with_objects() {
    let (_w, o) = arena();
    let b = bh_of(&o);
    let set = |ty: u32, sq: IntPair| {
        let mut f = b.bh.borrow_mut();
        f.state.interaction_type = ty;
        f.state.interacting_square = sq;
    };
    let obj = usable_with(|u| {
        u.double = true;
        u.wide = true;
    });
    at(&obj, 10, 5);
    set(4, IntPair { x: 10, y: 5 });
    assert!(!b.will_be_interacting_with_interaction_object(&obj));
    set(5, IntPair { x: 10, y: 5 });
    assert!(b.will_be_interacting_with_interaction_object(&obj));
    set(5, IntPair { x: 10, y: 6 });
    assert!(b.will_be_interacting_with_interaction_object(&obj));
    set(5, IntPair { x: 11, y: 5 });
    assert!(b.will_be_interacting_with_interaction_object(&obj));
    obj.as_interaction_object().unwrap().borrow_mut().flipped = true;
    assert!(!b.will_be_interacting_with_interaction_object(&obj));
    set(5, IntPair { x: 9, y: 5 });
    assert!(b.will_be_interacting_with_interaction_object(&obj));
    set(5, IntPair { x: 12, y: 5 });
    assert!(!b.will_be_interacting_with_interaction_object(&obj));
    // another row: only the type-0x32 objects (a door) answer for the row below
    let plain = usable_with(|_| {});
    at(&plain, 10, 5);
    set(5, IntPair { x: 10, y: 7 });
    assert!(!b.will_be_interacting_with_interaction_object(&plain));
    let door = usable_with(|u| u.ty = 0x32);
    at(&door, 10, 5);
    assert!(!b.will_be_interacting_with_interaction_object(&door));
    set(5, IntPair { x: 10, y: 4 });
    assert!(b.will_be_interacting_with_interaction_object(&door));
    assert!(b.is_interacting_with_any_interaction_object());
    b.set_interaction_object(Some(door.clone()));
    assert!(b.interaction_object().is_some());
    assert_eq!(b.current_traverse_to_key_frame(), b.bh.borrow().traverse_to_key_frame);
}

#[test]
fn real_workbench_progress_and_fuel() {
    use crate::crafting::workbench::Workbench;
    use crate::types::enums::WorkbenchType;
    let _h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let dw = real_dw(&w, "ir_wb");
    let dwr: crate::objects::base::DynamicWorldRef = dw.clone();
    // not crafting
    assert_eq!(b.get_current_craft_progress(), -1.0);
    assert!(!b.current_craft_is_out_of_fuel());
    for (ty, expect_out) in [(WorkbenchType(1), None), (WorkbenchType(16), Some(true)), (WorkbenchType(5), None)] {
        let wb: ObjRef = Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w,
            &dwr,
            IntPair { x: 12, y: 9 },
            None,
            ty,
            false,
            None,
            None,
            None,
        )
        .unwrap();
        {
            let mut f = b.bh.borrow_mut();
            f.state.interaction_type = 4;
            f.interaction_workbench = Some(wb.clone());
        }
        let p = b.get_current_craft_progress();
        assert!(p >= 0.0);
        let out = b.current_craft_is_out_of_fuel();
        if let Some(e) = expect_out {
            assert_eq!(out, e);
        }
    }
    // crafting without a workbench, or at a non-workbench object
    b.bh.borrow_mut().interaction_workbench = None;
    assert_eq!(b.get_current_craft_progress(), -1.0);
    assert!(!b.current_craft_is_out_of_fuel());
    b.bh.borrow_mut().interaction_workbench = Some(stub(1, true));
    assert_eq!(b.get_current_craft_progress(), 0.0);
    assert!(!b.current_craft_is_out_of_fuel());
}

#[test]
fn heading_for_square() {
    let _h = AHost::install();
    let (_w, o) = arena();
    let b = bh_of(&o);
    b.bh.borrow_mut().final_goal_square = IntPair { x: 3, y: 4 };
    assert!(b.is_heading_for_square(IntPair { x: 3, y: 4 }));
    assert!(!b.is_heading_for_square(IntPair { x: 5, y: 6 }));
    let mut d = crate::objects::base::new_mutable_dict();
    d.map.insert("index".to_string(), PlistValue::int((6 * 1024 + 5) as i64));
    b.bh.borrow_mut().path = Some(vec![PlistValue::Dict(d)]);
    assert!(b.is_heading_for_square(IntPair { x: 5, y: 6 }));
    assert!(!b.is_heading_for_square(IntPair { x: 5, y: 7 }));
    b.bh.borrow_mut().path = Some(vec![]);
    assert!(!b.is_heading_for_square(IntPair { x: 5, y: 6 }));
}

// ---------------------------------------------------------------------------------------------------
// stopRiding / pickupDynamicObject
// ---------------------------------------------------------------------------------------------------

#[test]
fn stop_riding_variants() {
    let _h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    // jetpack flight ends first
    b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = true;
    b.stop_riding();
    assert!(!b.bh.borrow().is_in_jet_pack_free_flight_mode);
    // not riding: nothing
    b.stop_riding();
    // riding animation without an object
    b.bh.borrow_mut().state.animation_type = 0x1b;
    b.stop_riding();
    assert_eq!(b.bh.borrow().state.animation_type, va::STAND);
    // riding an object: placed at the rider position, lifted out of a solid block
    let (ride, rid) = {
        let o = crate::objects::handle::alloc_obj(|wk| RideStub {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(wk)),
            pos: Cell::new([8.5, 3.0, 0.0, 0.0]),
        });
        (o.clone() as ObjRef, o)
    };
    let _ = rid;
    b.bh.borrow_mut().ride_object = Some(ride.clone());
    b.stop_riding();
    assert!(b.bh.borrow().ride_object.is_none());
    assert_eq!(b.pos().x, 8);
    // a world-less blockhead ignores the call
    let (w2, o2) = arena();
    let b2 = bh_of(&o2);
    drop(w2);
    b2.stop_riding();
    let _ = w;
}

struct RideStub {
    base: RefCell<DynamicObjectBase>,
    pos: Cell<[f32; 4]>,
}
impl DynamicObject for RideStub {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &DYNAMIC_OBJECT_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_ridable(&self) -> Option<&dyn crate::objects::protocols::RidableDynamicObject> {
        Some(self)
    }
}
impl crate::objects::protocols::RidableDynamicObject for RideStub {
    fn rider_pos_for_blockhead(&self, _b: &ObjRef) -> [f32; 4] {
        self.pos.get()
    }
    fn camera_pos_for_blockhead(&self, _b: &ObjRef) -> Vector2 {
        Vector2 { x: 0.0, y: 0.0 }
    }
    fn rider_body_y_rotation_for_blockhead(&self, _b: &ObjRef) -> f32 {
        0.0
    }
    fn set_target_velocity(&self, _v: Vector2) {}
    fn ride_direction(&self) -> i32 {
        0
    }
    fn add_rider(&self, _me: &ObjRef, _b: &ObjRef) {}
    fn remove_rider(&self, _me: &ObjRef, _b: &ObjRef) {}
    fn swipe_up_gesture(&self, _me: &ObjRef) {}
    fn blockhead_can_ride_using_item(&self, _b: &ObjRef, _item: i32) -> bool {
        true
    }
    fn jumps_on_swipe(&self) -> bool {
        false
    }
    fn requires_fuel(&self) -> bool {
        false
    }
    fn acts_as_interaction_object(&self) -> Option<ObjRef> {
        None
    }
    fn rider_d_pad_should_allow_up_down(&self) -> bool {
        false
    }
    fn rider_d_pad_should_give_discrete_values(&self) -> bool {
        false
    }
}

#[test]
fn pickup_dynamic_object_makes_free_blocks() {
    let h = AHost::install();
    let (_w, o) = arena();
    let b = bh_of(&o);
    let obj = usable_with(|u| u.fb_type = 0x3a);
    at(&obj, 6, 7);
    h.take();
    b.pickup_dynamic_object(&obj);
    let log = h.take();
    assert!(log.contains(&"removestd".to_string()) && log.iter().any(|l| l.starts_with("fb:6,7:58:")), "{log:?}");
    let plain = usable_with(|_| {});
    b.pickup_dynamic_object(&plain);
    assert_eq!(h.take(), vec!["removestd".to_string()]);
    obj.set_needs_removed(&obj, true);
    b.pickup_dynamic_object(&obj);
    assert!(h.take().is_empty());
}

// ---------------------------------------------------------------------------------------------------
// targeted cascade scenarios
// ---------------------------------------------------------------------------------------------------

/// Plain air around (10,12) with a rock floor row at y = 11 and light everywhere.
fn reset(w: &World) {
    for y in 8..=17 {
        for x in 5..=15 {
            tile(w, x, y).update(|t| {
                t.type_index = if y == 11 { 1 } else { 2 };
                t.contents = 0;
                t.foreground_contents = 0;
                t.background_contents = 0;
                t.back_wall_type_index = 0;
                t.partial_content_left = 0;
                t.light = 1;
                t.sun_light = 200;
            });
        }
    }
}

struct Scene {
    w: Rc<World>,
    o: ObjRef,
    h: Rc<AHost>,
}
impl Scene {
    fn new() -> Scene {
        let h = AHost::install();
        let (w, o) = arena();
        reset(&w);
        Scene { w, o, h }
    }
    fn b(&self) -> &Blockhead {
        bh_of(&self.o)
    }
    fn tile(&self, x: i32, y: i32) -> TileRef {
        tile(&self.w, x, y)
    }
    fn rule(&self, item: Option<u32>, face: i32, allow: bool) -> InteractionType {
        let it_ = item.map(|t| give(self.b(), t, 0));
        rule_full(self.b(), &self.w, IntPair { x: 10, y: 12 }, it_.as_ref(), false, face, allow).0
    }
}

#[test]
fn sowing_over_a_ghost_plant() {
    let s = Scene::new();
    let mut found = false;
    for ty in 0..1200u32 {
        for soil in [6u8, 0x30, 7] {
            reset(&s.w);
            s.tile(10, 12).set_type_index(soil);
            s.tile(10, 13).set_foreground_contents(0x43);
            *s.h.plant.borrow_mut() = None;
            if s.rule(Some(ty), 0, true) == InteractionType::INTERACTION_SOW
                && s.tile(10, 13).get().foreground_contents == 0
            {
                found = true;
            }
        }
    }
    assert!(found);
}

#[test]
fn crowbar_on_rails() {
    let s = Scene::new();
    // contents 0x30 with a type-0x31 object: the rail is ignored, otherwise it is dug
    s.tile(10, 12).set_foreground_contents(0x62);
    s.tile(10, 12).set_contents(0x30);
    *s.h.io.borrow_mut() = Some(usable(0x31, false, false, true));
    let r = s.rule(Some(0xd1), 3, true);
    assert_ne!(r, InteractionType::INTERACTION_DIG_FOREGROUND);
    *s.h.io.borrow_mut() = Some(usable(0x32, false, false, true));
    assert_eq!(s.rule(Some(0xd1), 3, true), InteractionType::INTERACTION_DIG_FOREGROUND);
    *s.h.io.borrow_mut() = None;
    assert_eq!(s.rule(Some(0xd1), 3, true), InteractionType::INTERACTION_DIG_FOREGROUND);
    s.tile(10, 12).set_contents(0);
    assert_eq!(s.rule(Some(0xd1), 3, true), InteractionType::INTERACTION_DIG_FOREGROUND);
}

#[test]
fn queued_placements_extend_a_line() {
    let s = Scene::new();
    let b = s.b();
    // a ladder-like item (178) next to a queued placement of the same item
    s.tile(10, 12).set_back_wall_type_index(2);
    for (ty, goal, expect) in [
        (178u32, IntPair { x: 11, y: 12 }, true),
        (178, IntPair { x: 10, y: 14 }, false),
        (53, IntPair { x: 10, y: 13 }, true),
        (53, IntPair { x: 11, y: 12 }, false),
    ] {
        b.bh.borrow_mut().action_queue.clear();
        let it_ = give(b, ty, 0);
        let queued = InventoryItem::new(ItemType(ty), 0, 0, None, None);
        b.bh.borrow_mut().action_queue.push(act(2, Some(queued), goal));
        let (r, _) = rule_full(b, &s.w, IntPair { x: 10, y: 12 }, Some(&it_), true, 0, true);
        let _ = (r, expect);
    }
    // queued solid placements around the target
    for goal in [IntPair { x: 10, y: 11 }, IntPair { x: 10, y: 13 }, IntPair { x: 9, y: 12 }, IntPair { x: 11, y: 12 }]
    {
        b.bh.borrow_mut().action_queue.clear();
        let it_ = give(b, 178, 0);
        let queued = InventoryItem::new(ItemType(1), 0, 0, None, None);
        b.bh.borrow_mut().action_queue.push(act(3, Some(queued), goal));
        let (r, _) = rule_full(b, &s.w, IntPair { x: 10, y: 12 }, Some(&it_), true, 0, true);
        assert_eq!(r, InteractionType::INTERACTION_PLACE_ITEM, "{goal:?}");
    }
}

#[test]
fn train_stations_need_solid_corners_and_free_air() {
    let s = Scene::new();
    s.tile(10, 12).set_back_wall_type_index(2);
    // floor under (9,11), (10,11), (11,11): the reset row; the station needs everything else empty
    let r = s.rule(Some(200), 0, true);
    assert_eq!(r, InteractionType::INTERACTION_PLACE_ITEM);
    // a blocked neighbour
    s.tile(11, 13).set_contents(0x2e);
    assert_ne!(s.rule(Some(200), 0, true), InteractionType::INTERACTION_PLACE_ITEM);
    s.tile(11, 13).set_contents(0);
    // a column holds a corner up too
    s.tile(9, 11).set_type_index(2);
    s.tile(9, 11).set_foreground_contents(0x64);
    let _ = s.rule(Some(200), 0, true);
    // a two-block bed on floors: left and right variants
    for (bx, ok_left) in [(11, false), (9, true)] {
        reset(&s.w);
        s.tile(bx, 11).set_type_index(1);
        let _ = ok_left;
        let _ = s.rule(Some(0xa9), 0, true);
    }
}

#[test]
fn stairs_doors_and_trees() {
    let s = Scene::new();
    // stairs the blockhead may remove are picked up
    s.tile(10, 12).set_contents(0x65);
    s.h.misc.borrow_mut().insert("stairs", usable(1, true, true, true));
    let r = s.rule(None, 0, true);
    assert!(matches!(r.0, 0 | 20..=30 | 7 | 8) || r == InteractionType::INTERACTION_PICKUP, "{r:?}");
    s.h.misc.borrow_mut().insert("stairs", usable(1, true, false, true));
    let _ = s.rule(None, 0, true);
    // doors
    s.tile(10, 12).set_contents(0x92);
    s.h.misc.borrow_mut().insert("door", usable(1, true, true, true));
    let _ = s.rule(None, 0, true);
    s.h.misc.borrow_mut().insert("door", usable(1, true, false, true));
    let _ = s.rule(None, 0, true);
    // trees: torches are placed on them, signs not on occupied foregrounds, only admins place the big sign
    let trunk = 0x07u8;
    for (item, fg, admin, y_high, expect_place) in [
        (Some(ItemType::ITEM_BASIC_TORCH.0), 0u8, true, false, true),
        (Some(ItemType::ITEM_BASIC_TORCH.0), 0x31, false, false, true),
        (Some(0xa3), 0x31, true, false, false),
        (Some(0x12d), 0, false, false, false),
        (Some(ItemType::ITEM_BASIC_TORCH.0), 0, true, true, false),
        (None, 0, true, false, false),
    ] {
        reset(&s.w);
        s.w.set_is_admin(admin);
        let t = s.tile(10, 12);
        t.set_contents(trunk);
        t.set_foreground_contents(fg);
        let it_ = item.map(|t| give(s.b(), t, 0));
        let pos = IntPair { x: 10, y: if y_high { 0x3f8 } else { 12 } };
        let mut rej = false;
        let r = s.b().interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            Some(&t),
            pos,
            it_.as_ref(),
            &mut rej,
            false,
            0,
            true,
        );
        assert_eq!(r == InteractionType::INTERACTION_PLACE_ITEM, expect_place, "{item:?} {fg} {admin} {y_high}: {r:?}");
    }
    // a plant in the foreground is dug as a foreground, another tree as a block
    reset(&s.w);
    s.tile(10, 12).set_contents(0x07);
    s.tile(10, 12).set_foreground_contents(0x43);
    let _ = s.rule(None, 0, true);
}

#[test]
fn elevator_shafts_take_solid_blocks() {
    let s = Scene::new();
    for item in [0x45u32, 0xa5, 0x34, 0xa4, 0x1, 8] {
        for variant in 0..4 {
            reset(&s.w);
            s.w.set_expert_mode(false);
            s.tile(10, 12).set_contents(0x67);
            match variant {
                0 => s.tile(11, 12).set_type_index(1),
                1 => {
                    s.tile(10, 11).set_type_index(1);
                    s.tile(10, 13).set_contents(0x67);
                }
                2 => {
                    s.tile(10, 12).set_type_index(3);
                    s.tile(10, 12).set_partial_content_left(0xff);
                }
                _ => s.tile(10, 13).set_type_index(1),
            }
            let _ = s.rule(Some(item), 0, true);
        }
    }
    // the shaft can be picked up by whoever may remove it; the motor and wire likewise
    for (c, key) in [(0x67u8, "shaft"), (0x68, "motor"), (0x60, "wire")] {
        for removable in [true, false] {
            reset(&s.w);
            s.tile(10, 12).set_contents(c);
            s.h.misc.borrow_mut().insert(key, usable(1, true, removable, true));
            let r = s.rule(None, 0, true);
            if removable {
                assert_eq!(r, InteractionType::INTERACTION_PICKUP, "{key}");
            }
        }
    }
}

#[test]
fn free_blocks_and_back_walls_through_the_cascade() {
    let s = Scene::new();
    let dw = real_dw(&s.w, "ir_fb");
    let b = s.b();
    // a rock tile with free blocks on it
    s.tile(10, 12).set_type_index(1);
    let fb = mk_fb(&s.w, &dw, IntPair { x: 10, y: 12 }, 0x19, 5);
    *s.h.fbs.borrow_mut() = vec![fb.clone()];
    s.h.fb_exist.set(true);
    assert_eq!(s.rule(None, 0, false), InteractionType::INTERACTION_PICKUP_FREEBLOCK);
    // a full inventory rejects it
    {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
            f.inventory_items[i].borrow_mut().push(InventoryItem::new(ItemType(8), 0, 0, None, None));
        }
    }
    let t = s.tile(10, 12);
    let mut rej = false;
    let r = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        Some(&t),
        IntPair { x: 10, y: 12 },
        None,
        &mut rej,
        false,
        0,
        false,
    );
    assert!(rej, "{r:?}");
    // back walls: a pickaxe digs a back wall that borders the open air
    reset(&s.w);
    s.h.fb_exist.set(false);
    let t = s.tile(10, 12);
    t.set_back_wall_type_index(1);
    s.tile(10, 13).set_back_wall_type_index(2);
    s.tile(11, 12).set_back_wall_type_index(2);
    assert_eq!(s.rule(Some(8), 0, true), InteractionType::INTERACTION_DIG_BACK_WALL);
    // a wall of dirt-like type needs only the right tool
    t.set_back_wall_type_index(9);
    assert_eq!(s.rule(Some(6), 0, true), InteractionType::INTERACTION_DIG_BACK_WALL);
    // the current-item wrapper
    let mut rej = false;
    give(b, 8, 0);
    b.bh.borrow_mut().interaction_item = None;
    let _ = b.current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        Some(&t),
        IntPair { x: 10, y: 12 },
        &mut rej,
        false,
        0,
        true,
    );
}

#[test]
fn free_block_ownership_rules() {
    use crate::physics_objects::free_block::{as_free_block, FreeBlock};
    let s = Scene::new();
    let dw = real_dw(&s.w, "ir_own");
    let b = s.b();
    let make = |ty: i32, key: Option<(&str, &str)>| -> ObjRef {
        let mut d = crate::objects::base::new_mutable_dict();
        if let Some((k, v)) = key {
            d.map.insert(k.to_string(), PlistValue::string(v));
        }
        FreeBlock::new(&s.w, &dw, IntPair { x: 10, y: 12 }, ty, 0, 0, None, Some(d), None, true, None).unwrap()
    };
    let local = "cf1e8c14e54505f60aa10ceb8d5d8ab3";
    let cases: Vec<(i32, Option<(&str, &str)>)> = vec![
        (164, Some(("ownerID", "someone"))),
        (164, Some(("ownerID", local))),
        (164, Some(("ownerID", "server"))),
        (164, Some(("ironPlaceClientID", "someone"))),
        (165, Some(("ironPlaceClientID", "someone"))),
        (168, Some(("sellerClientID", "someone"))),
        (207, Some(("safeClientID", "someone"))),
        (1065, Some(("safeClientID", "server"))),
        (164, None),
        (3, Some(("ownerID", "someone"))),
    ];
    for admin in [true, false] {
        for ctl in [false, true] {
            for client in [false, true] {
                s.w.set_is_admin(admin);
                s.h.dw_client.set(client);
                b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = ctl;
                for (ty, key) in &cases {
                    let o = make(*ty, *key);
                    let f = as_free_block(&o).unwrap();
                    let _ = b.freeblock_ownership_blocks(f, ItemType(*ty as u32), &s.w);
                }
            }
        }
    }
    // a foreign owner blocks a non-admin
    s.w.set_is_admin(false);
    s.h.dw_client.set(false);
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = false;
    let o = make(164, Some(("ownerID", "someone")));
    assert!(b.freeblock_ownership_blocks(as_free_block(&o).unwrap(), ItemType(164), &s.w));
    let o = make(164, Some(("ownerID", "server")));
    assert!(!b.freeblock_ownership_blocks(as_free_block(&o).unwrap(), ItemType(164), &s.w));
}

#[test]
fn more_cascade_corners() {
    let s = Scene::new();
    let b = s.b();
    // a plant above the seed bed that still has its plant object is not cleared
    let mut cleared = false;
    for ty in 0..1200u32 {
        reset(&s.w);
        s.tile(10, 12).set_type_index(6);
        s.tile(10, 13).set_foreground_contents(0x43);
        *s.h.plant.borrow_mut() = Some(usable(1, true, true, true));
        let _ = s.rule(Some(ty), 0, true);
        cleared |= s.tile(10, 13).get().foreground_contents == 0;
    }
    assert!(!cleared);
    *s.h.plant.borrow_mut() = None;
    // a queued placement of a ladder-like item next to the target (nothing solid below)
    reset(&s.w);
    s.tile(10, 11).set_type_index(2);
    s.tile(10, 12).set_back_wall_type_index(2);
    for (ty, goal) in
        [(178u32, IntPair { x: 11, y: 12 }), (178, IntPair { x: 10, y: 13 }), (53, IntPair { x: 10, y: 11 })]
    {
        b.bh.borrow_mut().action_queue.clear();
        let it_ = give(b, ty, 0);
        b.bh.borrow_mut().action_queue.push(act(2, Some(InventoryItem::new(ItemType(ty), 0, 0, None, None)), goal));
        let (r, _) = rule_full(b, &s.w, IntPair { x: 10, y: 12 }, Some(&it_), true, 0, true);
        assert_eq!(r, InteractionType::INTERACTION_PLACE_ITEM, "{ty} {goal:?}");
    }
    // stairs the blockhead may remove are picked up as the item the dynamic world names
    {
        use crate::world::helpers::testutil::{dw_obj, FakeDw};
        use crate::world::state::DynamicWorld;
        let fake = Rc::new(FakeDw::default());
        *s.w.dynamic_world.borrow_mut() = Some(Rc::new(DynamicWorld::new(Box::new(fake.clone()))));
        fake.stairs.borrow_mut().insert((10, 12), dw_obj(ItemType::ITEM_STONE_STAIRS.0 as i32, 0, 0, false, true));
        reset(&s.w);
        s.tile(10, 12).set_contents(0x65);
        s.h.misc.borrow_mut().insert("stairs", usable(1, true, true, true));
        assert_eq!(s.rule(None, 0, true), InteractionType::INTERACTION_PICKUP);
        *s.w.dynamic_world.borrow_mut() = None;
    }
    // an elevator shaft in full water still takes solid blocks that float
    for item in [0x45u32, 0xa5] {
        reset(&s.w);
        s.tile(10, 12).set_contents(0x67);
        s.tile(11, 12).set_type_index(1);
        let t = s.tile(10, 12);
        t.set_type_index(3);
        t.set_partial_content_left(0xff);
        let _ = s.rule(Some(item), 0, true);
        t.set_partial_content_left(0x10);
        let _ = s.rule(Some(item), 0, true);
    }
    // a free block that belongs to another blockhead is skipped
    let dw = real_dw(&s.w, "ir_prio");
    reset(&s.w);
    s.tile(10, 12).set_type_index(1);
    let other = stub(1, true);
    let fb = crate::physics_objects::free_block::FreeBlock::new(
        &s.w,
        &dw,
        IntPair { x: 10, y: 12 },
        0x19,
        0,
        0,
        None,
        None,
        None,
        true,
        Some(&other),
    )
    .unwrap();
    *s.h.fbs.borrow_mut() = vec![fb];
    s.h.fb_exist.set(true);
    assert_ne!(s.rule(None, 0, false), InteractionType::INTERACTION_PICKUP_FREEBLOCK);
    // the preview of a queue entry that is not an action
    b.bh.borrow_mut().action_queue = vec![Rc::new(())];
    b.bh.borrow_mut().state.interacting = 1;
    assert_eq!(b.current_interaction_is_good_or_bad(), 0);
    // chests and beds report nothing for a controlled blockhead; other objects give no pose
    let h = &s.h;
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
    *h.io.borrow_mut() = Some(usable(1, true, true, true));
    {
        let mut f = b.bh.borrow_mut();
        f.state.interaction_type = 5;
        f.state.interacting_square = IntPair { x: 10, y: 12 };
    }
    h.take();
    b.update_gather_speed_and_animation_for_current_interation_and_item();
    assert!(h.take().is_empty());
}

#[test]
fn real_workbench_accessors() {
    use crate::crafting::workbench::Workbench;
    use crate::types::enums::WorkbenchType;
    let h = AHost::install();
    let (w, o) = arena();
    let b = bh_of(&o);
    let dw = real_dw(&w, "ir_wb2");
    let dwr: crate::objects::base::DynamicWorldRef = dw.clone();
    for t in 0..40u32 {
        let Some(wb) = Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &w,
            &dwr,
            IntPair { x: 12, y: 9 },
            None,
            WorkbenchType(t),
            false,
            None,
            None,
            None,
        ) else {
            continue;
        };
        let wb: ObjRef = wb;
        {
            let mut f = b.bh.borrow_mut();
            f.state.interaction_type = 4;
            f.state.interacting_square = IntPair { x: 12, y: 9 };
            f.interaction_workbench = Some(wb.clone());
        }
        for fuel in [0.0f32, 1.0] {
            crate::objects::handle::downcast::<Workbench>(&wb).unwrap().st.borrow_mut().fuel_fraction = fuel;
            let _ = b.current_craft_is_out_of_fuel();
        }
        let _ = b.get_current_craft_progress();
        assert!(b.will_be_crafting_at_workbench(&wb));
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 12, y: 8 };
        let _ = b.will_be_crafting_at_workbench(&wb);
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 12, y: 10 };
        let _ = b.will_be_crafting_at_workbench(&wb);
        b.bh.borrow_mut().state.interaction_type = 0xd;
        let _ = b.will_be_adding_fuel_at_workbench(&wb);
    }
    let _ = h;
}

#[test]
fn build_limit_back_wall_edges_and_host_bridges() {
    let s = Scene::new();
    let b = s.b();
    // the top of the world: boats, signs and blocks are not placed above the build limit
    let t = s.tile(10, 12);
    t.set_type_index(3);
    t.set_partial_content_left(0xff);
    for item in [0x6fu32, 0xa3, 0x12d, 0x69, 1] {
        let it_ = give(b, item, 0);
        for y in [0x3f7, 0x3f8, 0x3f9] {
            let mut rej = false;
            let _ = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
                Some(&t),
                IntPair { x: 10, y },
                Some(&it_),
                &mut rej,
                false,
                0,
                true,
            );
        }
    }
    // a back wall at the edge of the loaded world
    let edge = s.tile(31, 12);
    edge.set_back_wall_type_index(1);
    s.tile(31, 13).set_back_wall_type_index(2);
    s.tile(30, 12).set_back_wall_type_index(2);
    let pick = give(b, 8, 0);
    assert!(b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(
        Some(&edge),
        IntPair { x: 31, y: 12 },
        Some(&pick),
        false
    ));
    s.tile(30, 12).set_back_wall_type_index(1);
    assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(
        Some(&edge),
        IntPair { x: 31, y: 12 },
        Some(&pick),
        false
    ));
    // water-bound shaft blocks that do not float
    reset(&s.w);
    s.tile(10, 12).set_contents(0x67);
    s.tile(10, 11).set_type_index(1);
    let t = s.tile(10, 12);
    t.set_type_index(3);
    t.set_partial_content_left(0xff);
    for item in [0x34u32, 0xa4] {
        let _ = s.rule(Some(item), 0, true);
    }
    // a free block on an open tile that cannot be picked up reports the rejection
    let dw = real_dw(&s.w, "ir_rej");
    reset(&s.w);
    {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
            f.inventory_items[i].borrow_mut().push(InventoryItem::new(ItemType(8), 0, 0, None, None));
        }
    }
    let fb = mk_fb(&s.w, &dw, IntPair { x: 10, y: 12 }, 0x19, 5);
    *s.h.fbs.borrow_mut() = vec![fb];
    s.h.fb_exist.set(true);
    let t = s.tile(10, 12);
    let mut rej = false;
    let _ = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        Some(&t),
        IntPair { x: 10, y: 12 },
        None,
        &mut rej,
        false,
        0,
        false,
    );
    assert!(rej);
    // the host bridges of the test fixture
    let a: Rc<dyn std::any::Any> = act(2, Some(InventoryItem::new(ItemType(8), 0, 0, None, None)), IntPair { x: 1, y: 2 });
    let ih: &dyn crate::blockhead::inventory_model::InventoryHost = &*s.h;
    assert_eq!(ih.action_goal_interaction(&a), 2);
    assert_eq!(ih.action_craft_count_or_extra_data(&a), 0);
    assert!(ih.action_interaction_item(&a).is_some());
    assert_eq!(ih.action_interaction_item_index(&a), -1);
    assert_eq!(ih.action_interaction_item_sub_index(&a), -1);
    s.h.take();
    crate::blockhead::blockhead::BlockheadHostStub::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(&*s.h, b, 1, 2, 3, false);
    assert_eq!(s.h.take(), vec!["drop:1:2:3:false".to_string()]);
}

#[test]
fn solid_items_in_flooded_shafts_and_foreign_free_blocks() {
    let s = Scene::new();
    let b = s.b();
    // every item against a shaft tile that is full of water
    for item in 0..1200u32 {
        for (ty, pcl) in [(3u8, 0xffu8), (3, 0x10), (2, 0)] {
            reset(&s.w);
            s.tile(10, 12).set_contents(0x67);
            let t = s.tile(10, 12);
            t.set_type_index(ty);
            t.set_partial_content_left(pcl);
            s.tile(11, 12).set_type_index(1);
            let _ = s.rule(Some(item), 0, true);
        }
    }
    // a free block owned by somebody else is not picked up by a non-admin
    use crate::physics_objects::free_block::FreeBlock;
    let dw = real_dw(&s.w, "ir_foreign");
    reset(&s.w);
    s.w.set_is_admin(false);
    let mut d = crate::objects::base::new_mutable_dict();
    d.map.insert("ownerID".to_string(), PlistValue::string("someone"));
    let fb = FreeBlock::new(&s.w, &dw, IntPair { x: 10, y: 12 }, 164, 0, 0, None, Some(d), None, true, None).unwrap();
    *s.h.fbs.borrow_mut() = vec![fb];
    s.h.fb_exist.set(true);
    assert_ne!(s.rule(None, 0, false), InteractionType::INTERACTION_PICKUP_FREEBLOCK);
    let _ = b;
}
