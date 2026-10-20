//! Pole items (North / West / South / East pole clothing): `poleItemTaken:` and `checkAndRestorePoleItems:`.
//!
//! Both methods return at once when `client != nil` (never on the server) or when the custom rule
//! `customRulesEnabled != 0 && poleItems == 1`. `check_and_restore_pole_items` is only ticked from
//! `DynamicWorld::update` when `!is_simulation`.

use super::lookups::ops;
use super::state::DynamicWorld;
use crate::foundation::gsdict::GsDict;
use crate::plist::value::PlistValue;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::helpers::tile_is_air_water_or_snow;
use crate::world::tiles::tile_at_world_position;

/// Phase A item order: SANTA_HAT 0x8e (142), FRUIT_HAT 0x8c (140), SOUTH_POLE_BOOTS 0x8f (143), GRASS_SKIRT 0x8d (141).
pub const POLE_ITEMS_PHASE_A: [i32; 4] = [0x8e, 0x8c, 0x8f, 0x8d];

/// Recheck period `2.0f` (float32).
const RECHECK_PERIOD: f32 = 2.0f32;
/// Add period `20.0f` (float32).
const ADD_PERIOD: f32 = 20.0f32;
/// Respawn delay in world seconds (float64, 10 h).
const RESPAWN_DELAY: f64 = 36000.0;

impl DynamicWorld {
    /// `customRules.customRulesEnabled != 0 && customRules.poleItems == 1`.
    fn pole_items_infinite(&self) -> bool {
        let rules = self.world().custom_rules();
        rules.custom_rules_enabled != 0 && rules.pole_items == 1
    }

    /// `x` of a pole item (`N = worldWidthMacro * 32`, int32): Phase B mapping 0x8c -> N/4, 0x8d -> 3*(N/4),
    /// 0x8f -> N/2, otherwise (0x8e) 0.
    fn pole_item_x(&self, item_type: i32) -> i32 {
        let n = self.world().world_width_macro().wrapping_mul(32);
        match item_type {
            0x8c => n / 4,
            0x8d => (n / 4).wrapping_mul(3),
            0x8f => n / 2,
            _ => 0,
        }
    }

    /// `h = max(rockAndDirt(x), lake(x))` (written as `if h <= lake { h = lake }`), `y = max(h, 0x200) + 0x10`.
    fn pole_item_pos(&self, x: i32) -> IntPair {
        let (mut h, lake) = match self.world_tile_loader.borrow().clone() {
            Some(l) => (l.max_of_rock_and_dirt_height_for_x(x), l.lake_height_for_x(x)),
            None => (0, 0),
        };
        if h <= lake {
            h = lake;
        }
        let y = (if h < 0x201 { 0x200 } else { h }).wrapping_add(0x10);
        IntPair { x, y }
    }

    /// `exists fb in [self freeBlocksAtPos:pos] with [fb itemType] == itemType && [fb hovers]`.
    fn hovering_free_block_exists(&self, pos: IntPair, item_type: i32) -> bool {
        let hooks = self.hooks();
        let o = ops();
        match self.free_blocks_at_pos(pos) {
            Some(list) => list.iter().any(|fb| hooks.item_type(fb) == item_type && o.freeblock_hovers(fb)),
            None => false,
        }
    }

    /// `-poleItemTaken:`: records `"%d"` -> world time (double).
    pub fn pole_item_taken(&self, item_type: i32) {
        if self.pole_items_infinite() {
            return;
        }
        let now = self.world().world_time();
        let mut d = self.pole_item_taken_times.borrow_mut();
        d.get_or_insert_with(GsDict::new).insert(format!("{}", item_type), PlistValue::real_f64(now));
    }

