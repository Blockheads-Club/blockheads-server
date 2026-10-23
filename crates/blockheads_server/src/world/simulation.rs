//! Offline catch-up simulation: `startSimulatingIfNeeded`, `continueSimulate`, `finishSimulating` (dead on the
//! dedicated server, `server != nil`) and the LIVE `addSimulationEventOfType:forBlockhead:extraData:` that fills the
//! away-client event dictionaries.
//!
//!
//! ## Storage
//! `World.simulation_events` / `away_client_simulation_events` still have the type `Option<PlistDict>` in world_state
//! (request R-2). The typed maps of this file live in a per-thread side table until that field type is changed; the
//! accessors [`with_simulation_events`] / [`with_away_client_simulation_events`] are the only places to change.

use std::cell::RefCell;

use crate::foundation::clock;
use crate::foundation::gsdict::{GsDict, GsKey};
use crate::foundation::nsnumber::NsNumber;
use crate::foundation::nsstring;
use crate::items::predicates::{item_type_is_solid, item_type_is_sowable};
use crate::objects::handle::ObjRef;
use crate::types::item_type::ItemType;
use crate::world::lifecycle::core_hooks;
use crate::world::state::World;

// ---------------------------------------------------------------------------------------------
// GsKey for the integer key types of the event dictionaries (hash and equality like `-[NSNumber hash]`)
// ---------------------------------------------------------------------------------------------

impl GsKey for i32 {
    fn gs_hash(&self) -> u64 {
        NsNumber::Int(*self as i64).gs_hash()
    }
    fn gs_eq(&self, other: &Self) -> bool {
        self == other
    }
}
impl GsKey for i64 {
    fn gs_hash(&self) -> u64 {
        NsNumber::Int(*self).gs_hash()
    }
    fn gs_eq(&self, other: &Self) -> bool {
        self == other
    }
}
impl GsKey for u64 {
    fn gs_hash(&self) -> u64 {
        NsNumber::UInt(*self).gs_hash()
    }
    fn gs_eq(&self, other: &Self) -> bool {
        self == other
    }
}

/// What `extraData:` carries per event type.
pub enum SimulationEventExtra<'a> {
    None,
    /// SE_CRAFT: `[extra craftableItem]` (zero struct if extra is nil)
    CraftedItem {
        crafted_item_type: i32,
        crafted_item_count: i32,
    },
    /// SE_GOT: `[extra itemType]` (free block); SE_PLACE: `[extra intValue]`
    ItemType(i32),
    /// SE_DAMAGE text; SE_COLLAPSE / SE_SLEEP ignore it
    Text(&'a str),
}

/// One entry of the per-blockhead dictionary (an `NSMutableDictionary` keyed by the event type `NSNumber`).
#[derive(Clone, Debug)]
pub enum SimEvent {
    /// SE_SLEEP (0), SE_COLLAPSE (5): `ev["count"]`
    Count(i32),
    /// SE_CRAFT (1) and SE_GOT (2): itemType NSNumber -> count NSNumber
    Items(GsDict<i64, i32>),
    /// SE_PLACE (3): `"solid"` / `"sow"` / `"item"` -> count
    Places(GsDict<String, i32>),
    /// SE_DAMAGE (4): text -> text (`[ev setObject:extra forKey:extra]`)
    Damage(GsDict<String, String>),
}

/// The per-blockhead dictionary: `"blockheadName"` plus the events keyed by the event type.
/// Note: the original keeps both in ONE dictionary (mixed `NSString` / `NSNumber` keys); the enumeration
/// order of that mixed dictionary is not reproduced by this split representation (contract 3.1 shape).
#[derive(Clone, Debug)]
pub struct BlockheadEvents {
    /// `"blockheadName"`
    pub name: Option<String>,
    /// key = event type
    pub events: GsDict<i32, SimEvent>,
}

