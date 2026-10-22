//! `world_ops` crafting: `craftItemFinished`, `craftAborted`, `warpInBlockhead`,
//! `blockheadReachedInteractionObjectDestination`.
//!
//!  `reportAchievementWithIdentifier:` is an empty
//! method on the server (R9): the achievement calls are not made.

use super::misc::{deps, zero_craftable_item};
use super::state::World;
use crate::foundation::md5::string_from_md5;
use crate::objects::handle::ObjRef;
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand::r01_tagged;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;

/// `PathExtraData` (the `extraData` NSDictionary of a path).
pub type PathExtraData = PlistValue;

/// `ITEM_NONE`-safe item id constants used by the leftover rules.
const CONTAINER_RETURNS_BUCKET: [ItemType; 8] = [
    ItemType::ITEM_COFFEE_CUP,
    ItemType::ITEM_FLATBREAD,
    ItemType::ITEM_YEAST,
    ItemType::ITEM_DOUGH,
    ItemType::ITEM_CLAY,
    ItemType::ITEM_MOZZARELLA,
    ItemType::ITEM_RAINBOW_CAKE,
    ItemType::ITEM_PLASTER,
];

/// The ore ladder of `ITEM_RANDOM_SLUICED_ORE` for a roll `r` in `[0, 1]`: thresholds are built in f64 and
/// rounded to f32 at each step; comparisons are `<=` in f32.
pub fn sluiced_ore_for_roll(r: f32) -> ItemType {
    let t1 = 0.25f32;
    let t2 = ((t1 as f64) + 0.25f64) as f32;
    let t3 = ((t2 as f64) + 0.2f64) as f32;
    let t4 = ((t3 as f64) + 0.2f64) as f32;
    let t5 = ((t4 as f64) + 0.06f64) as f32;
    let t6 = ((t5 as f64) + 0.02f64) as f32;
    if r <= t1 {
        ItemType::ITEM_COPPER_ORE
    } else if r <= t2 {
        ItemType::ITEM_TIN_ORE
    } else if r <= t3 {
        ItemType::ITEM_IRON_ORE
    } else if r <= t4 {
        ItemType::ITEM_COAL
    } else if r <= t5 {
        ItemType::ITEM_GOLD_ORE
    } else if r <= t6 {
        ItemType::ITEM_TITANIUM_ORE
    } else {
        ItemType::ITEM_PLATINUM_ORE
    }
}

