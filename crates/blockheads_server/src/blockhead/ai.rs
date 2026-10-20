//! Class `BlockheadAI`: the autonomous behaviour of an idle Blockhead (never constructed on the dedicated server,
//! but ported completely: it defines `isRunByAI`, the `isAI` gate of `queueActionWithGoalPos` and `noPathToAction`).
//!
//! Deviation: the contract's `Weak<Blockhead>` is an [`ObjWeak`] here because player
//! and AI Blockheads live as `ObjRef` (`Rc<dyn DynamicObject>`) and cannot be recovered as `Rc<Blockhead>`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::items::food::food_type_for_item_type;
use crate::items::predicates::*;
use crate::objects::handle::{ptr_eq, ObjRef, ObjWeak};
use crate::plants::plant::as_plant;
use crate::rng::glibc_rand::{r01, rand};
use crate::types::enums::{FoodType, TileType};
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::types::tile::Tile;
use crate::world::helpers::*;
use crate::world::loot::item_type_from_tile_is_foregorund;
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;
use crate::worldgen::noise::NoiseFunction;

use super::blockhead::{as_blockhead, host, Blockhead};
use super::interaction_rules::actions_host;

/// `AI_MAX_ACTIONS`.
pub const AI_MAX_ACTIONS: usize = 16;

/// The ivars of `BlockheadAI`.
pub struct AiState {
    /// 0x018
    pub found_tile_pos: [IntPair; 16],
    /// 0x098
    pub found_tile: [bool; 16],
    /// 0x0a8 ring buffer of recently used tiles
    pub previous_action_tiles: [IntPair; 32],
    /// 0x1a8
    pub previous_action_count: i32,
    /// 0x1ac
    pub inactivity_timer: f32,
    /// 0x1b0 `(-1,-1)` = none
    pub goal_dig_tile: IntPair,
    /// 0x1b8
    pub current_search_level: i32,
    /// 0x1bc
    pub current_search_level_count: i32,
    /// 0x1c0
    pub last_frame_had_actions: bool,
    /// 0x1c4
    pub placable_item: ItemType,
    /// 0x1c8
    pub sowable_item: ItemType,
    /// 0x1cc
    pub random_inactivity_timer: f32,
    /// 0x1d0
    pub only_move_this_time: bool,
    /// 0x1d8
    pub move_direction_noise_function: Option<Rc<NoiseFunction>>,
    /// 0x1e0
    pub move_direction: f32,
    /// 0x1e4
    pub time_elapsed: f32,
    /// 0x1e8
    pub must_sleep_timer: f32,
}

/// `BlockheadAI: NSObject`.
pub struct BlockheadAI {
    blockhead: ObjWeak,
    world: Weak<World>,
    st: RefCell<AiState>,
}

const ZERO: IntPair = IntPair { x: 0, y: 0 };

fn sun_or_art(t: &Tile, th: u16) -> bool {
    (t.sun_light as u16) > th || t.artificial_light_r > th || t.artificial_light_g > th || t.artificial_light_b > th
}

