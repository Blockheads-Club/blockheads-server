//! owned by workbench
//!
//! `Workbench: InteractionObject`: every craft station, fuel burner, power machine and portal (object type 45,
//! interaction object type 1). This file holds the struct, the three constructors, light handling, save / net data,
//! `remote_update`, level upgrades, electricity API, predicates and the `DynamicObject` / `InteractionObjectLike` /
//! `PowerUser` impls. The craft queue and the per-tick `update` live in `workbench_craft.rs`.
//!
//!
//! ## Seams
//! The concrete `Blockhead`, `DynamicWorld` and `CrystalManager` are not reachable through typed APIs from this
//! layer (they are wired by the integrator), so the calls that cannot be made through `World`, `ArtificialLight`
//! or the object_model traits go through [`WorkbenchHost`] (inert defaults). Calls already covered by
//! `world::misc::WorldOpsDeps` (free blocks, fire, particles, `isClientBlockheadBeingControlledByServer`,
//! `addToCrystalDiscrepency`, `stopInteracting`) use that seam, so the integrator wires them once.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::crafting::craftable_item_object::{
    craftable_item_object_from_save_dict, CraftableItemObject, CraftableItemObjectRef,
};
use crate::fmath::cconv::f32_to_i32;
use crate::fmath::libm::powf;
use crate::foundation::exception::range_exception_beyond_size;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::items::inventory_item::{InventoryItem, ItemRef};
use crate::items::misc::{
    fuel_count_for_workbench_and_level, item_type_for_workbench_type_and_level, workbench_requires_electricty,
    workbench_requires_fuel,
};
use crate::items::predicates::workbench_type_is_double_height;
use crate::items::recipes::craftable_items_for_workbench_type_and_level;
use crate::items::workbench_rules::{
    fuel_to_add_for_item_and_workbench_and_level, fuel_types_for_workbench_and_level,
    name_for_workbench_type_and_level, number_of_craftable_items_for_workbench_type_and_level,
    number_of_levels_for_workbench_type, upgrade_name_for_workbench_type_and_level,
};
use crate::lighting::artificial_light::ArtificialLight;
use crate::lighting::sunlight::WorldHelper;
use crate::objects::base::{
    base_remove_from_macro_block, new_mutable_dict, padded_bytes, ub_null_tile_deref, DynamicObject, DynamicObjectBase,
    DynamicWorldRef, WorldApi, WorldRef,
};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, opt_ptr_eq, ptr_eq, CacheRef, ObjRef};
use crate::objects::interaction_object::{
    interaction_object_blockhead_unloaded, interaction_object_blockheads_loaded, interaction_object_dealloc,
    interaction_object_get_save_dict,
    interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name,
    interaction_object_init_with_world_dynamic_world_cache_net_data,
    interaction_object_init_with_world_dynamic_world_save_dict_cache,
    interaction_object_interaction_object_creation_net_data, interaction_object_remote_blockhead_removed_with_id,
    interaction_object_remote_update, interaction_object_should_add_to_macro_block, InteractionObjectLike,
    InteractionObjectState,
};
use crate::objects::protocols::PowerUser;
use crate::path::wire_path::WirePowerSource;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::{r01, r01_tagged, rand};
use crate::types::enums::WorkbenchType;
use crate::types::item_type::ItemType;
use crate::types::netdata::WorkbenchNetData;
use crate::types::structs::{CraftableItem, IntPair, WireStruct};
use crate::world::helpers::{tile_is_air_or_snow, tile_is_half_depth, tile_is_solid, tile_is_tree};
use crate::world::state::World;
use crate::world::tiles::{tile_at_world_position, tile_at_world_position_loaded};

/// `DYNAMIC_OBJECT_TYPE_WORKBENCH`.
pub const DYNAMIC_OBJECT_TYPE_WORKBENCH: i32 = 45;
/// `INTERACTION_OBJECT_TYPE_WORKBENCH`.
pub const INTERACTION_OBJECT_TYPE_WORKBENCH: u16 = 1;

/// `WORKBENCH_BASIC_PORTAL`
pub const WB_BASIC_PORTAL: i32 = 1;
/// `WORKBENCH_CAMPFIRE`
pub const WB_CAMPFIRE: i32 = 3;
/// `WORKBENCH_PLACED_PORTAL`
pub const WB_PLACED_PORTAL: i32 = 13;
/// `WORKBENCH_STEAM_GENERATOR`
pub const WB_STEAM_GENERATOR: i32 = 15;
/// `WORKBENCH_SOLAR_PANEL`
pub const WB_SOLAR_PANEL: i32 = 20;
/// `WORKBENCH_FLYWHEEL`
pub const WB_FLYWHEEL: i32 = 21;
/// `WORKBENCH_EASEL`
pub const WB_EASEL: i32 = 24;
/// `WORKBENCH_REFINERY`
pub const WB_REFINERY: i32 = 26;

/// `Workbench: InteractionObject <AddFuelObject, PowerUser, CraftProgressCraftingObject>`.
pub static WORKBENCH_CLASS: ClassInfo = ClassInfo {
    name: "Workbench",
    superclass: Some(&INTERACTION_OBJECT_CLASS),
    protocols: &["AddFuelObject", "PowerUser", "CraftProgressCraftingObject"],
};

// =====================================================================================================
// Host seam
// =====================================================================================================

crate::hooks::hook_trait! {
/// Calls `Workbench` makes into the `Blockhead` and `DynamicWorld` classes that have no typed API at this layer.
/// All methods are inert by default; the integrator implements the trait once and installs it with
/// [`install_host`]. Selector names are given per method; `nil` receivers are the caller's business.
#[allow(unused_variables)]
pub trait WorkbenchHost, stub WorkbenchHostStub {
    /// `-[DynamicWorld portalIsBeingRemovedAtPos:]`
    fn portal_is_being_removed_at_pos(&self, pos: IntPair) {}
    /// `-[DynamicWorld findAndSubtractAllPowerUpTo:forUser:]` (calls `subtract_electricty` on `user`).
    fn find_and_subtract_all_power_up_to_for_user(&self, required: u16, user: &ObjRef) -> i32 {
        0
    }
    /// `-[Blockhead paused]`
    fn bh_paused(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead hasCoffeeEnergy]`
    fn bh_has_coffee_energy(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead unableToWorkReason]` (0 = can work)
    fn bh_unable_to_work_reason(&self, bh: &ObjRef) -> i32 {
        0
    }
    /// `-[Blockhead shouldContinueSimulating]`
    fn bh_should_continue_simulating(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead isCraftingAtAnyWorkbench]`
    fn bh_is_crafting_at_any_workbench(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead isAddingFuelToAnyWorkbench]`
    fn bh_is_adding_fuel_to_any_workbench(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead interactionWorkbench]`
    fn bh_interaction_workbench(&self, bh: &ObjRef) -> Option<ObjRef> {
        None
    }
    /// `-[Blockhead setInteractionWorkbench:]`
    fn bh_set_interaction_workbench(&self, bh: &ObjRef, wb: Option<&ObjRef>) {}
    /// `-[Blockhead setInteractionObject:]`
    #[provided]
    fn bh_set_interaction_object(&self, bh: &ObjRef, o: Option<&ObjRef>) {
        if let Some(b) = bh.as_blockhead() {
            b.set_interaction_object(o);
        }
    }
    /// `-[Blockhead doubleTimeUnlocked]`
    fn bh_double_time_unlocked(&self, bh: &ObjRef) -> bool {
        false
    }
    /// `-[Blockhead subtractItemsFromInventoryOfType:count:dataB:]` (may return fewer than `count` items).
    fn bh_subtract_items_from_inventory_of_type_count_data_b(
        &self,
        bh: &ObjRef,
        item_type: ItemType,
        count: i32,
        data_b: i32,
    ) -> Vec<ItemRef> {
        Vec::new()
    }
    /// `-[Blockhead addItemToInventory:]`
    fn bh_add_item_to_inventory(&self, bh: &ObjRef, item: &ItemRef) {}
    /// `-[Blockhead craftItemFinished:atWorkbench:]` (stopInteracting + drop `interactionWorkbench`).
    fn bh_craft_item_finished_at_workbench(&self, bh: &ObjRef, completed: bool, wb: &ObjRef) {}
    /// `-[Blockhead netInteractionObjectWasLoaded:]`
    fn bh_net_interaction_object_was_loaded(&self, bh: &ObjRef, wb: &ObjRef) {}
}
}

/// All-inert host.
pub struct NullHost;
impl WorkbenchHostStub for NullHost {}

thread_local! {
    static HOST: RefCell<Rc<dyn WorkbenchHost>> = RefCell::new(Rc::new(NullHost));
}

/// Installs the [`WorkbenchHost`] adapter (main thread; one world per process).
pub fn install_host(h: Rc<dyn WorkbenchHost>) {
    HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed host (cheap `Rc` clone; never hold a `RefCell` borrow across a host call).
pub fn host() -> Rc<dyn WorkbenchHost> {
    HOST.with(|c| c.borrow().clone())
}

// =====================================================================================================
// State
// =====================================================================================================

/// Own ivars of `Workbench`. `Default` = ObjC `alloc` zero-fill (note
/// `saved_blockhead_index_fuel` is 0 here and is set to -1 only by the save-dict init).
#[derive(Default)]
pub struct WorkbenchState {
    /// 0xa0 child light (fuel glow / portal light)
    pub light: Option<Rc<ArtificialLight>>,
    /// 0xb0 retained
    pub current_fuel_blockhead: Option<ObjRef>,
    /// 0xb8
    pub is_in_use_fuel: bool,
    /// 0xbc
    pub saved_blockhead_index_fuel: i32,
    /// 0xc0 `type`
    pub ty: i32,
    /// 0xc4
    pub number_of_craftable_items: i32,
    /// 0xc8
    pub number_of_craftable_items_up_to_current_level: i32,
    /// 0xd0 one slice per level; allocated once by the first `init_level_stuff`
    pub craftable_items: Vec<&'static [CraftableItem]>,
    /// 0xd8
    pub selected_index: i32,
    /// 0xe0: `None` = nil array, `Some(vec![])` = empty non-nil array (they behave differently in `abort_craft`)
    pub source_items: [Option<Vec<ItemRef>>; 8],
    /// 0x120
    pub x_scroll: f32,
    /// 0x124
    pub level: i32,
    /// 0x128
    pub crafting_item_object: Option<CraftableItemObjectRef>,
    /// 0x138
    pub craft_progress_count: f32,
    /// 0x13c
    pub hurry_timer: f32,
    /// 0x140
    pub hurry_seconds: f32,
    /// 0x144
    pub hurrying: bool,
    /// 0x148
    pub hurry_cost: i32,
    /// 0x14c
    pub fire_spread_timer: f32,
    /// 0x150
    pub fuel_fraction: f32,
    /// 0x154
    pub fuel_counter: f32,
    /// 0x158
    pub has_fuel: bool,
    /// 0x159
    pub requires_fuel: bool,
    /// 0x15a
    pub available_electricity: u16,
    /// 0x15c
    pub requires_electricity: bool,
    /// 0x160
    pub count: i32,
    /// 0x164
    pub count_left: i32,
    /// 0x168
    pub count_created: i32,
    /// 0x16c `rand() % 8`, never read on the server
    pub animation_loop_index: i32,
    /// 0x178
    pub last_world_time: f64,
    /// 0x180
    pub fraction_complete: f32,
    /// 0x188
    pub remote_fuel_blockhead_in_use_unique_id: u64,
    /// 0x190
    pub particle_create_timer_steam_engine: f32,
    /// 0x1a0
    pub paused: bool,
    /// 0x1d0 (sic)
    pub time_since_found_electricty: f32,
}

/// `Workbench`.
pub struct Workbench {
    pub(crate) base: RefCell<DynamicObjectBase>,
    pub(crate) io: RefCell<InteractionObjectState>,
    pub(crate) st: RefCell<WorkbenchState>,
    world_w: Weak<World>,
}

// =====================================================================================================
// Free helpers
// =====================================================================================================

/// `(float)rand() / 2147483648.0f` as used by the particle loops.
fn rf() -> f32 {
    r01_tagged("workbench.particle")
}

/// The 7-draw particle burst of `upgradeToNextLevel`, `remoteUpdate:` (8 iterations) and `hurryCompletion:`
/// (16 iterations). The arguments of `addParticleAtPos:...` are evaluated in the original even though the server
/// emitter ignores them, so only the draw count is observable; the order is kept anyway.
pub fn particle_burst_rands(iterations: usize, pos: IntPair) {
    let hit = [pos.x as f32 + 0.5f32, pos.y as f32, -0.5f32];
    for _ in 0..iterations {
        let r1 = rf();
        let d = (r1 as f64 * 1.5f64) as f32 as f64;
        let color = [(d + 0.231373f64) as f32, (d + 0.347059f64) as f32, 1.0f32, 0.5f32];
        let r2 = rf();
        let r3 = rf();
        let off = [r2 - 0.5f32, r3 + r3, 0.0f32];
        let p = [hit[0] + off[0], hit[1] + off[1], hit[2] + off[2]];
        let vx = (rf() - 0.5f32) * 4.0f32;
        let vy = (rf() + -0.3f32) * 4.0f32;
        let life = rf() * 4.0f32 + 1.0f32;
        let scale = rf() * 4.0f32 + 4.0f32;
        crate::world::misc::deps().particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(
            p,
            [vx, vy, 0.0],
            color,
            2,
            life,
            scale,
        );
    }
}

fn get<'a>(d: &'a PlistDict, k: &str) -> Option<&'a PlistValue> {
    d.map.get(&k.to_string())
}

fn get_f32(d: &PlistDict, k: &str) -> f32 {
    get(d, k).map_or(0.0, |v| v.float_value())
}

fn get_i32(d: &PlistDict, k: &str) -> i32 {
    get(d, k).map_or(0, |v| v.int_value())
}

fn get_bool(d: &PlistDict, k: &str) -> bool {
    get(d, k).is_some_and(|v| v.bool_value())
}

/// `[NSString stringWithFormat:@"sourceItems_%d", i]`
fn source_items_key(i: usize) -> String {
    format!("sourceItems_{}", i)
}

// =====================================================================================================
// Construction
// =====================================================================================================

impl Workbench {
    /// `[Workbench alloc]`: zero-initialised state, built with `Rc::new_cyclic`.
    fn alloc(world: &Rc<World>) -> Rc<Workbench> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| Workbench {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(WorkbenchState::default()),
            world_w: w,
        })
    }

    /// `self.world` (concrete). `None` only after the world was dropped.
    pub(crate) fn w(&self) -> Option<Rc<World>> {
        self.world_w.upgrade()
    }

    /// `self.dynamicWorld`
    pub(crate) fn dw(&self) -> Option<DynamicWorldRef> {
        self.base.borrow().dynamic_world.as_ref().and_then(|w| w.upgrade())
    }

    pub(crate) fn pos(&self) -> IntPair {
        self.base.borrow().pos
    }

    pub(crate) fn wt(&self) -> WorkbenchType {
        WorkbenchType(self.st.borrow().ty as u32)
    }

    /// NOTIFY: `[dynamicWorld dynamicWorldChangedAtPos:pos objectType:[self objectType]]`.
    pub(crate) fn notify(&self) {
        if let Some(dw) = self.dw() {
            dw.dynamic_world_changed_at_pos_object_type(self.pos(), DYNAMIC_OBJECT_TYPE_WORKBENCH);
        }
    }

    fn expert_mode(&self) -> bool {
        self.w().is_some_and(|w| w.expert_mode())
    }

    fn is_cloud_game(&self) -> bool {
        self.w().is_some_and(|w| w.is_cloud_game())
    }

    /// `[world worldTime]` (0.0 for a nil world)
    pub(crate) fn world_time(&self) -> f64 {
        self.w().map_or(0.0, |w| WorldApi::world_time(&*w))
    }

    /// `[light release]; light = nil` (the child is only retained by the workbench at that point).
    fn release_light(&self) {
        let l = self.st.borrow_mut().light.take();
        if let Some(l) = l {
            l.dealloc_light();
        }
    }

    /// `[[ArtificialLight alloc] initWithWorld:... parentObject:self... lightDirection:0]`
    fn new_light(
        &self,
        me: &ObjRef,
        pos: IntPair,
        r: i32,
        g: i32,
        b: i32,
        heat: i32,
        radius: i32,
    ) -> Option<Rc<ArtificialLight>> {
        let world = self.w()?;
        let dw = self.dw()?;
        let cache: CacheRef = self.base.borrow().cache.clone();
        ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
            &world,
            &dw,
            pos,
            cache,
            Some(me),
            r,
            g,
            b,
            heat,
            radius,
            0,
        )
    }

    /// `-[Workbench initWithWorld:dynamicWorld:atPosition:cache:type:flipped:saveDict:placedByClient:clientName:]`
    /// RNG: CAMPFIRE draws one `rand()` here, then `init_sub_derived_items` two.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        ty: WorkbenchType,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Workbench>> {
        let this = Self::alloc(world);
        let me: ObjRef = this.clone();
        let wref: WorldRef = world.clone();
        // the base initialisers always succeed for a position, so `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me, &wref, dw, pos, cache, None, flipped, save_dict, client_id, client_name,
        )
        .then_some(())?;
        let ty_i = ty.0 as i32;
        {
            let mut s = this.st.borrow_mut();
            s.ty = ty_i;
            s.fuel_counter = 1.0;
        }
        if workbench_requires_fuel(ty, 0) {
            let mut s = this.st.borrow_mut();
            s.fuel_fraction = 1.0;
            s.has_fuel = true;
        }
        if ty_i == WB_CAMPFIRE {
            let t = r01() * 20.0f32 + 30.0f32;
            this.st.borrow_mut().fire_spread_timer = t;
        } else if ty_i == WB_STEAM_GENERATOR {
            this.st.borrow_mut().available_electricity = 0x7f;
        } else if ty_i == WB_SOLAR_PANEL {
            this.st.borrow_mut().available_electricity = 7;
        }
        if let Some(sd) = save_dict {
            let mut s = this.st.borrow_mut();
            s.fuel_fraction = get_f32(sd, "fuelFraction");
            s.has_fuel = get_bool(sd, "hasFuel");
            s.level = get_i32(sd, "level");
            s.available_electricity = get(sd, "availableElectricity").map_or(0, |v| v.unsigned_int_value()) as u16;
        }
        let level = this.st.borrow().level;
        if ty_i == WB_PLACED_PORTAL {
            // `pos` here is the ivar (wrapped / clamped by the super init)
            let bp = this.pos();
            let below = IntPair { x: bp.x, y: bp.y - 1 };
            let t = tile_at_world_position_loaded(below.x, below.y, world);
            if let Some(t) = t {
                t.update(|t| {
                    t.contents = 0;
                    if (0..=5).contains(&level) {
                        t.retype((42 + level) as u8);
                    }
                    if t.back_wall_type_index == 2 {
                        t.back_wall_type_index = 10;
                    }
                });
            }
            WorldHelper::update_sun_light_removed_for_tile_at_pos_world(t, below, world);
        }
        let (rf_, has_fuel) = {
            let s = this.st.borrow();
            (workbench_requires_fuel(ty, s.level), s.has_fuel)
        };
        let p = this.pos();
        if !rf_ {
            if ty_i == WB_BASIC_PORTAL || ty_i == WB_PLACED_PORTAL {
                this.update_portal_light(&me);
            }
        } else if !has_fuel {
            if ty_i == WB_CAMPFIRE {
                let l = this.new_light(&me, p, 0x40, 0x32, 4, 1, 8);
                this.st.borrow_mut().light = l;
            }
        } else {
            let l = this.new_light(&me, p, 0xdc, 0x82, 0x30, 4, 10);
            this.st.borrow_mut().light = l;
        }
        this.init_sub_derived_items();
        let t = this.world_time();
        this.st.borrow_mut().last_world_time = t;
        Some(this)
    }

    /// Inherited `InteractionObject...atPosition:cache:item:flipped:saveDict:placedByClient:clientName:` run on a
    /// zero-state Workbench (Workbench does not override the `item:` selector). No rand(), no light, no
    /// `initSubDerivedItems`, `lastWorldTime` stays 0.
    #[allow(clippy::too_many_arguments)]
    pub fn init_inherited_item(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Workbench>> {
        let this = Self::alloc(world);
        let me: ObjRef = this.clone();
        let wref: WorldRef = world.clone();
        interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me, &wref, dw, pos, cache, None, flipped, save_dict, client_id, client_name,
        )
        .then_some(())?;
        Some(this)
    }

    /// `-[Workbench initWithWorld:dynamicWorld:saveDict:cache:]` (load from the world save).
    /// RNG: two draws from `init_sub_derived_items`.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Workbench>> {
        let this = Self::alloc(world);
        let me: ObjRef = this.clone();
        let wref: WorldRef = world.clone();
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me, &wref, dw, save_dict, cache.clone())
            .then_some(())?;
        let sd = save_dict;
        {
            let mut s = this.st.borrow_mut();
            s.ty = get_i32(sd, "workbenchType");
            s.selected_index = get_i32(sd, "selectedIndex");
            s.x_scroll = get_f32(sd, "xScroll");
            s.level = get_i32(sd, "level");
            s.craft_progress_count = get_f32(sd, "craftProgressCount");
            s.hurry_timer = get_f32(sd, "hurryTimer");
            s.hurry_seconds = get_f32(sd, "hurrySeconds");
            s.hurrying = get_bool(sd, "hurrying");
            s.hurry_cost = get_i32(sd, "hurryCost");
            s.fire_spread_timer = get_f32(sd, "fireSpreadTimer");
            s.fuel_fraction = get_f32(sd, "fuelFraction");
            s.has_fuel = get_bool(sd, "hasFuel");
            s.last_world_time = get(sd, "lastWorldTime").map_or(0.0, |v| v.double_value());
            s.is_in_use_fuel = get_bool(sd, "isInUseFuel");
            s.available_electricity = get(sd, "availableElectricity").map_or(0, |v| v.unsigned_int_value()) as u16;
            s.saved_blockhead_index_fuel = -1;
            if let Some(v) = get(sd, "currentBlockheadIndexFuel") {
                s.saved_blockhead_index_fuel = v.int_value();
            }
        }
        let in_use = this.io.borrow().is_in_use;
        // `setInteractionWorkbench:` for the current and the fuel blockhead is a message to nil here: both are only
        // resolved later, by `blockheads_loaded`
        if in_use {
            this.restore_crafting_state(sd);
        }
        if let Some(PlistValue::Dict(ld)) = get(sd, "lightDict") {
            if let (Some(w), Some(d)) = (this.w(), this.dw()) {
                let l = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
                    &w,
                    &d,
                    ld,
                    cache,
                    Some(&me),
                );
                this.st.borrow_mut().light = l;
            }
        }
        this.init_sub_derived_items();
        Some(this)
    }

    /// `-[Workbench initWithWorld:dynamicWorld:cache:netData:]` (client-replica creation, `isNet = true`).
    /// Raises `NSRangeException` for a blob shorter than 0x38.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Workbench>> {
        let this = Self::alloc(world);
        let me: ObjRef = this.clone();
        let wref: WorldRef = world.clone();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me, &wref, dw, cache, net_data) {
            return None;
        }
        if net_data.len() < 0x38 {
            range_exception_beyond_size("getBytes:length:", 0, 0x38, net_data.len() as u64);
        }
        let nd = <WorkbenchNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x38));
        {
            let mut s = this.st.borrow_mut();
            s.ty = nd.type_ as i32;
            s.is_in_use_fuel = nd.is_in_use_fuel != 0;
            s.level = nd.level as i32;
            s.has_fuel = nd.has_fuel != 0;
            s.fuel_fraction = (nd.fuel_fraction as i32 as f32) / 255.0f32;
            s.available_electricity = nd.available_electricity;
        }
        this.init_sub_derived_items();
        let t = this.world_time();
        this.st.borrow_mut().last_world_time = t;
        for b in dw.blockheads() {
            host().bh_net_interaction_object_was_loaded(&b, &me);
        }
        if net_data.len() > 0x38 {
            let (oid, oname) = Self::owner_from_tail(&net_data[0x38..]);
            this.base.borrow_mut().owner_id = oid;
            this.io.borrow_mut().owner_name = oname;
        }
        Some(this)
    }

    /// `propertyListFromData([[netData subdataWithRange:(0x38,...)] gzipInflate])` -> `ownerID`, `ownerName`.
    fn owner_from_tail(tail: &[u8]) -> (Option<String>, Option<String>) {
        let plist = gzip_inflate(tail).and_then(|d| property_list_from_data(&d));
        match plist {
            Some(PlistValue::Dict(d)) => (
                get(&d, "ownerID").and_then(|v| v.as_str()).map(|s| s.to_string()),
                get(&d, "ownerName").and_then(|v| v.as_str()).map(|s| s.to_string()),
            ),
            _ => (None, None),
        }
    }

    /// The `craftingItemDatav2` / legacy `craftingItemData` / `count*` / `sourceItems_%d` restore shared by the
    /// load init and `blockhead_would_like_to_take_ownership_with_save_dict`.
    fn restore_crafting_state(&self, sd: &PlistDict) {
        let cio: Option<CraftableItemObjectRef> = match get(sd, "craftingItemDatav2") {
            None => match get(sd, "craftingItemData") {
                Some(PlistValue::Data(b)) => {
                    if b.bytes.len() < 0x7c {
                        range_exception_beyond_size("getBytes:length:", 0, 0x7c, b.bytes.len() as u64);
                    }
                    Some(
                        CraftableItemObject::init_with_craftable_item(<CraftableItem as WireStruct>::read_le(&b.bytes))
                            as CraftableItemObjectRef,
                    )
                }
                _ => None,
            },
            Some(PlistValue::Dict(v2)) => Some(craftable_item_object_from_save_dict(v2)),
            Some(_) => Some(CraftableItemObject::init_with_save_dict(&new_mutable_dict()) as CraftableItemObjectRef),
        };
        self.st.borrow_mut().crafting_item_object = cio.clone();
        if let Some(cio) = cio {
            let ci = cio.craftable_item();
            let mut s = self.st.borrow_mut();
            s.count = get_i32(sd, "count");
            s.count_left = get_i32(sd, "countLeft");
            s.count_created = get_i32(sd, "countCreated");
            let n = ci.number_of_required_items.clamp(0, 8) as usize;
            for i in 0..n {
                s.source_items[i] = None;
                if ci.required_items[i] != 11 {
                    if let Some(PlistValue::Array(a)) = get(sd, &source_items_key(i)) {
                        if !a.items.is_empty() {
                            let mut v = Vec::new();
                            for d in &a.items {
                                if let PlistValue::Data(b) = d {
                                    v.push(InventoryItem::from_save_data(&b.bytes));
                                }
                            }
                            s.source_items[i] = Some(v);
                        }
                    }
                }
            }
        }
    }

    /// `-[Workbench initLevelStuff]`. The per-level array is allocated once (first `numberOfLevels`);
    /// later calls overwrite the first entries but never grow it.
    pub fn init_level_stuff(&self) {
        let expert = self.expert_mode();
        let cloud = self.is_cloud_game();
        let (ty, level) = {
            let s = self.st.borrow();
            (WorkbenchType(s.ty as u32), s.level)
        };
        let n_levels = number_of_levels_for_workbench_type(ty, expert);
        let rules_enabled = self.w().map_or(0, |w| w.custom_rules().custom_rules_enabled);
        let portal_chests_allowed = rules_enabled == 0 && !expert;
        {
            let mut s = self.st.borrow_mut();
            if s.craftable_items.is_empty() {
                s.craftable_items = vec![&[][..]; n_levels.max(0) as usize];
            }
            s.number_of_craftable_items = 0;
            s.number_of_craftable_items_up_to_current_level = 0;
        }
        for i in 0..n_levels {
            let items = craftable_items_for_workbench_type_and_level(ty, i, cloud, expert);
            let n = number_of_craftable_items_for_workbench_type_and_level(
                ty,
                i,
                level,
                cloud,
                portal_chests_allowed,
                expert,
            );
            let mut s = self.st.borrow_mut();
            if (i as usize) < s.craftable_items.len() {
                s.craftable_items[i as usize] = items;
            }
            s.number_of_craftable_items = s.number_of_craftable_items.wrapping_add(n);
            if i <= level {
                s.number_of_craftable_items_up_to_current_level =
                    s.number_of_craftable_items_up_to_current_level.wrapping_add(n);
            }
        }
        let mut s = self.st.borrow_mut();
        s.requires_fuel = workbench_requires_fuel(ty, level);
        s.requires_electricity = workbench_requires_electricty(ty, level);
    }

    /// `-[Workbench initSubDerivedItems]`: `init_level_stuff`, `rand() % 8`, `fuelCounter = r01 * 5`,
    /// tile graphic (server only).
    pub fn init_sub_derived_items(&self) {
        self.init_level_stuff();
        let a = rand() % 8;
        self.st.borrow_mut().animation_loop_index = a;
        let fc = r01() * 5.0f32;
        self.st.borrow_mut().fuel_counter = fc;
        // `dw.isClient` is NO on the dedicated server, so the tile graphic is always updated
        if let Some(w) = self.w() {
            let (ty, level) = {
                let s = self.st.borrow();
                (WorkbenchType(s.ty as u32), s.level)
            };
            w.update_tile_graphic_for_workbench_of_type_at_pos_level(ty, self.pos(), level);
        }
    }

    /// `-[Workbench dealloc]`: light, crafting object, fuel blockhead, `sourceItems`, then super.
    pub fn dealloc(&self, me: &ObjRef) {
        self.release_light();
        {
            let mut s = self.st.borrow_mut();
            s.crafting_item_object = None;
            s.current_fuel_blockhead = None;
            for i in 0..8 {
                s.source_items[i] = None;
            }
        }
        interaction_object_dealloc(me);
    }
}