impl World {
    /// `-[World craftItemFinished:atWorkbench:allFinished:blockhead:]`. Called by Workbench when a craft
    /// timer completes. `crafted` is a CraftableItemObject.
    pub fn craft_item_finished_at_workbench_all_finished_blockhead(
        &self,
        crafted: Option<&ObjRef>,
        workbench: Option<&ObjRef>,
        _all_finished: bool,
        bh: Option<&ObjRef>,
    ) {
        let d = deps();
        let ci = match crafted {
            Some(c) => d.craftable_item(c),
            None => zero_craftable_item(),
        };
        let wb_pos: Option<IntPair> = workbench.map(|w| d.workbench_pos(w));
        let create_position = match wb_pos {
            Some(p) => IntPair { x: p.x, y: p.y + 1 },
            None => IntPair { x: 0, y: 0 },
        };
        let crafted_type = ItemType(ci.crafted_item_type);
        if crafted_type == ItemType::ITEM_WORKBENCH_UPGRADE {
            if let Some(w) = workbench {
                d.workbench_upgrade_to_next_level(w);
            }
        } else if crafted_type == ItemType::ITEM_BLOCKHEAD {
            // server branch; the achievement calls are empty methods (R9)
            if let Some(c) = crafted {
                d.load_new_blockhead_at_pos_craftable_item_object_unique_id(
                    wb_pos.unwrap_or(IntPair { x: 0, y: 0 }),
                    c,
                    0xFFFF_FFFF_FFFF_FFFFu64,
                );
            }
        } else {
            // isSimulating is never true on the server
            for _ in 0..ci.crafted_item_count.max(0) {
                let mut type_to_create = ItemType(ci.crafted_item_type);
                if type_to_create == ItemType::ITEM_RANDOM_SLUICED_ORE {
                    // one rand() per produced item, drawn before addExpectedCraftItem and createFreeBlock
                    let r = r01_tagged("craft.sluice");
                    type_to_create = sluiced_ore_for_roll(r);
                }
                if let Some(b) = bh {
                    d.bh_add_expected_craft_item(b, type_to_create);
                }
                let save_dict = crafted.and_then(|c| d.free_block_creation_item_save_dict(c));
                d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    create_position,
                    type_to_create,
                    ci.crafted_item_data_a,
                    ci.crafted_item_data_b,
                    None,
                    save_dict.as_ref(),
                    false,
                    3,
                    bh,
                );
            }
            // container leftovers, spawned once after the loop
            let t = crafted_type;
            let wb_type = workbench.map(|w| d.workbench_type(w).0).unwrap_or(0);
            let bucket = |d: &dyn super::misc::WorldOpsDeps| {
                d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    create_position,
                    ItemType::ITEM_TIN_BUCKET,
                    0,
                    0,
                    None,
                    None,
                    false,
                    0,
                    bh,
                );
            };
            if CONTAINER_RETURNS_BUCKET.contains(&t) {
                bucket(&*d);
            } else if t == ItemType::ITEM_SALT {
                bucket(&*d);
                if wb_type == 0x13 {
                    d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        create_position,
                        ItemType::ITEM_POT,
                        0,
                        0,
                        None,
                        None,
                        false,
                        0,
                        bh,
                    );
                }
            } else if wb_type == 0xc && crate::items::predicates::item_type_can_be_colored(t) {
                bucket(&*d);
            }
        }
    }

    /// `-[World craftAbortedForWorkbench:withBlockhead:]`: empty body.
    pub fn craft_aborted_for_workbench_with_blockhead(&self, _workbench: Option<&ObjRef>, _bh: Option<&ObjRef>) {}

    /// `-[World warpInBlockhead:atWorkbench:withBlockhead:]` (host branch). No caller on the dedicated server (UI
    /// path); ported for completeness.
    /// `uniqueIDReturnedFromServer:`.
    pub fn warp_in_blockhead_at_workbench_with_blockhead(&self, obj: &ObjRef, workbench: &ObjRef, bh: Option<&ObjRef>) {
        let d = deps();
        let wb_pos = d.workbench_pos(workbench);
        match bh {
            None => {
                d.load_new_blockhead_at_pos_craftable_item_object_unique_id(wb_pos, obj, 0xFFFF_FFFF_FFFF_FFFFu64);
            }
            Some(b) => {
                let ci = d.craftable_item(obj);
                let bpos = b.base().borrow().pos;
                let refund_pos = IntPair { x: bpos.x, y: bpos.y + 1 };
                let n = ci.number_of_required_items.clamp(0, 8) as usize;
                let spawn = |t: ItemType, count: i32| {
                    for _ in 0..count.max(0) {
                        d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                            refund_pos, t, 0, 0, None, None, false, 2, Some(b),
                        );
                    }
                };
                for i in 0..n {
                    let req = ItemType(ci.required_items[i]);
                    let cnt = ci.required_item_counts[i];
                    if req == ItemType::ITEM_TIME_CRYSTAL {
                        let amount = d.crystal_manager_amount();
                        if amount < cnt {
                            return;
                        }
                        d.crystal_manager_set_count_watcher_none();
                        d.crystal_manager_modify_modify_string(
                            cnt,
                            &string_from_md5(&format!(
                                "7acfe93afc08c{}65ae2c54ecaf07f",
                                amount.wrapping_sub(cnt).wrapping_add(0x49)
                            )),
                        );
                    } else {
                        let removed = d.bh_subtract_items_from_inventory_of_type_count_data_b(
                            b,
                            req,
                            cnt,
                            ci.required_item_data_bs[i],
                        );
                        let short = cnt - removed;
                        if short > 0 {
                            for j in 0..i {
                                let rj = ItemType(ci.required_items[j]);
                                if rj != ItemType::ITEM_TIME_CRYSTAL {
                                    spawn(rj, ci.required_item_counts[j]);
                                }
                            }
                            spawn(req, cnt - short);
                            for j in 0..i {
                                let rj = ItemType(ci.required_items[j]);
                                let cj = ci.required_item_counts[j];
                                if rj == ItemType::ITEM_TIME_CRYSTAL && cj < 0xc351 {
                                    d.crystal_manager_set_count_watcher_none();
                                    let amount = d.crystal_manager_amount();
                                    d.crystal_manager_modify_modify_string(
                                        -cj,
                                        &string_from_md5(&format!(
                                            "7acfe93afc08c{}65ae2c54ecaf07f",
                                            amount.wrapping_add(cj).wrapping_add(0x49)
                                        )),
                                    );
                                }
                            }
                            return;
                        }
                    }
                }
                d.load_new_blockhead_at_pos_craftable_item_object_unique_id(wb_pos, obj, 0xFFFF_FFFF_FFFF_FFFFu64);
            }
        }
    }

    /// `-[World blockheadReachedInteractionObjectDestination:pathExtraData:]`: start the interaction if the object
    /// can still be used, else stop interacting. `path_extra_data["interactionObject"]` is an object reference
    /// that a plist cannot hold, so the caller passes it through [`InteractionDestination`].
    pub fn blockhead_reached_interaction_object_destination_path_extra_data(
        &self,
        bh: &ObjRef,
        path_extra_data: Option<&InteractionDestination>,
    ) {
        let d = deps();
        let io = path_extra_data.and_then(|p| p.interaction_object.as_ref());
        if let Some(io) = io {
            if !d.obj_needs_removed(io)
                && d.io_can_be_used_by_blockhead(io, bh)
                && d.bh_can_use_dynamic_object(bh, io)
                && !d.io_is_in_use(io)
            {
                d.io_start_interaction_with_blockhead(io, bh);
                return;
            }
        }
        d.bh_stop_interacting(bh);
    }
}

