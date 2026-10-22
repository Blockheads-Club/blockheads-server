//! Production `BlockheadHost`: the calls `blockhead_core` makes into the sibling Blockhead modules, the World, the
//! DynamicWorld, BHServer and the object classes. Before, only the four methods `updatePosition:` needs were
//! forwarded; everything else (pickups, placing, removing, NPC interaction, crafting, fishing, ...) ran against the
//! inert stand-in for every blockhead the server simulates.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::BlockheadGlue;
use crate::blockhead::action::ActionBridge;
use crate::blockhead::blockhead::{
    ActionRef, Blockhead, BlockheadHostStub, FishingRodRef, PathExtraData, Remover, SimulationEventExtra,
};
use crate::crafting::craftable_item_object::{
    BlockheadCraftableItemObject, CraftableItemObject, CraftableItemObjectRef,
};
use crate::crafting::workbench::Workbench;
use crate::items::inventory_item::{ItemRef, SubSlot};
use crate::objects::handle::{downcast, is_kind_of_class, ObjRef};
use crate::physics_objects::fishing_rod::FishingRod;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::enums::WorkbenchType;
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, InteractionTestResult, Vector2};
use crate::types::tile::Tile;
use crate::world::state::{TileRef, World};

thread_local! {
    /// The object entries of the last `infoForPathRecalculation` extra data per (blockhead, path type): the path
    /// request goes through the path creator as a plist, which cannot hold the workbench / interaction object /
    /// craftable item object the original's dictionary carries; `setPath:...extraData:` gets them back from here.
    pub(super) static PENDING_PATH_OBJECTS: RefCell<HashMap<(u64, i32), PathExtraData>> = RefCell::new(HashMap::new());
}

/// Temporary `SubSlot`s sharing the items of `subs` (the inherent methods take shared slots).
fn as_slots(subs: Option<&[Vec<ItemRef>]>) -> Option<Vec<SubSlot>> {
    subs.map(|s| s.iter().map(|v| Rc::new(RefCell::new(v.clone()))).collect())
}

/// A craftable item object carried as `Rc<dyn Any>` (path extra data, `Action`) back as its trait object.
fn cio_from_any(a: &Rc<dyn Any>) -> Option<CraftableItemObjectRef> {
    if let Ok(c) = a.clone().downcast::<BlockheadCraftableItemObject>() {
        return Some(c);
    }
    if let Ok(c) = a.clone().downcast::<CraftableItemObject>() {
        return Some(c);
    }
    if let Ok(c) = a.clone().downcast::<crate::interaction::painting::PaintingCraftableItemObject>() {
        return Some(c);
    }
    None
}

fn fishing_rod_of(r: &FishingRodRef) -> Option<&FishingRod> {
    r.downcast_ref::<FishingRod>()
}

fn workbench_of(o: &ObjRef) -> Option<&Workbench> {
    downcast::<Workbench>(o)
}

impl BlockheadGlue {
    fn world_rc(&self) -> Option<Rc<World>> {
        self.glue()?.world()
    }
    fn dw_rc(&self) -> Option<Rc<super::RealDw>> {
        self.glue()?.dw()
    }
    fn srv_rc(&self) -> Option<Rc<crate::server::bhserver::BHServer>> {
        self.glue()?.srv()
    }
}

