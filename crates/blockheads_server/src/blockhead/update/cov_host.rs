//! A recording, scriptable `BlockheadHost` for the coverage tests of `update`, `interaction_rules` and
//! `state_changes`: the `testutil::TestHost` answers plus integer / object knobs for the other host methods.

use crate::blockhead::blockhead::*;
use crate::items::inventory_item::ItemRef;
use crate::objects::handle::ObjRef;
use crate::plist::value::PlistValue;
use crate::types::item_type::ItemType;
use crate::types::structs::{InteractionTestResult, IntPair, Vector2};
use crate::world::state::TileRef;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Recording host with scripted answers.
#[derive(Default)]
pub struct CovHost {
    pub log: RefCell<Vec<String>>,
    pub cit: Cell<u32>,
    pub item_available: Cell<bool>,
    pub is_npc: Cell<bool>,
    pub food_item: Cell<i32>,
    pub capturable: Cell<bool>,
    pub milkable: Cell<bool>,
    pub shaveable: Cell<bool>,
    pub can_ride: Cell<bool>,
    pub belongs: Cell<bool>,
    pub torch: RefCell<Option<ObjRef>>,
    pub can_pick: Cell<bool>,
    pub plant: RefCell<Option<ObjRef>>,
    pub io_by_id: RefCell<Option<ObjRef>>,
    pub adding_fuel: Cell<bool>,
    pub light_index: Cell<Option<i32>>,
    /// Scripted integer / flag answers of the extra host methods (see `kv`).
    pub knobs: RefCell<std::collections::HashMap<&'static str, i64>>,
    /// Scripted object answers (see `obj`).
    pub objs: RefCell<std::collections::HashMap<&'static str, ObjRef>>,
}
impl CovHost {
    pub fn set(&self, k: &'static str, v: i64) {
        self.knobs.borrow_mut().insert(k, v);
    }
    pub fn kv(&self, k: &str) -> i64 {
        self.knobs.borrow().get(k).copied().unwrap_or(0)
    }
    pub fn flag(&self, k: &str) -> bool {
        self.kv(k) != 0
    }
    pub fn set_obj(&self, k: &'static str, o: Option<ObjRef>) {
        match o {
            Some(o) => {
                self.objs.borrow_mut().insert(k, o);
            }
            None => {
                self.objs.borrow_mut().remove(k);
            }
        }
    }
    pub fn obj(&self, k: &str) -> Option<ObjRef> {
        self.objs.borrow().get(k).cloned()
    }
    pub fn install() -> Rc<CovHost> {
        let h = Rc::new(CovHost { can_pick: Cell::new(true), ..Default::default() });
        set_host(h.clone());
        h
    }
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
    }
    fn rec(&self, s: String) {
        self.log.borrow_mut().push(s);
    }
}
impl BlockheadHostStub for CovHost {
    fn add_simulation_event_of_type_for_blockhead_extra_data(
        &self,
        event_type: i32,
        _b: &ObjRef,
        extra: SimulationEventExtra,
    ) {
        self.rec(format!("sim:{}:{:?}", event_type, extra));
    }
    fn report_achievement_with_identifier(&self, id: &str) {
        self.rec(format!("ach:{}", id));
    }
    fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        _s: Option<&[Vec<ItemRef>]>,
        _d: Option<&PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority: Option<&ObjRef>,
    ) {
        self.rec(format!(
            "fb:{},{}:{}:{}:{}:{}:{}:{}",
            pos.x,
            pos.y,
            item_type.0,
            data_a,
            data_b,
            hovers,
            play_sound,
            priority.is_some()
        ));
    }
    fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
        &self,
        _b: &Blockhead,
        index: i32,
        sub_index: i32,
        count: i32,
        ignore: bool,
    ) {
        self.rec(format!("drop:{}:{}:{}:{}", index, sub_index, count, ignore));
    }
    fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        _b: &Blockhead,
        _t: Option<TileRef>,
        _p: IntPair,
        _i: Option<&ItemRef>,
        _r: &mut bool,
        _ia: bool,
        _f: i32,
        _ap: bool,
    ) -> u32 {
        self.cit.get()
    }
    fn has_interaction_inventory_item_available(&self, _b: &Blockhead) -> bool {
        self.item_available.get()
    }
    fn send_gather_notification_for_tile_at_pos(&self, _t: Option<TileRef>, pos: IntPair) {
        self.rec(format!("gather:{},{}", pos.x, pos.y));
    }
    fn load_gather_block_at_pos(&self, pos: IntPair) {
        self.rec(format!("gatherblock:{},{}", pos.x, pos.y));
    }
    fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
        &self,
        x: i32,
        y: i32,
        c: i32,
        fgc: i32,
        _b: &ObjRef,
        only_c: bool,
        only_fg: bool,
    ) {
        self.rec(format!("removetile:{},{}:{}:{}:{}:{}", x, y, c, fgc, only_c, only_fg));
    }
    fn increment_usage_of_interaction_item(&self, _b: &Blockhead, attack: bool) {
        self.rec(format!("usage:{}", attack));
    }
    fn remove_interaction_item(&self, _b: &Blockhead, f: bool) {
        self.rec(format!("removeitem:{}", f));
    }
    fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, dir: i32) {
        self.rec(format!("door:{},{}:{}:{}", pos.x, pos.y, open, dir));
    }
    fn fill_tile_at_pos_with_type(&self, _t: Option<TileRef>, pos: IntPair, ty: i32) {
        self.rec(format!("fill:{},{}:{}", pos.x, pos.y, ty));
    }
    fn stop_riding(&self, _b: &Blockhead) {
        self.rec("stopriding".to_string());
    }
    fn is_npc(&self, _o: &ObjRef) -> bool {
        self.is_npc.get()
    }
    fn npc_food_item_type(&self, _o: &ObjRef) -> i32 {
        self.food_item.get()
    }
    fn npc_can_be_captured_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead, _t: ItemType) -> bool {
        self.capturable.get()
    }
    fn npc_can_be_milked_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.milkable.get()
    }
    fn npc_can_be_shaved_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.shaveable.get()
    }
    fn ridable_blockhead_can_ride_using_item(&self, _o: &ObjRef, _b: &Blockhead, _i: i32) -> bool {
        self.can_ride.get()
    }
    fn npc_belongs_to_player_with_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.belongs.get()
    }
    fn torch_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
        self.torch.borrow().clone()
    }
    fn can_pick_up_item_of_type_sub_items(&self, _b: &Blockhead, _t: ItemType, _s: Option<&[Vec<ItemRef>]>) -> i32 {
        self.can_pick.get() as i32
    }
    fn remove_object_at_pos(&self, which: Remover, pos: IntPair) -> Option<ObjRef> {
        self.rec(format!("remove:{:?}:{},{}", which, pos.x, pos.y));
        None
    }
    fn add_item_to_inventory_flash(&self, b: &Blockhead, item: &ItemRef, flash: bool) {
        self.rec(format!("additem:{}:{}", item.borrow().item_type.0, flash));
        let _ = b;
    }
    fn get_plant_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
        self.plant.borrow().clone()
    }
    fn interaction_object_with_id(&self, id: i64) -> Option<ObjRef> {
        self.rec(format!("ioid:{id}"));
        self.io_by_id.borrow().clone()
    }
    fn blockhead_would_like_to_take_ownership_with_save_dict(&self, _o: &ObjRef, _b: &Blockhead, _d: &PlistValue) {
        self.rec("takeownership".to_string());
    }
    fn is_adding_fuel_to_any_workbench(&self, _b: &Blockhead) -> bool {
        self.adding_fuel.get()
    }
    fn server_client_light_block_index_for_net_id(&self, _id: Option<&str>) -> Option<i32> {
        self.light_index.get()
    }
    fn inventory_was_changed_sub_index_was_usage(&self, _b: &Blockhead, i: i32, s: i32, u: bool) {
        self.rec(format!("invchanged:{}:{}:{}", i, s, u));
    }
    fn die_for_good(&self, _b: &Blockhead) {
        self.rec("dieforgood".to_string());
    }
    fn abort_in_progress_path_if_for_blockhead(&self, _b: &ObjRef) {
        self.rec("abortpath".to_string());
    }
    fn send_damage_for_net_blockhead_recoil(&self, d: f32, _b: &ObjRef, r: bool) {
        self.rec(format!("netdamage:{}:{}", d, r));
    }
    fn increment_damage_of_armor_clothing(&self, _b: &Blockhead, d: f32) {
        self.rec(format!("armor:{}", d));
    }
    fn unable_to_work_reason(&self, _b: &Blockhead) -> i32 {
        self.kv("unable") as i32
    }
    fn workbench_has_required_fuel(&self, _wb: &ObjRef) -> bool {
        !self.flag("nofuel")
    }
    fn workbench_type(&self, _wb: &ObjRef) -> i32 {
        self.kv("wbtype") as i32
    }
    fn workbench_level(&self, _wb: &ObjRef) -> i32 {
        self.kv("wblevel") as i32
    }
    fn workbench_uses_stores_conducts_or_produces_electricity(&self, _wb: &ObjRef) -> bool {
        self.flag("wbelec")
    }
    fn workbench_abort_immediately_and_restore_blockhead_items(&self, _wb: &ObjRef) {
        self.rec("wbabort".to_string());
    }
    fn is_bed(&self, _o: &ObjRef) -> bool {
        self.flag("bed")
    }
    fn is_donkey(&self, _o: &ObjRef) -> bool {
        self.flag("donkey")
    }
    fn object_item_type(&self, _o: &ObjRef) -> i32 {
        self.kv("objtype") as i32
    }
    fn npc_can_be_fed_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.flag("canfeed")
    }
    fn npc_feed_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) {
        self.rec("feed".to_string());
    }
    fn npc_set_free_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) {
        self.rec("setfree".to_string());
    }
    fn npc_capture_by_blockhead_with_item_type(&self, _o: &ObjRef, _b: &Blockhead, t: i32) -> bool {
        self.rec(format!("capture:{t}"));
        self.flag("capture_ok")
    }
    fn npc_shave_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.rec("shave".to_string());
        self.flag("shave_ok")
    }
    fn npc_milk_by_blockhead(&self, _o: &ObjRef, _b: &Blockhead) -> bool {
        self.rec("milk".to_string());
        self.flag("milk_ok")
    }
    fn increment_usage_of_clothing(&self, _b: &Blockhead) {
        self.rec("clothing".to_string());
    }
    fn increment_usage_of_ice_clothing(&self, _b: &Blockhead) {
        self.rec("iceclothing".to_string());
    }
    fn increment_fuel_usage(&self, _b: &Blockhead) {
        self.rec("fuel".to_string());
    }
    fn increment_passive_item_usage(&self, _b: &Blockhead) {
        self.rec("passive".to_string());
    }
    fn paint_tile_at_pos_color_index_face_index_paint_blockhead(
        &self,
        _t: Option<TileRef>,
        _p: IntPair,
        color: u16,
        face: i32,
        _b: &ObjRef,
    ) {
        self.rec(format!("paint:{color}:{face}"));
    }
    fn remove_paint_at_tile_at_pos_face_index_paint_blockhead(
        &self,
        _t: Option<TileRef>,
        _p: IntPair,
        face: i32,
        _b: &ObjRef,
    ) {
        self.rec(format!("unpaint:{face}"));
    }
    fn remove_water_tile_at_pos(&self, _p: IntPair) {
        self.rec("rmwater".to_string());
    }
    fn remove_back_wall_at_pos_remove_blockhead(&self, p: IntPair, _b: &ObjRef) {
        self.rec(format!("rmbackwall:{},{}", p.x, p.y));
    }
    fn remove_workbench_at_pos_remove_blockhead(&self, _p: IntPair, _b: &ObjRef) {
        self.rec("rmworkbench".to_string());
    }
    fn remove_interaction_object_at_pos_remove_blockhead(&self, _p: IntPair, _b: &ObjRef) {
        self.rec("rmio".to_string());
    }
    fn fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
        &self,
        _t: Option<TileRef>,
        p: IntPair,
        ty: i32,
        da: u16,
        db: u16,
        _c: Option<&str>,
        _sd: Option<&PlistValue>,
        by: Option<&ObjRef>,
        _n: Option<&str>,
    ) {
        self.rec(format!("fillx:{},{}:{ty}:{da}:{db}:{}", p.x, p.y, by.is_some()));
    }
    fn place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        ty: i32,
        _p: IntPair,
        _sd: Option<&PlistValue>,
        _c: Option<&str>,
        _b: &ObjRef,
        _n: Option<&str>,
    ) {
        self.rec(format!("wbplace:{ty}"));
    }
    fn place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        _i: &ItemRef,
        _p: IntPair,
        _sd: Option<&PlistValue>,
        _c: Option<&str>,
        _b: &ObjRef,
        _n: Option<&str>,
    ) -> Option<ObjRef> {
        self.rec("ioplace".to_string());
        self.obj("placed_io")
    }
    fn display_ownership_areas(&self) {
        self.rec("ownershipareas".to_string());
    }
    fn action_in_progress(&self, _a: &ActionRef) -> bool {
        self.flag("inprogress")
    }
    fn good_or_bad_interaction_for_action(&self, _b: &Blockhead, _a: Option<&ActionRef>) -> InteractionTestResult {
        InteractionTestResult {
            good_or_bad: self.kv("gob") as i32,
            render_item_type: 0,
            render_item_data_a: 0,
            render_item_data_b: 0,
        }
    }
    fn action_interaction_test_result(&self, _a: &ActionRef) -> InteractionTestResult {
        InteractionTestResult {
            good_or_bad: self.kv("gob_action") as i32,
            render_item_type: 0,
            render_item_data_a: 0,
            render_item_data_b: 0,
        }
    }
    fn action_interaction_object_id(&self, _a: &ActionRef) -> i64 {
        self.kv("aoid")
    }
    fn action_inventory_change_int_for_key(&self, _a: &ActionRef, _k: &str) -> Option<i32> {
        if self.knobs.borrow().contains_key("invchg") {
            Some(self.kv("invchg") as i32)
        } else {
            None
        }
    }
    fn action_set_inventory_change_int_for_key(&self, _a: &ActionRef, k: &str, v: i32) {
        self.rec(format!("setinvchg:{k}:{v}"));
    }
    fn interaction_object_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
        self.obj("io_at")
    }
    fn interaction_object_type(&self, _o: &ObjRef) -> i32 {
        self.kv("iotype") as i32
    }
    fn interaction_object_is_in_use(&self, _o: &ObjRef) -> bool {
        self.flag("iouse")
    }
    fn interaction_object_remove(&self, _o: &ObjRef, _b: &Blockhead) {
        self.rec("ioremove".to_string());
    }
    fn workbench_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
        self.obj("wb_at")
    }
    fn plant_gather_progress_for_tile(&self, _p: &ObjRef, _pos: IntPair) -> i32 {
        self.kv("plantprog") as i32
    }
    fn plant_set_gather_progress_for_tile(&self, _p: &ObjRef, v: i32, _pos: IntPair) {
        self.rec(format!("plantset:{v}"));
    }
    fn plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
        &self,
        _p: &ObjRef,
        _pos: IntPair,
        _b: &Blockhead,
        m: i32,
    ) -> i32 {
        self.rec(format!("plantharvest:{m}"));
        self.kv("plantn") as i32
    }
    fn pos_of_doors_other_block_at_pos(&self, _p: IntPair) -> IntPair {
        if self.knobs.borrow().contains_key("doorx") {
            IntPair { x: self.kv("doorx") as i32, y: self.kv("doory") as i32 }
        } else {
            IntPair { x: -1, y: -1 }
        }
    }
    fn open_elevator_at_pos(&self, p: IntPair) {
        self.rec(format!("openelevator:{},{}", p.x, p.y));
    }
    fn elevator_motor_for_shaft_at_pos_use_power(&self, p: IntPair) {
        self.rec(format!("elevpower:{},{}", p.x, p.y));
    }
    fn fire_projectile_from_to_at_fire_item_type_firer(
        &self,
        _f: Vector2,
        _t: Vector2,
        _a: Option<&ObjRef>,
        ty: i32,
        _fr: Option<&ObjRef>,
    ) {
        self.rec(format!("fire:{ty}"));
    }
    fn check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(
        &self,
        _t: Vector2,
        _l: bool,
        all: bool,
    ) -> Option<ObjRef> {
        self.rec(format!("harmcheck:{all}"));
        self.obj("harm")
    }
    fn fishing_rod_update(&self, _r: &FishingRodRef, _dt: f32, _a: f32, sim: bool) {
        self.rec(format!("rod:{sim}"));
    }
    fn update_jet_motion(&self, _b: &Blockhead, _dt: f32) {
        self.rec("jetmotion".to_string());
    }
    fn jetpack_fuel_count(&self, b: &Blockhead) -> i32 {
        if self.knobs.borrow().contains_key("jetfuel") {
            self.kv("jetfuel") as i32
        } else if b.is_net() {
            10
        } else {
            0
        }
    }
    fn can_fly(&self, _b: &Blockhead) -> bool {
        self.flag("canfly")
    }
    fn update_gather_speed_and_animation_for_current_interation_and_item(&self, _b: &Blockhead) {
        self.rec("gatherspeed".to_string());
    }
}