/// The part of a path's `extraData` dictionary that `blockheadReachedInteractionObjectDestination:` reads
/// (`extraData[@"interactionObject"]`). Note: pathfinding models `extraData`
/// as a plist value; an object reference is carried beside it.
#[derive(Clone, Default)]
pub struct InteractionDestination {
    /// `extraData[@"interactionObject"]`
    pub interaction_object: Option<ObjRef>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::helpers::testutil::new_obj;
    use crate::world::misc::{install_deps, testkit, NullDeps, WorldOpsDeps};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// T9: sluice ladder boundaries.
    #[test]
    fn sluice_ladder() {
        let two31 = 2147483648.0f32;
        assert_eq!(sluiced_ore_for_roll(0.0), ItemType(31));
        assert_eq!(sluiced_ore_for_roll(0.25 * two31 / two31), ItemType(31)); // boundary `<=`
        assert_eq!(sluiced_ore_for_roll(0.26), ItemType(36));
        assert_eq!(sluiced_ore_for_roll(0.5), ItemType(36));
        assert_eq!(sluiced_ore_for_roll(0.7), ItemType(32));
        assert_eq!(sluiced_ore_for_roll(0.9), ItemType(51));
        assert_eq!(sluiced_ore_for_roll(0.96), ItemType(73));
        assert_eq!(sluiced_ore_for_roll(0.979), ItemType(279));
        assert_eq!(sluiced_ore_for_roll(0.981), ItemType(261));
        assert_eq!(sluiced_ore_for_roll(1.0), ItemType(261));
    }