// =====================================================================================================
// Accessors and predicates
// =====================================================================================================

impl Workbench {
    /// `-type` (selector `type`)
    pub fn workbench_type(&self) -> WorkbenchType {
        self.wt()
    }
    /// `-level`
    pub fn level(&self) -> i32 {
        self.st.borrow().level
    }
    /// `-selectedIndex`
    pub fn selected_index(&self) -> i32 {
        self.st.borrow().selected_index
    }
    /// `-setSelectedIndex:`
    pub fn set_selected_index(&self, i: i32) {
        self.st.borrow_mut().selected_index = i;
    }
    /// `-xScroll`
    pub fn x_scroll(&self) -> f32 {
        self.st.borrow().x_scroll
    }
    /// `-setXScroll:`
    pub fn set_x_scroll(&self, x: f32) {
        self.st.borrow_mut().x_scroll = x;
    }
    /// `-numberOfCraftableItems`
    pub fn number_of_craftable_items(&self) -> i32 {
        self.st.borrow().number_of_craftable_items
    }
    /// `-numberOfCraftableItemsUpToCurrentLevel`
    pub fn number_of_craftable_items_up_to_current_level(&self) -> i32 {
        self.st.borrow().number_of_craftable_items_up_to_current_level
    }
    /// `-craftingItemObject`
    pub fn crafting_item_object(&self) -> Option<CraftableItemObjectRef> {
        self.st.borrow().crafting_item_object.clone()
    }
    /// `-count`
    pub fn count(&self) -> i32 {
        self.st.borrow().count
    }
    /// `-countLeft`
    pub fn count_left(&self) -> i32 {
        self.st.borrow().count_left
    }
    /// `-fractionComplete`
    pub fn fraction_complete(&self) -> f32 {
        self.st.borrow().fraction_complete
    }
    /// `-availableElectricity`
    pub fn available_electricity(&self) -> u16 {
        self.st.borrow().available_electricity
    }
    /// `-title`: `nameForWorkbenchTypeAndLevel(type, level, [world expertMode])`
    pub fn title(&self) -> String {
        let (t, l) = (self.wt(), self.level());
        name_for_workbench_type_and_level(t, l, self.expert_mode())
    }
    /// `-upgradeName`
    pub fn upgrade_name(&self) -> String {
        upgrade_name_for_workbench_type_and_level(self.wt(), self.level())
    }
    /// `-craftableItems`
    pub fn craftable_items(&self) -> Vec<&'static [CraftableItem]> {
        self.st.borrow().craftable_items.clone()
    }
    /// `-isDoubleHeight`
    pub fn is_double_height(&self) -> bool {
        workbench_type_is_double_height(self.wt())
    }
    /// `-requiresFuel` (recomputed, NOT the ivar).
    pub fn requires_fuel(&self) -> bool {
        workbench_requires_fuel(self.wt(), self.level())
    }
    /// `-requiresElectricty` (sic).
    pub fn requires_electricty(&self) -> bool {
        workbench_requires_electricty(self.wt(), self.level())
    }
    /// `-requiresHumanInteraction`: `isInUseFuel`.
    pub fn requires_human_interaction(&self) -> bool {
        self.st.borrow().is_in_use_fuel
    }
    /// `-hasRequiredFuel`.
    pub fn has_required_fuel(&self) -> bool {
        let s = self.st.borrow();
        if s.has_fuel || !s.requires_fuel {
            if s.available_electricity != 0 {
                true
            } else {
                !s.requires_electricity
            }
        } else {
            false
        }
    }
    /// `-fuelCount`: `n = (int)(10.0f * fuelFraction + 0.99f); n < 1 ? 0: min(n, 10)`.
    pub fn fuel_count(&self) -> i32 {
        let ff = self.st.borrow().fuel_fraction;
        let n = f32_to_i32(10.0f32 * ff + 0.99f32);
        if n < 1 {
            0
        } else if n > 10 {
            10
        } else {
            n
        }
    }
    /// `-fuelItemCount`
    pub fn fuel_item_count(&self) -> i32 {
        fuel_count_for_workbench_and_level(self.wt(), self.level())
    }
    /// `-fuelTypesCount`
    pub fn fuel_types_count(&self) -> i32 {
        fuel_count_for_workbench_and_level(self.wt(), self.level())
    }
    /// `-fuelItems`
    pub fn fuel_items(&self) -> Option<[ItemType; 3]> {
        fuel_types_for_workbench_and_level(self.wt(), self.level())
    }
    /// `-fuelTypes`
    pub fn fuel_types(&self) -> Option<[ItemType; 3]> {
        fuel_types_for_workbench_and_level(self.wt(), self.level())
    }
    /// `-isStorageDevice`: FLYWHEEL.
    pub fn is_storage_device(&self) -> bool {
        self.st.borrow().ty == WB_FLYWHEEL
    }
    /// `-generatesElectricity`: STEAM_GENERATOR or SOLAR_PANEL.
    pub fn generates_electricity(&self) -> bool {
        let t = self.st.borrow().ty;
        t == WB_STEAM_GENERATOR || t == WB_SOLAR_PANEL
    }
    /// `-conductsElectricity`.
    pub fn conducts_electricity(&self) -> bool {
        self.is_storage_device() || self.generates_electricity()
    }
    /// `-usesStoresConductsOrProducesElectricity`.
    pub fn uses_stores_conducts_or_produces_electricity(&self) -> bool {
        matches!(self.st.borrow().ty, 15 | 20 | 21 | 17 | 16 | 27 | 18 | 19 | 26 | 29 | 30)
    }
    /// `-energyFraction`.
    pub fn energy_fraction(&self) -> f32 {
        let (t, avail) = {
            let s = self.st.borrow();
            (s.ty, s.available_electricity)
        };
        if t == WB_SOLAR_PANEL {
            let mut cl = self.combined_light_for_solar_panel();
            if cl < 0.1f32 {
                cl = 0.0;
            }
            return (if 1.0f64 <= cl as f64 { 1.0f64 } else { cl as f64 }) as f32;
        }
        if t == WB_FLYWHEEL {
            return (avail as f32) / 8192.0f32;
        }
        0.0
    }
    /// `-destroyItemType` / `-freeblockCreationItemType`: `itemTypeForWorkbenchTypeAndLevel(type, level)`.
    pub fn destroy_item_type(&self) -> ItemType {
        item_type_for_workbench_type_and_level(self.wt(), self.level())
    }
    /// `-hurryCostForCraftTimeRemaining:totalCraftTime:`: `n = (secondsLeft - 1) / 10 + 1; n < 2 ? 1: n`
    /// (C truncating division; `total_craft_time` unused).
    pub fn hurry_cost_for_craft_time_remaining_total_craft_time(
        &self,
        seconds_left: i32,
        _total_craft_time: i32,
    ) -> i32 {
        let n = seconds_left.wrapping_sub(1) / 10 + 1;
        if n < 2 {
            1
        } else {
            n
        }
    }

    /// The tile above the bench, shared by both solar light functions.
    fn solar_tile(&self) -> Option<crate::world::state::TileRef> {
        let w = self.w()?;
        let p = self.pos();
        tile_at_world_position(p.x, p.y + 1, &w.macro_tiles(), &w)
    }

    /// `-combinedLightForSolarPanelWithFullSunlight` (float32 result).
    pub fn combined_light_for_solar_panel_with_full_sunlight(&self) -> f32 {
        let t = self.solar_tile();
        if !tile_is_air_or_snow(t) {
            return 0.0;
        }
        let tile = t.unwrap().get();
        if tile.contents == 46 {
            return 0.0;
        }
        // `sun_light` is a byte, so `s` cannot exceed 1.0
        let mut s = powf((tile.sun_light as f32) / 255.0f32, 4.0f32);
        let mut a =
            tile.artificial_light_r.wrapping_add(tile.artificial_light_g).wrapping_add(tile.artificial_light_b) as f32;
        if s <= 0.0 {
            s = 0.0;
        }
        if 2000.0f32 < a {
            a = 2000.0;
        }
        let mut c = ((s as f64) + (a as f64) * 0.0001f64) as f32;
        if tile_is_tree(t) {
            c /= 2.0f32;
        }
        c
    }

    /// `-combinedLightForSolarPanel`.
    pub fn combined_light_for_solar_panel(&self) -> f32 {
        // no world, no tile (`solar_tile` needs it too), so the day and weather lookups below always have one
        let Some(w) = self.w() else { return 0.0 };
        let t = self.solar_tile();
        if !tile_is_air_or_snow(t) {
            return 0.0;
        }
        let tile = t.unwrap().get();
        if tile.contents == 46 {
            return 0.0;
        }
        let mut s = powf((tile.sun_light as f32) / 255.0f32, 4.0f32);
        let mut a =
            tile.artificial_light_r.wrapping_add(tile.artificial_light_g).wrapping_add(tile.artificial_light_b) as f32;
        let day = w.get_day_night_fraction_for_x_at_world_time(self.pos().x as f32, WorldApi::world_time(&*w));
        s *= (day - 0.23f32) * 6.0f32;
        if s >= 1.0f32 {
            s = 1.0;
        }
        let wf = w.get_weather_fraction_for_pos(self.pos());
        if wf > 0.0 {
            let w2 = if (wf as f64) < 0.2f64 { wf as f64 } else { 0.2f64 };
            s = ((s as f64) * (1.0f64 - w2 * 4.0f64)) as f32;
        }
        if s <= 0.0 {
            s = 0.0;
        }
        if 2000.0f32 < a {
            a = 2000.0;
        }
        let mut c = ((s as f64) + (a as f64) * 0.0001f64) as f32;
        if tile_is_tree(t) {
            c /= 2.0f32;
        }
        c
    }
}