/// uniqueID (`NSNumber` unsigned long) -> events.
pub type BlockheadEventsMap = GsDict<u64, BlockheadEvents>;
/// clientID -> events of the blockheads simulated while that client was away.
pub type AwayClientSimulationEvents = GsDict<String, BlockheadEventsMap>;

/// Event types (`enum SimulationEvent`).
pub const SE_SLEEP: i32 = 0;
pub const SE_CRAFT: i32 = 1;
pub const SE_GOT: i32 = 2;
pub const SE_PLACE: i32 = 3;
pub const SE_DAMAGE: i32 = 4;
pub const SE_COLLAPSE: i32 = 5;

thread_local! {
    static SIMULATION_EVENTS: RefCell<Option<BlockheadEventsMap>> = const { RefCell::new(None) };
    static AWAY_EVENTS: RefCell<Option<AwayClientSimulationEvents>> = const { RefCell::new(None) };
}

/// Run `f` on `World.simulationEvents` (nil = `None`).
pub fn with_simulation_events<R>(f: impl FnOnce(&mut Option<BlockheadEventsMap>) -> R) -> R {
    SIMULATION_EVENTS.with(|s| f(&mut s.borrow_mut()))
}

/// Run `f` on `World.awayClientSimulationEvents` (nil = `None`). Used by world_ops
/// `welcome_back_events_message_for_client_id`.
pub fn with_away_client_simulation_events<R>(f: impl FnOnce(&mut Option<AwayClientSimulationEvents>) -> R) -> R {
    AWAY_EVENTS.with(|s| f(&mut s.borrow_mut()))
}

/// `[awayClientSimulationEvents removeObjectForKey:clientID]` (nil dictionary: no-op).
pub fn away_events_remove_client(client_id: &str) {
    with_away_client_simulation_events(|a| {
        if let Some(m) = a.as_mut() {
            m.remove(&client_id.to_string());
        }
    });
}

/// Apply the per-type update of the event dictionary `ev`.
fn update_event(ev_map: &mut GsDict<i32, SimEvent>, ty: i32, extra: &SimulationEventExtra) {
    match ty {
        SE_CRAFT => {
            let (t, n) = match extra {
                SimulationEventExtra::CraftedItem { crafted_item_type, crafted_item_count } => {
                    (*crafted_item_type, *crafted_item_count)
                }
                _ => (0, 0), // `[extra craftableItem]` on nil: zero struct
            };
            if !matches!(ev_map.get(&ty), Some(SimEvent::Items(_))) {
                ev_map.insert(ty, SimEvent::Items(GsDict::new()));
            }
            if let Some(SimEvent::Items(items)) = ev_map.get_mut(&ty) {
                let cur = items.get(&(t as i64)).copied().unwrap_or(0);
                items.insert(t as i64, cur.wrapping_add(n));
            }
        }
        SE_GOT => {
            let t = match extra {
                SimulationEventExtra::ItemType(t) => *t,
                _ => 0,
            };
            if !matches!(ev_map.get(&ty), Some(SimEvent::Items(_))) {
                ev_map.insert(ty, SimEvent::Items(GsDict::new()));
            }
            if let Some(SimEvent::Items(items)) = ev_map.get_mut(&ty) {
                let cur = items.get(&(t as i64)).copied().unwrap_or(0);
                items.insert(t as i64, cur.wrapping_add(1));
            }
        }
        SE_PLACE => {
            let t = match extra {
                SimulationEventExtra::ItemType(t) => *t,
                _ => 0,
            };
            let item = ItemType(t as u32);
            let key = if item_type_is_solid(item) {
                "solid"
            } else if item_type_is_sowable(item) {
                "sow"
            } else {
                "item"
            };
            if !matches!(ev_map.get(&ty), Some(SimEvent::Places(_))) {
                ev_map.insert(ty, SimEvent::Places(GsDict::new()));
            }
            if let Some(SimEvent::Places(p)) = ev_map.get_mut(&ty) {
                let cur = p.get(&key.to_string()).copied().unwrap_or(0);
                p.insert(key.to_string(), cur.wrapping_add(1));
            }
        }
        SE_DAMAGE => {
            // `[ev setObject:extra forKey:extra]`; nil would raise, unreachable
            if let SimulationEventExtra::Text(s) = extra {
                if !matches!(ev_map.get(&ty), Some(SimEvent::Damage(_))) {
                    ev_map.insert(ty, SimEvent::Damage(GsDict::new()));
                }
                if let Some(SimEvent::Damage(d)) = ev_map.get_mut(&ty) {
                    d.insert(s.to_string(), s.to_string());
                }
            }
        }
        _ => {
            // SE_SLEEP, SE_COLLAPSE (and any other value): ev["count"] += 1
            let cur = match ev_map.get(&ty) {
                Some(SimEvent::Count(c)) => *c,
                _ => 0,
            };
            ev_map.insert(ty, SimEvent::Count(cur.wrapping_add(1)));
        }
    }
}

