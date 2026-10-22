//! Production `ActionsHost`, `InventoryHost`, `FishingHost`, `FireHost` and `HostileHost`: the blockhead / NPC
//! seams that reach the live `DynamicWorld` and the object classes. None of them was installed before, so the server
//! ran every one of these calls against the inert stand-in (no cave torches or glow blocks on walking, no
//! treasure, no fishing, no free-block pickups, ...).

use std::any::Any;
use std::rc::Rc;

use super::{bh_of, BlockheadGlue, RealDw};
use crate::blockhead::action::ActionBridge;
use crate::blockhead::blockhead::{ActionRef, Blockhead, FishingRodRef, SimulationEventExtra};
use crate::blockhead::inventory_model::{FreeBlockData, InventoryHostStub};
use crate::items::inventory_item::ItemRef;
use crate::objects::handle::{downcast, ObjRef};
use crate::physics_objects::free_block::FreeBlock;
use crate::plist::value::PlistValue;
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, Vector2};
use crate::world::state::TileRef;

impl BlockheadGlue {
    fn live_dw(&self) -> Option<Rc<RealDw>> {
        self.glue()?.dw()
    }
}

fn free_block_of(o: &ObjRef) -> Option<&FreeBlock> {
    downcast::<FreeBlock>(o)
}

fn workbench_of(o: &ObjRef) -> Option<&crate::crafting::workbench::Workbench> {
    downcast::<crate::crafting::workbench::Workbench>(o)
}

impl crate::blockhead::interaction_rules::ActionsHost for BlockheadGlue {
    fn free_blocks_exist_at_pos(&self, _b: &Blockhead, pos: IntPair) -> bool {
        self.live_dw().is_some_and(|d| d.free_blocks_exist_at_pos(pos))
    }
    fn blockhead_at_pos(&self, _b: &Blockhead, pos: IntPair) -> Option<ObjRef> {
        self.live_dw()?.blockhead_at_pos(pos)
    }
    fn check_for_train_car_under_tap(&self, _b: &Blockhead, tap: Vector2) -> Option<ObjRef> {
        self.live_dw()?.check_for_train_car_under_tap(tap)
    }
    fn interaction_object_type_for_object_at_pos(&self, _b: &Blockhead, pos: IntPair) -> i32 {
        self.live_dw().map_or(0, |d| d.interaction_object_type_for_object_at_pos(pos))
    }
    fn load_glow_block_if_needed_at_pos_tile(&self, _b: &Blockhead, pos: IntPair, tile: &TileRef) {
        if let Some(d) = self.live_dw() {
            d.load_glow_block_if_needed_at_pos_tile(pos, tile);
        }
    }
    fn add_torch_at_pos_of_type(&self, _b: &Blockhead, pos: IntPair, item_type: i32) {
        if let Some(d) = self.live_dw() {
            d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(pos, item_type, 0, 0, None, None);
        }
    }
    fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
        &self,
        _b: &Blockhead,
        tile: &TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
        if let Some(d) = self.live_dw() {
            d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                tile,
                pos,
                load_troll,
                load_treasure,
            );
        }
    }
    fn world_contents_changed_at_pos(&self, _b: &Blockhead, pos: IntPair) {
        if let Some(d) = self.live_dw() {
            d.world_contents_changed_at_pos(pos);
        }
    }
    fn remove_standard_object(&self, _b: &Blockhead, o: &ObjRef) {
        if let Some(d) = self.live_dw() {
            d.remove_standard_object(Some(o));
        }
    }
    fn all_blockheads_including_net(&self, _b: &Blockhead) -> Vec<ObjRef> {
        self.live_dw().map_or_else(Vec::new, |d| d.all_blockheads_including_net())
    }
    fn dynamic_world_is_client(&self) -> bool {
        self.live_dw().is_some_and(|d| d.is_client())
    }
}