// =====================================================================================================
// Light handling
// =====================================================================================================

impl Workbench {
    /// `-getLightRGB` (structure checked against the reference).
    pub fn get_light_rgb(&self) -> [f32; 3] {
        let (ty, level, has_fuel) = {
            let s = self.st.borrow();
            (s.ty, s.level, s.has_fuel)
        };
        let (mut r, mut g, mut b) = (0i32, 0i32, 0i32);
        if !workbench_requires_fuel(WorkbenchType(ty as u32), level) {
            if ty == WB_BASIC_PORTAL || ty == WB_PLACED_PORTAL {
                r = 0x99;
                g = 0xbd;
                if level == 4 {
                    r = 255;
                    g = 0;
                    b = 0;
                } else if level == 3 {
                    r = 0;
                    g = 255;
                    b = 0;
                } else {
                    if level == 2 {
                        r = 0;
                        g = 0;
                    } else if level == 1 {
                        r = 0x87;
                        g = 0x73;
                    } else if level == 5 {
                        r = 200;
                        g = 255;
                    }
                    b = 255;
                }
            }
        } else if !has_fuel {
            if ty == WB_CAMPFIRE {
                r = 0x40;
                g = 0x32;
                b = 4;
            }
        } else {
            r = 0xdc;
            g = 0x82;
            b = 0x30;
        }
        [r as f32, g as f32, b as f32]
    }

    /// `-updatePortalLight`: replaces the light at `(pos.x, pos.y - 1)`, heat -2, radius 12.
    pub fn update_portal_light(&self, me: &ObjRef) {
        let old = self.st.borrow_mut().light.take();
        if let Some(l) = old {
            l.remove_from_tiles();
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
        let rgb = self.get_light_rgb();
        let p = self.pos();
        let l = self.new_light(me, IntPair { x: p.x, y: p.y - 1 }, rgb[0] as i32, rgb[1] as i32, rgb[2] as i32, -2, 12);
        self.st.borrow_mut().light = l;
    }

    /// `-updateHasFuel` (float32 compares; the removal uses `removeFromMacroBlock` only).
    pub fn update_has_fuel(&self, me: &ObjRef) {
        let (has_fuel, ff) = {
            let s = self.st.borrow();
            (s.has_fuel, s.fuel_fraction)
        };
        let p = self.pos();
        if !has_fuel || ff >= 0.0001f32 {
            if !has_fuel && ff > 0.0 {
                let old = self.st.borrow_mut().light.take();
                if let Some(l) = old {
                    l.remove_from_macro_block_light();
                    l.dealloc_light();
                }
                let l = self.new_light(me, p, 0xdc, 0x82, 0x30, 4, 10);
                {
                    let mut s = self.st.borrow_mut();
                    s.light = l;
                    s.has_fuel = true;
                }
                self.notify();
            }
        } else {
            let (req, ty) = {
                let mut s = self.st.borrow_mut();
                s.fuel_fraction = 0.0;
                s.fuel_counter = 0.0;
                s.has_fuel = false;
                (s.requires_fuel, s.ty)
            };
            if req && !me.is_net() {
                let old = self.st.borrow_mut().light.take();
                if let Some(l) = old {
                    l.remove_from_macro_block_light();
                    l.dealloc_light();
                }
                if ty == WB_CAMPFIRE {
                    let l = self.new_light(me, p, 0x40, 0x32, 4, 4, 12);
                    self.st.borrow_mut().light = l;
                }
            }
            self.notify();
        }
    }
}

// =====================================================================================================
// Save / net data
// =====================================================================================================

impl Workbench {
    /// `-getSaveDict`.
    pub fn get_save_dict_impl(&self, me: &ObjRef) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        let in_use = self.io.borrow().is_in_use;
        let (cur_fuel, cio, light) = {
            let s = self.st.borrow();
            (s.current_fuel_blockhead.clone(), s.crafting_item_object.clone(), s.light.clone())
        };
        {
            let s = self.st.borrow();
            let m = &mut d.map;
            m.insert("workbenchType".into(), PlistValue::int(s.ty as i64));
            m.insert("selectedIndex".into(), PlistValue::int(s.selected_index as i64));
            m.insert("xScroll".into(), PlistValue::real_f32(s.x_scroll));
            m.insert("level".into(), PlistValue::int(s.level as i64));
            m.insert("craftProgressCount".into(), PlistValue::real_f32(s.craft_progress_count));
            m.insert("hurryTimer".into(), PlistValue::real_f32(s.hurry_timer));
            m.insert("hurrySeconds".into(), PlistValue::real_f32(s.hurry_seconds));
            m.insert("hurrying".into(), PlistValue::boolean(s.hurrying));
            m.insert("hurryCost".into(), PlistValue::int(s.hurry_cost as i64));
            m.insert("availableElectricity".into(), PlistValue::int(s.available_electricity as i64));
            m.insert("fireSpreadTimer".into(), PlistValue::real_f32(s.fire_spread_timer));
            m.insert("fuelFraction".into(), PlistValue::real_f32(s.fuel_fraction));
            m.insert("hasFuel".into(), PlistValue::boolean(s.has_fuel));
            m.insert("lastWorldTime".into(), PlistValue::real_f64(s.last_world_time));
        }
        if let Some(cf) = cur_fuel {
            let bl = self.dw().map_or_else(Vec::new, |d| d.blockheads());
            if let Some(i) = bl.iter().position(|b| ptr_eq(b, &cf)) {
                d.map.insert("currentBlockheadIndexFuel".into(), PlistValue::int(i as i64));
            }
        }
        if in_use {
            if let Some(cio) = cio {
                let cd = cio.get_save_dict();
                d.map.insert("craftingItemDatav2".into(), PlistValue::Dict(cd));
                let ci = cio.craftable_item();
                let s = self.st.borrow();
                d.map.insert("count".into(), PlistValue::int(s.count as i64));
                d.map.insert("countLeft".into(), PlistValue::int(s.count_left as i64));
                d.map.insert("countCreated".into(), PlistValue::int(s.count_created as i64));
                let n = ci.number_of_required_items.clamp(0, 8) as usize;
                for i in 0..n {
                    if ci.required_items[i] != 11 {
                        if let Some(v) = &s.source_items[i] {
                            if !v.is_empty() {
                                let arr: Vec<PlistValue> =
                                    v.iter().map(|it| PlistValue::data(it.borrow().save_data())).collect();
                                d.map.insert(source_items_key(i), PlistValue::array(arr));
                            }
                        }
                    }
                }
            }
        }
        if let Some(l) = light {
            let lo: ObjRef = l.clone();
            let ld = DynamicObject::get_save_dict(&*l, &lo);
            d.map.insert("lightDict".into(), PlistValue::Dict(ld));
        }
        d
    }

    /// `-updateNetDataForClient:`: the 0x38-byte struct, then `gzip(plist{ownerID?, ownerName?})`.
    pub fn update_net_data_for_client_impl(&self, me: &ObjRef) -> Vec<u8> {
        let io_nd = interaction_object_interaction_object_creation_net_data(me);
        let (ty, level, ff, has_fuel, in_use_fuel, avail, cur_fuel, remote_fuel) = {
            let s = self.st.borrow();
            (
                s.ty,
                s.level,
                s.fuel_fraction,
                s.has_fuel,
                s.is_in_use_fuel,
                s.available_electricity,
                s.current_fuel_blockhead.clone(),
                s.remote_fuel_blockhead_in_use_unique_id,
            )
        };
        let uid = if in_use_fuel {
            match cur_fuel {
                Some(c) => c.unique_id(),
                None => remote_fuel,
            }
        } else {
            0
        };
        let nd = WorkbenchNetData {
            interaction_object_creation_net_data: io_nd,
            is_in_use_fuel_blockhead_unique_id: uid,
            fuel_fraction: f32_to_i32(ff * 255.0f32) as i16,
            type_: ty as u8,
            level: level as u8,
            has_fuel: has_fuel as u8,
            is_in_use_fuel: in_use_fuel as u8,
            available_electricity: avail,
        };
        let mut data = nd.to_vec();
        let mut pd = new_mutable_dict();
        if let Some(o) = me.owner_id() {
            pd.map.insert("ownerID".into(), PlistValue::string(o));
        }
        if let Some(n) = self.io.borrow().owner_name.clone() {
            pd.map.insert("ownerName".into(), PlistValue::string(n));
        }
        let plist = data_from_property_list(&PlistValue::Dict(pd)).unwrap_or_default();
        data.extend_from_slice(&gzip_deflate(&plist));
        data
    }
}

// =====================================================================================================
// Level upgrade, remote update, electricity, fuel
// =====================================================================================================

impl Workbench {
    /// Portal base-stone write shared by `upgradeToNextLevel` and `remoteUpdate:` (level 1..5 only), then
    /// `updatePortalLight` in every case. Returns after the light update.
    fn portal_stone_and_light(&self, me: &ObjRef, report_achievement: bool) {
        let (ty, level) = {
            let s = self.st.borrow();
            (s.ty, s.level)
        };
        if ty != WB_BASIC_PORTAL && ty != WB_PLACED_PORTAL {
            return;
        }
        let world = self.w();
        let p = self.pos();
        let stone = world.as_ref().and_then(|w| tile_at_world_position_loaded(p.x, p.y - 1, w));
        // a stone tile implies a world
        if let (Some(t), Some(w)) = (stone, &world) {
            if (1..=5).contains(&level) {
                let idx = if ty == WB_BASIC_PORTAL { 0x20 + level } else { 0x2a + level };
                t.update(|t| t.retype(idx as u8));
                if report_achievement {
                    let cur = self.io.borrow().current_blockhead.clone();
                    let controlled = match &cur {
                        Some(c) => crate::world::misc::deps().bh_is_client_blockhead_being_controlled_by_server(c),
                        None => false,
                    };
                    if !controlled {
                        let id = match level {
                            1 => "grp.amethyst_portal",
                            2 => "grp.sapphire_portal",
                            3 => "grp.emerald_portal",
                            4 => "grp.ruby_portal",
                            _ => "grp.diamond_portal",
                        };
                        w.report_achievement_with_identifier(id);
                    }
                }
            }
        }
        self.update_portal_light(me);
    }

    /// `-upgradeToNextLevel` (56 `rand()` draws).
    pub fn upgrade_to_next_level(&self, me: &ObjRef) {
        {
            let mut s = self.st.borrow_mut();
            s.level = s.level.wrapping_add(1);
        }
        self.init_level_stuff();
        let (ty, level) = {
            let s = self.st.borrow();
            (WorkbenchType(s.ty as u32), s.level)
        };
        if let Some(w) = self.w() {
            w.update_tile_graphic_for_workbench_of_type_at_pos_level(ty, self.pos(), level);
        }
        self.notify();
        me.set_update_needs_to_be_sent(true);
        self.st.borrow_mut().selected_index = 0;
        particle_burst_rands(8, self.pos());
        // Flurry "Workbench_Upgraded": no server effect (the name lookup has no side effect either).
        self.portal_stone_and_light(me, true);
    }

    /// `-setLevelSilently:`.
    pub fn set_level_silently(&self, _me: &ObjRef, level: i32) {
        self.st.borrow_mut().level = level;
        self.init_level_stuff();
        let ty = self.wt();
        if let Some(w) = self.w() {
            w.update_tile_graphic_for_workbench_of_type_at_pos_level(ty, self.pos(), self.level());
        }
        self.notify();
    }

    /// `-remoteUpdate:`.
    pub fn remote_update_impl(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        self.notify();
        if net_data.len() < 0x38 {
            range_exception_beyond_size("getBytes:length:", 0, 0x38, net_data.len() as u64);
        }
        let nd = <WorkbenchNetData as WireStruct>::read_le(&padded_bytes(net_data, 0x38));
        let dw = self.dw();
        let is_server = dw.as_ref().is_some_and(|d| d.is_server());
        let h = host();
        // ---- 4. in-use (crafting) state
        {
            let was_in_use = self.io.borrow().is_in_use;
            self.io.borrow_mut().is_in_use = nd.interaction_object_creation_net_data.is_in_use != 0;
            let (in_use, cur) = {
                let io = self.io.borrow();
                (io.is_in_use, io.current_blockhead.clone())
            };
            if in_use && was_in_use {
                if let Some(c) = &cur {
                    let id = nd.interaction_object_creation_net_data.is_in_use_blockhead_unique_id;
                    if c.unique_id() != id && !is_server {
                        if let Some(w) = self.w() {
                            w.craft_aborted_for_workbench_with_blockhead(Some(me), Some(c));
                        }
                        crate::world::misc::deps().bh_stop_interacting(c);
                        self.io.borrow_mut().current_blockhead = None;
                    }
                }
            }
            let (in_use, cur_none) = {
                let io = self.io.borrow();
                (io.is_in_use, io.current_blockhead.is_none())
            };
            if in_use && cur_none {
                let id = nd.interaction_object_creation_net_data.is_in_use_blockhead_unique_id;
                self.io.borrow_mut().remote_blockhead_in_use_unique_id = id;
                if id == 0 {
                    self.io.borrow_mut().is_in_use = false;
                    me.set_update_needs_to_be_sent(true);
                } else {
                    let bl = dw.as_ref().map_or_else(Vec::new, |d| d.blockheads());
                    for b in bl {
                        // literal: the scan has no break and a MATCH clears the remote state
                        let remote = self.io.borrow().remote_blockhead_in_use_unique_id;
                        if b.unique_id() == remote {
                            {
                                let mut io = self.io.borrow_mut();
                                io.remote_blockhead_in_use_unique_id = 0;
                                io.is_in_use = false;
                            }
                            me.set_update_needs_to_be_sent(true);
                        }
                    }
                }
            }
        }
        // ---- 5. fuel in-use state
        {
            let was = self.st.borrow().is_in_use_fuel;
            self.st.borrow_mut().is_in_use_fuel = nd.is_in_use_fuel != 0;
            let (now, curf) = {
                let s = self.st.borrow();
                (s.is_in_use_fuel, s.current_fuel_blockhead.clone())
            };
            if now && was {
                if let Some(c) = &curf {
                    if c.unique_id() != nd.is_in_use_fuel_blockhead_unique_id && !is_server {
                        crate::world::misc::deps().bh_stop_interacting(c);
                        let mut s = self.st.borrow_mut();
                        s.current_fuel_blockhead = None;
                        s.is_in_use_fuel = true;
                    }
                }
            }
            let (now, none) = {
                let s = self.st.borrow();
                (s.is_in_use_fuel, s.current_fuel_blockhead.is_none())
            };
            if now && none {
                self.st.borrow_mut().remote_fuel_blockhead_in_use_unique_id = nd.is_in_use_fuel_blockhead_unique_id;
            }
        }
        // ---- 6. fuel fraction
        {
            let new_fuel = (nd.fuel_fraction as i32 as f32) / 255.0f32;
            let cur = self.st.borrow().fuel_fraction;
            let eps = 0.01f32;
            let tiny = 0.001f32;
            let changed = (cur + eps < new_fuel)
                || (new_fuel < cur - eps)
                || (nd.fuel_fraction == 0 && tiny < cur)
                || (nd.fuel_fraction > 0 && cur < tiny);
            if changed {
                let mut ignore = false;
                if me.is_net() {
                    let (in_use, cb) = {
                        let io = self.io.borrow();
                        (io.is_in_use, io.current_blockhead.clone())
                    };
                    if in_use && !cb.as_ref().is_some_and(|c| c.is_net()) && new_fuel < cur {
                        ignore = true;
                    }
                    let (in_use_fuel, cf) = {
                        let s = self.st.borrow();
                        (s.is_in_use_fuel, s.current_fuel_blockhead.clone())
                    };
                    if !in_use && in_use_fuel && !cf.as_ref().is_some_and(|c| c.is_net()) {
                        ignore = true;
                    }
                }
                if !ignore {
                    if self.st.borrow().ty == WB_STEAM_GENERATOR && new_fuel < cur {
                        let mut t = (cur - new_fuel) * 500.0f32;
                        if t >= 4.0f32 {
                            t = 4.0;
                        }
                        self.st.borrow_mut().particle_create_timer_steam_engine = t;
                    }
                    self.st.borrow_mut().fuel_fraction = new_fuel;
                    self.update_has_fuel(me);
                    if !me.is_net() {
                        me.set_update_needs_to_be_sent(true);
                    }
                }
            }
        }
        // ---- 7. electricity (unconditional)
        self.st.borrow_mut().available_electricity = nd.available_electricity;
        // ---- 8. level change
        if self.st.borrow().level < nd.level as i32 {
            self.st.borrow_mut().level = nd.level as i32;
            self.init_level_stuff();
            if is_server {
                let (ty, level) = {
                    let s = self.st.borrow();
                    (WorkbenchType(s.ty as u32), s.level)
                };
                if let Some(w) = self.w() {
                    w.update_tile_graphic_for_workbench_of_type_at_pos_level(ty, self.pos(), level);
                }
            }
            self.notify();
            particle_burst_rands(8, self.pos());
            self.portal_stone_and_light(me, false);
        }
        // ---- 9. owner data
        if self.io.borrow().current_blockhead.is_none() {
            let (oid, oname) = Self::owner_from_tail(&net_data[0x38..]);
            me.set_owner_id(oid);
            self.io.borrow_mut().owner_name = oname;
        }
        let _ = h;
    }