/// Look up / create the `BlockheadEvents` of `unique_id` in `map` and run the type update.
fn add_to_map(
    map: &mut BlockheadEventsMap,
    unique_id: u64,
    name: Option<String>,
    ty: i32,
    extra: &SimulationEventExtra,
) {
    if map.get(&unique_id).is_none() {
        map.insert(unique_id, BlockheadEvents { name, events: GsDict::new() });
    }
    let bd = map.get_mut(&unique_id).expect("just inserted");
    update_event(&mut bd.events, ty, extra);
}

impl World {
    /// `-[World startSimulatingIfNeeded]`: YES only when not simulating, no client, no server, no tutorial and
    /// `lastUpdateTime >= 1.0`; the dedicated server has a server so it is always NO.
    pub fn start_simulating_if_needed(&self) -> bool {
        if self.is_simulating() || self.server.borrow().is_some() || self.last_update_time() < 1.0 {
            return false;
        }
        let now = clock::reference_time();
        let elapsed = now - self.last_update_time();
        if elapsed >= 1.0 {
            let hooks = core_hooks();
            let mut action_count = 0i32;
            let mut should_sim = false;
            let bhs = if self.dynamic_world.borrow().is_some() { hooks.dynamic_world_blockheads() } else { Vec::new() };
            for bh in &bhs {
                if hooks.blockhead_asleep(bh) {
                    action_count += 1;
                }
                if hooks.blockhead_regenerating(bh) {
                    action_count += 1;
                }
                if hooks.blockhead_has_actions(bh) {
                    action_count += hooks.blockhead_action_count(bh);
                }
                if hooks.blockhead_should_continue_simulating(bh) {
                    should_sim = true;
                }
            }
            self.set_total_number_of_actions_to_simulate(action_count);
            if should_sim {
                self.set_is_simulating(true);
                let left = if elapsed >= 86400.0 { 86400.0 } else { elapsed };
                self.set_time_left_to_simulate(left);
                self.set_total_time_to_simulate(left as f32);
                self.set_simulation_progress(0.0);
                with_simulation_events(|s| *s = Some(GsDict::new()));
                hooks.dynamic_world_set_paused(false);
                return true;
            }
        }
        false
    }