impl crate::blockhead::blockhead::BlockheadHost for BlockheadGlue {
    // ================= sibling Blockhead methods (blockhead_actions / inventory, via the ready-made bridge) ====
    fn update_position(&self, b: &Blockhead, p: IntPair) {
        BlockheadHostStub::update_position(&ActionBridge, b, p)
    }
    fn stop_riding(&self, b: &Blockhead) {
        BlockheadHostStub::stop_riding(&ActionBridge, b)
    }
    fn pickup_dynamic_object(&self, b: &Blockhead, o: &ObjRef) {
        BlockheadHostStub::pickup_dynamic_object(&ActionBridge, b, o)
    }
    #[allow(clippy::too_many_arguments)]
    fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        b: &Blockhead,
        tile: Option<TileRef>,
        pos: IntPair,
        item: Option<&ItemRef>,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> u32 {
        BlockheadHostStub::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            &ActionBridge, b, tile, pos, item, rejected, include_actions, face_index, allow_protected_actions,
        )
    }
    fn good_or_bad_interaction_for_action(&self, b: &Blockhead, action: Option<&ActionRef>) -> InteractionTestResult {
        BlockheadHostStub::good_or_bad_interaction_for_action(&ActionBridge, b, action)
    }
    fn update_gather_speed_and_animation_for_current_interation_and_item(&self, b: &Blockhead) {
        BlockheadHostStub::update_gather_speed_and_animation_for_current_interation_and_item(&ActionBridge, b)
    }
    fn is_adding_fuel_to_any_workbench(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::is_adding_fuel_to_any_workbench(&ActionBridge, b)
    }
    fn current_traverse_to_key_frame(&self, b: &Blockhead) -> i32 {
        BlockheadHostStub::current_traverse_to_key_frame(&ActionBridge, b)
    }
    fn next_pos(&self, b: &Blockhead) -> IntPair {
        BlockheadHostStub::next_pos(&ActionBridge, b)
    }
    fn info_for_path_recalculation(&self, b: &Blockhead) -> Option<PlistDict> {
        let info = b.info_for_path_recalculation()?;
        let mut d = crate::foundation::gsdict::GsDict::new();
        d.insert("goalX".to_string(), PlistValue::int(info.goal.x as i64));
        d.insert("goalY".to_string(), PlistValue::int(info.goal.y as i64));
        d.insert("goalInteraction".to_string(), PlistValue::int(info.goal_interaction as i64));
        d.insert("pathType".to_string(), PlistValue::int(info.path_type as i64));
        let key = (b.me().unique_id(), info.path_type);
        PENDING_PATH_OBJECTS.with(|m| {
            let mut m = m.borrow_mut();
            match &info.extra {
                Some(e) => {
                    d.insert("extraData".to_string(), e.to_plist());
                    m.insert(key, e.clone());
                }
                None => {
                    m.remove(&key);
                }
            }
        });
        let mut out = crate::objects::base::new_mutable_dict();
        out.map = d;
        Some(out)
    }
    fn set_path_type_goal_interaction_extra_data(
        &self,
        b: &Blockhead,
        path: Option<Vec<PlistValue>>,
        ty: i32,
        goal_interaction: i32,
        extra: Option<PathExtraData>,
    ) {
        let key = (b.me().unique_id(), ty);
        let pending = PENDING_PATH_OBJECTS.with(|m| m.borrow_mut().remove(&key));
        let extra = match (extra, pending) {
            (Some(e), Some(p)) => Some(e.with_objects_from(&p)),
            (e, _) => e,
        };
        b.set_path_type_goal_interaction_extra_data(path, ty, goal_interaction, extra)
    }
    fn set_waiting_for_path_to_pos(&self, b: &Blockhead, p: IntPair) {
        b.set_waiting_for_path_to_pos(p)
    }
    fn set_no_longer_waiting_for_path(&self, b: &Blockhead) {
        b.set_no_longer_waiting_for_path()
    }
    fn can_crawl(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::can_crawl(&ActionBridge, b)
    }
    fn can_fly(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::can_fly(&ActionBridge, b)
    }
    fn tap_is_within_body_radius(&self, b: &Blockhead, tap: Vector2) -> bool {
        BlockheadHostStub::tap_is_within_body_radius(&ActionBridge, b, tap)
    }
    fn hit_with_force_blockhead(&self, b: &Blockhead, force: i32, hitter: Option<&ObjRef>) {
        BlockheadHostStub::hit_with_force_blockhead(&ActionBridge, b, force, hitter)
    }
    fn will_die_if_hit_by_force(&self, b: &Blockhead, force: i32) -> bool {
        BlockheadHostStub::will_die_if_hit_by_force(&ActionBridge, b, force)
    }
    fn center(&self, b: &Blockhead) -> Vector2 {
        BlockheadHostStub::center(&ActionBridge, b)
    }
    fn is_interacting_with_any_interaction_object(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::is_interacting_with_any_interaction_object(&ActionBridge, b)
    }
    fn set_interaction_object(&self, b: &Blockhead, o: Option<&ObjRef>) {
        BlockheadHostStub::set_interaction_object(&ActionBridge, b, o)
    }
    fn set_interaction_workbench(&self, b: &Blockhead, o: Option<&ObjRef>) {
        BlockheadHostStub::set_interaction_workbench(&ActionBridge, b, o)
    }
    fn can_pick_up_item_of_type_sub_items(
        &self,
        b: &Blockhead,
        item_type: ItemType,
        sub_items: Option<&[Vec<ItemRef>]>,
    ) -> i32 {
        let slots = as_slots(sub_items);
        b.can_pick_up_item_of_type_sub_items(item_type, slots.as_deref())
    }
    fn can_pick_up_item_of_type_sub_items_data_a_data_b(
        &self,
        b: &Blockhead,
        item_type: ItemType,
        sub_items: Option<&[Vec<ItemRef>]>,
        data_a: u16,
        data_b: u16,
    ) -> i32 {
        let slots = as_slots(sub_items);
        b.can_pick_up_item_of_type_sub_items_data_a_data_b(item_type, slots.as_deref(), data_a, data_b)
    }
    fn add_item_to_inventory_flash_disable_warp_check_force_slot_index(
        &self,
        b: &Blockhead,
        item: &ItemRef,
        flash: bool,
        disable_warp_check: bool,
        force_slot_index: i32,
    ) {
        b.add_item_to_inventory_flash_disable_warp_check_force_slot_index(
            item,
            flash,
            disable_warp_check,
            force_slot_index,
        );
    }
    fn add_item_to_inventory_flash(&self, b: &Blockhead, item: &ItemRef, flash: bool) {
        b.add_item_to_inventory_flash(item, flash);
    }
    fn pickup_freeblock_if_possible_in_tile_intentional(
        &self,
        b: &Blockhead,
        fb: &ObjRef,
        tile: Option<TileRef>,
        intentional: bool,
    ) -> bool {
        b.pickup_freeblock_if_possible_in_tile_intentional(fb, tile.as_ref(), intentional)
    }
    fn increment_usage_of_interaction_item(&self, b: &Blockhead, used_in_attack: bool) {
        b.increment_usage_of_interaction_item(used_in_attack)
    }
    fn increment_usage_of_clothing(&self, b: &Blockhead) {
        b.increment_usage_of_clothing()
    }
    fn increment_usage_of_ice_clothing(&self, b: &Blockhead) {
        b.increment_usage_of_ice_clothing()
    }
    fn increment_fuel_usage(&self, b: &Blockhead) {
        b.increment_fuel_usage()
    }
    fn increment_passive_item_usage(&self, b: &Blockhead) {
        b.increment_passive_item_usage()
    }
    fn increment_damage_of_armor_clothing(&self, b: &Blockhead, damage: f32) {
        b.increment_damage_of_armor_clothing(damage)
    }
    fn inventory_was_changed_sub_index_was_usage(&self, b: &Blockhead, index: i32, sub_index: i32, was_usage: bool) {
        b.inventory_was_changed_sub_index_was_usage(index, sub_index, was_usage)
    }
    fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
        &self,
        b: &Blockhead,
        index: i32,
        sub_index: i32,
        count: i32,
        ignore_freeblocks: bool,
    ) {
        b.drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(index, sub_index, count, ignore_freeblocks)
    }
    fn remove_interaction_item(&self, b: &Blockhead, flag: bool) {
        b.remove_interaction_item(flag)
    }
    fn has_interaction_inventory_item_available(&self, b: &Blockhead) -> bool {
        b.has_interaction_inventory_item_available()
    }
    fn can_use_dynamic_object(&self, b: &Blockhead, o: &ObjRef) -> bool {
        b.can_use_dynamic_object(o)
    }
    fn stop_all_actions(&self, b: &Blockhead) {
        BlockheadHostStub::stop_all_actions(&ActionBridge, b)
    }
    fn stop_fishing(&self, b: &Blockhead) {
        BlockheadHostStub::stop_fishing(&ActionBridge, b)
    }
    fn die_for_good(&self, b: &Blockhead) {
        BlockheadHostStub::die_for_good(&ActionBridge, b)
    }
    fn unable_to_work_reason(&self, b: &Blockhead) -> i32 {
        BlockheadHostStub::unable_to_work_reason(&ActionBridge, b)
    }
    fn jetpack_fuel_count(&self, b: &Blockhead) -> i32 {
        BlockheadHostStub::jetpack_fuel_count(&ActionBridge, b)
    }
    fn update_jet_motion(&self, b: &Blockhead, dt: f32) {
        BlockheadHostStub::update_jet_motion(&ActionBridge, b, dt)
    }
    fn free_block_pickup_radius(&self, b: &Blockhead) -> f32 {
        BlockheadHostStub::free_block_pickup_radius(&ActionBridge, b)
    }
    fn dpad_should_be_displayed(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::dpad_should_be_displayed(&ActionBridge, b)
    }
    fn current_item(&self, b: &Blockhead) -> Option<ItemRef> {
        b.current_item()
    }
    fn current_interaction_requires_human_input(&self, b: &Blockhead) -> bool {
        BlockheadHostStub::current_interaction_requires_human_input(&ActionBridge, b)
    }

    // ================= Action accessors =================
    fn action_in_progress(&self, a: &ActionRef) -> bool {
        BlockheadHostStub::action_in_progress(&ActionBridge, a)
    }
    fn action_set_in_progress(&self, a: &ActionRef, v: bool) {
        BlockheadHostStub::action_set_in_progress(&ActionBridge, a, v)
    }
    fn action_goal_tile_pos(&self, a: &ActionRef) -> IntPair {
        BlockheadHostStub::action_goal_tile_pos(&ActionBridge, a)
    }
    fn action_get_save_dict(&self, a: &ActionRef) -> PlistValue {
        BlockheadHostStub::action_get_save_dict(&ActionBridge, a)
    }
    fn action_init_with_save_dict_inventory_items(&self, dict: &PlistValue, inventory_items: &[SubSlot]) -> ActionRef {
        BlockheadHostStub::action_init_with_save_dict_inventory_items(&ActionBridge, dict, inventory_items)
    }
    fn action_interaction_test_result(&self, a: &ActionRef) -> InteractionTestResult {
        BlockheadHostStub::action_interaction_test_result(&ActionBridge, a)
    }
    fn action_interaction_object_id(&self, a: &ActionRef) -> i64 {
        BlockheadHostStub::action_interaction_object_id(&ActionBridge, a)
    }
    fn action_inventory_change_int_for_key(&self, a: &ActionRef, key: &str) -> Option<i32> {
        BlockheadHostStub::action_inventory_change_int_for_key(&ActionBridge, a, key)
    }
    fn action_set_inventory_change_int_for_key(&self, a: &ActionRef, key: &str, value: i32) {
        BlockheadHostStub::action_set_inventory_change_int_for_key(&ActionBridge, a, key, value)
    }

    // ================= World =================
    fn add_simulation_event_of_type_for_blockhead_extra_data(
        &self,
        event_type: i32,
        b: &ObjRef,
        extra: SimulationEventExtra,
    ) {
        use crate::world::simulation::SimulationEventExtra as WorldExtra;
        if let Some(w) = self.world_rc() {
            let e = match &extra {
                SimulationEventExtra::None => WorldExtra::None,
                SimulationEventExtra::Text(s) => WorldExtra::Text(s),
                SimulationEventExtra::ItemType(t) => WorldExtra::ItemType(*t),
            };
            w.add_simulation_event_of_type_for_blockhead_extra_data(event_type, Some(b), e);
        }
    }
    fn report_achievement_with_identifier(&self, id: &str) {
        if let Some(w) = self.world_rc() {
            w.report_achievement_with_identifier(id);
        }
    }
    fn abort_in_progress_path_if_for_blockhead(&self, b: &ObjRef) {
        if let Some(w) = self.world_rc() {
            w.abort_in_progress_path_if_for_blockhead(b);
        }
    }
    fn fire_projectile_from_to_at_fire_item_type_firer(
        &self,
        from: Vector2,
        to: Vector2,
        at: Option<&ObjRef>,
        fire_item_type: i32,
        firer: Option<&ObjRef>,
    ) {
        if let Some(w) = self.world_rc() {
            w.fire_projectile_from_to_at_fire_item_type_firer(from, to, at, fire_item_type, firer);
        }
    }
    fn tile_is_lit_for_client_at_pos_tile(
        &self,
        client_id: Option<&str>,
        pos: IntPair,
        tile_ref: Option<TileRef>,
        _tile: Option<Tile>,
    ) -> bool {
        self.world_rc().map_or(false, |w| w.tile_is_lit_for_client_at_pos_tile(client_id, pos, tile_ref))
    }
    fn place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        workbench_type: i32,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        placed_by_client: Option<&str>,
        placed_by_blockhead: &ObjRef,
        placed_by_client_name: Option<&str>,
    ) {
        if let Some(w) = self.world_rc() {
            w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                WorkbenchType(workbench_type as u32),
                pos,
                save_dict,
                placed_by_client,
                Some(placed_by_blockhead),
                placed_by_client_name,
            );
        }
    }
    fn place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        item: &ItemRef,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        placed_by_client: Option<&str>,
        placed_by_blockhead: &ObjRef,
        placed_by_client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let w = self.world_rc()?;
        let it = {
            let i = item.borrow();
            crate::items::inventory_item::InventoryItem {
                item_type: i.item_type,
                data_a: i.data_a,
                data_b: i.data_b,
                sub_items: i.sub_items.clone(),
                selected_sub_item_index: i.selected_sub_item_index,
                dynamic_object_save_dict: i.dynamic_object_save_dict.clone(),
            }
        };
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            &it,
            pos,
            save_dict,
            placed_by_client,
            Some(placed_by_blockhead),
            placed_by_client_name,
        )
    }
    fn fill_tile_at_pos_with_type(&self, tile: Option<TileRef>, pos: IntPair, item_type: i32) {
        if let Some(w) = self.world_rc() {
            w.fill_tile_at_pos_with_type(tile.as_ref(), pos, ItemType(item_type as u32));
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        placed_by_client: Option<&str>,
        save_dict: Option<&PlistValue>,
        placed_by_blockhead: Option<&ObjRef>,
        placed_by_client_name: Option<&str>,
    ) {
        if let Some(w) = self.world_rc() {
            w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
                tile.as_ref(),
                pos,
                ItemType(item_type as u32),
                data_a,
                data_b,
                placed_by_client,
                save_dict,
                placed_by_blockhead,
                placed_by_client_name,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
        &self,
        x: i32,
        y: i32,
        contents_count: i32,
        foreground_count: i32,
        remove_blockhead: &ObjRef,
        only_remove_contents: bool,
        only_remove_foreground_contents: bool,
    ) {
        if let Some(w) = self.world_rc() {
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                x,
                y,
                contents_count,
                foreground_count,
                Some(remove_blockhead),
                only_remove_contents,
                only_remove_foreground_contents,
            );
        }
    }
    fn remove_back_wall_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {
        if let Some(w) = self.world_rc() {
            w.remove_back_wall_at_pos_remove_blockhead(pos, Some(b));
        }
    }
    fn remove_water_tile_at_pos(&self, pos: IntPair) {
        if let Some(w) = self.world_rc() {
            w.remove_water_tile_at_pos(pos);
        }
    }
    fn paint_tile_at_pos_color_index_face_index_paint_blockhead(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        color_index: u16,
        face_index: i32,
        b: &ObjRef,
    ) {
        if let (Some(w), Some(t)) = (self.world_rc(), tile) {
            w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, pos, color_index, face_index as u8, Some(b));
        }
    }
    fn remove_paint_at_tile_at_pos_face_index_paint_blockhead(
        &self,
        tile: Option<TileRef>,
        pos: IntPair,
        face_index: i32,
        b: &ObjRef,
    ) {
        if let (Some(w), Some(t)) = (self.world_rc(), tile) {
            w.remove_paint_at_tile_at_pos_face_index_paint_blockhead(&t, pos, face_index as u8, Some(b));
        }
    }
    fn send_gather_notification_for_tile_at_pos(&self, tile: Option<TileRef>, pos: IntPair) {
        if let (Some(w), Some(t)) = (self.world_rc(), tile) {
            w.send_gather_notification_for_tile_at_pos(&t, pos);
        }
    }
    fn display_ownership_areas(&self) {
        if let Some(w) = self.world_rc() {
            w.display_ownership_areas();
        }
    }
    fn blockhead_reached_interaction_object_destination_path_extra_data(
        &self,
        b: &ObjRef,
        extra: Option<&PathExtraData>,
    ) {
        if let Some(w) = self.world_rc() {
            let dest = extra.map(|e| crate::world::crafting::InteractionDestination {
                interaction_object: e.object_for_key("interactionObject"),
            });
            w.blockhead_reached_interaction_object_destination_path_extra_data(b, dest.as_ref());
        }
    }

    // ================= BHServer =================
    fn remove_curse_words_from_blockhead_name(&self, name: Option<&str>) -> Option<String> {
        match self.srv_rc() {
            Some(s) => s.remove_curse_words_from_blockhead_name(name),
            None => name.map(str::to_string),
        }
    }
    fn player_is_admin_with_id(&self, id: Option<&str>) -> bool {
        self.srv_rc().map_or(false, |s| s.player_is_admin_with_id(id))
    }
    fn update_player(&self, id: Option<&str>) {
        if let (Some(s), Some(id)) = (self.srv_rc(), id) {
            s.update_player(id);
        }
    }
    fn player_name_for_player_with_id_including_old_players(&self, id: Option<&str>) -> Option<String> {
        self.srv_rc()?.player_name_for_player_with_id_including_old_players(id)
    }

    // ================= DynamicWorld lookups =================
    fn interaction_object_with_id(&self, id: i64) -> Option<ObjRef> {
        self.dw_rc()?.interaction_object_with_id(id as u64)
    }
    fn interaction_object_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.interaction_object_at_pos(pos)
    }
    fn workbench_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.workbench_at_pos(pos)
    }
    fn ridable_object_with_id(&self, id: u64) -> Option<ObjRef> {
        self.dw_rc()?.ridable_object_with_id(id)
    }
    fn npc_with_id(&self, id: u64) -> Option<ObjRef> {
        self.dw_rc()?.npc_with_id(id)
    }
    fn get_plant_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.get_plant_at_pos(pos)
    }
    fn free_blocks_at_pos(&self, pos: IntPair) -> Vec<ObjRef> {
        self.dw_rc().and_then(|d| d.free_blocks_at_pos(pos)).unwrap_or_default()
    }
    fn door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.door_at_pos(pos)
    }
    fn window_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.window_at_pos(pos)
    }
    fn egg_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.egg_at_pos(pos)
    }
    fn torch_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.torch_at_pos(pos)
    }
    fn painting_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.painting_at_pos(pos)
    }
    fn column_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.column_at_pos(pos)
    }
    fn ladder_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.ladder_at_pos(pos)
    }
    fn wire_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.wire_at_pos(pos)
    }
    fn stairs_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.stairs_at_pos(pos)
    }
    fn elevator_shaft_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.elevator_shaft_at_pos(pos)
    }
    fn elevator_motor_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        self.dw_rc()?.elevator_motor_at_pos(pos)
    }
    fn remove_object_at_pos(&self, which: Remover, pos: IntPair) -> Option<ObjRef> {
        let d = self.dw_rc()?;
        match which {
            Remover::Torch => d.remove_torch_at_pos(pos),
            Remover::Egg => d.remove_egg_at_pos(pos),
            Remover::Painting => d.remove_painting_at_pos(pos),
            Remover::Ladder => d.remove_ladder_at_pos(pos),
            Remover::Column => d.remove_column_at_pos(pos),
            Remover::Wire => d.remove_wire_at_pos(pos),
            Remover::Stairs => d.remove_stairs_at_pos(pos),
            Remover::ElevatorShaft => d.remove_elevator_shaft_at_pos(pos),
            Remover::ElevatorMotor => d.remove_elevator_motor_at_pos(pos),
            Remover::Rail => d.remove_rail_at_pos(pos),
            Remover::Window => d.remove_window_at_pos(pos),
            Remover::Door => d.remove_door_at_pos(pos),
        }
    }
    fn remove_interaction_object_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {
        if let Some(d) = self.dw_rc() {
            d.remove_interaction_object_at_pos_remove_blockhead(pos, Some(b));
        }
    }
    fn remove_workbench_at_pos_remove_blockhead(&self, pos: IntPair, b: &ObjRef) {
        if let Some(d) = self.dw_rc() {
            d.remove_workbench_at_pos_remove_blockhead(pos, Some(b));
        }
    }
    fn load_gather_block_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw_rc() {
            d.load_gather_block_at_pos(pos);
        }
    }
    fn pos_of_doors_other_block_at_pos(&self, pos: IntPair) -> IntPair {
        self.dw_rc().map_or(IntPair { x: -1, y: -1 }, |d| d.pos_of_doors_other_block_at_pos(pos))
    }
    fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, direction: i32) {
        if let Some(d) = self.dw_rc() {
            d.set_door_at_pos_to_open_direction(pos, open, direction);
        }
    }
    fn open_elevator_at_pos(&self, pos: IntPair) {
        if let Some(d) = self.dw_rc() {
            d.open_elevator_at_pos(pos);
        }
    }
    fn elevator_motor_for_shaft_at_pos_use_power(&self, pos: IntPair) {
        if let Some(m) = self.dw_rc().and_then(|d| d.elevator_motor_for_shaft_at_pos(pos)) {
            if let Some(motor) = downcast::<crate::building::elevator::ElevatorMotor>(&m) {
                motor.use_power(&m);
            }
        }
    }
    fn check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
        &self,
        tap: Vector2,
        ignore_local: bool,
        ignore_all: bool,
    ) -> Option<ObjRef> {
        self.dw_rc()?.check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
            tap,
            ignore_local,
            ignore_all,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[Vec<ItemRef>]>,
        save_dict: Option<&PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority: Option<&ObjRef>,
    ) {
        if let Some(d) = self.dw_rc() {
            let slots = as_slots(sub_items);
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos,
                item_type,
                data_a,
                data_b,
                slots.as_deref().map(super::sub_items_to_owned),
                save_dict.cloned(),
                hovers,
                play_sound,
                priority,
            );
        }
    }

    // ================= classes =================
    fn is_npc(&self, o: &ObjRef) -> bool {
        crate::npc::npc::as_npc(o).is_some()
    }
    fn is_donkey(&self, o: &ObjRef) -> bool {
        is_kind_of_class(o, &crate::npc::donkey::DONKEY_CLASS_INFO)
    }
    fn is_bed(&self, o: &ObjRef) -> bool {
        downcast::<crate::building::bed::Bed>(o).is_some()
    }
    fn npc_food_item_type(&self, npc: &ObjRef) -> i32 {
        crate::npc::npc::as_npc(npc).map_or(0, |n| n.food_item_type())
    }
    fn npc_can_be_captured_by_blockhead(&self, npc: &ObjRef, b: &Blockhead, item_type: ItemType) -> bool {
        crate::npc::npc::as_npc(npc)
            .map_or(false, |n| n.can_be_captured_by_blockhead_with_item_type(&b.me(), item_type.0 as i32))
    }
    fn npc_can_be_milked_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.can_be_milked_by_blockhead(&b.me()))
    }
    fn npc_can_be_shaved_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.can_be_shaved_by_blockhead(&b.me()))
    }
    fn npc_belongs_to_player_with_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.belongs_to_player_with_blockhead(&b.me()))
    }
    fn npc_can_be_fed_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.can_be_fed_by_blockhead(npc, &b.me()))
    }
    fn npc_feed_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) {
        if let Some(n) = crate::npc::npc::as_npc(npc) {
            n.feed_by_blockhead(npc, &b.me());
        }
    }
    fn npc_set_free_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) {
        if let Some(n) = crate::npc::npc::as_npc(npc) {
            n.set_free_by_blockhead(npc, &b.me());
        }
    }
    fn npc_capture_by_blockhead_with_item_type(&self, npc: &ObjRef, b: &Blockhead, item_type: i32) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.capture_by_blockhead_with_item_type(npc, &b.me(), item_type))
    }
    fn npc_shave_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.shave_by_blockhead(npc, &b.me()))
    }
    fn npc_milk_by_blockhead(&self, npc: &ObjRef, b: &Blockhead) -> bool {
        crate::npc::npc::as_npc(npc).map_or(false, |n| n.milk_by_blockhead(npc, &b.me()))
    }
    fn npc_begin_blockhead_inspection(&self, npc: &ObjRef, b: &Blockhead) {
        if let Some(n) = crate::npc::npc::as_npc(npc) {
            n.begin_blockhead_inspection(&b.me());
        }
    }
    fn npc_inspection_stopped(&self, npc: &ObjRef) {
        if let Some(n) = crate::npc::npc::as_npc(npc) {
            n.inspection_stopped();
        }
    }
    fn npc_remote_blockhead_hooked_you(&self, npc: &ObjRef, b: &Blockhead) {
        // only ClownFish implements `remoteBlockheadHookedYou:`
        if downcast::<crate::npc::clown_fish::ClownFish>(npc).is_some() {
            crate::npc::clown_fish::ClownFish::remote_block_head_hooked_you(npc, &b.me());
        }
    }
    fn donkey_breed(&self, o: &ObjRef) -> u16 {
        crate::npc::npc::as_npc(o).map_or(0, |n| n.breed())
    }
    fn egg_breed(&self, o: &ObjRef) -> u16 {
        downcast::<crate::npc::egg::Egg>(o).map_or(0, |e| e.breed())
    }
    fn torch_data_a_b(&self, o: &ObjRef) -> (u16, u16) {
        downcast::<crate::lighting::torch::Torch>(o).map_or((0, 0), |t| (t.data_a(), t.data_b()))
    }
    fn object_item_type(&self, o: &ObjRef) -> i32 {
        use crate::building::{bed::Bed, column::Column, door::Door, stairs::Stairs};
        if let Some(p) = downcast::<crate::interaction::painting::Painting>(o) {
            p.item_type()
        } else if let Some(c) = downcast::<Column>(o) {
            c.item_type()
        } else if let Some(s) = downcast::<Stairs>(o) {
            s.item_type()
        } else if let Some(d) = downcast::<Door>(o) {
            d.item_type()
        } else if let Some(b) = downcast::<Bed>(o) {
            b.item_type()
        } else {
            0
        }
    }
    fn workbench_type(&self, wb: &ObjRef) -> i32 {
        workbench_of(wb).map_or(0, |w| w.workbench_type().0 as i32)
    }
    fn workbench_level(&self, wb: &ObjRef) -> i32 {
        workbench_of(wb).map_or(0, |w| w.level())
    }
    fn workbench_has_required_fuel(&self, wb: &ObjRef) -> bool {
        workbench_of(wb).map_or(false, |w| w.has_required_fuel())
    }
    fn workbench_uses_stores_conducts_or_produces_electricity(&self, wb: &ObjRef) -> bool {
        workbench_of(wb).map_or(false, |w| w.uses_stores_conducts_or_produces_electricity())
    }
    fn workbench_abort_immediately_and_restore_blockhead_items(&self, wb: &ObjRef) {
        if let Some(w) = workbench_of(wb) {
            w.abort_immediately_and_restore_blockhead_items(wb);
        }
    }
    fn workbench_is_in_use(&self, wb: &ObjRef) -> bool {
        wb.is_in_use()
    }
    fn workbench_current_blockhead(&self, wb: &ObjRef) -> Option<ObjRef> {
        wb.current_blockhead()
    }
    fn workbench_craft_item_with_blockhead_count(
        &self,
        wb: &ObjRef,
        item: &Rc<dyn Any>,
        b: &Blockhead,
        count: i32,
    ) -> bool {
        match (workbench_of(wb), cio_from_any(item)) {
            (Some(w), Some(cio)) => w.craft_item_with_blockhead_craft_progress_ui_count(wb, cio, &b.me(), None, count),
            _ => false,
        }
    }
    fn workbench_start_managing_fuel_with_blockhead(&self, wb: &ObjRef, b: &Blockhead) {
        if let Some(w) = workbench_of(wb) {
            w.start_managing_fuel_with_blockhead(wb, &b.me());
        }
    }
    fn set_workbench_choice_ui_option(&self, o: &ObjRef, v: i32) {
        if let Some(p) = downcast::<crate::interaction::trade_portal::TradePortal>(o) {
            p.set_workbench_choice_ui_option(v);
        } else if let Some(t) = downcast::<crate::trains::steam_train::SteamTrain>(o) {
            t.set_workbench_choice_ui_option(o, v);
        }
    }
    fn interaction_object_type(&self, o: &ObjRef) -> i32 {
        o.as_interaction_like().map_or(0, |i| i.interaction_object_type() as i32)
    }
    fn interaction_object_is_in_use(&self, o: &ObjRef) -> bool {
        o.is_in_use()
    }
    fn interaction_object_remove(&self, o: &ObjRef, b: &Blockhead) {
        if let Some(i) = o.as_interaction_like() {
            i.remove(o, Some(&b.me()));
        }
    }
    fn blockhead_would_like_to_take_ownership_with_save_dict(&self, o: &ObjRef, b: &Blockhead, d: &PlistValue) {
        if let Some(i) = o.as_interaction_like() {
            let sd = match d {
                PlistValue::Dict(sd) => Some(sd),
                _ => None,
            };
            i.blockhead_would_like_to_take_ownership_with_save_dict(o, &b.me(), sd);
        }
    }
    fn plant_gather_progress_for_tile(&self, plant: &ObjRef, pos: IntPair) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| p.gather_progress_for_tile(pos))
    }
    fn plant_set_gather_progress_for_tile(&self, plant: &ObjRef, progress: i32, pos: IntPair) {
        if let Some(p) = crate::plants::plant::as_plant(plant) {
            p.set_gather_progress_for_tile(progress, pos);
        }
    }
    fn plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        plant: &ObjRef,
        pos: IntPair,
        b: &Blockhead,
        multiplier: i32,
    ) -> i32 {
        crate::plants::plant::as_plant(plant).map_or(0, |p| {
            p.tile_harvested_remove_blockhead_correct_tool_multiplier(plant, pos, Some(&b.me()), multiplier)
        })
    }

    // ================= FishingRod =================
    fn fishing_rod_valid(&self, r: &FishingRodRef) -> bool {
        fishing_rod_of(r).map_or(false, |f| f.valid())
    }
    fn fishing_rod_hooked_fish_unique_id(&self, r: &FishingRodRef) -> u64 {
        fishing_rod_of(r).and_then(|f| f.hooked_fish()).map_or(0, |f| f.unique_id())
    }
    fn fishing_rod_new_with_swipe(&self, b: &Blockhead, swipe: Vector2) -> FishingRodRef {
        match self.world_rc() {
            Some(w) => {
                let rod = FishingRod::init_with_world_blockhead_cache(&w, &b.me(), None);
                rod.directional_swipe(swipe);
                rod
            }
            None => Rc::new(()),
        }
    }
    fn fishing_rod_update(&self, r: &FishingRodRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if let Some(f) = fishing_rod_of(r) {
            f.update_accurate_dt_is_simulation(dt, accurate_dt, is_simulation);
        }
    }

    // ================= ServerClient =================
    fn server_client_light_block_index_for_net_id(&self, net_id: Option<&str>) -> Option<i32> {
        let client = self.glue()?.client(net_id?)?;
        Some(client.light_block_index())
    }
    fn fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
        &self,
        pos: IntPair,
        light_block_index: i32,
        b: Option<&ObjRef>,
    ) {
        if let Some(w) = self.world_rc() {
            w.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(pos, light_block_index, b);
        }
    }
}