    /// `-subtractElectricty:` (sic).
    pub fn subtract_electricty(&self, me: &ObjRef, amount: u16) -> bool {
        let success = {
            let s = self.st.borrow();
            amount <= s.available_electricity
        };
        if success {
            {
                let mut s = self.st.borrow_mut();
                s.available_electricity = s.available_electricity.wrapping_sub(amount);
            }
            me.set_update_needs_to_be_sent(true);
            self.notify();
        }
        let steam = {
            let s = self.st.borrow();
            s.ty == WB_STEAM_GENERATOR && s.fuel_fraction > 0.0 && s.available_electricity < 100
        };
        if steam {
            {
                let mut s = self.st.borrow_mut();
                while s.fuel_fraction > 0.0 && s.available_electricity < 100 {
                    s.fuel_fraction -= 0.000583f32;
                    s.available_electricity = s.available_electricity.wrapping_add(10);
                    s.particle_create_timer_steam_engine += 0.1f32;
                }
                if s.particle_create_timer_steam_engine > 4.0f32 {
                    s.particle_create_timer_steam_engine = 4.0;
                }
            }
            self.notify();
            me.set_update_needs_to_be_sent(true);
            self.update_has_fuel(me);
        }
        success
    }

    /// `-addToFuel:`.
    pub fn add_to_fuel(&self, me: &ObjRef, fuel_to_add: i32) {
        let req = self.st.borrow().requires_fuel;
        if !req {
            return;
        }
        {
            let mut s = self.st.borrow_mut();
            s.fuel_fraction += 0.1f32 * (fuel_to_add as f32);
            if s.fuel_fraction > 1.0f32 {
                s.fuel_fraction = 1.0;
            }
            if s.ty == WB_STEAM_GENERATOR && s.fuel_fraction > 0.0 && s.available_electricity < 100 {
                while s.fuel_fraction > 0.0 && s.available_electricity < 100 {
                    s.fuel_fraction -= 0.0005f32;
                    s.available_electricity = s.available_electricity.wrapping_add(1);
                }
            }
        }
        self.update_has_fuel(me);
        me.set_update_needs_to_be_sent(true);
        self.notify();
    }

    /// `-addToFuelForItem:`
    pub fn add_to_fuel_for_item(&self, me: &ObjRef, item: ItemType) {
        let n = fuel_to_add_for_item_and_workbench_and_level(item, self.wt(), self.level());
        self.add_to_fuel(me, n);
    }

    /// `-setPaused:`
    pub fn set_paused(&self, paused: bool) {
        self.st.borrow_mut().paused = paused;
    }

    /// `-remove:`.
    pub fn remove(&self, me: &ObjRef, remove_blockhead: Option<&ObjRef>) {
        let (ty, level) = {
            let s = self.st.borrow();
            (s.ty, s.level)
        };
        if ty == WB_BASIC_PORTAL || me.needs_removed() || me.is_net() {
            return;
        }
        if self.io.borrow().is_in_use {
            let cb = self.io.borrow().current_blockhead.clone();
            let remote = self.io.borrow().remote_blockhead_in_use_unique_id;
            if cb.as_ref().is_some_and(|c| c.is_net()) || remote != 0 {
                self.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
                return;
            }
            self.abort_craft(me);
        }
        if self.st.borrow().is_in_use_fuel {
            let mut s = self.st.borrow_mut();
            s.is_in_use_fuel = false;
            s.current_fuel_blockhead = None;
        }
        let wt = WorkbenchType(ty as u32);
        let p = self.pos();
        let sd = self.get_save_dict_impl(me);
        crate::world::misc::deps().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            p,
            item_type_for_workbench_type_and_level(wt, level),
            0,
            0,
            None,
            Some(&PlistValue::Dict(sd)),
            false,
            2,
            remove_blockhead,
        );
        if let Some(w) = self.w() {
            let rm = |x: i32, y: i32| {
                w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(x, y, 0, 0, remove_blockhead)
            };
            rm(p.x, p.y);
            if ty == WB_PLACED_PORTAL {
                rm(p.x, p.y - 1);
                rm(p.x, p.y + 1);
            } else if ty == WB_EASEL || ty == WB_REFINERY {
                rm(p.x, p.y + 1);
            }
        }
        DynamicObject::set_needs_removed(self, me, true);
    }

    /// `-worldChanged:` (support loss).
    pub fn world_changed_impl(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if me.is_net() {
            return;
        }
        let light = self.st.borrow().light.clone();
        if let Some(l) = light {
            l.world_changed_positions(positions);
        }
        let ty = self.st.borrow().ty;
        if ty == WB_BASIC_PORTAL {
            return;
        }
        let Some(world) = self.w() else { return };
        let width = world.world_width_macro().wrapping_mul(32);
        let half_neg = world.world_width_macro().wrapping_mul(-32) / 2;
        let pos = self.pos();
        for p in positions.iter() {
            let mut d = pos.x.wrapping_sub(p.x);
            if d >= width / 2 {
                d = d.wrapping_sub(width);
            } else if d < half_neg {
                d = d.wrapping_add(width);
            }
            if d.wrapping_abs() < 2 && (p.y == pos.y - 1 || pos.y <= p.y) {
                let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                if below.is_some() && !tile_is_solid(below) {
                    if ty == WB_CAMPFIRE {
                        self.remove(me, None);
                        return;
                    }
                    if !tile_is_half_depth(below, &world, IntPair { x: pos.x, y: pos.y - 1 })
                        && below.unwrap().get().contents != 96
                    {
                        let here = tile_at_world_position_loaded(pos.x, pos.y, &world);
                        // the bench's own tile is always loaded
                        if here.is_some_and(|h| h.get().back_wall_type_index == 2) {
                            let mut blocked = false;
                            for n in [
                                IntPair { x: pos.x - 1, y: pos.y },
                                IntPair { x: pos.x + 1, y: pos.y },
                                IntPair { x: pos.x, y: pos.y + 1 },
                            ] {
                                let tn = tile_at_world_position_loaded(n.x, n.y, &world);
                                match tn {
                                    None => {
                                        // UB-PARITY: the original dereferences the NULL tile.
                                        ub_null_tile_deref();
                                        blocked = true;
                                        break;
                                    }
                                    Some(t) => {
                                        if tile_is_half_depth(tn, &world, n)
                                            || t.get().contents == 96
                                            || tile_is_solid(tn)
                                        {
                                            blocked = true;
                                            break;
                                        }
                                    }
                                }
                            }
                            if !blocked {
                                self.remove(me, None);
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    /// `-setNeedsRemoved:` (type 1: nothing at all).
    pub fn set_needs_removed_impl(&self, _me: &ObjRef, v: bool) {
        if self.st.borrow().ty != WB_BASIC_PORTAL {
            self.base.borrow_mut().needs_removed = v;
            if v {
                let l = self.st.borrow().light.clone();
                if let Some(l) = l {
                    l.remove_from_tiles();
                }
            }
        }
    }

    /// `-removeFromMacroBlock`.
    pub fn remove_from_macro_block_impl(&self, me: &ObjRef) {
        let old = self.st.borrow_mut().light.take();
        if let Some(l) = old {
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
        let ty = self.st.borrow().ty;
        if ty == WB_BASIC_PORTAL || ty == WB_PLACED_PORTAL {
            host().portal_is_being_removed_at_pos(self.pos());
        }
        base_remove_from_macro_block(me);
    }

    /// `-blockheadsLoaded`.
    pub fn blockheads_loaded_impl(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me);
        let h = host();
        let saved = self.st.borrow().saved_blockhead_index_fuel;
        if saved != -1 {
            let bl = self.dw().map_or_else(Vec::new, |d| d.blockheads());
            if saved >= 0 && (saved as usize) < bl.len() {
                self.st.borrow_mut().current_fuel_blockhead = Some(bl[saved as usize].clone());
            }
            self.st.borrow_mut().saved_blockhead_index_fuel = -1;
            if self.st.borrow().is_in_use_fuel {
                // sic: uses currentBlockhead (+0x50) with setInteractionObject:
                let cb = self.io.borrow().current_blockhead.clone();
                if let Some(c) = cb {
                    h.bh_set_interaction_object(&c, Some(me));
                }
            }
        }
        if self.io.borrow().is_in_use {
            let cb = self.io.borrow().current_blockhead.clone();
            if let Some(c) = cb {
                h.bh_set_interaction_workbench(&c, Some(me));
            }
        }
        if self.st.borrow().is_in_use_fuel {
            let cf = self.st.borrow().current_fuel_blockhead.clone();
            if let Some(c) = cf {
                h.bh_set_interaction_workbench(&c, Some(me));
            }
        }
    }

    /// `-blockheadWouldLikeToTakeOwnership:withSaveDict:` (the whole body is inside
    /// `if currentBlockhead == nil`).
    pub fn blockhead_would_like_to_take_ownership_with_save_dict_impl(
        &self,
        me: &ObjRef,
        blockhead: &ObjRef,
        save_dict: Option<&PlistDict>,
    ) {
        if self.io.borrow().current_blockhead.is_some() {
            return;
        }
        let empty = new_mutable_dict();
        let sd = save_dict.unwrap_or(&empty);
        self.io.borrow_mut().current_blockhead = Some(blockhead.clone());
        {
            let mut s = self.st.borrow_mut();
            s.craft_progress_count = get_f32(sd, "craftProgressCount");
            s.hurry_timer = get_f32(sd, "hurryTimer");
            s.hurry_seconds = get_f32(sd, "hurrySeconds");
            s.hurrying = get_bool(sd, "hurrying");
            s.hurry_cost = get_i32(sd, "hurryCost");
        }
        self.io.borrow_mut().is_in_use = true;
        self.st.borrow_mut().crafting_item_object = None;
        self.restore_crafting_state(sd);
        host().bh_set_interaction_workbench(blockhead, Some(me));
        me.set_update_needs_to_be_sent(true);
        self.notify();
    }

    /// `-blockheadUnloaded:`.
    pub fn blockhead_unloaded_impl(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b);
        let is_fuel = opt_ptr_eq(&self.st.borrow().current_fuel_blockhead, b);
        if is_fuel {
            let mut s = self.st.borrow_mut();
            s.current_fuel_blockhead = None;
            s.is_in_use_fuel = false;
        }
    }

    /// `-remoteBlockheadRemovedWithID:`.
    pub fn remote_blockhead_removed_with_id_impl(&self, me: &ObjRef, id: u64) {
        interaction_object_remote_blockhead_removed_with_id(me, id);
        let mut s = self.st.borrow_mut();
        if id == s.remote_fuel_blockhead_in_use_unique_id {
            s.remote_fuel_blockhead_in_use_unique_id = 0;
            s.is_in_use_fuel = false;
        }
    }

    /// `-requiresPhysicalBlock`.
    pub fn requires_physical_block_impl(&self) -> bool {
        let is_net = self.base.borrow().is_net;
        let (in_use_fuel, ty, avail, since) = {
            let s = self.st.borrow();
            (s.is_in_use_fuel, s.ty, s.available_electricity, s.time_since_found_electricty)
        };
        let in_use = self.io.borrow().is_in_use;
        if is_net {
            if in_use_fuel {
                return true;
            }
            return in_use;
        }
        // (`InteractionObject`'s own `requiresPhysicalBlock` is a constant NO)
        if in_use_fuel || in_use {
            return true;
        }
        if ty == WB_SOLAR_PANEL && avail < 100 {
            let light = self.combined_light_for_solar_panel_with_full_sunlight();
            if light > 0.01f32 {
                return true;
            }
        }
        if ty == WB_FLYWHEEL && avail < 0x2000 {
            return since < 20.0f32;
        }
        false
    }
}

// =====================================================================================================
// Trait impls
// =====================================================================================================

impl DynamicObject for Workbench {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &WORKBENCH_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    /// `-objectType` = 45.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_WORKBENCH
    }
    fn requires_physical_block(&self) -> bool {
        self.requires_physical_block_impl()
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.destroy_item_type().0 as i32
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict_impl(me))
    }
    fn free_block_creation_data_a(&self) -> u16 {
        0
    }
    fn free_block_creation_data_b(&self) -> u16 {
        0
    }
    fn get_light_rgb(&self) -> [f32; 3] {
        Workbench::get_light_rgb(self)
    }
    fn remove_from_macro_block(&self, me: &ObjRef) {
        self.remove_from_macro_block_impl(me);
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        self.update_accurate_dt_is_simulation_impl(me, dt, accurate_dt, is_simulation);
    }
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        self.set_needs_removed_impl(me, v);
    }
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        self.world_changed_impl(me, positions);
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        self.get_save_dict_impl(me)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        // InteractionObject's creationNetDataForClient: sends the virtual updateNetDataForClient:.
        Some(self.update_net_data_for_client_impl(me))
    }
    fn update_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        Some(self.update_net_data_for_client_impl(me))
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        self.remote_update_impl(me, net_data);
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        self.blockhead_unloaded_impl(me, b);
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        self.blockheads_loaded_impl(me);
    }
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        let l = self.st.borrow().light.clone();
        if let Some(l) = l {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(macro_x, macro_y);
        }
    }
    fn dealloc(&self, me: &ObjRef) {
        Workbench::dealloc(self, me);
    }
    fn as_power_user(&self) -> Option<&dyn PowerUser> {
        Some(self)
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
}

impl InteractionObjectLike for Workbench {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_WORKBENCH
    }
    fn is_double_height(&self) -> bool {
        Workbench::is_double_height(self)
    }
    fn destroy_item_type(&self) -> i32 {
        Workbench::destroy_item_type(self).0 as i32
    }
    fn title(&self) -> String {
        Workbench::title(self)
    }
    fn requires_human_interaction(&self) -> bool {
        Workbench::requires_human_interaction(self)
    }
    fn can_be_used_in_expert_mode_when_not_owned(&self) -> bool {
        self.st.borrow().ty == WB_BASIC_PORTAL
    }
    fn remove(&self, me: &ObjRef, blockhead: Option<&ObjRef>) {
        Workbench::remove(self, me, blockhead);
    }
    fn set_paused(&self, _me: &ObjRef, paused: bool) {
        Workbench::set_paused(self, paused);
    }
    fn blockhead_would_like_to_take_ownership_with_save_dict(
        &self,
        me: &ObjRef,
        b: &ObjRef,
        save_dict: Option<&PlistDict>,
    ) {
        self.blockhead_would_like_to_take_ownership_with_save_dict_impl(me, b, save_dict);
    }
    fn remote_blockhead_removed_with_id(&self, me: &ObjRef, id: u64) {
        self.remote_blockhead_removed_with_id_impl(me, id);
    }
}

impl PowerUser for Workbench {
    fn is_storage_device(&self) -> bool {
        Workbench::is_storage_device(self)
    }
}

impl WirePowerSource for Workbench {
    fn available_electricity(&self) -> u16 {
        Workbench::available_electricity(self)
    }
    fn generates_electricity(&self) -> bool {
        Workbench::generates_electricity(self)
    }
    fn is_storage_device(&self) -> bool {
        Workbench::is_storage_device(self)
    }
    fn conducts_electricity(&self) -> bool {
        Workbench::conducts_electricity(self)
    }
    fn subtract_electricty(&self, amount: u16) -> bool {
        let me = DynamicObject::me(self);
        Workbench::subtract_electricty(self, &me, amount)
    }
}