    /// `-[World addSimulationEventOfType:forBlockhead:extraData:]`: LIVE for blockheads simulated while their client
    /// is away (`isClientBlockheadBeingControlledByServer`): fills `awayClientSimulationEvents[client][uniqueID]
    /// [type]`; the other branch writes only into `simulationEvents` (nil on the server: no effect).
    pub fn add_simulation_event_of_type_for_blockhead_extra_data(
        &self,
        ty: i32,
        blockhead: Option<&ObjRef>,
        extra: SimulationEventExtra,
    ) {
        let hooks = core_hooks();
        let bh = match blockhead {
            Some(b) => b,
            None => return, // every message to a nil blockhead is a no-op (non-away branch on a local scratch dict)
        };
        if !(hooks.blockhead_should_continue_simulating(bh) || ty != SE_DAMAGE) {
            return; // damage ignored unless the blockhead still simulates
        }
        let unique_id = bh.base().borrow().unique_id;
        let name = hooks.blockhead_name(bh);
        if !hooks.blockhead_is_client_blockhead_being_controlled_by_server(bh) {
            with_simulation_events(|s| {
                if let Some(map) = s.as_mut() {
                    add_to_map(map, unique_id, name, ty, &extra);
                }
                // `simulationEvents` nil: the events land in a scratch dictionary that is dropped
            });
        } else {
            let client_id = match hooks.blockhead_client_id(bh) {
                Some(c) => c,
                None => {
                    // the original allocates awayClientSimulationEvents first, then returns
                    with_away_client_simulation_events(|a| {
                        if a.is_none() {
                            *a = Some(GsDict::new());
                        }
                    });
                    return;
                }
            };
            with_away_client_simulation_events(|a| {
                let away = a.get_or_insert_with(GsDict::new);
                if away.get(&client_id).is_none() {
                    away.insert(client_id.clone(), GsDict::new());
                }
                let cd = away.get_mut(&client_id).expect("just inserted");
                add_to_map(cd, unique_id, name, ty, &extra);
            });
        }
    }

    /// `-[World continueSimulate]` -> BOOL. Called each frame while `is_simulating`; unreachable on the dedicated
    /// server (never simulating), ported for completeness.
    pub fn continue_simulate(&self) -> bool {
        if !self.is_simulating() {
            return false;
        }
        let hooks = core_hooks();
        let left = self.time_left_to_simulate();
        let dt: f32 = (if 0.999f32 as f64 <= left { 0.999f32 as f64 } else { left }) as f32;
        let bhs = if self.dynamic_world.borrow().is_some() { hooks.dynamic_world_blockheads() } else { Vec::new() };
        let mut all = true;
        let mut any_asleep = false;
        if !bhs.is_empty() {
            for bh in &bhs {
                if !hooks.blockhead_asleep(bh) {
                    if hooks.blockhead_meditating(bh) {
                        any_asleep = true;
                    } else if !hooks.blockhead_regenerating(bh) && !hooks.blockhead_on_trade_mission(bh) {
                        all = false;
                    }
                } else {
                    any_asleep = true;
                }
            }
        }
        if !(all && any_asleep) {
            if self.fast_forward() {
                self.set_fast_forward(false);
            }
        } else if !self.fast_forward() {
            self.set_fast_forward(true);
        }
        let step = if self.fast_forward() { (dt / 20.0f32) as f64 } else { dt as f64 };
        self.set_time_left_to_simulate(left - step);

        let mut action_count = 0i32;
        let mut should_sim = false;
        for bh in &bhs {
            if hooks.blockhead_cancel_simulate_due_to_collapse(bh) {
                continue;
            }
            if hooks.blockhead_asleep(bh) {
                action_count += 1;
            }
            if hooks.blockhead_regenerating(bh) {
                action_count += 1;
            }
            if hooks.blockhead_has_actions(bh) && !hooks.blockhead_current_interaction_requires_human_input(bh) {
                action_count += hooks.blockhead_action_count(bh);
            }
            if hooks.blockhead_should_continue_simulating(bh) {
                should_sim = true;
            }
        }
        let time_left = self.time_left_to_simulate();
        if action_count > 0 {
            let total_actions = self.total_number_of_actions_to_simulate();
            if action_count < (time_left / 30.0) as i32 || self.total_time_to_simulate() < 1.0 {
                let frac = 1.0f32 - (action_count as f32 / total_actions as f32).min(1.0);
                self.set_simulation_progress(self.simulation_progress().max(frac));
            } else {
                let frac = 1.0f64 - (time_left / self.total_time_to_simulate() as f64).min(1.0);
                self.set_simulation_progress((self.simulation_progress() as f64).max(frac) as f32);
            }
        }
        if !should_sim {
            self.finish_simulating();
            return false;
        }
        let mut mult = 1.0f32;
        let rules = self.custom_rules();
        if rules.custom_rules_enabled != 0 {
            if rules.day_night_cycle == 0 {
                mult = 0.25;
            } else if rules.day_night_cycle == 2 {
                mult = 4.0;
            }
        }
        self.set_world_time(self.world_time() + (dt * mult) as f64);
        self.set_no_rain_timer(self.no_rain_timer() + dt);
        if self.dynamic_world.borrow().is_some() {
            hooks.dynamic_world_simulate(dt);
        }
        if self.used_physical_blocks_len() > 0x80 {
            self.save_all();
            // `[self decommisionAllBlocksBlockToSavePhyscialBlock:nil]` (world_ops,)
            self.decommision_all_blocks_block_to_save_physcial_block(false);
        }
        hooks.world_path_request_pump(self);
        if self.time_left_to_simulate() < 1.0 {
            self.finish_simulating();
        }
        self.is_simulating()
    }

