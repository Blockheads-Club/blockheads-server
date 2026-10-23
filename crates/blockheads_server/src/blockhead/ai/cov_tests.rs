//! Coverage tests for `ai.rs`: a deterministic fuzz of the tile tester and scripted runs of the update loop.

use super::super::blockhead::testutil::{bh_of, tile};
use super::super::interaction_rules::testhost::real_dw;
use super::super::interaction_rules::testhost::*;
use super::*;
use crate::rng::glibc_rand::srand;
use std::rc::Rc;

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

fn mk() -> (Rc<World>, ObjRef, Rc<AHost>, Rc<BlockheadAI>) {
    let h = AHost::install();
    let (w, o) = arena();
    light_all(&w);
    let ai = BlockheadAI::init_with_blockhead_world(&o, &w);
    ai.with_state(|s| {
        s.only_move_this_time = false;
        s.must_sleep_timer = 5.0;
        s.placable_item = ItemType(0);
        s.sowable_item = ItemType(0);
    });
    (w, o, h, ai)
}

fn set(w: &World, x: i32, y: i32, f: impl FnOnce(&mut Tile)) {
    tile(w, x, y).update(f);
}

const TYPES: [u8; 14] = [2, 2, 2, 2, 5, 3, 1, 6, 0x1b, 7, 0x3a, 0x10, 0x31, 4];
const CONTENTS: [u8; 10] = [0, 0, 0, 0, 0x03, 0x06, 0x30, 0x2b, 0x67, 0x60];
const FGS: [u8; 12] = [0, 0, 0, 0, 0x43, 0x39, 0x37, 0x35, 0x33, 0x3b, 0x4e, 0x42];

fn randomize(rng: &mut Rng, w: &World, cx: i32, cy: i32) {
    for dx in -2..=2 {
        for dy in -2..=2 {
            let (x, y) = (cx + dx, cy + dy);
            let t = rng.pick(&TYPES);
            let c = if rng.chance(25) { rng.pick(&CONTENTS) } else { 0 };
            let fg = if rng.chance(25) { rng.pick(&FGS) } else { 0 };
            let bg = if rng.chance(8) { 0x46 } else { 0 };
            let bw = rng.pick(&[0u8, 1, 2, 2]);
            let sun = rng.pick(&[0u8, 0x20, 0x50, 200]);
            let art = if rng.chance(20) { rng.pick(&[0x10u16, 0x50]) } else { 0 };
            let light = rng.chance(85) as u8;
            let explored = rng.chance(50) as u8;
            set(w, x, y, |tl| {
                tl.type_index = t;
                tl.contents = c;
                tl.foreground_contents = fg;
                tl.background_contents = bg;
                tl.back_wall_type_index = bw;
                tl.sun_light = sun;
                tl.artificial_light_r = art;
                tl.artificial_light_g = art;
                tl.artificial_light_b = art;
                tl.light = light;
                tl.explored_fraction = explored;
                tl.partial_content_left = if t == 3 { rng.pick(&[0x40u8, 0xff]) } else { 0 };
            });
        }
    }
}

fn fuzz_tile_tester(seed: u64, iterations: u32) {
    let (w, o, h, ai) = mk();
    let b = bh_of(&o);
    let mut rng = Rng(seed);
    let center = IntPair { x: 10, y: 12 };
    let mut hits = 0u32;
    let placables = [0u32, 0x11, 0x2f, 0xb7, 0x9d, 0x4b, 200, 53, 178, 0x45];
    let sowables = [0u32, 21, 144, 295, 0x90, 0x127, 0x12c, 1, 3];
    for i in 0..iterations {
        randomize(&mut rng, &w, center.x, center.y);
        w.set_world_time(rng.below(2_000_000) as f64);
        w.set_is_admin(true);
        let pos = IntPair { x: center.x + rng.below(5) as i32 - 2, y: center.y + rng.below(5) as i32 - 2 };
        ai.with_state(|s| {
            s.found_tile = [false; 16];
            for k in 0..16 {
                s.found_tile[k] = rng.chance(15);
            }
            s.goal_dig_tile = if rng.chance(15) { pos } else { IntPair { x: -1, y: -1 } };
            s.previous_action_tiles = [IntPair { x: 0, y: 0 }; 32];
            if rng.chance(15) {
                s.previous_action_tiles[3] = pos;
            }
            s.must_sleep_timer = if rng.chance(50) { -1.0 } else { 5.0 };
            s.only_move_this_time = rng.chance(40);
            s.move_direction = if rng.chance(50) { 1.0 } else { -1.0 };
            s.placable_item = ItemType(rng.pick(&placables));
            s.sowable_item = ItemType(rng.pick(&sowables));
        });
        {
            let mut f = b.bh.borrow_mut();
            f.state.energy = rng.pick(&[0.1f32, 0.3, 0.45, 0.9, 1.2]);
            f.state.drown_fraction = if rng.chance(25) { 0.5 } else { 0.0 };
            f.state.sub_animation_type = if rng.chance(10) { 6 } else { 0 };
            f.is_client_blockhead_being_controlled_by_server = rng.chance(15);
        }
        h.io_type.set(rng.pick(&[0, 3]));
        give_random(b, &mut rng);
        if rng.chance(20) {
            let f = b.bh.borrow();
            for i in 1..8 {
                f.inventory_items[i].borrow_mut().clear();
                f.inventory_items[i].borrow_mut().push(crate::items::inventory_item::InventoryItem::new(
                    ItemType(8),
                    0,
                    0,
                    None,
                    None,
                ));
            }
        }
        if ai.test_tile_at_pos(pos) {
            hits += 1;
        }
        let _ = i;
    }
    assert!(hits > 100, "{hits}");
}