    /// `-checkAndRestorePoleItems:`. Float32 timers; Phase A (every >20 s) marks missing items as taken now;
    /// Phase B (every >2 s) respawns expired entries in `GsDict` enumeration order (observable through unique ids).
    pub fn check_and_restore_pole_items(&self, dt: f32) {
        if self.pole_items_infinite() {
            return;
        }
        let world = self.world();
        self.pole_item_restore_recheck_timer.set(self.pole_item_restore_recheck_timer.get() + dt);
        self.pole_item_restore_add_timer.set(self.pole_item_restore_add_timer.get() + dt);
        if self.pole_item_restore_recheck_timer.get() > RECHECK_PERIOD {
            self.pole_item_restore_recheck_timer.set(0.0);
            if self.pole_item_restore_add_timer.get() > ADD_PERIOD {
                self.pole_item_restore_add_timer.set(self.pole_item_restore_add_timer.get() - ADD_PERIOD);
                // Phase A
                let n = world.world_width_macro().wrapping_mul(32);
                for (idx, &item_type) in POLE_ITEMS_PHASE_A.iter().enumerate() {
                    let key = format!("{}", item_type);
                    let pending = self.pole_item_taken_times.borrow().as_ref().map_or(false, |d| d.get(&key).is_some());
                    if !pending {
                        let x = (n / 4).wrapping_mul(idx as i32);
                        let pos = self.pole_item_pos(x);
                        let tile = tile_at_world_position(pos.x, pos.y, &world.macro_tiles(), &world);
                        if tile.is_some() && !self.hovering_free_block_exists(pos, item_type) {
                            let now = world.world_time();
                            self.pole_item_taken_times
                                .borrow_mut()
                                .get_or_insert_with(GsDict::new)
                                .insert(key, PlistValue::real_f64(now));
                        }
                    }
                }
            }
            // Phase B
            let entries: Vec<(String, f64)> = match self.pole_item_taken_times.borrow().as_ref() {
                Some(d) => d.iter().map(|(k, v)| (k.clone(), v.double_value())).collect(),
                None => Vec::new(),
            };
            let mut items_to_remove: Vec<String> = Vec::new();
            for (key, taken) in entries {
                let now = world.world_time();
                if now - taken > RESPAWN_DELAY {
                    let item_type = crate::foundation::nsstring::integer_value(&key) as i32;
                    let x = self.pole_item_x(item_type);
                    let pos = self.pole_item_pos(x);
                    let tile = tile_at_world_position(pos.x, pos.y, &world.macro_tiles(), &world);
                    if tile.is_some() && tile_is_air_water_or_snow(tile) {
                        if !self.hovering_free_block_exists(pos, item_type) {
                            self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                pos,
                                ItemType(item_type as u32),
                                0,
                                0,
                                None,
                                None,
                                true,
                                0,
                                None,
                            );
                        }
                        items_to_remove.push(key);
                    }
                }
            }
            if let Some(d) = self.pole_item_taken_times.borrow_mut().as_mut() {
                for k in items_to_remove {
                    d.remove(&k);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::DwWorldGen;
    use crate::types::structs::CustomRules;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Gen;
    impl DwWorldGen for Gen {
        fn tree_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            Rc::new(RefCell::new(vec![]))
        }
        fn npc_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            Rc::new(RefCell::new(vec![]))
        }
        fn plant_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            Rc::new(RefCell::new(vec![]))
        }
        fn max_of_rock_and_dirt_height_for_x(&self, _x: i32) -> i32 {
            100
        }
        fn lake_height_for_x(&self, _x: i32) -> i32 {
            50
        }
    }

    /// timers, strict `>` at 36000, infinite rule disables both methods.
    #[test]
    fn taken_and_timers() {
        let d = dw("pole_t", 4);
        d.pole_item_taken(0x8e);
        assert_eq!(d.pole_item_taken_times.borrow().as_ref().unwrap().len(), 1);
        // no tiles are loaded -> Phase A / B find no tile and change nothing; timers advance in f32
        d.check_and_restore_pole_items(1.0);
        assert_eq!(d.pole_item_restore_recheck_timer.get(), 1.0);
        d.check_and_restore_pole_items(1.2); // 2.2 > 2.0 -> reset
        assert_eq!(d.pole_item_restore_recheck_timer.get(), 0.0);
        assert_eq!(d.pole_item_restore_add_timer.get(), 2.2f32);
        d.pole_item_restore_add_timer.set(20.1);
        d.pole_item_restore_recheck_timer.set(2.1);
        d.check_and_restore_pole_items(0.0);
        assert!((d.pole_item_restore_add_timer.get() - 0.1).abs() < 1e-5);
        // infinite rule: nothing recorded
        let d2 = dw("pole_inf", 4);
        let mut cr = CustomRules::default();
        cr.custom_rules_enabled = 1;
        cr.pole_items = 1;
        d2.world().set_custom_rules(cr);
        d2.pole_item_taken(0x8e);
        assert!(d2.pole_item_taken_times.borrow().is_none());
        d2.check_and_restore_pole_items(10.0);
        assert_eq!(d2.pole_item_restore_recheck_timer.get(), 0.0);
    }

    #[test]
    fn positions() {
        let d = dw("pole_pos", 4); // N = 128
        *d.world_tile_loader.borrow_mut() = Some(Rc::new(Gen));
        assert_eq!(d.pole_item_x(0x8c), 32);
        assert_eq!(d.pole_item_x(0x8d), 96);
        assert_eq!(d.pole_item_x(0x8f), 64);
        assert_eq!(d.pole_item_x(0x8e), 0);
        // h = 100 < 0x201 -> 0x200 + 0x10
        assert_eq!(d.pole_item_pos(32).y, 0x210);
    }
}