    struct CraftDeps {
        rec: Rc<testkit::Rec>,
        item: RefCell<crate::types::structs::CraftableItem>,
        wb_type: u32,
    }
    impl crate::world::misc::WorldOpsDepsStub for CraftDeps {
        fn craftable_item(&self, _o: &ObjRef) -> crate::types::structs::CraftableItem {
            *self.item.borrow()
        }
        fn workbench_pos(&self, _w: &ObjRef) -> IntPair {
            IntPair { x: 10, y: 20 }
        }
        fn workbench_type(&self, _w: &ObjRef) -> crate::types::enums::WorkbenchType {
            crate::types::enums::WorkbenchType(self.wb_type)
        }
        fn bh_add_expected_craft_item(&self, _bh: &ObjRef, t: ItemType) {
            self.rec.push(format!("expect({})", t.0));
        }
        fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            &self,
            pos: IntPair,
            t: ItemType,
            a: u16,
            b: u16,
            s: Option<&[crate::items::inventory_item::SubSlot]>,
            sd: Option<&PlistValue>,
            h: bool,
            snd: i32,
            bh: Option<&ObjRef>,
        ) {
            crate::world::misc::WorldOpsDeps::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&*self.rec,
                pos, t, a, b, s, sd, h, snd, bh,
            );
        }
    }

    fn item(t: u32, count: i32) -> crate::types::structs::CraftableItem {
        let mut c = zero_craftable_item();
        c.crafted_item_type = t;
        c.crafted_item_count = count;
        c
    }

    /// T9 / W12: free blocks one tile above the bench, expected-craft before create, leftovers.
    #[test]
    fn craft_item_finished_arms() {
        let rec = Rc::new(testkit::Rec::default());
        let w = World::alloc(32);
        let wb = new_obj();
        let co = new_obj();
        let bh = new_obj();
        // loaf: flatbread x1 returns the tin bucket
        install_deps(Rc::new(CraftDeps { rec: rec.clone(), item: RefCell::new(item(318, 1)), wb_type: 3 }));
        w.craft_item_finished_at_workbench_all_finished_blockhead(Some(&co), Some(&wb), true, Some(&bh));
        assert_eq!(
            rec.take(),
            vec![
                "expect(318)",
                "freeBlock(10,21) type=318 a=0 b=0 hovers=false snd=3 bh=true",
                "freeBlock(10,21) type=102 a=0 b=0 hovers=false snd=0 bh=true"
            ]
        );
        // salt on an electric stove returns bucket and pot
        install_deps(Rc::new(CraftDeps { rec: rec.clone(), item: RefCell::new(item(313, 2)), wb_type: 0x13 }));
        w.craft_item_finished_at_workbench_all_finished_blockhead(Some(&co), Some(&wb), true, Some(&bh));
        let log = rec.take();
        assert_eq!(log.len(), 2 * 2 + 2);
        assert!(log[4].contains("type=102") && log[5].contains("type=180"));
        // upgrade / blockhead arms spawn nothing
        install_deps(Rc::new(CraftDeps { rec: rec.clone(), item: RefCell::new(item(132, 1)), wb_type: 0x13 }));
        w.craft_item_finished_at_workbench_all_finished_blockhead(Some(&co), Some(&wb), true, Some(&bh));
        assert!(rec.take().is_empty());
        // nil crafted object: type 0, 0 items; dye bench with itemTypeCanBeColored(0) == false
        install_deps(Rc::new(CraftDeps { rec: rec.clone(), item: RefCell::new(item(0, 0)), wb_type: 0xc }));
        w.craft_item_finished_at_workbench_all_finished_blockhead(None, Some(&wb), true, None);
        assert!(rec.take().is_empty());
        install_deps(Rc::new(NullDeps));
    }

    /// T9: exactly one rand() per sluice output, ore chosen before addExpectedCraftItem.
    #[test]
    fn sluice_consumes_one_rand_per_item() {
        let rec = Rc::new(testkit::Rec::default());
        let w = World::alloc(32);
        let wb = new_obj();
        let co = new_obj();
        let bh = new_obj();
        install_deps(Rc::new(CraftDeps { rec: rec.clone(), item: RefCell::new(item(299, 3)), wb_type: 29 }));
        crate::rng::glibc_rand::srand(7);
        let expect: Vec<ItemType> = (0..3).map(|_| sluiced_ore_for_roll(crate::rng::glibc_rand::r01())).collect();
        crate::rng::glibc_rand::srand(7);
        w.craft_item_finished_at_workbench_all_finished_blockhead(Some(&co), Some(&wb), true, Some(&bh));
        let log = rec.take();
        for (i, t) in expect.iter().enumerate() {
            assert_eq!(log[i * 2], format!("expect({})", t.0));
            assert!(log[i * 2 + 1].contains(&format!("type={}", t.0)));
        }
        assert_eq!(log.len(), 6);
        // the RNG advanced exactly three times
        let next = crate::rng::glibc_rand::rand();
        crate::rng::glibc_rand::srand(7);
        for _ in 0..3 {
            crate::rng::glibc_rand::rand();
        }
        assert_eq!(next, crate::rng::glibc_rand::rand());
        install_deps(Rc::new(NullDeps));
    }
}