impl crate::blockhead::inventory_model::InventoryHost for BlockheadGlue {
    fn free_block_data(&self, fb: &ObjRef) -> Option<FreeBlockData> {
        let f = free_block_of(fb)?;
        Some(FreeBlockData {
            item_type: ItemType(f.item_type() as u32),
            data_a: f.data_a(),
            data_b: f.data_b(),
            sub_items: f.sub_items(),
            dynamic_object_save_dict: f.dynamic_object_save_dict().map(PlistValue::Dict),
            hovers: f.hovers(),
            priority_blockhead: f.priority_blockhead(),
        })
    }
    fn free_block_priority_blockhead_cannot_pickup(&self, fb: &ObjRef) {
        if let Some(f) = free_block_of(fb) {
            f.priority_blockhead_cannot_pickup(fb);
        }
    }
    fn dynamic_world_is_client(&self) -> bool {
        self.live_dw().is_some_and(|d| d.is_client())
    }
    /// `extraData` is the free block: SE_GOT records `[freeBlock itemType]`.
    fn add_simulation_event_got(&self, b: &ObjRef, fb: &ObjRef) {
        let t = free_block_of(fb).map_or(0, |f| f.item_type());
        crate::blockhead::blockhead::host().add_simulation_event_of_type_for_blockhead_extra_data(
            2,
            b,
            SimulationEventExtra::ItemType(t),
        );
    }
    fn world_start_using_camera(&self) {
        // `-[World startUsingCamera]` is empty in the server build (CLIENT-ONLY): nothing to do.
    }
    fn harmable_dynamic_object_with_id(&self, id: u64) -> Option<ObjRef> {
        self.live_dw()?.harmable_dynamic_object_with_id(id)
    }
    fn boat_with_id(&self, id: u64) -> Option<ObjRef> {
        self.live_dw()?.boat_with_id(id)
    }
    fn train_car_with_id(&self, id: u64) -> Option<ObjRef> {
        self.live_dw()?.train_car_with_id(id)
    }
    fn dynamic_world_active_blockhead(&self) -> Option<ObjRef> {
        self.live_dw()?.active_blockhead()
    }
    fn can_fish(&self, b: &Blockhead) -> bool {
        b.can_fish()
    }
    fn fishing_rod_new(&self, b: &Blockhead) -> FishingRodRef {
        match self.glue().and_then(|g| g.world()) {
            Some(w) => {
                crate::physics_objects::fishing_rod::FishingRod::init_with_world_blockhead_cache(&w, &b.me(), None)
            }
            // no World: `[FishingRod alloc]` still yields an object; nothing can use it
            None => Rc::new(()),
        }
    }
    fn ride_object_set_target_velocity_zero(&self, o: &ObjRef) {
        if let Some(r) = o.as_ridable() {
            r.set_target_velocity(Vector2 { x: 0.0, y: 0.0 });
        }
    }
    fn has_blockhead_ai(&self, _b: &Blockhead) -> bool {
        // `blockheadAI` is only created when the (client-side) hat texture exists: always nil on the server.
        false
    }
    fn blockhead_ai_no_path_to_action(&self, _b: &Blockhead) {
        // `[nil noPathToAction]`: the server never has a BlockheadAI (see `has_blockhead_ai`).
    }
    fn npc_blockhead_is_coming_to_inspect(&self, npc: &ObjRef, b: &Blockhead) {
        if let Some(n) = crate::npc::npc::as_npc(npc) {
            n.blockhead_is_coming_to_inspect(&b.me());
        }
    }
    fn action_goal_interaction(&self, a: &ActionRef) -> i32 {
        InventoryHostStub::action_goal_interaction(&ActionBridge, a)
    }
    fn action_interaction_item(&self, a: &ActionRef) -> Option<ItemRef> {
        InventoryHostStub::action_interaction_item(&ActionBridge, a)
    }
    fn action_interaction_item_index(&self, a: &ActionRef) -> i32 {
        InventoryHostStub::action_interaction_item_index(&ActionBridge, a)
    }
    fn action_interaction_item_sub_index(&self, a: &ActionRef) -> i32 {
        InventoryHostStub::action_interaction_item_sub_index(&ActionBridge, a)
    }
    fn action_set_interaction_item(&self, a: &ActionRef, item: Option<&ItemRef>) {
        InventoryHostStub::action_set_interaction_item(&ActionBridge, a, item)
    }
    fn action_set_interaction_item_index(&self, a: &ActionRef, v: i32) {
        InventoryHostStub::action_set_interaction_item_index(&ActionBridge, a, v)
    }
    fn action_set_interaction_item_sub_index(&self, a: &ActionRef, v: i32) {
        InventoryHostStub::action_set_interaction_item_sub_index(&ActionBridge, a, v)
    }
    fn action_craft_count_or_extra_data(&self, a: &ActionRef) -> i32 {
        InventoryHostStub::action_craft_count_or_extra_data(&ActionBridge, a)
    }
    fn action_path_type(&self, a: &ActionRef) -> i32 {
        InventoryHostStub::action_path_type(&ActionBridge, a)
    }
    fn action_craftable_item_object(&self, a: &ActionRef) -> Option<Rc<dyn Any>> {
        InventoryHostStub::action_craftable_item_object(&ActionBridge, a)
    }
    fn workbench_currently_crafting_item_type(&self, wb: &ObjRef) -> i32 {
        workbench_of(wb).map_or(0, |w| w.currently_crafting_item_type().0 as i32)
    }
    fn workbench_total_items_left_to_craft(&self, wb: &ObjRef) -> i32 {
        workbench_of(wb).map_or(0, |w| w.total_items_left_to_craft())
    }
}