impl BlockheadAI {
    /// `-initWithBlockhead:world:`. Five `rand()` draws in this order: noise seed, `level = rand() % 4`,
    /// `count = rand() % max(level^2, 1)`, `timer = R*16 + 2`, `onlyMove = 0.5 < R` (trap 6).
    pub fn init_with_blockhead_world(bh: &ObjRef, world: &Rc<World>) -> Rc<BlockheadAI> {
        let seed = rand();
        let noise =
            NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
                3, 3, 2, 0.9f64, seed, true, false, 1.2f64,
            );
        let time_elapsed = 0.0f32;
        let move_direction = noise.get_x_y_octaves(0.257f64 + (time_elapsed as f64) * 1.0e-5f64, 0.3564f64, 4) as f32;
        let (placable, sowable) = match as_blockhead(bh) {
            Some(b) => (b.placable_light_for_ai_item_type(), b.sowable_item_for_ai_item_type()),
            None => (ItemType(0), ItemType(0)),
        };
        let level = rand() % 4;
        let div = std::cmp::max(level.wrapping_mul(level), 1);
        let count = rand() % div;
        let timer = r01() * 16.0f32 + 2.0f32;
        let only_move = 0.5f32 < r01();
        Rc::new(BlockheadAI {
            blockhead: Rc::downgrade(bh),
            world: Rc::downgrade(world),
            st: RefCell::new(AiState {
                found_tile_pos: [ZERO; 16],
                found_tile: [false; 16],
                previous_action_tiles: [ZERO; 32],
                previous_action_count: 0,
                inactivity_timer: 0.0,
                goal_dig_tile: IntPair { x: -1, y: -1 },
                current_search_level: level,
                current_search_level_count: count,
                last_frame_had_actions: false,
                placable_item: placable,
                sowable_item: sowable,
                random_inactivity_timer: timer,
                only_move_this_time: only_move,
                move_direction_noise_function: Some(Rc::new(noise)),
                move_direction,
                time_elapsed,
                must_sleep_timer: 0.0,
            }),
        })
    }

    /// `-dealloc`: releases the noise function.
    pub fn dealloc(&self) {
        self.st.borrow_mut().move_direction_noise_function = None;
    }

    /// Copy of the ivars (tests).
    pub fn with_state<R>(&self, f: impl FnOnce(&mut AiState) -> R) -> R {
        f(&mut self.st.borrow_mut())
    }

    fn bh_obj(&self) -> Option<ObjRef> {
        self.blockhead.upgrade()
    }

    /// `-testTileAtPos:`: the first acceptable match for a still-empty slot sets `foundTile[k]` and returns YES.
    /// Rule order is exact (earlier rules shadow later ones).
    pub fn test_tile_at_pos(&self, pos: IntPair) -> bool {
        let Some(world) = self.world.upgrade() else { return false };
        let Some(bo) = self.bh_obj() else { return false };
        let Some(bh) = as_blockhead(&bo) else { return false };
        let h = host();
        let ah = actions_host();
        let tat = |x: i32, y: i32| tile_at_world_position_loaded(x, y, &world);
        let Some(tref) = tat(pos.x, pos.y) else { return false };
        let t = tref.get();
        let lit = |t: &Tile| bh.tile_is_lit_for_self_at_pos_impl(t, pos);
        let ctl = bh.is_client_blockhead_being_controlled_by_server();
        if !lit(&t) {
            if !ctl {
                return false;
            }
            if t.explored_fraction == 0 {
                return false;
            }
        }
        if t.sun_light == 0 && t.artificial_light_r == 0 && t.artificial_light_g == 0 && t.artificial_light_b == 0 {
            return false;
        }
        let tod = world.get_day_night_fraction_for_x_at_world_time(pos.x as f32, world.world_time());
        let mut should_sleep = false;
        let mut must_sleep = false;
        let energy = bh.energy();
        let must_sleep_timer = self.st.borrow().must_sleep_timer;
        if energy < 0.5f32 && !bh.asleep() {
            if energy < 0.25f32 {
                should_sleep = true;
                if energy < 0.2f32 && must_sleep_timer < 0.0f32 {
                    must_sleep = true;
                }
            }
            if tod >= 0.2f32 {
                if tod < 0.25f32 && energy < 0.4f32 {
                    should_sleep = true;
                }
            } else {
                should_sleep = true;
            }
        }
        let solid_neighbour = |p: IntPair| -> bool {
            tile_is_solid(tat(p.x, p.y + 1))
                || tile_is_solid(tat(p.x - 1, p.y))
                || tile_is_solid(tat(p.x + 1, p.y))
                || {
                    let b = tat(p.x, p.y - 1);
                    tile_is_solid(b) || tile_is_water(b)
                }
        };
        let found = |k: usize| self.st.borrow().found_tile[k];
        let set_found = |k: usize| {
            let mut s = self.st.borrow_mut();
            s.found_tile[k] = true;
            s.found_tile_pos[k] = pos;
        };
        let prev_hit = || self.st.borrow().previous_action_tiles.iter().any(|p| *p == pos);
        // 2 GET_AIR
        if !found(2) && bh.drown_fraction() > 0.1f32 {
            if prev_hit() {
                return false;
            }
            if (tile_is_air_or_snow(Some(tref)) || (tile_is_water(Some(tref)) && t.partial_content_left != 0xff))
                && t.contents == 0
                && t.foreground_contents == 0
                && t.background_contents == 0
                && sun_or_art(&t, 0x40)
                && solid_neighbour(pos)
            {
                set_found(2);
                return true;
            }
        }
        // 0 SLEEP_IN_BED / 1 SLEEP_ON_SPOT
        if should_sleep {
            if !found(0) && t.contents == 0x30 && ah.interaction_object_type_for_object_at_pos(bh, pos) == 3 {
                set_found(0);
                return true;
            }
            if !found(1) {
                if must_sleep && bh.can_sleep_on_spot() {
                    set_found(1);
                    bh.sleep_on_spot_if_possible_otherwise_cancel_actions();
                    self.st.borrow_mut().must_sleep_timer = 10.0f32;
                    return true;
                }
                if t.type_index == 2
                    && t.contents == 0
                    && t.background_contents == 0
                    && t.foreground_contents == 0
                    && tile_is_solid(tat(pos.x, pos.y - 1))
                {
                    set_found(1);
                    return true;
                }
            }
        }
        if prev_hit() {
            return false;
        }
        // 9 MOVE (no foundTile[9] guard)
        let (only_move, move_dir) = {
            let s = self.st.borrow();
            (s.only_move_this_time, s.move_direction)
        };
        if only_move {
            let bx = bh.pos().x;
            if ((bx < pos.x) == (move_dir > 0.0f32)) && tile_is_air_or_snow(Some(tref)) {
                let ok_vis = lit(&t) || (ctl && t.explored_fraction != 0);
                if ok_vis
                    && t.contents == 0
                    && t.foreground_contents == 0
                    && t.background_contents == 0
                    && sun_or_art(&t, 0x40)
                    && solid_neighbour(pos)
                {
                    set_found(9);
                    return true;
                }
            }
        }
        // 3 DIG_HOLE
        if !found(3) && self.st.borrow().goal_dig_tile == pos {
            set_found(3);
            return true;
        }
        // 5 PLACE_TORCH
        let placable_item = self.st.borrow().placable_item;
        if !found(5)
            && placable_item.0 != 0
            && tile_is_air_or_snow(Some(tref))
            && t.foreground_contents == 0
            && (t.sun_light < 0x40 || tod < 0.2f32)
            && t.artificial_light_r < 0x40
            && t.artificial_light_g < 0x40
            && t.artificial_light_b < 0x40
        {
            let p = placable_item;
            let mut placable = false;
            if item_type_is_placable_on_back_wall(p)
                && t.back_wall_type_index != 2
                && (t.type_index != 3 || t.partial_content_left != 0xff)
                && item_type_occupies_background_contents(p)
                && t.background_contents == 0
                && t.foreground_contents == 0
                && t.contents == 0
                && (!matches!(t.back_wall_type_index, 1 | 0x26 | 0x27 | 0x28 | 0x29)
                    || item_type_can_be_placed_on_background_rock(p))
            {
                placable = true;
            }
            if !placable && tile_is_tree(Some(tref)) && item_type_is_placable_on_trees(p) {
                placable = true;
            }
            if !placable && item_type_is_placable_on_wall(p) {
                let solid = item_type_is_solid(p);
                let r = tat(pos.x + 1, pos.y);
                if tile_is_solid(r) && (solid || !tile_contains_door(r)) {
                    placable = true;
                }
                let l = tat(pos.x - 1, pos.y);
                if tile_is_solid(l) && (solid || !tile_contains_door(l)) {
                    placable = true;
                }
            }
            if !placable && item_type_is_placable_on_floor(p) && tile_is_solid(tat(pos.x, pos.y - 1)) {
                placable = true;
            }
            if !item_type_occupies_background_contents(p)
                && item_type_is_placable_on_back_wall(p)
                && t.back_wall_type_index != 2
                && t.contents == 0
                && t.foreground_contents == 0
            {
                placable = true;
            }
            if placable {
                set_found(5);
                return true;
            }
        }
        // bushes / plants
        if tile_is_bush_or_scrub(Some(tref)) {
            if !found(7) {
                for fb in h.free_blocks_at_pos(pos).iter() {
                    let Some(f) = crate::physics_objects::free_block::as_free_block(fb) else { continue };
                    let ty = ItemType(f.item_type() as u32);
                    if food_type_for_item_type(ty, f.data_b() as i32) != FoodType::FOOD_NONE
                        && bh.can_pick_up_item_of_type_sub_items(ty, f.sub_items().as_deref()) == 1
                        && bh.count_of_inventory_items_of_type_include_actions(ty, false) < 99
                    {
                        set_found(7);
                        return true;
                    }
                }
            }
            if tile_is_plant(Some(tref)) {
                let plant = h.get_plant_at_pos(pos);
                if let Some(pl) = plant.as_ref().and_then(as_plant) {
                    if pl.is_flowering()
                        && bh.can_pick_up_item_of_type_sub_items(ItemType(pl.dropped_item_type() as u32), None) == 1
                    {
                        if !found(8) && bh.item_index_with_good_interaction_type_for_tile(&tref).x != -1 {
                            set_found(8);
                            return true;
                        }
                        if !found(12) {
                            set_found(12);
                            return true;
                        }
                    }
                }
            }
            if t.contents == 6
                && bh.can_pick_up_item_of_type_sub_items(ItemType(4), None) == 1
                && bh.count_of_inventory_items_of_type_include_actions(ItemType(4), false) < 196
            {
                if !found(13) && bh.item_index_with_good_interaction_type_for_tile(&tref).x != -1 {
                    set_found(13);
                    return true;
                }
                if !found(14) {
                    set_found(14);
                    return true;
                }
            }
        }
        // 4 GATHER_GEMS
        if !found(4)
            && t.back_wall_type_index == 1
            && matches!(t.foreground_contents, 0x39 | 0x37 | 0x35 | 0x33 | 0x3b | 0x4e)
        {
            let it = item_type_from_tile_is_foregorund(tref, pos, true, &world);
            if bh.can_pick_up_item_of_type_sub_items(it, None) == 1 {
                set_found(4);
                return true;
            }
        }
        // 10 PLANT_FOOD
        let sowable = self.st.borrow().sowable_item;
        if !found(10) && sowable.0 != 0 {
            let mut ok = false;
            if sowable.0 != 144
                && sowable.0 != 295
                && item_type_is_sowable_in_tile_type(sowable, TileType(t.type_index as u32))
            {
                let a = tat(pos.x, pos.y + 1);
                if let Some(a) = a {
                    if tile_is_air_or_snow(Some(a)) && a.get().back_wall_type_index == 2 {
                        let at = a.get();
                        let c = if sowable_item_type_occupies_foreground(sowable) {
                            at.foreground_contents
                        } else {
                            at.contents
                        };
                        if c == 0 {
                            ok = true;
                        }
                    }
                }
            }
            if ok {
                set_found(10);
                return true;
            }
        }
        // 11 EXPLORE_UNEXPLORED_AREAS
        if !found(11)
            && !ctl
            && !lit(&t)
            && tile_is_air_or_snow(Some(tref))
            && t.contents == 0
            && t.foreground_contents == 0
            && t.background_contents == 0
            && sun_or_art(&t, 0x40)
            && solid_neighbour(pos)
        {
            set_found(11);
            return true;
        }
        // 6 / 15 MINE_RESOURCE
        if !found(6) || !found(15) {
            let mut check = false;
            if matches!(t.type_index, 6 | 0x1b | 0x1c) || t.type_index == 1 {
                if t.contents != 0 {
                    let it = item_type_from_tile_is_foregorund(tref, pos, false, &world);
                    if bh.can_pick_up_item_of_type_sub_items(it, None) == 1 {
                        check = true;
                    }
                }
            }
            if check {
                let a = tat(pos.x, pos.y + 1);
                let mut air = tile_is_air_or_snow(a);
                if !air {
                    air = tile_is_air_or_snow(tat(pos.x - 1, pos.y));
                } else if let Some(a) = a {
                    let at = a.get();
                    if at.contents != 0 || at.foreground_contents != 0 {
                        return false;
                    }
                }
                if !air {
                    air = tile_is_air_or_snow(tat(pos.x + 1, pos.y));
                    if !air {
                        air = tile_is_air_or_snow(tat(pos.x, pos.y - 1));
                    }
                }
                if air {
                    if !found(6) && bh.item_index_with_good_interaction_type_for_tile(&tref).x != -1 {
                        set_found(6);
                        return true;
                    }
                    if !found(15) {
                        set_found(15);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// `-update:isSimulation:`.
    pub fn update_is_simulation(&self, dt: f32, is_simulation: bool) {
        let Some(world) = self.world.upgrade() else { return };
        let Some(bo) = self.bh_obj() else { return };
        let Some(bh) = as_blockhead(&bo) else { return };
        let h = host();
        let ah = actions_host();
        if is_simulation {
            let mut human_active = false;
            for other in ah.all_blockheads_including_net(bh) {
                if ptr_eq(&other, &bo) {
                    continue;
                }
                let Some(ob) = as_blockhead(&other) else { continue };
                if ob.is_run_by_ai() {
                    continue;
                }
                // `![[blockhead clientID] isEqualToString:[other clientID]]`: a nil receiver or argument compares NO,
                // so two nil ids are NOT equal.
                let same_client = match (bh.client_id(), ob.client_id()) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                };
                if world.server().is_some() && bh.is_client_blockhead_being_controlled_by_server() && !same_client {
                    continue;
                }
                if ob.asleep() {
                    human_active = true;
                }
                if ob.has_actions() && !bh.current_interaction_requires_human_input() {
                    human_active = true;
                }
            }
            if !human_active {
                return;
            }
        }
        {
            let mut s = self.st.borrow_mut();
            s.time_elapsed += dt;
            s.must_sleep_timer -= dt;
        }
        // `requiresMotionEvents` (rideObject==nil: needs jetpack free-flight && !dpad; then !isNet && no rod && !controlledByServer
        // && !mapVisible) can only be YES for a server blockhead; BlockheadAI is never built on the server, so NO is unobservable.
        if bh.has_actions() {
            let mut s = self.st.borrow_mut();
            if !s.last_frame_had_actions {
                s.inactivity_timer = 0.0;
                s.current_search_level = 0;
                s.current_search_level_count = 0;
                s.found_tile = [false; 16];
                s.last_frame_had_actions = true;
            }
            return;
        }
        let was_active = self.st.borrow().last_frame_had_actions;
        if was_active {
            let placable = bh.placable_light_for_ai_item_type();
            let sowable = bh.sowable_item_for_ai_item_type();
            let mut s = self.st.borrow_mut();
            s.placable_item = placable;
            s.sowable_item = sowable;
            s.last_frame_had_actions = false;
            if bh.drown_fraction() <= 0.1f32 {
                let level = rand() % 4;
                s.current_search_level = level;
                let div = std::cmp::max(level.wrapping_mul(level), 1);
                s.current_search_level_count = rand() % div;
                s.random_inactivity_timer = r01() * 16.0f32 + 2.0f32;
                s.only_move_this_time = 0.5f32 < r01();
            } else {
                s.current_search_level = 0;
                s.current_search_level_count = 0;
                s.random_inactivity_timer = r01() + 0.5f32;
                s.only_move_this_time = false;
            }
            let te = s.time_elapsed;
            s.move_direction = match &s.move_direction_noise_function {
                Some(n) => n.get_x_y_octaves(0.257f64 + (te as f64) * 1.0e-5f64, 0.3564f64, 4) as f32,
                None => 0.0,
            };
        }
        {
            self.st.borrow_mut().inactivity_timer += dt;
        }
        let found_idx = self.st.borrow().found_tile.iter().position(|f| *f).unwrap_or(16);
        let (inactivity, rand_timer) = {
            let s = self.st.borrow();
            (s.inactivity_timer, s.random_inactivity_timer)
        };
        if inactivity > rand_timer && (bh.fullness() < 0.5f32 || bh.health() < 1.0f32) && bh.drown_fraction() < 0.1f32 {
            let idx = bh.most_common_food_type_index();
            if idx.x != -1 {
                bh.set_current_item_to_item_at_index(idx);
                bh.use_current_item_if_possible();
                self.st.borrow_mut().inactivity_timer = 0.0;
                return;
            }
        }
        if inactivity < rand_timer && bh.drown_fraction() < 0.1f32 {
            return;
        }
        let level_now = self.st.borrow().current_search_level;
        if found_idx > 15 || (found_idx != 2 && level_now < 17) {
            // ---- search phase
            if level_now > 64 {
                let placable = bh.placable_light_for_ai_item_type();
                let sowable = bh.sowable_item_for_ai_item_type();
                let mut s = self.st.borrow_mut();
                s.placable_item = placable;
                s.sowable_item = sowable;
                s.last_frame_had_actions = false;
                s.current_search_level = 0;
                s.current_search_level_count = 0;
                s.random_inactivity_timer = r01() * 16.0f32 + 2.0f32;
                s.only_move_this_time = 0.5f32 < r01();
            }
            let p = bh.pos();
            let l = self.st.borrow().current_search_level;
            let (x_min, x_max, y_min, y_max) = (p.x - l, p.x + l, p.y - l, p.y + l);
            let limit = self.st.borrow().current_search_level_count + 4;
            let mut i = 0;
            for x in 0..=l {
                if self.st.borrow().current_search_level_count <= i {
                    self.test_tile_at_pos(IntPair { x: p.x + x, y: y_min });
                    self.test_tile_at_pos(IntPair { x: p.x + x, y: y_max });
                    if x != 0 {
                        self.test_tile_at_pos(IntPair { x: p.x - x, y: y_min });
                        self.test_tile_at_pos(IntPair { x: p.x - x, y: y_max });
                    }
                    let mut s = self.st.borrow_mut();
                    s.current_search_level_count += 1;
                    if s.current_search_level_count >= limit {
                        return;
                    }
                }
                i += 1;
            }
            for y in 0..l {
                if self.st.borrow().current_search_level_count <= i {
                    self.test_tile_at_pos(IntPair { x: x_min, y: p.y + y });
                    self.test_tile_at_pos(IntPair { x: x_max, y: p.y + y });
                    if y != 0 {
                        self.test_tile_at_pos(IntPair { x: x_min, y: p.y - y });
                        self.test_tile_at_pos(IntPair { x: x_max, y: p.y - y });
                    }
                    let mut s = self.st.borrow_mut();
                    s.current_search_level_count += 1;
                    if s.current_search_level_count >= limit {
                        return;
                    }
                }
                i += 1;
            }
            let mut s = self.st.borrow_mut();
            s.current_search_level_count = 0;
            s.current_search_level += 1;
            return;
        }
        // ---- act phase
        let fpos = self.st.borrow().found_tile_pos[found_idx];
        let Some(tile) = tile_at_world_position_loaded(fpos.x, fpos.y, &world) else { return };
        let mut bail = bh.asleep();
        match found_idx {
            6 | 8 | 13 => {
                let idx = bh.item_index_with_good_interaction_type_for_tile(&tile);
                if idx.x == -1 {
                    bail = true;
                } else {
                    bh.set_current_item_to_item_at_index(idx);
                }
            }
            3 => {
                let idx = bh.item_index_with_good_interaction_type_for_tile(&tile);
                bh.set_current_item_to_item_at_index(if idx.x == -1 { IntPair { x: 0, y: 0 } } else { idx });
                let g = self.st.borrow().goal_dig_tile;
                let t = tile_at_world_position_loaded(g.x, g.y - 1, &world);
                let digable = tile_is_solid(t)
                    && (tile_is_unplaced_block_minable_with_pickaxe(t)
                        || t.map_or(false, |t| matches!(t.get().type_index, 6 | 0x1b | 0x1c | 7 | 8 | 0x3a)));
                self.st.borrow_mut().goal_dig_tile =
                    if digable { IntPair { x: g.x, y: g.y - 1 } } else { IntPair { x: -1, y: -1 } };
            }
            5 => {
                let idx = bh.placable_light_for_ai_item_index();
                if idx.x == -1 {
                    bail = true;
                } else {
                    bh.set_current_item_to_item_at_index(idx);
                }
            }
            10 => {
                let idx = bh.sowable_item_for_ai_item_index();
                if idx.x == -1 {
                    bail = true;
                } else {
                    bh.set_current_item_to_item_at_index(idx);
                }
            }
            _ => {
                if found_idx == 1 && bh.can_sleep_on_spot() && self.st.borrow().must_sleep_timer < 0.0f32 {
                    bh.sleep_on_spot_if_possible_otherwise_cancel_actions();
                    self.st.borrow_mut().must_sleep_timer = 10.0f32;
                    bail = true;
                } else if !bail {
                    bh.set_current_item_to_item_at_index(IntPair { x: 0, y: 0 });
                }
            }
        }
        if bail {
            return;
        }
        world.queue_blockhead_ai_action_to_tile_at_pos_for_blockhead(&tile, fpos, &bo);
        let mut s = self.st.borrow_mut();
        if s.previous_action_count == 32 {
            s.previous_action_count = 0;
        }
        let c = s.previous_action_count as usize;
        s.previous_action_tiles[c] = fpos;
        s.previous_action_count += 1;
        s.inactivity_timer = 0.0;
        s.current_search_level = 0;
        s.current_search_level_count = 0;
        s.found_tile = [false; 16];
        let _ = h;
    }

    /// `-noPathToAction`: tries to set a new `goalDigTile` above the unreachable tile. With
    /// `previousActionCount == 0` the original reads `foundTile[8..16]` as two LE i32.
    pub fn no_path_to_action(&self) {
        let Some(world) = self.world.upgrade() else { return };
        let tat = |x: i32, y: i32| tile_at_world_position_loaded(x, y, &world);
        let a = {
            let s = self.st.borrow();
            if s.previous_action_count == 0 {
                let ft: Vec<u8> = s.found_tile[8..16].iter().map(|b| *b as u8).collect();
                IntPair {
                    x: i32::from_le_bytes([ft[0], ft[1], ft[2], ft[3]]),
                    y: i32::from_le_bytes([ft[4], ft[5], ft[6], ft[7]]),
                }
            } else {
                s.previous_action_tiles[(s.previous_action_count - 1) as usize]
            }
        };
        let goal = self.st.borrow().goal_dig_tile;
        if goal.x == -1 || (a.x.wrapping_sub(goal.x)).wrapping_abs() > 15 {
            let mut found_solid = false;
            for i in 0..16 {
                let t = tat(a.x, a.y + i + 1);
                if found_solid {
                    if tile_is_air_water_or_snow(t) {
                        let tt = t.unwrap().get();
                        if tt.contents == 0
                            && tt.foreground_contents == 0
                            && tt.background_contents == 0
                            && sun_or_art(&tt, 0x40)
                        {
                            let mut s = self.st.borrow_mut();
                            s.goal_dig_tile = IntPair { x: a.x, y: a.y + i };
                            let mut clash = false;
                            for j in 0..32 {
                                if s.previous_action_tiles[j] == s.goal_dig_tile {
                                    s.goal_dig_tile = IntPair { x: -1, y: -1 };
                                    clash = true;
                                }
                            }
                            if clash {
                                return;
                            }
                            for j in 0..32 {
                                if s.previous_action_tiles[j] == a {
                                    s.previous_action_tiles[j] = IntPair { x: -1, y: -1 };
                                }
                            }
                            return;
                        }
                    }
                } else if tile_is_solid(t) {
                    let tt = t.unwrap().get();
                    if !(tile_is_unplaced_block_minable_with_pickaxe(t)
                        || matches!(tt.type_index, 6 | 0x1b | 0x1c | 7 | 8 | 0x3a))
                    {
                        return;
                    }
                    found_solid = true;
                }
            }
        }
    }
}

#[allow(dead_code)]
type _Unused = (Option<TileRef>, Option<Blockhead>);

#[cfg(test)]
mod tests {
    use super::super::interaction_rules::testhost::*;
    use super::*;
    use crate::blockhead::blockhead::testutil::*;
    use crate::rng::glibc_rand::srand;

    fn mk() -> (Rc<World>, ObjRef, Rc<AHost>) {
        let h = AHost::install();
        let (w, o) = arena();
        light_all(&w);
        (w, o, h)
    }

    /// : `init` consumes exactly the five documented draws in order.
    #[test]
    fn init_rand_order() {
        let (w, o, _h) = mk();
        srand(7);
        let r: Vec<i32> = (0..7).map(|_| crate::rng::glibc_rand::rand()).collect();
        srand(7);
        let ai = BlockheadAI::init_with_blockhead_world(&o, &w);
        let next = crate::rng::glibc_rand::rand();
        assert_eq!(next, r[5], "exactly five draws");
        ai.with_state(|s| {
            let level = r[1] % 4;
            assert_eq!(s.current_search_level, level);
            assert_eq!(s.current_search_level_count, r[2] % std::cmp::max(level * level, 1));
            assert_eq!(s.random_inactivity_timer, (r[3] as f32 / 2147483648.0f32) * 16.0f32 + 2.0f32);
            assert_eq!(s.only_move_this_time, 0.5f32 < (r[4] as f32 / 2147483648.0f32));
            assert_eq!(s.goal_dig_tile, IntPair { x: -1, y: -1 });
            assert!(s.move_direction_noise_function.is_some());
            assert!(!s.last_frame_had_actions && s.previous_action_count == 0 && s.found_tile == [false; 16]);
            assert_eq!(s.time_elapsed, 0.0);
        });
        // the noise function is seeded with the first draw
        let n = NoiseFunction::init_with_frequency_x_frequency_y_frequency_z_amplitude_seed_tileable_loop_persistance(
            3, 3, 2, 0.9, r[0], true, false, 1.2,
        );
        ai.with_state(|s| assert_eq!(s.move_direction, n.get_x_y_octaves(0.257, 0.3564, 4) as f32));
        ai.dealloc();
        ai.with_state(|s| assert!(s.move_direction_noise_function.is_none()));
    }

    fn new_ai() -> (Rc<World>, ObjRef, Rc<AHost>, Rc<BlockheadAI>) {
        let (w, o, h) = mk();
        let ai = BlockheadAI::init_with_blockhead_world(&o, &w);
        ai.with_state(|s| {
            s.only_move_this_time = false;
            s.must_sleep_timer = 5.0;
            s.placable_item = ItemType(0);
            s.sowable_item = ItemType(0);
        });
        (w, o, h, ai)
    }

    #[test]
    fn test_tile_rules_in_order() {
        let (w, o, h, ai) = new_ai();
        let b = bh_of(&o);
        let pos = IntPair { x: 8, y: 8 };
        // completely dark tiles never match; so do tiles that are not lit for the Blockhead
        tile(&w, 8, 8).update(|t| {
            t.sun_light = 0;
        });
        assert!(!ai.test_tile_at_pos(pos));
        tile(&w, 8, 8).update(|t| {
            t.sun_light = 200;
            t.light = 0;
        });
        assert!(!ai.test_tile_at_pos(pos));
        tile(&w, 8, 8).update(|t| t.light = 1);
        assert!(!ai.test_tile_at_pos(pos));
        // MOVE: no foundTile[9] guard; direction sign must match the side of the tile
        ai.with_state(|s| {
            s.only_move_this_time = true;
            s.move_direction = -1.0;
        });
        assert!(!ai.test_tile_at_pos(pos));
        ai.with_state(|s| s.move_direction = 1.0);
        assert!(ai.test_tile_at_pos(pos));
        ai.with_state(|s| {
            assert!(s.found_tile[9] && s.found_tile_pos[9] == pos);
        });
        // found twice: the MOVE rule has no guard
        assert!(ai.test_tile_at_pos(pos));
        // recently used tiles are rejected before MOVE
        ai.with_state(|s| s.previous_action_tiles[31] = pos);
        assert!(!ai.test_tile_at_pos(pos));
        ai.with_state(|s| {
            s.previous_action_tiles = [IntPair { x: 0, y: 0 }; 32];
            s.found_tile = [false; 16];
            s.only_move_this_time = false;
        });
        // DIG_HOLE
        ai.with_state(|s| s.goal_dig_tile = pos);
        assert!(ai.test_tile_at_pos(pos));
        ai.with_state(|s| assert!(s.found_tile[3]));
        assert!(!ai.test_tile_at_pos(pos));
        ai.with_state(|s| s.goal_dig_tile = IntPair { x: -1, y: -1 });
        // GET_AIR takes precedence when the Blockhead is drowning
        b.bh.borrow_mut().state.drown_fraction = 0.5;
        assert!(ai.test_tile_at_pos(pos));
        ai.with_state(|s| assert!(s.found_tile[2] && s.found_tile_pos[2] == pos));
        // a recently used tile blocks GET_AIR
        ai.with_state(|s| {
            s.found_tile = [false; 16];
            s.previous_action_tiles[0] = pos;
        });
        assert!(!ai.test_tile_at_pos(pos));
        b.bh.borrow_mut().state.drown_fraction = 0.0;
        ai.with_state(|s| s.previous_action_tiles = [IntPair { x: 0, y: 0 }; 32]);
        // sleeping: low energy; a bed first, then the spot
        b.bh.borrow_mut().state.energy = 0.3;
        h.io_type.set(3);
        tile(&w, 9, 9).update(|t| t.contents = 0x30);
        assert!(ai.test_tile_at_pos(IntPair { x: 9, y: 9 }));
        ai.with_state(|s| assert!(s.found_tile[0]));
        assert!(ai.test_tile_at_pos(pos));
        ai.with_state(|s| assert!(s.found_tile[1] && s.found_tile_pos[1] == pos));
        // plant food: a sowable item and a matching tile
        b.bh.borrow_mut().state.energy = 1.0;
        ai.with_state(|s| s.found_tile = [false; 16]);
    }

    #[test]
    fn no_path_to_action_rows() {
        let (w, _o, _h, ai) = new_ai();
        // dig up from (10,3) through rock to the air at y = 8
        ai.with_state(|s| {
            s.previous_action_tiles[0] = IntPair { x: 10, y: 3 };
            s.previous_action_count = 1;
        });
        ai.no_path_to_action();
        ai.with_state(|s| {
            assert_eq!(s.goal_dig_tile, IntPair { x: 10, y: 7 });
            assert_eq!(s.previous_action_tiles[0], IntPair { x: -1, y: -1 });
        });
        // a goal within 15 columns is kept
        ai.with_state(|s| s.previous_action_tiles[0] = IntPair { x: 20, y: 3 });
        ai.no_path_to_action();
        ai.with_state(|s| assert_eq!(s.goal_dig_tile, IntPair { x: 10, y: 7 }));
        // clash with a recently used tile: goal reset, ring untouched
        ai.with_state(|s| {
            s.goal_dig_tile = IntPair { x: -1, y: -1 };
            s.previous_action_tiles[0] = IntPair { x: 10, y: 3 };
            s.previous_action_tiles[5] = IntPair { x: 10, y: 7 };
            s.previous_action_count = 6;
            s.previous_action_tiles[5] = IntPair { x: 10, y: 3 };
            s.previous_action_tiles[7] = IntPair { x: 10, y: 7 };
        });
        ai.no_path_to_action();
        ai.with_state(|s| {
            assert_eq!(s.goal_dig_tile, IntPair { x: -1, y: -1 });
            assert_eq!(s.previous_action_tiles[0], IntPair { x: 10, y: 3 });
        });
        // an unminable solid tile ends the search without a goal
        tile(&w, 30, 5).update(|t| t.type_index = 0x10);
        ai.with_state(|s| {
            s.previous_action_tiles = [IntPair { x: 0, y: 0 }; 32];
            s.previous_action_tiles[0] = IntPair { x: 30, y: 4 };
            s.previous_action_count = 1;
            s.goal_dig_tile = IntPair { x: -1, y: -1 };
        });
        ai.no_path_to_action();
        ai.with_state(|s| assert_eq!(s.goal_dig_tile, IntPair { x: -1, y: -1 }));
        // previousActionCount == 0 reads foundTile[8..16] as two LE i32: x = 1, y = 1
        ai.with_state(|s| {
            s.previous_action_count = 0;
            s.found_tile = [false; 16];
            s.found_tile[8] = true;
            s.found_tile[12] = true;
        });
        ai.no_path_to_action();
        ai.with_state(|s| assert_eq!(s.goal_dig_tile, IntPair { x: 1, y: 7 }));
    }

    #[test]
    fn update_transitions_and_search_ring() {
        let (w, o, _h, ai) = new_ai();
        let b = bh_of(&o);
        // simulation tick with nobody asleep / busy: nothing happens (not even the clock)
        ai.update_is_simulation(0.5, true);
        ai.with_state(|s| assert_eq!(s.time_elapsed, 0.0));
        // with actions: the state is reset once
        b.bh.borrow_mut().action_queue.push(Rc::new(()));
        ai.with_state(|s| {
            s.inactivity_timer = 3.0;
            s.current_search_level = 5;
            s.found_tile[4] = true;
        });
        ai.update_is_simulation(0.5, false);
        ai.with_state(|s| {
            assert_eq!(s.time_elapsed, 0.5);
            assert_eq!(s.must_sleep_timer, 4.5);
            assert!(s.last_frame_had_actions);
            assert_eq!((s.inactivity_timer, s.current_search_level, s.found_tile[4]), (0.0, 0, false));
        });
        // becoming idle: four draws (not drowning), the timer restarts, the clock accumulates
        b.bh.borrow_mut().action_queue.clear();
        srand(11);
        let r: Vec<i32> = (0..6).map(|_| crate::rng::glibc_rand::rand()).collect();
        srand(11);
        ai.update_is_simulation(0.25, false);
        assert_eq!(crate::rng::glibc_rand::rand(), r[4]);
        ai.with_state(|s| {
            assert!(!s.last_frame_had_actions);
            let level = r[0] % 4;
            assert_eq!(s.current_search_level, level);
            assert_eq!(s.current_search_level_count, r[1] % std::cmp::max(level * level, 1));
            assert_eq!(s.random_inactivity_timer, (r[2] as f32 / 2147483648.0f32) * 16.0f32 + 2.0f32);
            assert_eq!(s.only_move_this_time, 0.5f32 < (r[3] as f32 / 2147483648.0f32));
            assert_eq!(s.inactivity_timer, 0.25);
        });
        // drowning: a single draw R + 0.5
        ai.with_state(|s| s.last_frame_had_actions = true);
        b.bh.borrow_mut().state.drown_fraction = 0.5;
        srand(11);
        ai.update_is_simulation(0.25, false);
        assert_eq!(crate::rng::glibc_rand::rand(), r[1]);
        ai.with_state(|s| {
            assert_eq!(s.random_inactivity_timer, (r[0] as f32 / 2147483648.0f32) + 0.5f32);
            assert!(!s.only_move_this_time);
            // (the drowning Blockhead does not wait: the same call already searched the level-0 ring)
            assert_eq!((s.current_search_level, s.current_search_level_count), (1, 0));
        });
        b.bh.borrow_mut().state.drown_fraction = 0.0;
        // still waiting: nothing is tested
        ai.with_state(|s| {
            s.found_tile = [false; 16];
            s.inactivity_timer = 0.0;
            s.random_inactivity_timer = 100.0;
        });
        ai.update_is_simulation(0.25, false);
        ai.with_state(|s| assert_eq!(s.found_tile, [false; 16]));
        // search ring of level 1 around (5,8): finishes in one call and finds the MOVE tile (6,8)
        ai.with_state(|s| {
            s.inactivity_timer = 100.0;
            s.random_inactivity_timer = 1.0;
            s.current_search_level = 1;
            s.current_search_level_count = 0;
            s.only_move_this_time = true;
            s.move_direction = 1.0;
        });
        ai.update_is_simulation(0.25, false);
        ai.with_state(|s| {
            assert!(s.found_tile[9]);
            assert_eq!((s.current_search_level, s.current_search_level_count), (2, 0));
        });
        // a partially searched ring stops after `count + 4` steps
        ai.with_state(|s| {
            s.found_tile = [false; 16];
            s.current_search_level = 20;
            s.current_search_level_count = 0;
            s.only_move_this_time = false;
        });
        ai.update_is_simulation(0.25, false);
        ai.with_state(|s| assert_eq!((s.current_search_level, s.current_search_level_count), (20, 4)));
        let _ = w;
        // levels above 64 restart the search
        ai.with_state(|s| {
            s.current_search_level = 65;
            s.current_search_level_count = 7;
        });
        ai.update_is_simulation(0.25, false);
        ai.with_state(|s| assert!(s.current_search_level < 3 && s.current_search_level_count <= 4 + 1));
    }

    #[test]
    fn act_phase_records_the_tile() {
        let (_w, o, _h, ai) = new_ai();
        let b = bh_of(&o);
        let _ = b;
        let pos = IntPair { x: 6, y: 8 };
        ai.with_state(|s| {
            s.found_tile[9] = true;
            s.found_tile_pos[9] = pos;
            s.current_search_level = 17;
            s.inactivity_timer = 100.0;
            s.random_inactivity_timer = 1.0;
            s.previous_action_count = 32;
        });
        ai.update_is_simulation(0.1, false);
        ai.with_state(|s| {
            assert_eq!(s.previous_action_count, 1, "the ring wraps at 32");
            assert_eq!(s.previous_action_tiles[0], pos);
            assert_eq!(s.found_tile, [false; 16]);
            assert_eq!((s.current_search_level, s.current_search_level_count), (0, 0));
            assert_eq!(s.inactivity_timer, 0.0);
        });
        // a sleeping Blockhead bails out before queueing
        ai.with_state(|s| {
            s.found_tile[9] = true;
            s.found_tile_pos[9] = pos;
            s.current_search_level = 17;
            s.inactivity_timer = 100.0;
        });
        bh_of(&o).bh.borrow_mut().state.sub_animation_type = 5;
        ai.update_is_simulation(0.1, false);
        ai.with_state(|s| assert!(s.found_tile[9] && s.previous_action_count == 1));
    }
}