fn give_random(b: &Blockhead, rng: &mut Rng) {
    use crate::items::inventory_item::InventoryItem;
    let ty = rng.pick(&[8u32, 25, 0x19, 0x31, 0x11, 0x2f, 13, 21, 0x69, 0x90, 0x127, 200, 53]);
    let f = b.bh.borrow();
    for i in 1..8 {
        f.inventory_items[i].borrow_mut().clear();
    }
    f.inventory_items[1].borrow_mut().push(InventoryItem::new(ItemType(ty), 0, 0, None, None));
}

#[test]
fn fuzz_tile_tester_a() {
    fuzz_tile_tester(0xa11c_e5ee_d001, 40_000);
}
#[test]
fn fuzz_tile_tester_b() {
    fuzz_tile_tester(0x0b0b_cafe_f00d, 40_000);
}

fn quiet(ai: &BlockheadAI) {
    ai.with_state(|s| {
        s.found_tile = [false; 16];
        s.inactivity_timer = 100.0;
        s.random_inactivity_timer = 1.0;
        s.current_search_level = 0;
        s.current_search_level_count = 0;
        s.last_frame_had_actions = false;
    });
}

#[test]
fn simulation_waits_for_a_human() {
    let (w, o, h, ai) = mk();
    let b = bh_of(&o);
    // nobody around: nothing happens
    ai.with_state(|s| s.time_elapsed = 0.0);
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert_eq!(s.time_elapsed, 0.0));
    // other blockheads of every kind
    let (w2, o2) = arena();
    let _ = w2;
    let human = o2;
    let ai_run = stub_blockhead(&w);
    *h.others.borrow_mut() = vec![o.clone(), ai_run.clone(), human.clone()];
    // an AI blockhead is ignored
    h.has_ai.set(true);
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert_eq!(s.time_elapsed, 0.0));
    h.has_ai.set(false);
    // a sleeping human wakes the world
    bh_of(&human).bh.borrow_mut().state.sub_animation_type = 5;
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert!(s.time_elapsed > 0.0));
    // a human with actions does too, unless the interaction needs a person
    bh_of(&human).bh.borrow_mut().state.sub_animation_type = 0;
    bh_of(&human).bh.borrow_mut().action_queue.push(Rc::new(()));
    ai.with_state(|s| s.time_elapsed = 0.0);
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert!(s.time_elapsed > 0.0));
    // a human of another client does not count for a controlled blockhead on a server
    *w.server.borrow_mut() = Some(Rc::new(crate::world::state::BHServer { api: None }));
    b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
    bh_of(&human).set_client_id("other");
    b.set_client_id("mine");
    ai.with_state(|s| s.time_elapsed = 0.0);
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert_eq!(s.time_elapsed, 0.0));
    // the same client counts
    bh_of(&human).set_client_id("mine");
    ai.update_is_simulation(0.1, true);
    ai.with_state(|s| assert!(s.time_elapsed > 0.0));
}

fn stub_blockhead(w: &Rc<World>) -> ObjRef {
    let (_w, _dw, dwref) = crate::blockhead::blockhead::testutil::setup();
    std::mem::forget((_dw, dwref.clone()));
    crate::blockhead::blockhead::testutil::new_bh(w, &dwref, 1)
}