impl crate::physics_objects::fishing_rod::FishingHost for BlockheadGlue {
    fn blockhead_fishing_rod_cast(&self, blockhead: &ObjRef, vec: Vector2) {
        if let Some(b) = bh_of(blockhead) {
            b.fishing_rod_cast(vec);
        }
    }
    fn clown_fish_die(&self, fish: &ObjRef, killer: Option<&ObjRef>) {
        if let Some(n) = crate::npc::npc::as_npc(fish) {
            n.die(fish, killer);
        }
    }
}

impl crate::physics_objects::fire_object::FireHost for BlockheadGlue {
    fn remove_plant_without_creating_freeblocks(&self, plant: &ObjRef) {
        if let Some(p) = crate::plants::plant::as_plant(plant) {
            p.remove_plant_without_creating_freeblocks(plant);
        }
    }
}

impl crate::npc::cave_troll::HostileHost for BlockheadGlue {
    fn npc_exists_at_pos_ignore_npc(&self, pos: IntPair, ignore: Option<&ObjRef>) -> bool {
        self.live_dw().is_some_and(|d| d.npc_exists_at_pos_ignore_npc(pos, ignore))
    }
    fn blockhead_occupies_tile_at_pos(&self, pos: IntPair) -> bool {
        self.live_dw().is_some_and(|d| d.blockhead_occupies_tile_at_pos_ignore_blockhead(pos, None))
    }
    fn create_free_block(&self, pos: IntPair, item_type: i32, priority_blockhead: Option<&ObjRef>) {
        if let Some(d) = self.live_dw() {
            crate::npc::cave_troll::HostileHost::create_free_block(
                &crate::npc::cave_troll::DynamicWorldHost(d),
                pos,
                item_type,
                priority_blockhead,
            );
        }
    }
    fn load_baby_npc_at_position(&self, pos: IntPair, npc_type: i32) {
        if let Some(d) = self.live_dw() {
            crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(
                &crate::npc::cave_troll::DynamicWorldHost(d),
                pos,
                npc_type,
            );
        }
    }
    fn cactus_available_food_at(&self, pos: IntPair) -> Option<f32> {
        let d = self.live_dw()?;
        crate::npc::cave_troll::HostileHost::cactus_available_food_at(&crate::npc::cave_troll::DynamicWorldHost(d), pos)
    }
    fn set_cactus_available_food_at(&self, pos: IntPair, v: f32) {
        if let Some(d) = self.live_dw() {
            crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(
                &crate::npc::cave_troll::DynamicWorldHost(d),
                pos,
                v,
            );
        }
    }
}