// =====================================================================================================
// Test kit and unit tests
// =====================================================================================================

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;
    use crate::items::inventory_item::SubSlot;
    use crate::lighting::artificial_light::testkit::world_with_blocks;
    use crate::objects::base::DynamicWorldApi;
    use crate::objects::interaction_object::testutil::TestBh;
    use crate::world::misc::install_deps;
    use std::cell::Cell;

    /// `DynamicWorldApi` double with scriptable blockhead list.
    pub struct MockDw {
        pub next_id: Cell<u64>,
        pub changed: RefCell<Vec<(IntPair, i32)>>,
        pub blockheads: RefCell<Vec<ObjRef>>,
        pub server: Cell<bool>,
    }
    impl DynamicWorldApi for MockDw {
        fn get_next_dynamic_object_id(&self) -> u64 {
            self.next_id.set(self.next_id.get() + 1);
            self.next_id.get()
        }
        fn dynamic_world_changed_at_pos_object_type(&self, pos: IntPair, t: i32) {
            self.changed.borrow_mut().push((pos, t));
        }
        fn is_server(&self) -> bool {
            self.server.get()
        }
        fn blockheads(&self) -> Vec<ObjRef> {
            self.blockheads.borrow().clone()
        }
        fn get_owner_name_for_object_owner_id(&self, _id: &str) -> Option<String> {
            None
        }
    }

    /// `WorldOpsDeps` recorder.
    #[derive(Default)]
    pub struct Deps {
        pub log: RefCell<Vec<String>>,
        pub controlled: Cell<bool>,
    }
    impl crate::world::misc::WorldOpsDepsStub for Deps {
        fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            &self,
            pos: IntPair,
            item_type: ItemType,
            data_a: u16,
            data_b: u16,
            _sub_items: Option<&[SubSlot]>,
            dict: Option<&PlistValue>,
            hovers: bool,
            play_sound: i32,
            _priority: Option<&ObjRef>,
        ) {
            self.log.borrow_mut().push(format!(
                "free({},{},{},{},{},{},{},{})",
                pos.x,
                pos.y,
                item_type.0,
                data_a,
                data_b,
                hovers,
                play_sound,
                dict.is_some()
            ));
        }
        fn place_fire_at_position(&self, pos: IntPair) {
            self.log.borrow_mut().push(format!("fire({},{})", pos.x, pos.y));
        }
        fn bh_is_client_blockhead_being_controlled_by_server(&self, _bh: &ObjRef) -> bool {
            self.controlled.get()
        }
        fn bh_add_to_crystal_discrepency(&self, _bh: &ObjRef, n: i32) {
            self.log.borrow_mut().push(format!("discrepency({})", n));
        }
        fn workbench_pos(&self, wb: &ObjRef) -> IntPair {
            wb.pos()
        }
        fn workbench_type(&self, wb: &ObjRef) -> WorkbenchType {
            wb.as_any().downcast_ref::<Workbench>().map_or(WorkbenchType(0), |w| w.workbench_type())
        }
        fn workbench_level(&self, wb: &ObjRef) -> i32 {
            wb.as_any().downcast_ref::<Workbench>().map_or(0, |w| w.level())
        }
        fn workbench_upgrade_to_next_level(&self, wb: &ObjRef) {
            if let Some(w) = wb.as_any().downcast_ref::<Workbench>() {
                w.upgrade_to_next_level(wb);
            }
        }
        fn craftable_item(&self, obj: &ObjRef) -> CraftableItem {
            crate::crafting::workbench_craft::craftable_item_object_of_shim(obj)
                .map_or_else(crate::world::misc::zero_craftable_item, |c| c.craftable_item())
        }
        fn particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(
            &self,
            _p: [f32; 3],
            _v: [f32; 3],
            _c: [f32; 4],
            _g: i32,
            _l: f32,
            _s: f32,
        ) {
            self.log.borrow_mut().push("particle".to_string());
        }
    }

    /// `WorkbenchHost` double.
    #[derive(Default)]
    pub struct Host {
        pub log: RefCell<Vec<String>>,
        pub paused: Cell<bool>,
        pub unable: Cell<i32>,
        pub crafting_any: Cell<bool>,
        pub iw: RefCell<Option<ObjRef>>,
        pub stock: RefCell<Vec<(u32, i32)>>,
        pub power: Cell<i32>,
        pub should_continue: Cell<bool>,
        pub double_time: Cell<bool>,
        pub coffee: Cell<bool>,
        pub adding_fuel: Cell<bool>,
    }
    impl WorkbenchHostStub for Host {
        fn portal_is_being_removed_at_pos(&self, pos: IntPair) {
            self.log.borrow_mut().push(format!("portalRemoved({},{})", pos.x, pos.y));
        }
        fn find_and_subtract_all_power_up_to_for_user(&self, required: u16, _user: &ObjRef) -> i32 {
            self.log.borrow_mut().push(format!("findPower({})", required));
            self.power.get().min(required as i32)
        }
        fn bh_paused(&self, _bh: &ObjRef) -> bool {
            self.paused.get()
        }
        fn bh_unable_to_work_reason(&self, _bh: &ObjRef) -> i32 {
            self.unable.get()
        }
        fn bh_has_coffee_energy(&self, _bh: &ObjRef) -> bool {
            self.coffee.get()
        }
        fn bh_is_adding_fuel_to_any_workbench(&self, _bh: &ObjRef) -> bool {
            self.adding_fuel.get()
        }
        fn bh_should_continue_simulating(&self, _bh: &ObjRef) -> bool {
            self.should_continue.get()
        }
        fn bh_is_crafting_at_any_workbench(&self, _bh: &ObjRef) -> bool {
            self.crafting_any.get()
        }
        fn bh_interaction_workbench(&self, _bh: &ObjRef) -> Option<ObjRef> {
            self.iw.borrow().clone()
        }
        fn bh_set_interaction_workbench(&self, _bh: &ObjRef, wb: Option<&ObjRef>) {
            self.log.borrow_mut().push(format!("setIW({})", wb.is_some()));
            *self.iw.borrow_mut() = wb.cloned();
        }
        fn bh_double_time_unlocked(&self, _bh: &ObjRef) -> bool {
            self.double_time.get()
        }
        fn bh_subtract_items_from_inventory_of_type_count_data_b(
            &self,
            _bh: &ObjRef,
            item_type: ItemType,
            count: i32,
            _data_b: i32,
        ) -> Vec<ItemRef> {
            let mut st = self.stock.borrow_mut();
            let mut out = Vec::new();
            if let Some(e) = st.iter_mut().find(|e| e.0 == item_type.0) {
                let take = e.1.min(count).max(0);
                e.1 -= take;
                for _ in 0..take {
                    out.push(InventoryItem::new(item_type, 0, 0, None, None));
                }
            }
            out
        }
        fn bh_add_item_to_inventory(&self, _bh: &ObjRef, item: &ItemRef) {
            self.log.borrow_mut().push(format!("addItem({})", item.borrow().item_type.0));
        }
        fn bh_craft_item_finished_at_workbench(&self, _bh: &ObjRef, completed: bool, _wb: &ObjRef) {
            self.log.borrow_mut().push(format!("craftItemFinished({})", completed));
        }
    }

    pub struct Fixture {
        pub world: Rc<World>,
        pub dw: Rc<MockDw>,
        pub dwr: DynamicWorldRef,
        pub deps: Rc<Deps>,
        pub host: Rc<Host>,
    }

    /// World of 4 macro tiles width with macro block (0,0) loaded; installs the deps / host doubles.
    pub fn fixture() -> Fixture {
        let world = world_with_blocks(4, &[(0, 0)]);
        let dw = Rc::new(MockDw {
            next_id: Cell::new(100),
            changed: RefCell::new(vec![]),
            blockheads: RefCell::new(vec![]),
            server: Cell::new(true),
        });
        let dwr: DynamicWorldRef = dw.clone();
        let deps = Rc::new(Deps::default());
        install_deps(deps.clone());
        let host = Rc::new(Host::default());
        host.should_continue.set(true);
        install_host(host.clone());
        crate::rng::glibc_rand::srand(1);
        crate::crafting::workbench_craft::reset_crystal_wallet();
        Fixture { world, dw, dwr, deps, host }
    }

    pub fn make(f: &Fixture, ty: u32, x: i32, y: i32) -> (Rc<Workbench>, ObjRef) {
        let wb = Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
            &f.world,
            &f.dwr,
            IntPair { x, y },
            None,
            WorkbenchType(ty),
            false,
            None,
            None,
            None,
        )
        .expect("workbench");
        let me: ObjRef = wb.clone();
        (wb, me)
    }

    pub fn blockhead(uid: u64, net: bool) -> ObjRef {
        TestBh::new(uid, net)
    }

    /// Number of `rand()` draws performed by `f` (compares the RNG state against a replayed stream).
    pub fn draws_during(f: impl FnOnce()) -> usize {
        crate::rng::glibc_rand::srand(777);
        f();
        let next = crate::rng::glibc_rand::rand();
        crate::rng::glibc_rand::srand(777);
        (0..400).find(|_| crate::rng::glibc_rand::rand() == next).unwrap_or(usize::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::crafting::craftable_item_object::{zero_craftable_item, CraftableItemObject};
    use crate::objects::interaction_object::testutil::TestBh;

    #[test]
    fn inherited_item_init_is_zero_state() {
        let f = fixture();
        let wb =
            Workbench::init_inherited_item(&f.world, &f.dwr, IntPair { x: 10, y: 10 }, None, false, None, None, None)
                .expect("init");
        let s = wb.st.borrow();
        assert_eq!(s.ty, 0);
        assert_eq!(s.level, 0);
        assert_eq!(s.fuel_counter, 0.0);
        assert_eq!(s.fuel_fraction, 0.0);
        assert!(!s.has_fuel);
        assert_eq!(s.last_world_time, 0.0);
        assert!(s.light.is_none());
        assert_eq!(s.available_electricity, 0);
    }

    #[test]
    fn light_rgb_table() {
        let f = fixture();
        let (wb, _me) = make(&f, 1, 10, 10);
        let expect = [
            (0, [153, 189, 255]),
            (1, [135, 115, 255]),
            (2, [0, 0, 255]),
            (3, [0, 255, 0]),
            (4, [255, 0, 0]),
            (5, [200, 255, 255]),
        ];
        for (lvl, rgb) in expect {
            wb.st.borrow_mut().level = lvl;
            assert_eq!(wb.get_light_rgb(), [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32], "portal level {}", lvl);
        }
        let (camp, _m) = make(&f, 3, 20, 10);
        camp.st.borrow_mut().has_fuel = false;
        assert_eq!(camp.get_light_rgb(), [64.0, 50.0, 4.0]);
        camp.st.borrow_mut().has_fuel = true;
        assert_eq!(camp.get_light_rgb(), [220.0, 130.0, 48.0]);
        let (plain, _m2) = make(&f, 2, 25, 10);
        assert_eq!(plain.get_light_rgb(), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn fuel_count_table() {
        let f = fixture();
        let (wb, _me) = make(&f, 3, 10, 10);
        for (ff, want) in [(0.0f32, 0), (0.0001, 0), (0.01, 1), (0.05, 1), (0.5, 5), (0.95, 10), (1.0, 10)] {
            wb.st.borrow_mut().fuel_fraction = ff;
            assert_eq!(wb.fuel_count(), want, "fuel {}", ff);
        }
    }

    #[test]
    fn hurry_cost_formula() {
        let f = fixture();
        let (wb, _me) = make(&f, 2, 10, 10);
        for (s, want) in [(0, 1), (1, 1), (10, 1), (11, 2), (25, 3), (-5, 1)] {
            assert_eq!(wb.hurry_cost_for_craft_time_remaining_total_craft_time(s, 99), want, "secs {}", s);
        }
    }

    #[test]
    fn add_to_fuel_cases() {
        let f = fixture();
        let (plain, pme) = make(&f, 2, 10, 10);
        let before = f.dw.changed.borrow().len();
        plain.add_to_fuel(&pme, 3);
        assert_eq!(f.dw.changed.borrow().len(), before, "non-fuel type is a no-op");
        let (camp, cme) = make(&f, 3, 20, 10);
        camp.st.borrow_mut().fuel_fraction = 0.95;
        camp.add_to_fuel(&cme, 2);
        assert_eq!(camp.st.borrow().fuel_fraction, 1.0);
        let (steam, sme) = make(&f, 15, 30, 10);
        {
            let mut s = steam.st.borrow_mut();
            s.fuel_fraction = 0.5;
            s.available_electricity = 0;
        }
        steam.add_to_fuel(&sme, 0);
        assert_eq!(steam.st.borrow().available_electricity, 100);
        assert!(steam.st.borrow().fuel_fraction < 0.5 && steam.st.borrow().fuel_fraction > 0.44);
    }

    #[test]
    fn subtract_electricty_cases() {
        let f = fixture();
        let (wb, me) = make(&f, 15, 10, 10);
        wb.st.borrow_mut().fuel_fraction = 0.0;
        wb.st.borrow_mut().available_electricity = 50;
        assert!(wb.subtract_electricty(&me, 20));
        assert_eq!(wb.available_electricity(), 30);
        assert!(!wb.subtract_electricty(&me, 31));
        assert_eq!(wb.available_electricity(), 30);
        // steam refill: runs even when the subtract failed
        wb.st.borrow_mut().fuel_fraction = 1.0;
        wb.st.borrow_mut().available_electricity = 95;
        assert!(!wb.subtract_electricty(&me, 96));
        assert_eq!(wb.available_electricity(), 105);
        assert!(wb.st.borrow().fuel_fraction < 1.0 && wb.st.borrow().fuel_fraction > 0.99);
        assert!(wb.st.borrow().particle_create_timer_steam_engine > 0.0999);
    }

    #[test]
    fn energy_fraction_flywheel_and_predicates() {
        let f = fixture();
        let (wb, _me) = make(&f, 21, 10, 10);
        wb.st.borrow_mut().available_electricity = 4096;
        assert_eq!(wb.energy_fraction(), 0.5);
        assert!(wb.is_storage_device() && wb.conducts_electricity() && !wb.generates_electricity());
        let (steam, _s) = make(&f, 15, 20, 10);
        assert!(steam.generates_electricity() && steam.conducts_electricity());
        assert_eq!(steam.energy_fraction(), 0.0);
        assert!(steam.uses_stores_conducts_or_produces_electricity());
    }

    #[test]
    fn net_data_round_trip() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        wb.st.borrow_mut().fuel_fraction = 0.5;
        wb.st.borrow_mut().level = 0;
        me.set_owner_id(Some("owner1".into()));
        let nd = wb.update_net_data_for_client_impl(&me);
        assert!(nd.len() > 0x38);
        let w = <WorkbenchNetData as WireStruct>::read_le(&nd);
        assert_eq!(w.fuel_fraction, 127);
        assert_eq!(w.type_, 3);
        assert_eq!(w.interaction_object_creation_net_data.interaction_object_type, 1);
        let (oid, _) = Workbench::owner_from_tail(&nd[0x38..]);
        assert_eq!(oid.as_deref(), Some("owner1"));
        // create a replica from it
        let rep =
            Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd).expect("replica");
        assert!(rep.base.borrow().is_net);
        assert!((rep.st.borrow().fuel_fraction - 127.0 / 255.0).abs() < 1e-7);
        assert_eq!(rep.base.borrow().owner_id.as_deref(), Some("owner1"));
        // remote_update back into another server workbench
        let (wb2, me2) = make(&f, 3, 20, 10);
        wb2.st.borrow_mut().fuel_fraction = 0.0;
        wb2.st.borrow_mut().has_fuel = false;
        wb2.remote_update_impl(&me2, &nd);
        assert!((wb2.st.borrow().fuel_fraction - 127.0 / 255.0).abs() < 1e-7);
        assert!(wb2.st.borrow().has_fuel);
        // an empty owner dict still appends a valid gzip stream
        let (wb3, me3) = make(&f, 2, 25, 10);
        let nd3 = wb3.update_net_data_for_client_impl(&me3);
        assert!(nd3.len() > 0x38);
        assert!(gzip_inflate(&nd3[0x38..]).is_some());
        // short blob raises
        let r = crate::foundation::exception::catch_ns(|| wb3.remote_update_impl(&me3, &nd3[..0x20]));
        assert_eq!(r.unwrap_err().name, "NSRangeException");
    }

    #[test]
    fn rng_draw_counts() {
        let f = fixture();
        assert_eq!(draws_during(|| drop(make(&f, 3, 10, 10))), 3, "campfire create");
        assert_eq!(draws_during(|| drop(make(&f, 2, 20, 10))), 2, "other create");
        let (wb, me) = make(&f, 2, 25, 10);
        let sd = wb.get_save_dict_impl(&me);
        assert_eq!(
            draws_during(|| drop(Workbench::init_with_world_dynamic_world_save_dict_cache(
                &f.world, &f.dwr, &sd, None
            ))),
            2,
            "load"
        );
        assert_eq!(draws_during(|| wb.upgrade_to_next_level(&me)), 56, "upgrade");
        assert_eq!(draws_during(|| wb.hurry_completion(&me, 5)), 112, "hurry");
        assert!(wb.st.borrow().hurrying);
        assert_eq!(draws_during(|| wb.hurry_completion(&me, 5)), 0, "second hurry does nothing");
        assert_eq!(f.deps.log.borrow().iter().filter(|l| *l == "particle").count(), 8 + 16);
    }

    #[test]
    fn campfire_timer_expiry_draws_two() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        wb.st.borrow_mut().fire_spread_timer = 0.5;
        wb.st.borrow_mut().has_fuel = true;
        assert_eq!(draws_during(|| wb.update_accurate_dt_is_simulation_impl(&me, 1.0, 1.0, false)), 2);
        let t = wb.st.borrow().fire_spread_timer;
        assert!((30.0..50.0).contains(&t));
    }

    #[test]
    fn save_dict_key_set_for_spawn_portal() {
        let f = fixture();
        let (wb, me) = make(&f, 1, 10, 10);
        let d = wb.get_save_dict_impl(&me);
        let mut keys: Vec<String> = d.map.keys_in_order();
        keys.sort();
        let want = [
            "availableElectricity",
            "craftProgressCount",
            "fireSpreadTimer",
            "flipped",
            "floatPos",
            "fuelFraction",
            "hasFuel",
            "hurryCost",
            "hurrySeconds",
            "hurryTimer",
            "hurrying",
            "interactionObjectType",
            "isInUse",
            "lastWorldTime",
            "level",
            "lightDict",
            "paintColor",
            "pos_x",
            "pos_y",
            "saveTime",
            "selectedIndex",
            "uniqueID",
            "workbenchType",
            "xScroll",
        ];
        assert_eq!(keys, want.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let ld = d.map.get(&"lightDict".to_string()).cloned();
        let Some(PlistValue::Dict(ld)) = ld else { unreachable!("lightDict") };
        assert_eq!(ld.map.get(&"maxRed".to_string()).unwrap().int_value(), 153);
        assert_eq!(ld.map.get(&"maxHeat".to_string()).unwrap().int_value(), -2);
        assert_eq!(ld.map.get(&"radius".to_string()).unwrap().int_value(), 12);
        assert_eq!(d.map.get(&"interactionObjectType".to_string()).unwrap().int_value(), 1);
        // reload
        let wb2 = Workbench::init_with_world_dynamic_world_save_dict_cache(&f.world, &f.dwr, &d, None).expect("reload");
        assert_eq!(wb2.st.borrow().ty, 1);
        assert!(wb2.st.borrow().light.is_some());
        assert_eq!(wb2.st.borrow().saved_blockhead_index_fuel, -1);
    }

    #[test]
    fn placed_portal_writes_base_stone() {
        let f = fixture();
        let (_wb, _me) = make(&f, 13, 10, 10);
        let t = crate::world::tiles::tile_at_world_position_loaded(10, 9, &f.world).unwrap().get();
        assert_eq!(t.type_index, 42);
        assert_eq!(t.back_wall_type_index, 10);
        assert_eq!(t.contents, 0);
    }

    #[test]
    fn upgrade_writes_portal_stone_and_light() {
        let f = fixture();
        let (wb, me) = make(&f, 13, 10, 10);
        wb.upgrade_to_next_level(&me);
        assert_eq!(wb.level(), 1);
        let t = crate::world::tiles::tile_at_world_position_loaded(10, 9, &f.world).unwrap().get();
        assert_eq!(t.type_index, 0x2b);
        assert_eq!(wb.get_light_rgb(), [135.0, 115.0, 255.0]);
        assert!(me.update_needs_to_be_sent());
    }

    #[test]
    fn set_needs_removed_type1_is_ignored() {
        let f = fixture();
        let (wb, me) = make(&f, 1, 10, 10);
        DynamicObject::set_needs_removed(&*wb, &me, true);
        assert!(!me.needs_removed());
        let (wb2, me2) = make(&f, 13, 20, 10);
        DynamicObject::set_needs_removed(&*wb2, &me2, true);
        assert!(me2.needs_removed());
    }

    #[test]
    fn world_changed_campfire_support_loss_removes() {
        let f = fixture();
        let (_wb, me) = make(&f, 3, 10, 10);
        // tile below (10,9) is air (not solid): any changed position near the bench removes a campfire
        let mut v = vec![IntPair { x: 10, y: 9 }];
        me.world_changed(&me, &mut v);
        assert!(me.needs_removed());
        assert!(f.deps.log.borrow().iter().any(|l| l.starts_with("free(10,10,")));
    }

    #[test]
    fn world_changed_wrap_arithmetic_triggers_for_far_x() {
        let f = fixture();
        // width 4 macros = 128 tiles; bench x = 0 and changed x = 127 -> d = -127 -> +128 = 1 -> |d| < 2
        let (_wb, me) = make(&f, 3, 0, 10);
        let mut v = vec![IntPair { x: 127, y: 9 }];
        me.world_changed(&me, &mut v);
        assert!(me.needs_removed());
        // type 1 never removes
        let (_wb1, me1) = make(&f, 1, 40, 10);
        let mut v = vec![IntPair { x: 40, y: 9 }];
        me1.world_changed(&me1, &mut v);
        assert!(!me1.needs_removed());
    }

    #[test]
    fn remove_for_basic_portal_does_nothing_and_placed_portal_drops_item() {
        let f = fixture();
        let (wb, me) = make(&f, 1, 10, 10);
        wb.remove(&me, None);
        assert!(!me.needs_removed());
        assert!(!format!("|{}", f.deps.log.borrow().join("|")).contains("|free("));
        let (wb2, me2) = make(&f, 13, 20, 10);
        wb2.remove(&me2, None);
        assert!(me2.needs_removed());
        let log = f.deps.log.borrow().clone();
        assert!(log.iter().any(|l| l.starts_with("free(20,10,")), "{:?}", log);
        let t = crate::world::tiles::tile_at_world_position_loaded(20, 9, &f.world).unwrap().get();
        let _ = t;
    }

    #[test]
    fn remove_from_macro_block_sends_portal_removed() {
        let f = fixture();
        let (_wb, me) = make(&f, 13, 20, 10);
        me.remove_from_macro_block(&me);
        assert!(f.host.log.borrow().iter().any(|l| l == "portalRemoved(20,10)"));
        let (_c, cme) = make(&f, 3, 30, 10);
        cme.remove_from_macro_block(&cme);
        assert!(!f.host.log.borrow().iter().any(|l| l == "portalRemoved(30,10)"));
    }

    #[test]
    fn requires_physical_block_rules() {
        let f = fixture();
        let (fw, fme) = make(&f, 21, 10, 10);
        fw.st.borrow_mut().available_electricity = 0;
        fw.st.borrow_mut().time_since_found_electricty = 5.0;
        assert!(fme.requires_physical_block());
        fw.st.borrow_mut().time_since_found_electricty = 20.0;
        assert!(!fme.requires_physical_block());
        fw.st.borrow_mut().available_electricity = 0x2000;
        fw.st.borrow_mut().time_since_found_electricty = 0.0;
        assert!(!fme.requires_physical_block());
        let (plain, pme) = make(&f, 2, 20, 10);
        assert!(!pme.requires_physical_block());
        plain.st.borrow_mut().is_in_use_fuel = true;
        assert!(pme.requires_physical_block());
    }

    #[test]
    fn fuel_state_transitions_and_embers_light() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        assert!(wb.st.borrow().has_fuel);
        assert!(wb.st.borrow().light.is_some());
        wb.st.borrow_mut().fuel_fraction = 0.00005;
        wb.update_has_fuel(&me);
        assert!(!wb.st.borrow().has_fuel);
        assert_eq!(wb.st.borrow().fuel_fraction, 0.0);
        assert!(wb.st.borrow().light.is_some(), "campfire embers light");
        wb.st.borrow_mut().fuel_fraction = 0.3;
        wb.update_has_fuel(&me);
        assert!(wb.st.borrow().has_fuel);
        assert_eq!(wb.get_light_rgb(), [220.0, 130.0, 48.0]);
    }

    #[test]
    fn solar_and_flywheel_update() {
        let f = fixture();
        let (wb, me) = make(&f, 20, 10, 10);
        {
            let mut s = wb.st.borrow_mut();
            s.available_electricity = 0;
            s.fuel_counter = 4.5;
        }
        // tile above is air with sun_light 0 -> combined light 0 -> no gain, counter wraps
        wb.update_accurate_dt_is_simulation_impl(&me, 0.1, 0.1, false);
        assert_eq!(wb.available_electricity(), 0);
        assert!((wb.st.borrow().fuel_counter - 0.6).abs() < 1e-5);
        let (fw, fme) = make(&f, 21, 20, 10);
        f.host.power.set(40);
        {
            let mut s = fw.st.borrow_mut();
            s.available_electricity = 10;
            s.fuel_counter = 4.95;
        }
        fw.update_accurate_dt_is_simulation_impl(&fme, 0.1, 0.1, false);
        assert_eq!(fw.available_electricity(), 50);
        assert!(f.host.log.borrow().iter().any(|l| l == &format!("findPower({})", 0x2000 - 10)));
        assert_eq!(fw.st.borrow().time_since_found_electricty, 0.0);
        f.host.power.set(0);
        fw.st.borrow_mut().fuel_counter = 4.95;
        fw.update_accurate_dt_is_simulation_impl(&fme, 0.1, 0.1, false);
        assert_eq!(fw.st.borrow().time_since_found_electricty, 5.0);
    }

    #[test]
    fn steam_generator_update_refills() {
        let f = fixture();
        let (wb, me) = make(&f, 15, 10, 10);
        {
            let mut s = wb.st.borrow_mut();
            s.available_electricity = 0;
            s.fuel_fraction = 0.5;
        }
        wb.update_accurate_dt_is_simulation_impl(&me, 0.1, 0.1, false);
        assert_eq!(wb.available_electricity(), 100);
        assert!(
            wb.st.borrow().particle_create_timer_steam_engine > 0.99
                && wb.st.borrow().particle_create_timer_steam_engine <= 4.0
        );
        assert!(wb.st.borrow().fuel_fraction < 0.5);
    }

    #[test]
    fn dealloc_clears_references() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        wb.st.borrow_mut().crafting_item_object =
            Some(CraftableItemObject::init_with_craftable_item(zero_craftable_item()));
        wb.dealloc(&me);
        assert!(wb.st.borrow().light.is_none() && wb.st.borrow().crafting_item_object.is_none());
    }

    fn recipe_two_sources() -> CraftableItem {
        let mut ci = zero_craftable_item();
        ci.crafted_item_type = 19;
        ci.crafted_item_count = 1;
        ci.number_of_required_items = 2;
        ci.required_items[0] = 20;
        ci.required_item_counts[0] = 2;
        ci.required_items[1] = 11;
        ci.required_item_counts[1] = 1;
        ci
    }

    fn bh_log(bh: &ObjRef) -> Vec<String> {
        bh.as_any().downcast_ref::<TestBh>().unwrap().log.borrow().clone()
    }

    #[test]
    fn null_host_is_inert() {
        let f = fixture();
        let (_wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let h: &dyn WorkbenchHost = &NullHost;
        h.portal_is_being_removed_at_pos(IntPair { x: 1, y: 2 });
        assert_eq!(h.find_and_subtract_all_power_up_to_for_user(10, &bh), 0);
        assert!(!h.bh_paused(&bh) && !h.bh_has_coffee_energy(&bh) && !h.bh_should_continue_simulating(&bh));
        assert!(!h.bh_is_crafting_at_any_workbench(&bh) && !h.bh_is_adding_fuel_to_any_workbench(&bh));
        assert!(!h.bh_double_time_unlocked(&bh));
        assert_eq!(h.bh_unable_to_work_reason(&bh), 0);
        assert!(h.bh_interaction_workbench(&bh).is_none());
        h.bh_set_interaction_workbench(&bh, Some(&me));
        h.bh_set_interaction_object(&bh, Some(&me));
        assert_eq!(bh_log(&bh), vec!["setIO(true)".to_string()]);
        assert!(h.bh_subtract_items_from_inventory_of_type_count_data_b(&bh, ItemType(20), 3, 0).is_empty());
        h.bh_add_item_to_inventory(&bh, &InventoryItem::new(ItemType(20), 0, 0, None, None));
        h.bh_craft_item_finished_at_workbench(&bh, true, &me);
        h.bh_net_interaction_object_was_loaded(&bh, &me);
    }

    #[test]
    fn accessors_setters_and_fuel_queries() {
        let f = fixture();
        let (wb, me) = make(&f, 14, 10, 10);
        wb.set_selected_index(3);
        wb.set_x_scroll(1.5);
        assert_eq!((wb.selected_index(), wb.x_scroll()), (3, 1.5));
        assert_eq!(wb.title(), "METALWORK BENCH LEVEL 1");
        assert_eq!(wb.upgrade_name(), "LEVEL 2 UPGRADE");
        assert_eq!(wb.craftable_items().len(), 2);
        assert!(wb.number_of_craftable_items() > 0);
        assert!(wb.number_of_craftable_items_up_to_current_level() > 0);
        assert!(wb.number_of_craftable_items_up_to_current_level() <= wb.number_of_craftable_items());
        assert!(!wb.is_double_height() && wb.requires_fuel() && !wb.requires_electricty());
        assert_eq!((wb.fuel_item_count(), wb.fuel_types_count()), (3, 3));
        assert_eq!(wb.fuel_items(), wb.fuel_types());
        assert_eq!(wb.fuel_items(), Some([ItemType(1049), ItemType(14), ItemType(51)]));
        assert_eq!(wb.destroy_item_type(), item_type_for_workbench_type_and_level(WorkbenchType(14), 0));
        wb.set_paused(true);
        assert!(wb.st.borrow().paused);
        // fuel availability
        assert!(wb.has_required_fuel());
        wb.st.borrow_mut().has_fuel = false;
        assert!(!wb.has_required_fuel());
        let (kiln, _k) = make(&f, 16, 20, 10);
        assert!(kiln.requires_electricty());
        assert!(!kiln.has_required_fuel());
        kiln.st.borrow_mut().available_electricity = 5;
        assert!(kiln.has_required_fuel());
        wb.st.borrow_mut().fuel_fraction = 5.0;
        assert_eq!(wb.fuel_count(), 10);
        // fuel by item
        let (camp, cme) = make(&f, 3, 30, 10);
        camp.st.borrow_mut().fuel_fraction = 0.1;
        camp.add_to_fuel_for_item(&cme, ItemType(1049));
        assert!((camp.st.borrow().fuel_fraction - 0.6).abs() < 1e-6);
        drop(me);
    }

    #[test]
    fn solar_panel_light_and_energy() {
        let f = fixture();
        let (wb, me) = make(&f, 20, 10, 10);
        let above = crate::world::tiles::tile_at_world_position_loaded(10, 11, &f.world).unwrap();
        // full sunlight reads the raw tile light, the artificial part is capped at 2000
        above.update(|t| t.sun_light = 255);
        assert!((wb.combined_light_for_solar_panel_with_full_sunlight() - 1.0).abs() < 1e-6);
        above.update(|t| t.artificial_light_r = 3000);
        assert!((wb.combined_light_for_solar_panel_with_full_sunlight() - 1.2).abs() < 1e-6);
        above.update(|t| t.sun_light = 0);
        assert!((wb.combined_light_for_solar_panel_with_full_sunlight() - 0.2).abs() < 1e-6);
        above.update(|t| t.contents = 0x03);
        assert!((wb.combined_light_for_solar_panel_with_full_sunlight() - 0.1).abs() < 1e-6, "trees halve it");
        above.update(|t| t.contents = 46);
        assert_eq!(wb.combined_light_for_solar_panel_with_full_sunlight(), 0.0);
        above.update(|t| {
            t.contents = 0;
            t.type_index = 10;
        });
        assert_eq!(wb.combined_light_for_solar_panel_with_full_sunlight(), 0.0, "solid tile above");
        assert_eq!(wb.combined_light_for_solar_panel(), 0.0);
        assert_eq!(wb.energy_fraction(), 0.0);
        // daylight: pick a world time where the sun is high at x = 10
        above.update(|t| {
            t.type_index = 2;
            t.sun_light = 255;
        });
        let mut best = 0.0f32;
        for step in 0..48 {
            f.world.set_world_time(step as f64 * 225.0);
            best = best.max(wb.combined_light_for_solar_panel());
        }
        assert!(best > 0.5, "daylight reaches the panel: {best}");
        // rain dims it, artificial light only adds
        let mut dimmed = 0.0f32;
        let mut t_best = 0.0;
        for step in 0..48 {
            f.world.set_world_time(step as f64 * 225.0);
            if wb.combined_light_for_solar_panel() >= best - 1e-6 {
                t_best = step as f64 * 225.0;
            }
        }
        f.world.set_world_time(t_best);
        let rules = crate::types::structs::CustomRules { custom_rules_enabled: 1, rainfall: 4, ..Default::default() };
        f.world.set_custom_rules(rules);
        dimmed = dimmed.max(wb.combined_light_for_solar_panel());
        assert!(dimmed < best, "rain dims the panel: {dimmed} < {best}");
        assert!(wb.energy_fraction() <= 1.0);
        // low light reads as zero energy
        f.world.set_custom_rules(Default::default());
        above.update(|t| {
            t.sun_light = 1;
            t.artificial_light_r = 0;
        });
        assert_eq!(wb.energy_fraction(), 0.0);
        // a bright tile reaches the clamp of one
        above.update(|t| {
            t.sun_light = 255;
            t.artificial_light_r = 20000;
        });
        assert_eq!(wb.energy_fraction(), 1.0);
        // a panel needs a physical block while the sun shines
        above.update(|t| t.artificial_light_r = 0);
        wb.st.borrow_mut().available_electricity = 1;
        assert!(me.requires_physical_block());
    }

    fn crafting_save_dict(ci: &CraftableItem, src: PlistValue) -> PlistDict {
        let mut sd = new_mutable_dict();
        sd.map.insert("craftingItemData".into(), PlistValue::data(ci.to_vec()));
        sd.map.insert("count".into(), PlistValue::int(4));
        sd.map.insert("countLeft".into(), PlistValue::int(3));
        sd.map.insert("countCreated".into(), PlistValue::int(1));
        sd.map.insert("sourceItems_0".into(), src);
        sd.map.insert("sourceItems_1".into(), PlistValue::array(vec![PlistValue::int(1)]));
        sd
    }

    #[test]
    fn restore_crafting_state_formats() {
        let f = fixture();
        let (wb, _me) = make(&f, 2, 10, 10);
        let ci = recipe_two_sources();
        let item = InventoryItem::new(ItemType(20), 7, 8, None, None);
        let blob = PlistValue::data(item.borrow().save_data());
        // legacy raw struct; foreign entries and the time-crystal style slot (item 11) are skipped
        let sd = crafting_save_dict(&ci, PlistValue::array(vec![blob.clone(), PlistValue::int(5)]));
        wb.restore_crafting_state(&sd);
        {
            let s = wb.st.borrow();
            assert_eq!((s.count, s.count_left, s.count_created), (4, 3, 1));
            assert_eq!(s.crafting_item_object.as_ref().unwrap().craftable_item(), ci);
            let src = s.source_items[0].as_ref().unwrap();
            assert_eq!((src.len(), src[0].borrow().data_a), (1, 7));
            assert!(s.source_items[1].is_none());
        }
        // an empty saved array leaves the slot unset
        let sd = crafting_save_dict(&ci, PlistValue::array(vec![]));
        wb.restore_crafting_state(&sd);
        assert!(wb.st.borrow().source_items[0].is_none());
        // a short raw struct raises
        let mut short = crafting_save_dict(&ci, blob);
        short.map.insert("craftingItemData".into(), PlistValue::data(vec![0; 10]));
        let r = crate::foundation::exception::catch_ns(|| wb.restore_crafting_state(&short));
        assert_eq!(r.unwrap_err().name, "NSRangeException");
        // no crafting data at all, or data of the wrong kind: no recipe
        for sd in [new_mutable_dict(), {
            let mut d = new_mutable_dict();
            d.map.insert("craftingItemData".into(), PlistValue::string("x"));
            d
        }] {
            wb.restore_crafting_state(&sd);
            assert!(wb.st.borrow().crafting_item_object.is_none());
        }
        // a v2 entry that is not a dictionary gives an empty recipe object
        let mut sd = new_mutable_dict();
        sd.map.insert("craftingItemDatav2".into(), PlistValue::int(1));
        wb.restore_crafting_state(&sd);
        assert_eq!(wb.st.borrow().crafting_item_object.as_ref().unwrap().craftable_item(), zero_craftable_item());
    }

    #[test]
    fn take_ownership_and_blockheads_loaded() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(5, false);
        let ci = recipe_two_sources();
        let mut sd = crafting_save_dict(&ci, PlistValue::array(vec![]));
        sd.map.insert("craftProgressCount".into(), PlistValue::real_f32(2.5));
        sd.map.insert("hurrying".into(), PlistValue::boolean(true));
        wb.blockhead_would_like_to_take_ownership_with_save_dict_impl(&me, &bh, Some(&sd));
        assert!(me.is_in_use());
        assert_eq!(wb.st.borrow().craft_progress_count, 2.5);
        assert!(wb.st.borrow().hurrying);
        assert!(f.host.log.borrow().iter().any(|l| l == "setIW(true)"));
        // a second blockhead is ignored while one is current
        let other = blockhead(6, false);
        wb.blockhead_would_like_to_take_ownership_with_save_dict_impl(&me, &other, None);
        assert!(wb.io.borrow().current_blockhead.as_ref().is_some_and(|c| c.unique_id() == 5));
        // reload links: the fuel blockhead index resolves and the blockheads get the bench back
        f.dw.blockheads.borrow_mut().extend([other.clone(), bh.clone()]);
        {
            let mut s = wb.st.borrow_mut();
            s.saved_blockhead_index_fuel = 1;
            s.is_in_use_fuel = true;
        }
        f.host.log.borrow_mut().clear();
        wb.blockheads_loaded_impl(&me);
        assert!(wb.st.borrow().current_fuel_blockhead.as_ref().is_some_and(|c| c.unique_id() == 5));
        assert_eq!(wb.st.borrow().saved_blockhead_index_fuel, -1);
        assert_eq!(f.host.log.borrow().iter().filter(|l| *l == "setIW(true)").count(), 2);
        // `interactionObject` goes to the blockhead the base class resolved from its own saved index (slot 0)
        let log = bh_log(&other);
        assert!(!log.is_empty() && log.iter().all(|l| l == "setIO(true)"), "{log:?}");
        // an index past the end of the list is dropped
        wb.st.borrow_mut().current_fuel_blockhead = None;
        wb.st.borrow_mut().saved_blockhead_index_fuel = 9;
        wb.blockheads_loaded_impl(&me);
        assert!(wb.st.borrow().current_fuel_blockhead.is_none());
        assert_eq!(wb.st.borrow().saved_blockhead_index_fuel, -1);
        // fuel blockhead unloads / remote fuel blockhead leaves
        wb.st.borrow_mut().current_fuel_blockhead = Some(other.clone());
        wb.st.borrow_mut().is_in_use_fuel = true;
        wb.blockhead_unloaded_impl(&me, &bh);
        assert!(wb.st.borrow().is_in_use_fuel);
        wb.blockhead_unloaded_impl(&me, &other);
        assert!(!wb.st.borrow().is_in_use_fuel && wb.st.borrow().current_fuel_blockhead.is_none());
        wb.st.borrow_mut().remote_fuel_blockhead_in_use_unique_id = 9;
        wb.st.borrow_mut().is_in_use_fuel = true;
        wb.remote_blockhead_removed_with_id_impl(&me, 4);
        assert!(wb.st.borrow().is_in_use_fuel);
        wb.remote_blockhead_removed_with_id_impl(&me, 9);
        assert!(!wb.st.borrow().is_in_use_fuel);
        assert_eq!(wb.st.borrow().remote_fuel_blockhead_in_use_unique_id, 0);
    }

    #[test]
    fn save_dict_with_crafting_and_fuel_blockhead_reloads() {
        let f = fixture();
        let (wb, me) = make(&f, 3, 10, 10);
        let bh = blockhead(5, false);
        let ci = recipe_two_sources();
        f.dw.blockheads.borrow_mut().push(bh.clone());
        {
            let mut s = wb.st.borrow_mut();
            s.crafting_item_object = Some(CraftableItemObject::init_with_craftable_item(ci));
            s.count = 4;
            s.count_left = 3;
            s.count_created = 1;
            s.source_items[0] = Some(vec![InventoryItem::new(ItemType(20), 1, 2, None, None)]);
            s.source_items[1] = Some(vec![InventoryItem::new(ItemType(11), 1, 2, None, None)]);
            s.current_fuel_blockhead = Some(bh.clone());
            s.is_in_use_fuel = true;
        }
        wb.io.borrow_mut().is_in_use = true;
        let sd = wb.get_save_dict_impl(&me);
        assert_eq!(sd.map.get(&"currentBlockheadIndexFuel".to_string()).unwrap().int_value(), 0);
        assert!(sd.map.get(&"craftingItemDatav2".to_string()).is_some());
        assert!(sd.map.get(&"sourceItems_0".to_string()).is_some());
        assert!(sd.map.get(&"sourceItems_1".to_string()).is_none(), "item 11 is never stored");
        assert_eq!(sd.map.get(&"countLeft".to_string()).unwrap().int_value(), 3);
        // not in the blockhead list: no index
        f.dw.blockheads.borrow_mut().clear();
        assert!(wb.get_save_dict_impl(&me).map.get(&"currentBlockheadIndexFuel".to_string()).is_none());
        // load it back: crafting state, fuel blockhead index and the owner name of the net blob
        let back = Workbench::init_with_world_dynamic_world_save_dict_cache(&f.world, &f.dwr, &sd, None).expect("load");
        assert_eq!(back.st.borrow().saved_blockhead_index_fuel, 0);
        assert!(back.io.borrow().is_in_use);
        assert_eq!(back.st.borrow().count_left, 3);
        assert_eq!(back.st.borrow().source_items[0].as_ref().unwrap().len(), 1);
        wb.io.borrow_mut().owner_name = Some("Ann".into());
        let nd = wb.update_net_data_for_client_impl(&me);
        let (_, name) = Workbench::owner_from_tail(&nd[0x38..]);
        assert_eq!(name.as_deref(), Some("Ann"));
        // garbage after the header decodes to no owner
        assert_eq!(Workbench::owner_from_tail(&[1, 2, 3]), (None, None));
    }

    /// Net blob of `wb` with the header edited by `edit`.
    fn blob(wb: &Workbench, me: &ObjRef, edit: impl FnOnce(&mut WorkbenchNetData)) -> Vec<u8> {
        let mut b = wb.update_net_data_for_client_impl(me);
        let mut nd = <WorkbenchNetData as WireStruct>::read_le(&b[..0x38]);
        edit(&mut nd);
        b[..0x38].copy_from_slice(&nd.to_vec());
        b
    }

    #[test]
    fn remote_update_crafting_user_changes() {
        let f = fixture();
        f.dw.server.set(false);
        let (wb, me) = make(&f, 2, 10, 10);
        let bh = blockhead(7, false);
        f.dw.blockheads.borrow_mut().push(bh.clone());
        // a bench that was idle gets a different user on a client: the stale local user is stopped
        wb.io.borrow_mut().current_blockhead = Some(bh.clone());
        let b = blob(&wb, &me, |n| {
            n.interaction_object_creation_net_data.is_in_use = 1;
            n.interaction_object_creation_net_data.is_in_use_blockhead_unique_id = 9;
        });
        wb.remote_update_impl(&me, &b);
        assert!(wb.io.borrow().current_blockhead.is_none());
        // the same on a bench that was already in use is handled by the base class
        {
            let mut io = wb.io.borrow_mut();
            io.is_in_use = true;
            io.current_blockhead = Some(bh.clone());
        }
        wb.remote_update_impl(&me, &b);
        assert!(wb.io.borrow().current_blockhead.is_none());
        // no local user and a remote one that is not around: the bench is released again
        assert_eq!(wb.io.borrow().remote_blockhead_in_use_unique_id, 9);
        // the remote user is in the list: its state clears (literal scan without break)
        let b = blob(&wb, &me, |n| {
            n.interaction_object_creation_net_data.is_in_use = 1;
            n.interaction_object_creation_net_data.is_in_use_blockhead_unique_id = 7;
        });
        wb.remote_update_impl(&me, &b);
        assert!(!me.is_in_use());
        assert_eq!(wb.io.borrow().remote_blockhead_in_use_unique_id, 0);
        assert!(me.update_needs_to_be_sent());
        // in use without any user id: not in use
        wb.io.borrow_mut().is_in_use = false;
        me.set_update_needs_to_be_sent(false);
        let b = blob(&wb, &me, |n| n.interaction_object_creation_net_data.is_in_use = 1);
        wb.remote_update_impl(&me, &b);
        assert!(!me.is_in_use() && me.update_needs_to_be_sent());
        // on a server a stale user of an idle bench is left alone
        f.dw.server.set(true);
        {
            let mut io = wb.io.borrow_mut();
            io.is_in_use = false;
            io.current_blockhead = Some(bh.clone());
        }
        let b = blob(&wb, &me, |n| {
            n.interaction_object_creation_net_data.is_in_use = 1;
            n.interaction_object_creation_net_data.is_in_use_blockhead_unique_id = 99;
        });
        wb.remote_update_impl(&me, &b);
        assert!(wb.io.borrow().current_blockhead.is_some());
    }

    #[test]
    fn remote_update_fuel_user_and_fraction() {
        let f = fixture();
        f.dw.server.set(false);
        let (wb, me) = make(&f, 3, 10, 10);
        let bh = blockhead(7, false);
        {
            let mut s = wb.st.borrow_mut();
            s.fuel_fraction = 0.5;
            s.is_in_use_fuel = true;
            s.current_fuel_blockhead = Some(bh.clone());
        }
        // another fuel user on a client replaces ours
        let b = blob(&wb, &me, |n| {
            n.is_in_use_fuel = 1;
            n.is_in_use_fuel_blockhead_unique_id = 8;
            n.fuel_fraction = 255;
        });
        wb.remote_update_impl(&me, &b);
        assert!(wb.st.borrow().current_fuel_blockhead.is_none());
        assert_eq!(wb.st.borrow().remote_fuel_blockhead_in_use_unique_id, 8);
        assert!((wb.st.borrow().fuel_fraction - 1.0).abs() < 1e-6);
        assert!(me.update_needs_to_be_sent());
        // fuel dropping to zero and rising again
        let b = blob(&wb, &me, |n| n.fuel_fraction = 0);
        wb.remote_update_impl(&me, &b);
        assert_eq!(wb.st.borrow().fuel_fraction, 0.0);
        assert!(!wb.st.borrow().has_fuel);
        let b = blob(&wb, &me, |n| n.fuel_fraction = 100);
        wb.remote_update_impl(&me, &b);
        assert!(wb.st.borrow().fuel_fraction > 0.39);
        // small changes are ignored
        me.set_update_needs_to_be_sent(false);
        let b = blob(&wb, &me, |n| n.fuel_fraction = 101);
        wb.remote_update_impl(&me, &b);
        assert!(!me.update_needs_to_be_sent());
    }

    #[test]
    fn remote_update_on_a_client_replica_guards_local_fuel_use() {
        let f = fixture();
        let (src, sme) = make(&f, 15, 10, 10);
        let nd = src.update_net_data_for_client_impl(&sme);
        let rep =
            Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd).expect("replica");
        let rme: ObjRef = rep.clone();
        let local = blockhead(3, false);
        // a local user of the bench is working: a lower fuel level from the server is ignored
        {
            let mut io = rep.io.borrow_mut();
            io.is_in_use = true;
            io.current_blockhead = Some(local.clone());
        }
        let before = rep.st.borrow().fuel_fraction;
        let b = blob(&src, &sme, |n| {
            n.fuel_fraction = 10;
            n.interaction_object_creation_net_data.is_in_use = 1;
            n.interaction_object_creation_net_data.is_in_use_blockhead_unique_id = 3;
        });
        rep.remote_update_impl(&rme, &b);
        assert_eq!(rep.st.borrow().fuel_fraction, before);
        // a local fuel user, bench not in use: also ignored
        rep.io.borrow_mut().is_in_use = false;
        rep.io.borrow_mut().current_blockhead = None;
        {
            let mut s = rep.st.borrow_mut();
            s.is_in_use_fuel = true;
            s.current_fuel_blockhead = Some(local.clone());
        }
        let b = blob(&src, &sme, |n| {
            n.fuel_fraction = 10;
            n.is_in_use_fuel = 1;
            n.is_in_use_fuel_blockhead_unique_id = 3;
        });
        rep.remote_update_impl(&rme, &b);
        assert_eq!(rep.st.borrow().fuel_fraction, before);
        // a steam generator's falling fuel starts the steam puffs
        {
            let mut s = rep.st.borrow_mut();
            s.is_in_use_fuel = false;
            s.current_fuel_blockhead = None;
        }
        let b = blob(&src, &sme, |n| n.fuel_fraction = 10);
        rep.remote_update_impl(&rme, &b);
        let puff = rep.st.borrow().particle_create_timer_steam_engine;
        assert!(puff > 0.0 && puff <= 4.0, "{puff}");
        // a far bigger drop is capped
        {
            let mut s = rep.st.borrow_mut();
            s.fuel_fraction = 1.0;
        }
        let b = blob(&src, &sme, |n| n.fuel_fraction = 0);
        rep.remote_update_impl(&rme, &b);
        assert_eq!(rep.st.borrow().particle_create_timer_steam_engine, 4.0);
    }

    #[test]
    fn remote_update_level_change_and_owner() {
        let f = fixture();
        let (wb, me) = make(&f, 13, 10, 10);
        me.set_owner_id(Some("o1".into()));
        wb.io.borrow_mut().owner_name = Some("Ann".into());
        let b = blob(&wb, &me, |n| n.level = 2);
        // on a server the tile graphic follows; the client branch skips it
        wb.remote_update_impl(&me, &b);
        assert_eq!(wb.level(), 2);
        let t = crate::world::tiles::tile_at_world_position_loaded(10, 9, &f.world).unwrap().get();
        assert_eq!(t.type_index, 0x2a + 2);
        f.dw.server.set(false);
        let b = blob(&wb, &me, |n| n.level = 3);
        wb.remote_update_impl(&me, &b);
        assert_eq!(wb.level(), 3);
        // the owner comes from the tail while nobody uses the bench
        me.set_owner_id(None);
        wb.remote_update_impl(&me, &b);
        assert_eq!(me.owner_id().as_deref(), Some("o1"));
        assert_eq!(wb.io.borrow().owner_name.as_deref(), Some("Ann"));
        // not while a user is present
        wb.io.borrow_mut().current_blockhead = Some(blockhead(1, false));
        me.set_owner_id(None);
        wb.remote_update_impl(&me, &b);
        assert_eq!(me.owner_id(), None);
    }

    #[test]
    fn remote_update_without_a_world_or_dw() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        let b = blob(&wb, &me, |n| n.level = 1);
        let Fixture { world, dw, dwr, deps, host } = f;
        drop((dw, dwr));
        drop(world);
        // both back references are gone: the update still applies the plain state
        wb.remote_update_impl(&me, &b);
        assert_eq!(wb.level(), 1);
        assert_eq!(wb.world_time(), 0.0);
        assert!(!wb.expert_mode() && !wb.is_cloud_game());
        assert!(wb.solar_tile().is_none());
        assert_eq!(wb.combined_light_for_solar_panel(), 0.0);
        wb.notify();
        drop((deps, host));
    }

    #[test]
    fn remove_variants() {
        let f = fixture();
        // easel and refinery are two tiles high
        for ty in [24, 26] {
            let (wb, me) = make(&f, ty, 10 + ty as i32, 10);
            wb.remove(&me, None);
            assert!(me.needs_removed());
        }
        // a user on another machine defers the removal
        let (wb, me) = make(&f, 2, 40, 10);
        wb.io.borrow_mut().is_in_use = true;
        wb.io.borrow_mut().current_blockhead = Some(blockhead(4, true));
        wb.remove(&me, None);
        assert!(!me.needs_removed());
        assert!(wb.io.borrow().needs_to_be_removed_when_interaction_ends);
        wb.io.borrow_mut().current_blockhead = None;
        wb.io.borrow_mut().remote_blockhead_in_use_unique_id = 5;
        wb.remove(&me, None);
        assert!(!me.needs_removed());
        // a local user is sent away and the fuel user cleared
        wb.io.borrow_mut().remote_blockhead_in_use_unique_id = 0;
        wb.io.borrow_mut().current_blockhead = Some(blockhead(4, false));
        {
            let mut s = wb.st.borrow_mut();
            s.crafting_item_object = Some(CraftableItemObject::init_with_craftable_item(recipe_two_sources()));
            s.is_in_use_fuel = true;
            s.current_fuel_blockhead = Some(blockhead(8, false));
        }
        wb.remove(&me, None);
        assert!(me.needs_removed());
        assert!(!wb.st.borrow().is_in_use_fuel && wb.st.borrow().current_fuel_blockhead.is_none());
        assert!(f.host.log.borrow().iter().any(|l| l == "craftItemFinished(false)"));
        // removing twice, or a replica, does nothing
        let n = f.deps.log.borrow().len();
        wb.remove(&me, None);
        assert_eq!(f.deps.log.borrow().len(), n);
    }

    #[test]
    fn world_changed_support_rules() {
        let f = fixture();
        let set = |x: i32, y: i32, edit: fn(&mut crate::types::tile::Tile)| {
            crate::world::tiles::tile_at_world_position_loaded(x, y, &f.world).unwrap().update(edit);
        };
        let ground = |t: &mut crate::types::tile::Tile| t.type_index = 10;
        // a workbench over air with open air around it falls
        let (_wb, me) = make(&f, 2, 10, 10);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(me.needs_removed());
        // solid ground below keeps it
        let (_wb2, me2) = make(&f, 2, 15, 10);
        set(15, 9, ground);
        me2.world_changed(&me2, &mut vec![IntPair { x: 15, y: 9 }]);
        assert!(!me2.needs_removed());
        // far away or far below changes are irrelevant
        let (_wb3, me3) = make(&f, 2, 20, 10);
        me3.world_changed(&me3, &mut vec![IntPair { x: 25, y: 9 }, IntPair { x: 20, y: 5 }]);
        assert!(!me3.needs_removed());
        // a back wall behind it keeps it; so do a blocked neighbour, a half depth one and a contents 96 one
        let (_wb4, me4) = make(&f, 2, 5, 10);
        set(5, 10, |t| t.back_wall_type_index = 7);
        me4.world_changed(&me4, &mut vec![IntPair { x: 5, y: 9 }]);
        assert!(!me4.needs_removed());
        for (x, edit) in [
            (6i32, ground as fn(&mut crate::types::tile::Tile)),
            (8, |t: &mut crate::types::tile::Tile| t.contents = 96),
        ] {
            let bx = x + 20;
            let (_w, m) = make(&f, 2, bx, 10);
            set(bx + 1, 10, edit);
            m.world_changed(&m, &mut vec![IntPair { x: bx, y: 9 }]);
            assert!(!m.needs_removed(), "neighbour edit at {bx}");
        }
        let (_wb5, me5) = make(&f, 2, 3, 10);
        set(3, 9, |t| t.contents = 96);
        me5.world_changed(&me5, &mut vec![IntPair { x: 3, y: 9 }]);
        assert!(!me5.needs_removed(), "contents 96 below keeps it");
        // a bench at the edge of the loaded area has an unloaded neighbour and stays
        let (_wb6, me6) = make(&f, 2, 0, 10);
        me6.world_changed(&me6, &mut vec![IntPair { x: 0, y: 9 }]);
        assert!(!me6.needs_removed());
        // replicas ignore the world; the basic portal never falls; a lost world is harmless
        let (src, sme) = make(&f, 2, 12, 10);
        let nd = src.update_net_data_for_client_impl(&sme);
        let rep = Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd).unwrap();
        let rme: ObjRef = rep;
        rme.world_changed(&rme, &mut vec![IntPair { x: 12, y: 9 }]);
        assert!(!rme.needs_removed());
    }

    #[test]
    fn world_changed_wraps_around_the_world_edge() {
        let f = fixture();
        // width 128: a bench at x = 127 hears about a change at x = 0 through the wrap
        let (wb, me) = make(&f, 2, 127, 10);
        let _ = wb;
        me.world_changed(&me, &mut vec![IntPair { x: 0, y: 9 }]);
        // the tile below (127, 9) is not loaded, so nothing happens either way
        assert!(!me.needs_removed());
        // and the other direction
        let (_wb2, me2) = make(&f, 2, 0, 10);
        me2.world_changed(&me2, &mut vec![IntPair { x: 127, y: 9 }]);
        assert!(!me2.needs_removed());
    }

    #[test]
    fn world_changed_without_a_world_does_nothing() {
        let f = fixture();
        let (_wb, me) = make(&f, 2, 10, 10);
        let Fixture { world, dw, dwr, deps, host } = f;
        drop(world);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 9 }]);
        assert!(!me.needs_removed());
        drop((dw, dwr, deps, host));
    }

    #[test]
    fn steam_puff_timer_is_capped_when_refilling() {
        let f = fixture();
        let (wb, me) = make(&f, 15, 10, 10);
        {
            let mut s = wb.st.borrow_mut();
            s.available_electricity = 0;
            s.fuel_fraction = 1.0;
            s.particle_create_timer_steam_engine = 4.0;
        }
        assert!(!wb.subtract_electricty(&me, 1));
        assert_eq!(wb.available_electricity(), 100);
        assert_eq!(wb.st.borrow().particle_create_timer_steam_engine, 4.0);
    }

    #[test]
    fn creation_from_a_save_dict_and_replica_creation() {
        let f = fixture();
        let mut sd = new_mutable_dict();
        sd.map.insert("fuelFraction".into(), PlistValue::real_f32(0.0));
        sd.map.insert("hasFuel".into(), PlistValue::boolean(false));
        sd.map.insert("level".into(), PlistValue::int(2));
        sd.map.insert("availableElectricity".into(), PlistValue::int(9));
        let at = |ty: u32, x: i32, sd: &PlistDict| {
            Workbench::init_with_world_dynamic_world_at_position_cache_type_flipped_save_dict_placed_by_client_client_name(
                &f.world,
                &f.dwr,
                IntPair { x, y: 10 },
                None,
                WorkbenchType(ty),
                false,
                Some(sd),
                Some("c1"),
                Some("Ann"),
            )
            .expect("workbench")
        };
        // a campfire that is out keeps its embers
        let camp = at(3, 10, &sd);
        assert!(!camp.st.borrow().has_fuel && camp.st.borrow().light.is_some());
        assert_eq!((camp.level(), camp.available_electricity()), (2, 9));
        assert_eq!(camp.io.borrow().owner_name.as_deref(), Some("Ann"));
        // other fuel users without fuel have no light at all
        let kiln = at(8, 12, &sd);
        assert!(kiln.st.borrow().light.is_none());
        // a placed portal takes its stone from the saved level; levels outside 0..=5 keep the type
        let portal = at(13, 14, &sd);
        assert_eq!(portal.level(), 2);
        let t = crate::world::tiles::tile_at_world_position_loaded(14, 9, &f.world).unwrap().get();
        assert_eq!(t.type_index, 44);
        sd.map.insert("level".into(), PlistValue::int(9));
        let _high = at(13, 16, &sd);
        let t = crate::world::tiles::tile_at_world_position_loaded(16, 9, &f.world).unwrap().get();
        assert_eq!(t.type_index, 2);
        // replicas: every blockhead hears about the new bench; an unloaded position yields nothing
        let bh = blockhead(3, false);
        f.dw.blockheads.borrow_mut().push(bh);
        let nd = camp.update_net_data_for_client_impl(&(camp.clone() as ObjRef));
        let rep =
            Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd).expect("replica");
        assert_eq!(rep.level(), 2);
        let mut far = nd.clone();
        let mut hdr = <WorkbenchNetData as WireStruct>::read_le(&far[..0x38]);
        hdr.interaction_object_creation_net_data.dynamic_object_net_data.pos_x = 100;
        hdr.interaction_object_creation_net_data.dynamic_object_net_data.pos_y = 100;
        far[..0x38].copy_from_slice(&hdr.to_vec());
        assert!(Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &far).is_none());
        // a short blob raises
        let r = crate::foundation::exception::catch_ns(|| {
            Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd[..0x30]).is_some()
        });
        assert_eq!(r.unwrap_err().name, "NSRangeException");
    }

    #[test]
    fn portal_upgrades_write_each_stone_and_report() {
        let f = fixture();
        for (ty, base, x) in [(1u32, 0x20, 12), (13, 0x2a, 20)] {
            let (wb, me) = make(&f, ty, x, 10);
            wb.io.borrow_mut().current_blockhead = Some(blockhead(3, false));
            for level in 1..=6 {
                wb.upgrade_to_next_level(&me);
                assert_eq!(wb.level(), level);
                let stone = crate::world::tiles::tile_at_world_position_loaded(x, 9, &f.world).unwrap().get();
                let want = if level <= 5 { base + level as u8 } else { base + 5 };
                assert_eq!(stone.type_index, want, "portal {ty} level {level}");
            }
        }
        // a blockhead the server controls does not earn the achievement; none present is fine too
        f.deps.controlled.set(true);
        let (wb, me) = make(&f, 13, 30, 10);
        wb.io.borrow_mut().current_blockhead = Some(blockhead(3, false));
        wb.upgrade_to_next_level(&me);
        wb.io.borrow_mut().current_blockhead = None;
        wb.upgrade_to_next_level(&me);
        assert_eq!(wb.level(), 2);
        // other benches have no stone to write
        let (plain, pme) = make(&f, 2, 35, 10);
        plain.upgrade_to_next_level(&pme);
        assert_eq!(plain.level(), 1);
    }

    #[test]
    fn set_level_silently_updates_without_a_send() {
        let f = fixture();
        let (wb, me) = make(&f, 2, 10, 10);
        me.set_update_needs_to_be_sent(false);
        let before = f.dw.changed.borrow().len();
        wb.set_level_silently(&me, 2);
        assert_eq!(wb.level(), 2);
        assert!(!me.update_needs_to_be_sent());
        assert_eq!(f.dw.changed.borrow().len(), before + 1);
        assert!(wb.number_of_craftable_items_up_to_current_level() > 0);
    }

    #[test]
    fn solar_panel_at_noon_and_in_the_dark() {
        let f = fixture();
        let (wb, _me) = make(&f, 20, 10, 10);
        let above = crate::world::tiles::tile_at_world_position_loaded(10, 11, &f.world).unwrap();
        above.update(|t| t.sun_light = 255);
        let mut best = (0.0f32, 0.0f64);
        // one year is 3600 seconds: look for the best moment of it
        for step in 0..360 {
            let time = step as f64 * 10.0;
            f.world.set_world_time(time);
            let c = wb.combined_light_for_solar_panel();
            if c > best.0 {
                best = (c, time);
            }
        }
        assert!(best.0 >= 1.0, "the sun saturates the panel: {best:?}");
        f.world.set_world_time(best.1);
        above.update(|t| t.contents = 0x03);
        assert!((wb.combined_light_for_solar_panel() - best.0 / 2.0).abs() < 1e-6, "trees halve it");
        above.update(|t| t.contents = 46);
        assert_eq!(wb.combined_light_for_solar_panel(), 0.0);
    }

    #[test]
    fn save_and_restore_edge_cases() {
        let f = fixture();
        // a placed portal whose stone tile is not loaded still lights up when it is upgraded
        let (p, pme) = make(&f, 13, 10, 0);
        p.upgrade_to_next_level(&pme);
        assert_eq!(p.level(), 1);
        // empty stored source slots are not written; missing arrays leave the slot unset
        let (wb, me) = make(&f, 2, 12, 10);
        {
            let mut s = wb.st.borrow_mut();
            s.crafting_item_object = Some(CraftableItemObject::init_with_craftable_item(recipe_two_sources()));
            s.source_items[0] = Some(vec![]);
        }
        wb.io.borrow_mut().is_in_use = true;
        let sd = wb.get_save_dict_impl(&me);
        assert!(sd.map.get(&"sourceItems_0".to_string()).is_none());
        wb.st.borrow_mut().source_items[0] = None;
        assert!(wb.get_save_dict_impl(&me).map.get(&"sourceItems_0".to_string()).is_none());
        let ci = recipe_two_sources();
        let mut plain = new_mutable_dict();
        plain.map.insert("craftingItemData".into(), PlistValue::data(ci.to_vec()));
        wb.restore_crafting_state(&plain);
        assert!(wb.st.borrow().source_items[0].is_none());
        // set_needs_removed(false) does not touch the light
        DynamicObject::set_needs_removed(&*wb, &me, false);
        assert!(!me.needs_removed());
        // blockheads_loaded with nothing to link
        {
            let mut s = wb.st.borrow_mut();
            s.saved_blockhead_index_fuel = 0;
            s.is_in_use_fuel = false;
        }
        wb.blockheads_loaded_impl(&me);
        assert_eq!(wb.st.borrow().saved_blockhead_index_fuel, -1);
        wb.io.borrow_mut().is_in_use = true;
        wb.io.borrow_mut().current_blockhead = None;
        wb.st.borrow_mut().is_in_use_fuel = true;
        wb.st.borrow_mut().current_fuel_blockhead = None;
        wb.blockheads_loaded_impl(&me);
    }

    #[test]
    fn world_gone_paths() {
        let f = fixture();
        let (wb, me) = make(&f, 24, 10, 10);
        let Fixture { world, dw, dwr, deps, host } = f;
        drop(world);
        // remove, derived items and fuel changes cope without a world
        wb.init_sub_derived_items();
        wb.remove(&me, None);
        assert!(me.needs_removed());
        drop((dw, dwr, deps, host));
    }

    #[test]
    fn trait_object_views() {
        let f = fixture();
        let (wb, me) = make(&f, 21, 10, 10);
        wb.st.borrow_mut().available_electricity = 4096;
        // DynamicObject
        assert!(me.occupies_normal_contents());
        assert_eq!(me.freeblock_creation_item_type(), wb.destroy_item_type().0 as i32);
        assert!(me.free_block_creation_save_dict(&me).is_some());
        assert_eq!((me.free_block_creation_data_a(), me.free_block_creation_data_b()), (0, 0));
        assert_eq!(me.get_light_rgb(), [0.0, 0.0, 0.0]);
        // InteractionObject
        let il = me.as_interaction_like().unwrap();
        assert!(!il.is_double_height());
        assert_eq!(il.destroy_item_type(), wb.destroy_item_type().0 as i32);
        assert_eq!(il.title(), "FLYWHEEL");
        assert!(!il.requires_human_interaction());
        assert!(!il.can_be_used_in_expert_mode_when_not_owned());
        il.set_paused(&me, true);
        assert!(wb.st.borrow().paused);
        assert!(il.io_state().borrow().owner_name.is_none());
        let bh = blockhead(5, false);
        il.blockhead_would_like_to_take_ownership_with_save_dict(&me, &bh, None);
        assert!(me.is_in_use());
        il.remote_blockhead_removed_with_id(&me, 77);
        // PowerUser / WirePowerSource
        assert!(me.as_power_user().unwrap().is_storage_device());
        let src: &dyn WirePowerSource = &*wb;
        assert_eq!(src.available_electricity(), 4096);
        assert!(src.is_storage_device() && src.conducts_electricity() && !src.generates_electricity());
        assert!(src.subtract_electricty(96));
        assert_eq!(wb.available_electricity(), 4000);
        // the basic portal may be used in expert mode without owning it
        let (_portal, pme) = make(&f, 1, 20, 10);
        assert!(pme.as_interaction_like().unwrap().can_be_used_in_expert_mode_when_not_owned());
        // virtual dispatch of the object itself
        assert_eq!(me.class_info().name, "Workbench");
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        assert_eq!(me.get_save_dict(&me).map.get(&"workbenchType".to_string()).unwrap().int_value(), 21);
        let created = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(created, me.update_net_data_for_client(&me, None).unwrap());
        me.remote_update(&me, &created);
        me.blockheads_loaded(&me);
        me.blockhead_unloaded(&me, &bh);
        me.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&me, 0, 0);
        let (camp, cme) = make(&f, 3, 5, 10);
        assert!(camp.st.borrow().light.is_some());
        cme.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&cme, 0, 0);
        // a bench that is being used without a recipe still saves
        let (idle, ime) = make(&f, 2, 3, 10);
        idle.io.borrow_mut().is_in_use = true;
        assert!(idle.get_save_dict_impl(&ime).map.get(&"craftingItemDatav2".to_string()).is_none());
        // the test doubles
        assert!(f.dwr.get_owner_name_for_object_owner_id("nobody").is_none());
        let deps = crate::world::misc::deps();
        let (bench, bme) = make(&f, 2, 25, 10);
        assert_eq!(deps.workbench_level(&bme), 0);
        deps.workbench_upgrade_to_next_level(&bme);
        assert_eq!(bench.level(), 1);
        assert_eq!(deps.workbench_level(&bme), 1);
        deps.workbench_upgrade_to_next_level(&blockhead(9, false));
    }

    #[test]
    fn solar_panel_physical_block_follows_the_light() {
        let f = fixture();
        // a solar panel in the dark does not need its block loaded, one in the sun does
        let (solar, sme) = make(&f, 20, 10, 10);
        solar.st.borrow_mut().available_electricity = 1;
        assert!(!sme.requires_physical_block());
        crate::world::tiles::tile_at_world_position_loaded(10, 11, &f.world).unwrap().update(|t| t.sun_light = 255);
        assert!(sme.requires_physical_block());
    }

    #[test]
    fn net_replica_physical_block_rules() {
        let f = fixture();
        let (src, sme) = make(&f, 2, 10, 10);
        let nd = src.update_net_data_for_client_impl(&sme);
        let rep = Workbench::init_with_world_dynamic_world_cache_net_data(&f.world, &f.dwr, None, &nd).unwrap();
        let rme: ObjRef = rep.clone();
        assert!(!rme.requires_physical_block());
        rep.io.borrow_mut().is_in_use = true;
        assert!(rme.requires_physical_block());
        rep.io.borrow_mut().is_in_use = false;
        rep.st.borrow_mut().is_in_use_fuel = true;
        assert!(rme.requires_physical_block());
    }
}