#[test]
fn the_update_loop_without_a_noise_function_and_with_hunger() {
    let (_w, o, _h, ai) = mk();
    let b = bh_of(&o);
    // idle again: no noise function gives a direction of zero
    ai.dealloc();
    ai.with_state(|s| s.last_frame_had_actions = true);
    ai.update_is_simulation(0.1, false);
    ai.with_state(|s| assert_eq!(s.move_direction, 0.0));
    // hungry with nothing to eat
    quiet(&ai);
    b.bh.borrow_mut().state.fullness = 0.1;
    {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
        }
    }
    ai.update_is_simulation(0.1, false);
    // searching moves on
    ai.with_state(|s| assert!(s.current_search_level >= 1 || s.current_search_level_count > 0));
}

#[test]
fn search_rings_resume_and_wrap() {
    let (_w, o, _h, ai) = mk();
    let _ = &o;
    // a ring that is resumed in the middle of the vertical sides, including the mirrored y != 0 steps
    quiet(&ai);
    ai.with_state(|s| {
        s.current_search_level = 2;
        s.current_search_level_count = 2;
    });
    ai.update_is_simulation(0.1, false);
    ai.with_state(|s| assert_eq!((s.current_search_level, s.current_search_level_count), (3, 0)));
    // a ring that is left after the limit inside the vertical sides
    quiet(&ai);
    ai.with_state(|s| {
        s.current_search_level = 2;
        s.current_search_level_count = 0;
    });
    ai.update_is_simulation(0.1, false);
    ai.with_state(|s| assert_eq!(s.current_search_level_count, 4));
    quiet(&ai);
    ai.with_state(|s| {
        s.current_search_level = 6;
        s.current_search_level_count = 5;
    });
    ai.update_is_simulation(0.1, false);
    // a level that is too deep starts over
    quiet(&ai);
    ai.with_state(|s| s.current_search_level = 65);
    ai.update_is_simulation(0.1, false);
    ai.with_state(|s| assert!(s.current_search_level <= 3));
}

#[test]
fn acting_on_each_kind_of_found_tile() {
    use crate::items::inventory_item::InventoryItem;
    let (w, o, _h, ai) = mk();
    let b = bh_of(&o);
    let pos = IntPair { x: 6, y: 8 };
    let act = |k: usize| {
        b.bh.borrow_mut().action_queue.clear();
        quiet(&ai);
        ai.with_state(|s| {
            s.found_tile[k] = true;
            s.found_tile_pos[k] = pos;
            s.current_search_level = 17;
        });
        ai.update_is_simulation(0.1, false);
    };
    let give = |ty: u32| {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
        }
        f.inventory_items[1].borrow_mut().push(InventoryItem::new(ItemType(ty), 0, 0, None, None));
    };
    // with and without a suitable tool in the bag
    for ty in [25u32, 8] {
        give(ty);
        for k in [6usize, 8, 13] {
            act(k);
        }
    }
    {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
        }
    }
    for k in [6usize, 8, 13] {
        act(k);
    }
    // digging a hole: the goal moves down through diggable ground, else it is dropped
    set(&w, 6, 7, |t| t.type_index = 6);
    set(&w, 6, 6, |t| t.type_index = 0x10);
    ai.with_state(|s| s.goal_dig_tile = pos);
    act(3);
    ai.with_state(|s| s.goal_dig_tile = IntPair { x: 6, y: 8 });
    act(3);
    ai.with_state(|s| s.goal_dig_tile = IntPair { x: 6, y: 7 });
    act(3);
    // torches and seeds
    give(0x11);
    act(5);
    give(21);
    for _ in 0..12 {
        b.bh.borrow().inventory_items[1].borrow_mut().push(InventoryItem::new(ItemType(21), 0, 0, None, None));
    }
    act(10);
    give(8);
    act(5);
    act(10);
    // the other kinds
    for k in [0usize, 4, 7, 9, 11, 12, 14, 15] {
        act(k);
    }
    // sleeping on the spot only when allowed and due
    b.bh.borrow_mut().state.energy = 0.1;
    ai.with_state(|s| s.must_sleep_timer = -5.0);
    act(1);
    ai.with_state(|s| s.must_sleep_timer = 5.0);
    act(1);
    // a tile that cannot be loaded
    quiet(&ai);
    ai.with_state(|s| {
        s.found_tile[9] = true;
        s.found_tile_pos[9] = IntPair { x: 5000, y: 5 };
        s.current_search_level = 5;
    });
    ai.update_is_simulation(0.1, false);
}