    /// `-[World finishSimulating]`.
    pub fn finish_simulating(&self) {
        let hooks = core_hooks();
        if self.dynamic_world.borrow().is_some() {
            hooks.dynamic_world_finish_simulating();
        }
        self.set_simulation_progress(1.0);
        self.set_time_left_to_simulate(0.0);
        self.set_is_simulating(false);
        self.set_last_update_time(clock::reference_time());
        let msg = hooks.world_welcome_back_message_for_simulation_events(self);
        if let Some(d) = self.core_delegate() {
            d.show_welcome_back_popup_with_message(msg.as_deref());
        }
        with_simulation_events(|s| *s = None);
        if self.dynamic_world.borrow().is_some() {
            hooks.dynamic_world_set_paused(true);
        }
        hooks.world_pause_button_tapped(self);
    }
}

/// `-[NSString hash]` of an event dictionary key, exposed for world_ops iteration-order tests.
#[allow(dead_code)]
pub fn event_string_key_hash(s: &str) -> u32 {
    nsstring::gs_hash(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::lifecycle::testkit::*;

    fn bh(f: &Fixture, id: u64, client: Option<&str>, away: bool, cont: bool) -> ObjRef {
        let o = crate::world::helpers::testutil::new_obj();
        o.base().borrow_mut().unique_id = id;
        f.rec.bh.borrow_mut().insert(
            id,
            Bh {
                client_id: client.map(|s| s.to_string()),
                name: Some(format!("BH{}", id)),
                controlled_by_server: away,
                should_continue: cont,
                ..Default::default()
            },
        );
        o
    }

    fn away_count(client: &str, id: u64, ty: i32, key: &str) -> i32 {
        with_away_client_simulation_events(|a| {
            let ev = a
                .as_ref()
                .unwrap()
                .get(&client.to_string())
                .unwrap()
                .get(&id)
                .unwrap()
                .events
                .get(&ty)
                .unwrap()
                .clone();
            match ev {
                SimEvent::Count(c) => c,
                SimEvent::Items(i) => *i.get(&key.parse::<i64>().unwrap()).unwrap(),
                SimEvent::Places(p) => *p.get(&key.to_string()).unwrap(),
                SimEvent::Damage(d) => d.get(&key.to_string()).map_or(0, |_| 1),
            }
        })
    }

    #[test]
    fn away_events_accumulate_per_type_with_mixed_key_orders() {
        let f = fixture(32);
        let w = &f.world;
        with_away_client_simulation_events(|a| *a = None);
        let b = bh(&f, 42, Some("cl"), true, true);
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, Some(&b), SimulationEventExtra::None);
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, Some(&b), SimulationEventExtra::None);
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_COLLAPSE, Some(&b), SimulationEventExtra::None);
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_CRAFT,
            Some(&b),
            SimulationEventExtra::CraftedItem { crafted_item_type: 77, crafted_item_count: 3 },
        );
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_CRAFT,
            Some(&b),
            SimulationEventExtra::CraftedItem { crafted_item_type: 77, crafted_item_count: 2 },
        );
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_GOT, Some(&b), SimulationEventExtra::ItemType(5));
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_GOT, Some(&b), SimulationEventExtra::ItemType(5));
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_GOT, Some(&b), SimulationEventExtra::ItemType(6));
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_DAMAGE,
            Some(&b),
            SimulationEventExtra::Text("FALL"),
        );
        // SE_PLACE: solid / sowable / other, selected by the item predicates
        let solid = (0..400u32).find(|&i| item_type_is_solid(ItemType(i))).unwrap() as i32;
        let other = (0..400u32)
            .find(|&i| !item_type_is_solid(ItemType(i)) && !item_type_is_sowable(ItemType(i)))
            .unwrap() as i32;
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_PLACE,
            Some(&b),
            SimulationEventExtra::ItemType(solid),
        );
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_PLACE,
            Some(&b),
            SimulationEventExtra::ItemType(solid),
        );
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_PLACE,
            Some(&b),
            SimulationEventExtra::ItemType(other),
        );
        assert_eq!(away_count("cl", 42, SE_SLEEP, ""), 2);
        assert_eq!(away_count("cl", 42, SE_COLLAPSE, ""), 1);
        assert_eq!(away_count("cl", 42, SE_CRAFT, "77"), 5);
        assert_eq!(away_count("cl", 42, SE_GOT, "5"), 2);
        assert_eq!(away_count("cl", 42, SE_GOT, "6"), 1);
        assert_eq!(away_count("cl", 42, SE_DAMAGE, "FALL"), 1);
        assert_eq!(away_count("cl", 42, SE_PLACE, "solid"), 2);
        assert_eq!(away_count("cl", 42, SE_PLACE, "item"), 1);
        with_away_client_simulation_events(|a| {
            let bd = a.as_ref().unwrap().get(&"cl".to_string()).unwrap().get(&42).unwrap();
            assert_eq!(bd.name.as_deref(), Some("BH42"));
        });
        // damage is ignored once the blockhead stops simulating; other types are not
        let done = bh(&f, 43, Some("cl"), true, false);
        w.add_simulation_event_of_type_for_blockhead_extra_data(
            SE_DAMAGE,
            Some(&done),
            SimulationEventExtra::Text("X"),
        );
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, Some(&done), SimulationEventExtra::None);
        with_away_client_simulation_events(|a| {
            let m = a.as_ref().unwrap().get(&"cl".to_string()).unwrap();
            assert!(matches!(m.get(&43).unwrap().events.get(&SE_DAMAGE), None));
            assert!(m.get(&43).unwrap().events.get(&SE_SLEEP).is_some());
        });
        // a blockhead without a client id allocates the dictionary and returns
        with_away_client_simulation_events(|a| *a = None);
        let anon = bh(&f, 44, None, true, true);
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, Some(&anon), SimulationEventExtra::None);
        with_away_client_simulation_events(|a| assert!(a.as_ref().unwrap().is_empty()));
        // non-away blockhead with simulationEvents == nil: no effect
        let local = bh(&f, 45, Some("x"), false, true);
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, Some(&local), SimulationEventExtra::None);
        with_simulation_events(|s| assert!(s.is_none()));
        // nil blockhead: no effect
        w.add_simulation_event_of_type_for_blockhead_extra_data(SE_SLEEP, None, SimulationEventExtra::None);
    }

    #[test]
    fn integer_keys_hash_like_nsnumber() {
        assert_eq!(GsKey::gs_hash(&-1i32), 4294967295);
        assert_eq!(GsKey::gs_hash(&5_000_000_000u64), 705032704);
        assert_eq!(GsKey::gs_hash(&7i64), 7);
        // iteration order of a dictionary with NSNumber keys: hash % buckets, newest first within a bucket
        let mut d: GsDict<u64, i32> = GsDict::new();
        for i in 0..5u64 {
            d.insert(i, i as i32);
        }
        assert_eq!(d.keys_in_order().len(), 5);
    }

    #[test]
    fn dedicated_server_never_starts_the_offline_simulation() {
        let f = fixture(32);
        let w = &f.world;
        w.set_last_update_time(100.0);
        assert!(!w.start_simulating_if_needed());
        assert!(!w.is_simulating());
        assert!(!w.continue_simulate());
    }

    #[test]
    fn simulation_loop_logic_for_a_serverless_world() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        *w.server.borrow_mut() = None;
        let _cl = crate::foundation::clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::foundation::clock::set_virtual(Some(1_800_000_000.0));
        let now = crate::foundation::clock::reference_time();
        w.set_last_update_time(now - 100000.0); // more than a day: clamped
        let b = bh(&f, 1, Some("p"), false, true);
        f.rec.blockheads.borrow_mut().push(b);
        assert!(w.start_simulating_if_needed());
        assert!(w.is_simulating());
        assert_eq!(w.time_left_to_simulate(), 86400.0);
        assert_eq!(w.total_time_to_simulate(), 86400.0f32);
        assert!(w.continue_simulate());
        // one step: dt = 0.999f widened, time_left reduced by it
        assert!((w.time_left_to_simulate() - (86400.0 - 0.999f32 as f64)).abs() < 1e-9);
        assert_eq!(w.no_rain_timer(), 0.999f32);
        assert!(w.world_time() > 0.0);
        w.set_time_left_to_simulate(0.5);
        assert!(!w.continue_simulate(), "< 1.0 left finishes");
        assert!(!w.is_simulating());
        assert_eq!(w.simulation_progress(), 1.0);
        with_simulation_events(|s| assert!(s.is_none()));
        crate::foundation::clock::set_virtual(None);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::world::lifecycle::testkit::*;

    fn bh(f: &Fixture, id: u64, edit: impl FnOnce(&mut Bh)) -> ObjRef {
        let o = crate::world::helpers::testutil::new_obj();
        o.base().borrow_mut().unique_id = id;
        let mut b = Bh {
            client_id: Some("p".into()),
            name: Some(format!("B{id}")),
            should_continue: true,
            ..Default::default()
        };
        edit(&mut b);
        f.rec.bh.borrow_mut().insert(id, b);
        f.rec.blockheads.borrow_mut().push(o.clone());
        o
    }

    #[test]
    fn events_without_their_extra_data_and_local_events() {
        let f = fixture(32);
        let w = &f.world;
        let local = bh(&f, 1, |_| {});
        with_simulation_events(|s| *s = Some(GsDict::new()));
        for (ty, extra) in [
            (SE_CRAFT, SimulationEventExtra::None),
            (SE_GOT, SimulationEventExtra::None),
            (SE_PLACE, SimulationEventExtra::None),
            (SE_DAMAGE, SimulationEventExtra::None),
            (SE_DAMAGE, SimulationEventExtra::Text("FIRE")),
            (SE_SLEEP, SimulationEventExtra::None),
        ] {
            w.add_simulation_event_of_type_for_blockhead_extra_data(ty, Some(&local), extra);
        }
        let sowable = (0..400u32).find(|&i| item_type_is_sowable(ItemType(i)) && !item_type_is_solid(ItemType(i)));
        if let Some(i) = sowable {
            w.add_simulation_event_of_type_for_blockhead_extra_data(
                SE_PLACE,
                Some(&local),
                SimulationEventExtra::ItemType(i as i32),
            );
        }
        with_simulation_events(|s| {
            let ev = &s.as_ref().unwrap().get(&1).unwrap().events;
            assert!(ev.get(&SE_CRAFT).is_some() && ev.get(&SE_DAMAGE).is_some());
            *s = None;
        });
        assert_eq!(event_string_key_hash("a"), nsstring::gs_hash("a"));
    }

    #[test]
    fn counting_actions_and_nothing_to_simulate() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        *w.server.borrow_mut() = None;
        let _cl = crate::foundation::clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::foundation::clock::set_virtual(Some(1_800_000_000.0));
        let now = crate::foundation::clock::reference_time();
        w.set_last_update_time(now - 50.0);
        let _ = bh(&f, 1, |b| {
            b.should_continue = false;
            b.asleep = true;
            b.regenerating = true;
            b.actions = 3;
        });
        // somebody sleeps but nobody continues: counted and refused
        assert!(!w.start_simulating_if_needed());
        assert_eq!(w.total_number_of_actions_to_simulate(), 5);
        // already simulating, a server, or recently updated
        w.set_last_update_time(now - 0.5);
        assert!(!w.start_simulating_if_needed());
        w.set_is_simulating(true);
        assert!(!w.start_simulating_if_needed());
        w.set_is_simulating(false);
        crate::foundation::clock::set_virtual(None);
    }

    #[test]
    fn the_catch_up_loop_tracks_progress_and_fast_forward() {
        let f = fixture(32);
        let w = &f.world;
        with_dw(w);
        let _cl = crate::foundation::clock::TEST_CLOCK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::foundation::clock::set_virtual(Some(1_800_000_000.0));
        // sleepers, meditators, regenerators, collapsed and busy blockheads
        let _ = bh(&f, 1, |b| b.asleep = true);
        let _ = bh(&f, 2, |b| b.meditating = true);
        let _ = bh(&f, 3, |b| b.regenerating = true);
        let _ = bh(&f, 4, |b| b.on_trade_mission = true);
        let _ = bh(&f, 5, |b| {
            b.collapse = true;
            b.asleep = true;
        });
        let _ = bh(&f, 6, |b| {
            b.actions = 4;
            b.needs_human = true;
            b.asleep = true;
        });
        let _ = bh(&f, 7, |b| {
            b.actions = 4;
            b.regenerating = true;
        });
        w.set_is_simulating(true);
        w.set_time_left_to_simulate(5000.0);
        w.set_total_time_to_simulate(5000.0);
        w.set_total_number_of_actions_to_simulate(10);
        assert!(w.continue_simulate());
        assert!(w.fast_forward(), "everybody is asleep, meditating or regenerating");
        // somebody awake switches it off again, and short totals use the action fraction
        let _ = bh(&f, 8, |b| b.actions = 1);
        w.set_total_time_to_simulate(0.5);
        assert!(w.continue_simulate());
        assert!(!w.fast_forward());
        // a long run uses the time fraction; custom day lengths scale the clock
        let mut rules = w.custom_rules();
        rules.custom_rules_enabled = 1;
        for cycle in [0, 2] {
            rules.day_night_cycle = cycle;
            w.set_custom_rules(rules);
            w.set_total_time_to_simulate(5000.0);
            w.set_time_left_to_simulate(4000.0);
            assert!(w.continue_simulate());
        }
        w.set_total_time_to_simulate(5000.0);
        w.set_time_left_to_simulate(60.0);
        assert!(w.continue_simulate());
        // many blocks in use: saved and decommissioned
        while w.used_physical_blocks_len() <= 0x80 {
            w.take_physical_block();
        }
        w.set_time_left_to_simulate(4000.0);
        assert!(w.continue_simulate());
        // a world without blockheads is finished at once
        f.rec.blockheads.borrow_mut().clear();
        w.set_time_left_to_simulate(4000.0);
        assert!(!w.continue_simulate());
        w.set_is_simulating(true);
        // nobody continues: finished
        let _ = bh(&f, 9, |b| b.should_continue = false);
        w.set_time_left_to_simulate(4000.0);
        assert!(!w.continue_simulate());
        assert!(!w.is_simulating());
        crate::foundation::clock::set_virtual(None);
    }
}