#[test]
fn no_path_corners() {
    let (w, _o, _h, ai) = mk();
    // looking up through open air finds nothing to dig
    ai.with_state(|s| {
        s.previous_action_tiles[0] = IntPair { x: 10, y: 12 };
        s.previous_action_count = 1;
        s.goal_dig_tile = IntPair { x: -1, y: -1 };
    });
    ai.no_path_to_action();
    ai.with_state(|s| assert_eq!(s.goal_dig_tile, IntPair { x: -1, y: -1 }));
    // rock, then air that is not free (contents), then free air: the free one becomes the goal
    for y in 10..=11 {
        set(&w, 20, y, |t| {
            t.type_index = 6;
        });
    }
    set(&w, 20, 12, |t| {
        t.type_index = 2;
        t.contents = 0x30;
        t.sun_light = 200;
    });
    set(&w, 20, 13, |t| {
        t.type_index = 2;
        t.contents = 0;
        t.sun_light = 200;
    });
    ai.with_state(|s| {
        s.previous_action_tiles = [IntPair { x: 0, y: 0 }; 32];
        s.previous_action_tiles[0] = IntPair { x: 20, y: 9 };
        s.previous_action_count = 1;
    });
    ai.no_path_to_action();
    ai.with_state(|s| assert_eq!(s.goal_dig_tile, IntPair { x: 20, y: 12 }));
}

#[test]
fn flowering_plants_without_a_tool_and_the_other_corners() {
    use crate::plants::plant::{new_plant_for_type, PlantLike};
    let (w, o, h, ai) = mk();
    let b = bh_of(&o);
    let rdw = real_dw(&w, "ai_cov_plant");
    let pos = IntPair { x: 14, y: 12 };
    set(&w, 14, 12, |t| {
        t.type_index = 2;
        t.contents = 0;
        t.foreground_contents = 0x43;
        t.sun_light = 200;
    });
    let plant = new_plant_for_type(1, &w, &rdw, pos, None, 5, 5, None, None, true).unwrap();
    crate::plants::plant::as_plant(&plant).unwrap().set_flowering(&plant, true);
    *h.plant.borrow_mut() = Some(plant);
    // no tool: slot 12, then nothing more
    {
        let f = b.bh.borrow();
        for i in 1..8 {
            f.inventory_items[i].borrow_mut().clear();
        }
    }
    assert!(ai.test_tile_at_pos(pos));
    ai.with_state(|s| assert!(s.found_tile[12] && !s.found_tile[8]));
    assert!(!ai.test_tile_at_pos(pos));
    // both slots taken: nothing left to find on a flowering plant
    ai.with_state(|s| {
        s.found_tile[8] = true;
        s.found_tile[12] = true;
    });
    assert!(!ai.test_tile_at_pos(pos));
    ai.with_state(|s| s.found_tile = [false; 16]);
    // a plant that does not flower is left alone
    let still = h.plant.borrow().clone().unwrap();
    crate::plants::plant::as_plant(&still).unwrap().set_flowering(&still, false);
    assert!(!ai.test_tile_at_pos(pos));
    // soil at the very top of the loaded block with a sowable item, but no tile above it
    set(&w, 16, 31, |t| {
        t.type_index = 6;
        t.sun_light = 200;
    });
    ai.with_state(|s| s.sowable_item = ItemType(21));
    let _ = ai.test_tile_at_pos(IntPair { x: 16, y: 31 });
    ai.with_state(|s| s.sowable_item = ItemType(0));
    // every sowable item, with free and with occupied space above the soil
    for above_contents in [0u8, 0x30] {
        for item in 0..1200u32 {
            set(&w, 16, 12, |t| {
                t.type_index = 6;
                t.sun_light = 200;
            });
            set(&w, 16, 13, |t| {
                t.type_index = 2;
                t.back_wall_type_index = 2;
                t.contents = above_contents;
                t.foreground_contents = above_contents;
            });
            ai.with_state(|s| {
                s.found_tile = [false; 16];
                s.sowable_item = ItemType(item);
            });
            let _ = ai.test_tile_at_pos(IntPair { x: 16, y: 12 });
        }
    }
    // ore at the very top of the loaded block has no tile above
    ai.with_state(|s| {
        s.found_tile = [false; 16];
        s.sowable_item = ItemType(0);
    });
    set(&w, 10, 31, |t| {
        t.type_index = 1;
        t.contents = 0x3d;
        t.sun_light = 200;
    });
    set(&w, 9, 31, |t| {
        t.type_index = 2;
        t.sun_light = 200;
    });
    let _ = ai.test_tile_at_pos(IntPair { x: 10, y: 31 });
    // a search that skips ahead inside the vertical sides
    quiet(&ai);
    ai.with_state(|s| {
        s.current_search_level = 2;
        s.current_search_level_count = 4;
    });
    b.bh.borrow_mut().action_queue.clear();
    ai.update_is_simulation(0.1, false);
}
