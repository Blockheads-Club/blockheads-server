//! Factories of `DynamicWorld`: sowing and loading of trees / plants, the standard dynamic-object factories,
//! add/remove wrappers, treasure chest / troll generation, doors, rails, boats, train cars, glow / gather blocks,
//! blockhead teleport and creation.
//!
//! Covers `addDoorAtPos:ofType:saveDict:placedByClient:`, `removeDoorAtPos:`, `placeBoatInWaterAtPos:...`,
//! `placeTrainCarAtPos:...`, `loadGlowBlockIfNeededAtPos:tile:`, `loadGatherBlockAtPos:`,
//! `teleportBlockhead:toWorkbench:`, `loadNewBlockheadAtPos:...` and `loadDebugChestAtPos:chest:`.
//!
//! All methods take `self: &Rc<DynamicWorld>` (the original passes `self` to the constructors). Concrete object
//! constructors that are not in the `ClassFactory` table go through [`DwOpsHooks`](super::lookups::DwOpsHooks).

use std::any::Any;
use std::rc::Rc;

use super::lookups::{ops, wi_key};
use super::state::DynamicWorld;
use crate::fmath::cconv::f32_to_i32;
use crate::items::inventory_item::{InventoryItem, ItemRef};
use crate::items::misc::interaction_object_type_for_item_type;
use crate::nslog;
use crate::objects::base::{DynamicWorldRef, WorldRef as ApiWorldRef};
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::objects::type_tables::{
    class_for_dynamic_object_type, class_for_interaction_object_type, object_type_has_static_position,
    object_type_is_interaction_object, PLANT_DYNAMIC_OBJECT_TYPES, TREE_DYNAMIC_OBJECT_TYPES,
};
use crate::plist::value::PlistDict;
use crate::rng::glibc_rand::{r01, r01_from, rand};
use crate::types::enums::NPCType;
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, Vector2};
use crate::world::growth::{plant_type_for_seed_item_type, tree_type_for_seed_item_type};
use crate::world::helpers::{
    dynamic_object_type_for_npc_type, tile_is_air, tile_is_air_or_snow, tile_is_air_water_or_snow, tile_is_plant,
    tile_is_solid,
};
use crate::world::state::TileRef;
use crate::world::tiles::{macro_index_at_world_index, tile_at_world_position_loaded, world_index_at_world_position};

/// Float32 `clamp(v, lo, hi)` of the treasure code.
#[inline]
fn clampf(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// `(float)rand() / 2147483648.0f` (`R()`), possibly exactly 1.0.
#[inline]
fn rr() -> f32 {
    r01()
}

impl DynamicWorld {
    /// `(World, DynamicWorld)` handles in the shape the object_model constructors take.
    fn api_refs(self: &Rc<Self>) -> (ApiWorldRef, DynamicWorldRef) {
        let w: ApiWorldRef = self.world();
        let d: DynamicWorldRef = self.clone();
        (w, d)
    }

    /// `tileAtWorldPositionLoaded(x, y, world)`.
    fn tile_at(&self, x: i32, y: i32) -> Option<TileRef> {
        tile_at_world_position_loaded(x, y, &self.world())
    }

    // =================================================================================================
    // Sowing and loading of trees and plants
    // =================================================================================================

    /// `-sowTreeNearParent:adult:adultMaxAge:`: two attempts, exactly one `rand()` each (no early exit).
    /// the attempt, RNG already drawn stays consumed).
    pub fn sow_tree_near_parent_adult_adult_max_age(self: &Rc<Self>, parent: &ObjRef, adult: bool, adult_max_age: f32) {
        let hooks = ops();
        let parent_pos = parent.pos();
        for _attempt in 0..2 {
            let r = rand();
            let ix = f32_to_i32(parent_pos.x as f32 + 32.0f32 * (r01_from(r) - 0.5f32));
            let x_pos = if ix < 0 { ix.wrapping_sub(1) } else { ix.wrapping_add(2) };
            let mut blocked = false;
            'types: for t in TREE_DYNAMIC_OBJECT_TYPES {
                for list in [self.dot_snapshot(t), self.add_snapshot(t)] {
                    for tree in list {
                        let px = tree.pos().x;
                        if x_pos.wrapping_sub(2) < px && px < x_pos.wrapping_add(2) {
                            blocked = true;
                            break 'types; // y is NOT compared
                        }
                    }
                }
            }
            if blocked {
                continue;
            }
            // find ground below the parent's row
            let mut y = parent_pos.y;
            let mut tile = self.tile_at(x_pos, y);
            let mut count = 0;
            let mut fail = false;
            while tile_is_air_water_or_snow(tile) {
                y -= 1;
                tile = self.tile_at(x_pos, y);
                count += 1;
                if count >= 16 {
                    fail = true;
                    break;
                }
            }
            if fail {
                continue;
            }
            let mut above = self.tile_at(x_pos, y + 1);
            if count == 0 {
                // ground at the parent's own row: climb through solid, at most 8 steps
                loop {
                    if tile_is_air_water_or_snow(above) {
                        break;
                    }
                    tile = above;
                    y += 1;
                    above = self.tile_at(x_pos, y + 1);
                    count += 1;
                    if count >= 8 {
                        fail = true;
                        break;
                    }
                }
                if fail {
                    continue;
                }
            }
            let (Some(tile), Some(above_t)) = (tile, above) else { continue }; // UB-PARITY: NULL tile deref in the original
            if hooks.tree_is_required_soil_type(parent, tile.type_index())
                && above_t.contents() == 0
                && tile_is_air_or_snow(above)
                && above_t.back_wall_type_index() == 2
            {
                self.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(
                    IntPair { x: x_pos, y: y + 1 },
                    hooks.tree_type(parent),
                    hooks.tree_max_height_gene_variation(parent) as i16,
                    hooks.tree_growth_rate_gene_variation(parent) as i16,
                    adult,
                    adult_max_age,
                );
            }
        }
    }

    /// `-findBreedingPlantNearPlant:`: one `rand()` only when candidates exist; `dynamicObjects` only.
    pub fn find_breeding_plant_near_plant(&self, plant: &ObjRef) -> Option<ObjRef> {
        let hooks = ops();
        let w = self.world().world_width_macro().wrapping_mul(32);
        let pp = plant.pos();
        let mut cands: Vec<ObjRef> = Vec::new();
        for other in self.dot_snapshot(plant.object_type()) {
            if ptr_eq(&other, plant) {
                continue;
            }
            if !hooks.plant_can_breed(&other) {
                continue;
            }
            let op = other.pos();
            let dx = super::lookups::wrap_dx(op.x.wrapping_sub(pp.x), w);
            if dx.wrapping_abs() < 11 && op.y.wrapping_sub(pp.y).wrapping_abs() < 11 {
                cands.push(other);
            }
        }
        if cands.is_empty() {
            return None;
        }
        let r = rand();
        let idx = ((r as i64 as u64) % cands.len() as u64) as usize;
        Some(cands[idx].clone())
    }

    /// `-sowPlantNearParent:`: not on a client; two attempts, one `rand()` each; the plant-type loop
    /// stops at the first match.
    pub fn sow_plant_near_parent(self: &Rc<Self>, parent: &ObjRef) {
        if self.is_client() {
            return;
        }
        let hooks = ops();
        let parent_pos = parent.pos();
        for _attempt in 0..2 {
            let r = rand();
            let ix = f32_to_i32(parent_pos.x as f32 + 4.0f32 * (r01_from(r) - 0.5f32));
            let x_pos = if ix < 0 { ix.wrapping_sub(1) } else { ix.wrapping_add(2) };
            let mut blocked = false;
            'types: for t in PLANT_DYNAMIC_OBJECT_TYPES {
                for list in [self.dot_snapshot(t), self.add_snapshot(t)] {
                    if list.iter().any(|o| o.pos().x == x_pos) {
                        blocked = true;
                        break 'types; // y not compared
                    }
                }
            }
            if blocked {
                continue;
            }
            let mut y = parent_pos.y;
            let mut tile = self.tile_at(x_pos, y);
            let mut count = 0;
            let mut fail = false;
            while tile_is_air_water_or_snow(tile) {
                y -= 1;
                tile = self.tile_at(x_pos, y);
                count += 1;
                if count >= 16 {
                    fail = true;
                    break;
                }
            }
            if fail {
                continue;
            }
            let mut above = self.tile_at(x_pos, y + 1);
            let pt = hooks.plant_type(parent);
            if count == 0 && pt != 7 {
                loop {
                    if tile_is_air_water_or_snow(above) {
                        break;
                    }
                    tile = above;
                    y += 1;
                    above = self.tile_at(x_pos, y + 1);
                    count += 1;
                    if count >= 8 {
                        fail = true;
                        break;
                    }
                }
                if fail {
                    continue;
                }
            }
            let pt = hooks.plant_type(parent);
            let mg = hooks.plant_max_age_gene_variation(parent) as i16;
            let gg = hooks.plant_growth_rate_gene_variation(parent) as i16;
            if pt == 7 {
                // VINE
                let Some(t) = tile else { continue };
                if hooks.plant_is_required_soil_type(parent, t.type_index()) && t.foreground_contents() == 0 {
                    self.load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(
                        IntPair { x: x_pos, y },
                        7,
                        mg,
                        gg,
                        false,
                    );
                }
            } else {
                let (Some(t), Some(a)) = (tile, above) else { continue };
                if hooks.plant_is_required_soil_type(parent, t.type_index()) && a.foreground_contents() == 0 {
                    let ok = if pt == 6 { a.type_index() == 3 } else { a.type_index() == 2 };
                    if ok {
                        self.load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(
                            IntPair { x: x_pos, y: y + 1 },
                            pt,
                            mg,
                            gg,
                            false,
                        );
                    }
                }
            }
        }
    }

    /// `-sowTreeOrPlantAtPosition:itemType:maxHeight:growthRate:`.
    pub fn sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
        self: &Rc<Self>,
        pos: IntPair,
        item_type: i32,
        max_height: i16,
        growth_rate: i16,
    ) {
        let tt = tree_type_for_seed_item_type(ItemType(item_type as u32));
        if tt.0 == 0 {
            let pt = plant_type_for_seed_item_type(ItemType(item_type as u32));
            self.load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(
                pos,
                pt.0 as i32,
                max_height,
                growth_rate,
                false,
            );
        } else {
            self.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(
                pos,
                tt.0 as i32,
                max_height,
                growth_rate,
                false,
                99999.0f32,
            );
        }
    }

    /// `-loadTreeAtPosition:type:maxHeight:growthRate:adultTree:adultMaxAge:`: duplicate check, construct,
    /// register (NOT in the by-position index, NO net send).
    ///
    pub fn load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(
        self: &Rc<Self>,
        pos: IntPair,
        tree_type: i32,
        mh: i16,
        gr: i16,
        adult_tree: bool,
        adult_max_age: f32,
    ) {
        let conflicts = |tree: &ObjRef| -> bool {
            let tp = tree.pos();
            tp.y == pos.y
                && pos.x.wrapping_sub(1) <= tp.x
                && tp.x <= pos.x.wrapping_add(1)
                && (adult_tree || tp.x == pos.x)
        };
        for t in TREE_DYNAMIC_OBJECT_TYPES {
            for tree in self.dot_snapshot(t) {
                if conflicts(&tree) {
                    nslog!("trying to load tree already loaded");
                    return;
                }
            }
            for tree in self.add_snapshot(t) {
                if conflicts(&tree) {
                    nslog!("trying to load tree already added to load queue");
                    return;
                }
            }
        }
        if let Some(tree) = ops().new_tree(self, tree_type, pos, mh, gr, adult_tree, adult_max_age) {
            self.register_to_add(&tree, tree.object_type());
        }
    }

    /// `-loadPlantAtPosition:type:maxAgeGene:growthRateGene:adult:`: registers in `toAdd` and the
    /// by-position index (no occupancy check), then SEND_CREATE.
    pub fn load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(
        self: &Rc<Self>,
        pos: IntPair,
        plant_type: i32,
        max_age_gene: i16,
        growth_rate_gene: i16,
        adult: bool,
    ) {
        if let Some(plant) = ops().new_plant(self, plant_type, pos, max_age_gene, growth_rate_gene, adult) {
            self.register_with_pos(&plant, plant.object_type(), pos);
            self.send_net_data_if_needed_for_object_is_creation(&plant, true);
        }
    }

    // =================================================================================================
    // Standard factories
    // =================================================================================================

    /// `-loadStandardDynamicObjectOfType:atPos:`: indexed by the PARAMETER type; no net send; skips the
    /// by-position index when `wi == -1`.
    pub fn load_standard_dynamic_object_of_type_at_pos(self: &Rc<Self>, t: i32, pos: IntPair) -> Option<ObjRef> {
        let mut wi = -1;
        if object_type_has_static_position(t) {
            wi = self.world_index(pos);
            if let Some(existing) = self.by_pos_get(t, wi) {
                if !existing.needs_removed() {
                    return None; // slot occupied by a live object
                }
            }
        }
        let ctor = class_for_dynamic_object_type(t)?.from_position?;
        let (w, d) = self.api_refs();
        let o = ctor(&w, &d, pos, self.cache.clone())?;
        self.register_to_add(&o, t);
        if wi != -1 {
            self.by_pos_set(t, wi, &o);
        }
        Some(o)
    }

    /// `-loadSurfaceBlockAtPos:`.
    pub fn load_surface_block_at_pos(self: &Rc<Self>, pos: IntPair) {
        self.load_standard_dynamic_object_of_type_at_pos(22, pos);
    }

    /// `-loadSnowSurfaceBlockAtPos:loadSnow:`.
    pub fn load_snow_surface_block_at_pos_load_snow(self: &Rc<Self>, pos: IntPair, load_snow: bool) {
        let o = self.load_standard_dynamic_object_of_type_at_pos(29, pos);
        if load_snow {
            if let Some(o) = o {
                ops().object_update_in_time_since_saved(&o);
            }
        }
    }

    /// `-loadNPCAtPosition:type:saveDict:isAdult:wasPlaced:placedByClient:`: the 512 cap and the crowding
    /// test apply only when `!was_placed`.
    pub fn load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        npc_type: i32,
        save_dict: Option<&PlistDict>,
        is_adult: bool,
        was_placed: bool,
        client_id: Option<&str>,
    ) -> bool {
        if !was_placed {
            let dt = dynamic_object_type_for_npc_type(NPCType(npc_type as u32)).0 as i32;
            if self.loaded_count_of_objects_of_type(dt) > 512 {
                return false;
            }
            if self.too_many_npcs_to_spawn_more_near_pos(pos) {
                return false;
            }
        }
        match ops().new_npc(self, npc_type, pos, save_dict, is_adult, was_placed, client_id) {
            Some(npc) => {
                self.register_to_add(&npc, npc.object_type());
                self.send_net_data_if_needed_for_object_is_creation(&npc, true);
                true
            }
            None => false,
        }
    }

    /// `-workbenchPlacedAtPosition:ofType:saveDict:placedByClient:clientName:`: `workbenchHasBeenCrafted`
    /// uses the `ty` ARGUMENT, the portal index uses `[wb type]`.
    pub fn workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
        self: &Rc<Self>,
        pos: IntPair,
        ty: i32,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
        cname: Option<&str>,
    ) -> Option<ObjRef> {
        let wb = ops().new_workbench(self, pos, ty, false, sd, cid, cname)?;
        self.register_with_pos(&wb, wb.object_type(), pos);
        self.send_net_data_if_needed_for_object_is_creation(&wb, true);
        if ty != 1 {
            self.workbench_has_been_crafted.set(true);
        }
        let t = self.hooks().workbench_type(&wb);
        if t == 1 || t == 13 {
            let wi = self.world_index(wb.pos());
            self.portal_positions.borrow_mut().add_index(wi as i64 as u64);
        }
        Some(wb)
    }

    /// `-interactionObjectPlacedAtPosition:withItem:flipped:saveDict:placedByClient:clientName:`.
    pub fn interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
        self: &Rc<Self>,
        pos: IntPair,
        item: &InventoryItem,
        flipped: bool,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
        cname: Option<&str>,
    ) -> Option<ObjRef> {
        let iot = interaction_object_type_for_item_type(item.item_type).0 as i32;
        if iot == 0 {
            return None;
        }
        let ctor = class_for_interaction_object_type(iot)?.from_position_interaction?;
        let (w, d) = self.api_refs();
        let o = ctor(&w, &d, pos, self.cache.clone(), Some(item), flipped, sd, cid, cname)?;
        self.register_with_pos(&o, o.object_type(), pos);
        self.send_net_data_if_needed_for_object_is_creation(&o, true);
        let hooks = ops();
        if iot == 8 {
            hooks.world_ownership_sign_was_placed_or_changed_at_pos(
                pos,
                hooks.interaction_object_land_owner_id(&o).as_deref(),
                hooks.interaction_object_width_radius(&o),
                hooks.interaction_object_height_radius(&o),
                false,
            );
        } else if iot == 6 {
            self.rail_or_station_name_changed();
        }
        Some(o)
    }

    /// `-placeFireAtPosition:`: `onFire = 1` only when the fire object was created, then
    /// `worldChanged(pos, reliable)`, then the net send.
    /// Synthesis (NULL tile: return None).
    pub fn place_fire_at_position(self: &Rc<Self>, pos: IntPair) -> Option<ObjRef> {
        let tile = self.tile_at(pos.x, pos.y)?;
        if tile.on_fire() != 0 {
            return None;
        }
        if tile_is_plant(Some(tile)) && self.get_plant_at_pos(pos).is_none() {
            tile.set_foreground_contents(0); // stale plant sprite
            return None;
        }
        let fire = self.load_standard_dynamic_object_of_type_at_pos(16, pos);
        if let Some(f) = &fire {
            tile.set_on_fire(1);
            self.world_changed_at_pos_send_reliably(pos, true);
            self.send_net_data_if_needed_for_object_is_creation(f, true);
        }
        fire
    }

    /// `-addTorchAtPos:ofType:dataA:dataB:saveDict:placedByClient:`. The original indexes the second lookup
    /// by the ItemType parameter (bug); rule per Synthesis resolutions: bug path only when `byPos[17]` has an entry;
    /// type 17 -> normal needsRemoved test, any other type -> return without adding. Registered under the literal 17.
    pub fn add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        ty: i32,
        a: u16,
        b: u16,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
    ) {
        let wi = self.world_index(pos);
        if self.by_pos_contains(17, wi) {
            // UB-PARITY
            if ty == 17 {
                if let Some(o) = self.by_pos_get(17, wi) {
                    if !o.needs_removed() {
                        return;
                    }
                }
            } else {
                return;
            }
        }
        if let Some(t) = ops().new_torch(self, pos, ty, a, b, sd, cid) {
            self.register_to_add(&t, 17);
            self.by_pos_set(17, wi, &t);
            self.send_net_data_if_needed_for_object_is_creation(&t, true);
        }
    }

    /// `-removeStandardObject:`: types 14, 24 and the 9 interaction types only log; doors go through
    /// `removeDoorAtPos:`; otherwise `setNeedsRemoved:` then the first of normal / foreground / background contents is
    /// cleared. The object is NOT erased from the maps.
    pub fn remove_standard_object(&self, object: Option<&ObjRef>) {
        let Some(o) = object else { return }; // nil: type 0, every send is a no-op
        let t = o.object_type();
        if t == 14 || t == 24 || object_type_is_interaction_object(t) {
            nslog!("Objects of type {} shouldn't be getting removed this way", t);
        } else if t == 20 {
            self.remove_door_at_pos(o.pos());
        } else {
            o.set_needs_removed(o, true);
            let p = o.pos();
            if o.occupies_normal_contents() {
                if let Some(tile) = self.tile_at(p.x, p.y) {
                    tile.set_contents(0);
                }
                self.world_changed_at_pos_send_reliably(p, true);
            } else if o.occupies_foreground_contents() {
                if let Some(tile) = self.tile_at(p.x, p.y) {
                    tile.set_foreground_contents(0);
                }
                self.world_changed_at_pos_send_reliably(p, true);
            } else if o.occupies_background_contents() {
                if let Some(tile) = self.tile_at(p.x, p.y) {
                    tile.set_background_contents(0);
                }
                self.world_changed_at_pos_send_reliably(p, true);
            }
        }
    }

    /// `-createTreasureChestOrTrollAtTile:atPos:loadTroll:loadTreasure:`. RNG order exactly as the spec.
    pub fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
        self: &Rc<Self>,
        tile: &TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
        if self.interaction_object_at_pos(pos).is_some() {
            return;
        }
        let hooks = ops();
        let mut troll_spawned = false;
        let at = |x: i32, y: i32| self.tile_at(x, y);
        let air_free = |t: Option<TileRef>| -> bool {
            tile_is_air(t) && t.map_or(false, |t| t.back_wall_type_index() == 1 && t.contents() == 0)
        };

        if load_troll && pos.y < 0x100 {
            let dx = if pos.x % 32 < 17 { 2 } else { -2 };
            let mut troll_pos = IntPair { x: pos.x + dx, y: pos.y - 1 };
            if tile_is_solid(at(troll_pos.x, pos.y - 2)) {
                let mut troll_tile = at(troll_pos.x, troll_pos.y);
                if tile_is_solid(troll_tile) {
                    troll_pos.y += 1;
                    troll_tile = at(troll_pos.x, troll_pos.y);
                    if tile_is_solid(troll_tile) {
                        troll_pos.y += 1;
                        troll_tile = at(troll_pos.x, troll_pos.y);
                    }
                }
                if air_free(troll_tile) {
                    let above = at(troll_pos.x, troll_pos.y + 1);
                    if air_free(above) {
                        self.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                            troll_pos, 6, None, true, false, None,
                        ); // result ignored
                        troll_spawned = true; // regardless of loadNPC's return value
                        for side in [-1, 1] {
                            let t = at(troll_pos.x + side, troll_pos.y + 1);
                            if air_free(t) && t.map_or(false, |t| t.foreground_contents() == 0) {
                                self.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
                                    IntPair { x: troll_pos.x + side, y: troll_pos.y + 1 },
                                    183,
                                    0,
                                    0,
                                    None,
                                    None,
                                );
                                if let Some(t) = t {
                                    t.set_foreground_contents(0x61);
                                }
                            }
                        }
                        // trade portal
                        let mut pp = troll_pos;
                        pp.x += dx;
                        pp.y -= 1;
                        if tile_is_solid(at(pp.x, troll_pos.y - 2)) {
                            let mut ptile = at(pp.x, pp.y);
                            if tile_is_solid(ptile) {
                                pp.y += 1;
                                ptile = at(pp.x, pp.y);
                                if tile_is_solid(ptile) {
                                    pp.y += 1;
                                    ptile = at(pp.x, pp.y);
                                }
                            }
                            if air_free(ptile) {
                                let a1 = at(pp.x, pp.y + 1);
                                let a2 = at(pp.x, pp.y + 2);
                                if air_free(a1) && air_free(a2) {
                                    let ppos = IntPair { x: pp.x, y: pp.y + 1 };
                                    if let Some(portal) =
                                        hooks.new_trade_portal(self, ppos, 210, false, None, None, None)
                                    {
                                        hooks.trade_portal_randomize_local_trade_offsets(&portal);
                                        self.register_with_pos(&portal, portal.object_type(), ppos);
                                        self.send_net_data_if_needed_for_object_is_creation(&portal, true);
                                        if let (Some(a1), Some(a2)) = (a1, a2) {
                                            a2.set_contents(0x30);
                                            a1.set_contents(0x30);
                                        }
                                        self.world_changed_at_pos_send_reliably(IntPair { x: pp.x, y: pp.y }, true);
                                        self.world_changed_at_pos_send_reliably(IntPair { x: pp.x, y: pp.y + 2 }, true);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if load_treasure {
            tile.set_contents(0x30); // the passed-in tile, before the chest is created
            if let Some(chest) = hooks.new_chest(self, pos, 1072, false, None, None, None) {
                let mut coins: Vec<ItemRef> = Vec::new();
                let mut fill_index = 0;
                let fill = rr() * rr() * rr(); // three rand() calls in this order
                let f16 = 16.0f32 * fill;
                let t = if 15.0f32 <= f16 { 15.0f32 } else { f16 };
                let number_of_coin_slots = f32_to_i32(t + 1.0f32);
                for _slot in 0..number_of_coin_slots {
                    let mut rt = rr();
                    if troll_spawned {
                        rt = 0.2f32 * rt + 0.8f32;
                    }
                    if rt <= 0.7f32 {
                        let mut coin_type = 166u32;
                        if 0.9f32 < rr() {
                            coin_type = 167;
                        }
                        let a = rr();
                        let b = rr();
                        let c = rr();
                        let raw = f32_to_i32(a * b * c * 400.0f32);
                        let mut count = f32_to_i32(clampf(raw as f32, 1.0f32, 99.0f32));
                        if count == 99 {
                            let rp = rr();
                            if 0.5f32 < rp {
                                coin_type = 260;
                                count = f32_to_i32(1.0f32 + (rp - 0.5f32) * 10.0f32);
                            }
                        }
                        for _ in 0..count {
                            coins.push(InventoryItem::new(ItemType(coin_type), 0, 0, None, None));
                        }
                        hooks.chest_move_inventory_items_from_array_to_index_count(
                            &chest, &mut coins, fill_index, count,
                        );
                    } else {
                        let mut item_type = 329u32;
                        if rt > 0.9f32 {
                            item_type = 72; // GOLD_INGOT (IRON_INGOT is overwritten in the binary)
                            if rt > 0.96f32 {
                                let g = rr();
                                item_type = if g > 0.5f32 {
                                    87
                                } else if g > 0.25f32 {
                                    86
                                } else if g > 0.125f32 {
                                    76
                                } else if g > 0.04f32 {
                                    75
                                } else {
                                    88
                                };
                            }
                        }
                        let a = rr();
                        let b = rr();
                        let count = f32_to_i32(clampf(f32_to_i32(a * b * 10.0f32) as f32, 1.0f32, 10.0f32));
                        for _ in 0..count {
                            coins.push(InventoryItem::new(ItemType(item_type), 0, 0, None, None));
                        }
                        hooks.chest_move_inventory_items_from_array_to_index_count(
                            &chest, &mut coins, fill_index, count,
                        );
                    }
                    fill_index += 1;
                }
                self.register_with_pos(&chest, chest.object_type(), pos);
                self.send_net_data_if_needed_for_object_is_creation(&chest, true);
            }
        }
    }

    /// `-addEggAtPos:saveDict:`.
    pub fn add_egg_at_pos_save_dict(self: &Rc<Self>, pos: IntPair, sd: Option<&PlistDict>) {
        if let Some(egg) = ops().new_egg(self, pos, sd) {
            self.register_with_pos(&egg, egg.object_type(), pos);
            self.send_net_data_if_needed_for_object_is_creation(&egg, true);
        }
    }

    /// `-addPaintingAtPos:ofType:saveDict:placedByClient:clientName:`.
    pub fn add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
        self: &Rc<Self>,
        pos: IntPair,
        ty: i32,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
        cname: Option<&str>,
    ) {
        let wi = self.world_index(pos);
        if let Some(existing) = self.by_pos_get(52, wi) {
            if !existing.needs_removed() {
                return;
            }
        }
        if let Some(p) = ops().new_painting(self, pos, ty, sd, cid, cname) {
            self.register_to_add(&p, p.object_type());
            self.by_pos_set(p.object_type(), wi, &p);
            self.send_net_data_if_needed_for_object_is_creation(&p, true);
        }
    }

    /// `-addStandardObjectAtPos:objectType:itemType:saveDict:placedByClient:`.
    pub fn add_standard_object_at_pos_object_type_item_type_save_dict_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        ot: i32,
        it: i32,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
    ) {
        let wi = self.world_index(pos);
        if let Some(existing) = self.by_pos_get(ot, wi) {
            if !existing.needs_removed() {
                return;
            }
        }
        let Some(ctor) = class_for_dynamic_object_type(ot).and_then(|f| f.from_position_type_save_dict) else { return };
        let (w, d) = self.api_refs();
        if let Some(o) = ctor(&w, &d, pos, self.cache.clone(), it, sd, cid) {
            self.register_to_add(&o, o.object_type());
            self.by_pos_set(o.object_type(), wi, &o);
            self.send_net_data_if_needed_for_object_is_creation(&o, true);
        }
    }

    /// `-openElevatorAtPos:`: the sound is client-only and dropped.
    pub fn open_elevator_at_pos(&self, pos: IntPair) {
        let hooks = ops();
        if let Some(s) = self.elevator_shaft_at_pos(pos) {
            hooks.elevator_shaft_open(&s);
        }
        if let Some(s) = self.elevator_shaft_at_pos(IntPair { x: pos.x, y: pos.y + 1 }) {
            hooks.elevator_shaft_open(&s);
        }
    }

    /// `-addRailAtPos:ofType:ownedByStation:`.
    pub fn add_rail_at_pos_of_type_owned_by_station(self: &Rc<Self>, pos: IntPair, ty: i32, owned: bool) {
        let wi = self.world_index(pos);
        if let Some(existing) = self.by_pos_get(40, wi) {
            if !existing.needs_removed() {
                return;
            }
        }
        if let Some(r) = ops().new_rail(self, pos, ty, owned) {
            self.register_to_add(&r, r.object_type());
            self.by_pos_set(r.object_type(), wi, &r);
            self.send_net_data_if_needed_for_object_is_creation(&r, true);
            self.rail_or_station_name_changed();
        }
    }

    /// `-addDoorAtPos:ofType:saveDict:placedByClient:`: the tile writes happen even when the slot was
    /// occupied (NULL tiles skipped, Synthesis).
    pub fn add_door_at_pos_of_type_save_dict_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        ty: i32,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
    ) {
        self.add_standard_object_at_pos_object_type_item_type_save_dict_placed_by_client(pos, 20, ty, sd, cid);
        if let Some(tile) = self.tile_at(pos.x, pos.y) {
            if ty == 52 || ty == 164 {
                tile.set_background_contents(0x46);
                if let Some(up) = self.tile_at(pos.x, pos.y + 1) {
                    up.set_background_contents(0x46);
                }
            } else if ty == 69 || ty == 165 {
                tile.set_background_contents(0x4b);
            } else if ty == 304 {
                tile.set_contents(0x92);
            }
        }
        // recalculateDrawBlockLightingForTile: empty on the server
        self.world_changed_at_pos_send_reliably(pos, true);
    }

    /// `-removeDoorAtPos:`.
    pub fn remove_door_at_pos(&self, pos: IntPair) -> Option<ObjRef> {
        let hooks = self.hooks();
        let mut remove_other = false;
        let mut other = pos;
        let mut door = self.object_of_type_at_pos(20, pos);
        match &door {
            None => {
                let below = IntPair { x: pos.x, y: pos.y - 1 };
                door = self.object_of_type_at_pos(20, below);
                if let Some(d) = door.clone() {
                    let it = hooks.item_type(&d);
                    if it == 52 || it == 164 {
                        if let Some(t) = self.tile_at(below.x, below.y) {
                            t.set_background_contents(0);
                        }
                        remove_other = true;
                        other = below;
                    } else {
                        door = None;
                    }
                }
            }
            Some(d) => {
                let it = hooks.item_type(d);
                if it == 52 || it == 164 {
                    let above = IntPair { x: pos.x, y: pos.y + 1 };
                    if let Some(t) = self.tile_at(above.x, above.y) {
                        t.set_background_contents(0);
                    }
                    remove_other = true;
                    other = above;
                }
            }
        }
        if let Some(d) = door.clone() {
            d.set_needs_removed(&d, true);
            if let Some(t) = self.tile_at(pos.x, pos.y) {
                if hooks.item_type(&d) == 304 {
                    t.set_contents(0);
                } else {
                    t.set_background_contents(0);
                }
            }
            self.world_changed_at_pos_send_reliably(pos, true);
            if remove_other {
                ops().world_remove_tile_at_world_x_world_y(other.x, other.y);
            }
        }
        door
    }

    /// `-placeBoatInWaterAtPos:saveDict:placedByClient:`.
    pub fn place_boat_in_water_at_pos_save_dict_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
    ) {
        if let Some(boat) = ops().new_boat(self, pos, sd, cid) {
            self.register_to_add(&boat, boat.object_type());
            self.send_net_data_if_needed_for_object_is_creation(&boat, true);
        }
    }

    /// `-placeTrainCarAtPos:ofType:saveDict:placedByClient:`: snap to the rail, hand car skips the
    /// adjacency step, otherwise position next to an existing car. A NULL tile at `fg(rtPos)` is "not a rail"
    /// (Synthesis resolutions).
    pub fn place_train_car_at_pos_of_type_save_dict_placed_by_client(
        self: &Rc<Self>,
        pos: IntPair,
        ty: i32,
        sd: Option<&PlistDict>,
        cid: Option<&str>,
    ) {
        const RAIL: u8 = 98;
        let mut pos = pos;
        let fg = |x: i32, y: i32| self.tile_at(x, y);
        if let Some(t) = fg(pos.x, pos.y) {
            if t.foreground_contents() != RAIL {
                if let Some(t2) = fg(pos.x, pos.y - 1) {
                    if t2.foreground_contents() == RAIL {
                        pos.y -= 1;
                    }
                }
            }
        }
        if ty != 204 {
            for offset in [-1i32, 1] {
                let Some(rt) = fg(pos.x + offset, pos.y) else { continue };
                let mut rail = true;
                let mut rt_pos = IntPair { x: pos.x + offset, y: pos.y };
                if rt.foreground_contents() != RAIL {
                    rt_pos = if !tile_is_solid(Some(rt)) {
                        IntPair { x: pos.x + offset, y: pos.y - 1 }
                    } else {
                        IntPair { x: pos.x + offset, y: pos.y + 1 }
                    };
                    let rt2 = fg(rt_pos.x, rt_pos.y);
                    if rt2.map_or(true, |t| t.foreground_contents() != RAIL) {
                        rail = false;
                    }
                }
                if rail {
                    let center = Vector2 { x: rt_pos.x as f32 + 0.5f32, y: rt_pos.y as f32 + 1.5f32 };
                    if self.check_for_train_car_under_tap(center).is_some() {
                        pos.x -= offset;
                        let left = fg(pos.x, pos.y);
                        if !tile_is_solid(left) {
                            if let Some(l) = left {
                                if l.foreground_contents() != RAIL {
                                    let bl = fg(pos.x, pos.y - 1);
                                    if !tile_is_solid(bl) {
                                        pos.y -= 1;
                                    }
                                }
                            }
                        } else {
                            pos.y += 1;
                        }
                        break;
                    }
                }
            }
        }
        if let Some(car) = ops().new_train_car(self, ty, pos, sd, cid) {
            self.register_to_add(&car, car.object_type());
            self.send_net_data_if_needed_for_object_is_creation(&car, true);
        }
    }

    /// `-loadGlowBlockIfNeededAtPos:tile:`.
    pub fn load_glow_block_if_needed_at_pos_tile(self: &Rc<Self>, pos: IntPair, tile: &TileRef) {
        let world = self.world();
        let wi = world_index_at_world_position(pos.x, pos.y, &world);
        let mi = macro_index_at_world_index(wi, &world);
        let Some(mt) = (if mi >= 0 { world.macro_tiles().get(mi as usize) } else { None }) else { return };
        if mt.physical_block().is_none() || !mt.dynamic_objects_loaded.get() {
            return;
        }
        if self.by_pos_contains(18, wi)
            || self.currently_adding_glow_blocks.borrow().contains(&wi_key(wi))
            || self.currently_loading_macro_blocks.borrow()[18].contains(&(mi as u32))
        {
            return;
        }
        self.currently_adding_glow_blocks.borrow_mut().insert(wi_key(wi));
        let gb = ops().new_glow_block(self, pos, *tile);
        self.currently_adding_glow_blocks.borrow_mut().remove(&wi_key(wi));
        if let Some(gb) = gb {
            let t = gb.object_type();
            self.register_to_add(&gb, t);
            self.by_pos_set(t, wi, &gb);
        }
    }

    /// `-loadGatherBlockAtPos:`: `client == nil` only.
    pub fn load_gather_block_at_pos(self: &Rc<Self>, pos: IntPair) {
        if !self.is_client() {
            self.load_standard_dynamic_object_of_type_at_pos(26, pos);
        }
    }

    /// The 256-particle burst of teleport / new blockhead: 7 `rand()` per particle (1792 total) in the order
    /// r1..r7; `ParticleEmitter instance` is fetched after r1.
    fn blockhead_particle_burst(&self, pos: IntPair) {
        let hooks = ops();
        let hit = [pos.x as f32 + 0.5f32, pos.y as f32, -0.5f32, 0.0f32];
        for _ in 0..256 {
            let r1 = rand();
            let d = ((r01_from(r1) as f64) * 1.5f64) as f32 as f64;
            let color = [(d + 0.231373f64) as f32, (d + 0.347059f64) as f32, 1.0f32, 0.5f32];
            // emitter = [ParticleEmitter instance] happens here (headless)
            let r2 = rand();
            let r3 = rand();
            let ppos = [hit[0] + (r01_from(r2) - 0.5f32), hit[1] + 2.0f32 * r01_from(r3), hit[2]];
            let r4 = rand();
            let r5 = rand();
            let vel = [(r01_from(r4) - 0.5f32) * 4.0f32, (r01_from(r5) + (-0.300000012f32)) * 4.0f32, 0.0f32];
            let r6 = rand();
            let r7 = rand();
            let life = r01_from(r6) * 4.0f32 + 1.0f32;
            let scale = r01_from(r7) * 4.0f32 + 4.0f32;
            hooks.particle_emitter_add_particle(ppos, vel, color, 2, life, scale, hit);
        }
    }

    /// `-teleportBlockhead:toWorkbench:`: `bh.teleportToPos(wb.pos)`, particle burst on success.
    pub fn teleport_blockhead_to_workbench(&self, bh: &ObjRef, wb: Option<&ObjRef>) -> bool {
        let pos = wb.map_or(IntPair { x: 0, y: 0 }, |w| w.pos());
        let ok = ops().blockhead_teleport_to_pos(bh, pos);
        if ok {
            self.blockhead_particle_burst(pos);
        }
        ok
    }

    /// `-loadNewBlockheadAtPos:craftableItemObject:uniqueID:`: step order is observable through RNG and
    /// packets. `cio` is an opaque `BlockheadCraftableItemObject`.
    pub fn load_new_blockhead_at_pos_craftable_item_object_unique_id(
        self: &Rc<Self>,
        pos: IntPair,
        cio: &ObjRef,
        uid: u64,
    ) {
        let number = self.blockheads.borrow().len() as i32;
        let Some(bh) = ops().new_blockhead(self, pos, number, cio, uid) else { return };
        self.blockheads.borrow_mut().push(bh.clone());
        self.hooks().fully_load_if_needed_around_pos(pos, 0xFFFF_FFFF, &bh);
        if self.blockheads.borrow().len() == 1 {
            self.active_blockhead_index.set(0);
        }
        let mut mock = vec![pos];
        bh.world_changed(&bh, &mut mock);
        // [[world uiManager] blockheadCountChanged] / fanfare: client only
        self.send_net_data_if_needed_for_object_is_creation(&bh, true);
        self.blockhead_particle_burst(pos);
    }

    /// `-loadDebugChestAtPos:chest:`.
    pub fn load_debug_chest_at_pos_chest(self: &Rc<Self>, pos: IntPair, chest: &ObjRef) {
        self.register_with_pos(chest, chest.object_type(), pos);
        self.send_net_data_if_needed_for_object_is_creation(chest, true);
    }
}

macro_rules! add_wrappers {
    ($( $(#[$m:meta])* $name:ident => $t:expr ),* $(,)?) => {
        impl DynamicWorld {
            $(
                $(#[$m])*
                pub fn $name(self: &Rc<Self>, pos: IntPair, ty: i32, sd: Option<&PlistDict>, cid: Option<&str>) {
                    self.add_standard_object_at_pos_object_type_item_type_save_dict_placed_by_client(pos, $t, ty, sd, cid);
                }
            )*
        }
    };
}

add_wrappers! {
    /// `-addLadderAtPos:ofType:saveDict:placedByClient:` (object type 19).
    add_ladder_at_pos_of_type_save_dict_placed_by_client => 19,
    /// `-addColumnAtPos:ofType:saveDict:placedByClient:` (53).
    add_column_at_pos_of_type_save_dict_placed_by_client => 53,
    /// `-addStairsAtPos:ofType:saveDict:placedByClient:` (54).
    add_stairs_at_pos_of_type_save_dict_placed_by_client => 54,
    /// `-addWireAtPos:ofType:saveDict:placedByClient:` (38).
    add_wire_at_pos_of_type_save_dict_placed_by_client => 38,
    /// `-addElevatorShaftAtPos:ofType:saveDict:placedByClient:` (56).
    add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client => 56,
    /// `-addElevatorMotorAtPos:ofType:saveDict:placedByClient:` (55).
    add_elevator_motor_at_pos_of_type_save_dict_placed_by_client => 55,
    /// `-addWindowAtPos:ofType:saveDict:placedByClient:` (31).
    add_window_at_pos_of_type_save_dict_placed_by_client => 31,
}

macro_rules! remove_wrappers {
    ($( $(#[$m:meta])* $name:ident => $lookup:ident $(, $after:ident)? );* $(;)?) => {
        impl DynamicWorld {
            $(
                $(#[$m])*
                pub fn $name(&self, pos: IntPair) -> Option<ObjRef> {
                    let x = self.$lookup(pos);
                    self.remove_standard_object(x.as_ref());
                    $( self.$after(); )?
                    x
                }
            )*
        }
    };
}

remove_wrappers! {
    /// `-removeTorchAtPos:`.
    remove_torch_at_pos => torch_at_pos;
    /// `-removeEggAtPos:`.
    remove_egg_at_pos => egg_at_pos;
    /// `-removePaintingAtPos:`.
    remove_painting_at_pos => painting_at_pos;
    /// `-removeLadderAtPos:`.
    remove_ladder_at_pos => ladder_at_pos;
    /// `-removeColumnAtPos:`.
    remove_column_at_pos => column_at_pos;
    /// `-removeStairsAtPos:`.
    remove_stairs_at_pos => stairs_at_pos;
    /// `-removeWireAtPos:`.
    remove_wire_at_pos => wire_at_pos;
    /// `-removeElevatorShaftAtPos:`.
    remove_elevator_shaft_at_pos => elevator_shaft_at_pos;
    /// `-removeElevatorMotorAtPos:`.
    remove_elevator_motor_at_pos => elevator_motor_at_pos;
    /// `-removeRailAtPos:`: then ALSO `railOrStationNameChanged`.
    remove_rail_at_pos => get_rail_at_pos, rail_or_station_name_changed;
    /// `-removeWindowAtPos:`.
    remove_window_at_pos => window_at_pos;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooks};
    use crate::dynworld::state::testutil::*;
    use std::cell::{Cell, RefCell};

    fn p(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// Hooks with a fake tree / plant population.
    struct H {
        soil_ok: bool,
        made: RefCell<Vec<String>>,
        next_id: Cell<u64>,
    }
    impl H {
        fn new() -> Rc<H> {
            Rc::new(H { soil_ok: false, made: RefCell::new(vec![]), next_id: Cell::new(1000) })
        }
    }
    impl crate::dynworld::lookups::DwOpsHooksStub for H {
        fn new_tree(
            &self,
            dw: &Rc<DynamicWorld>,
            tree_type: i32,
            pos: IntPair,
            _mh: i16,
            _gr: i16,
            _adult: bool,
            _ama: f32,
        ) -> Option<ObjRef> {
            self.made.borrow_mut().push(format!("tree{}@{},{}", tree_type, pos.x, pos.y));
            let id = dw.get_next_dynamic_object_id();
            let o: ObjRef = Obj::new(1, id, Rc::new(RefCell::new(vec![])));
            o.base().borrow_mut().pos = pos;
            Some(o)
        }
        fn new_plant(
            &self,
            dw: &Rc<DynamicWorld>,
            plant_type: i32,
            pos: IntPair,
            _a: i16,
            _b: i16,
            _adult: bool,
        ) -> Option<ObjRef> {
            self.made.borrow_mut().push(format!("plant{}@{},{}", plant_type, pos.x, pos.y));
            let id = dw.get_next_dynamic_object_id();
            let o: ObjRef = Obj::new(10, id, Rc::new(RefCell::new(vec![])));
            o.base().borrow_mut().pos = pos;
            Some(o)
        }
        fn plant_can_breed(&self, _p: &ObjRef) -> bool {
            true
        }
        fn blockhead_teleport_to_pos(&self, _bh: &ObjRef, _pos: IntPair) -> bool {
            true
        }
        fn tree_is_required_soil_type(&self, _t: &ObjRef, _s: u8) -> bool {
            self.soil_ok
        }
        fn tree_type(&self, _t: &ObjRef) -> i32 {
            1
        }
    }

    fn obj(ty: i32, id: u64, pos: IntPair) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = pos;
        o
    }

    /// The next `n` rand() draws after `srand(seed)`.
    fn draws(seed: u32, n: usize) -> Vec<i32> {
        crate::rng::glibc_rand::srand(seed);
        (0..n).map(|_| rand()).collect()
    }

    /// sow_tree_near_parent consumes exactly 2 draws (no tiles loaded -> abandons attempts).
    #[test]
    fn sow_tree_two_draws() {
        let d = dw("fac_sow_tree", 2);
        set_ops_hooks(H::new());
        let parent = obj(1, 1, p(10, 10));
        crate::rng::glibc_rand::srand(1);
        d.sow_tree_near_parent_adult_adult_max_age(&parent, false, 99999.0);
        let after = rand();
        assert_eq!(after, draws(1, 3)[2]);
    }

    #[test]
    fn sow_plant_two_draws_and_breeding() {
        let d = dw("fac_sow_plant", 2);
        set_ops_hooks(H::new());
        let parent = obj(10, 1, p(10, 10));
        crate::rng::glibc_rand::srand(1);
        d.sow_plant_near_parent(&parent);
        assert_eq!(rand(), draws(1, 3)[2]);
        // breeding: zero draws without candidates, one with
        crate::rng::glibc_rand::srand(1);
        assert!(d.find_breeding_plant_near_plant(&parent).is_none());
        assert_eq!(rand(), draws(1, 1)[0]);
        let other = obj(10, 2, p(12, 11));
        d.do_set(10, 1, parent.clone());
        d.do_set(10, 2, other.clone());
        crate::rng::glibc_rand::srand(1);
        let c = d.find_breeding_plant_near_plant(&parent).unwrap();
        assert_eq!(c.unique_id(), 2);
        assert_eq!(rand(), draws(1, 2)[1]);
        // out of range candidate is ignored
        let far = obj(10, 3, p(25, 11));
        d.do_set(10, 3, far);
        d.do_erase(10, 2);
        assert!(d.find_breeding_plant_near_plant(&parent).is_none());
    }

    /// load_tree: duplicate rules (+-1 for adults, exact x otherwise) and registration without index.
    #[test]
    fn load_tree_duplicates() {
        let d = dw("fac_tree", 2);
        let h = H::new();
        set_ops_hooks(h.clone());
        d.do_set(1, 50, obj(1, 50, p(10, 5)));
        d.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(p(11, 5), 1, 0, 0, false, 1.0); // not exact: ok
        assert_eq!(h.made.borrow().len(), 1);
        d.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(p(11, 5), 1, 0, 0, true, 1.0); // adult: rejected
        assert_eq!(h.made.borrow().len(), 1);
        d.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(p(10, 6), 1, 0, 0, true, 1.0); // other row: ok
        assert_eq!(h.made.borrow().len(), 2);
        assert_eq!(d.ta_len(1), 2);
        assert_eq!(d.bp_len(1), 0);
        // a queued tree blocks an exact repeat
        d.load_tree_at_position_type_max_height_growth_rate_adult_tree_adult_max_age(p(11, 5), 1, 0, 0, false, 1.0);
        assert_eq!(h.made.borrow().len(), 2);
    }

    #[test]
    fn plant_registers_in_index() {
        let d = dw("fac_plant", 2);
        let h = H::new();
        set_ops_hooks(h.clone());
        d.load_plant_at_position_type_max_age_gene_growth_rate_gene_adult(p(3, 3), 4, 0, 0, false);
        assert_eq!(d.ta_len(10), 1);
        assert_eq!(d.bp_len(10), 1);
    }

    /// teleport consumes 1792 draws.
    #[test]
    fn teleport_draws_1792() {
        let d = dw("fac_tp", 2);
        set_ops_hooks(H::new());
        let bh = obj(24, 1, p(1, 1));
        let wb = obj(45, 2, p(8, 9));
        crate::rng::glibc_rand::srand(7);
        assert!(d.teleport_blockhead_to_workbench(&bh, Some(&wb)));
        assert_eq!(rand(), draws(7, 1793)[1792]);
    }

    /// remove_standard_object: never erases from the maps; logs for protected types; sets needsRemoved.
    #[test]
    fn remove_standard_sets_flag() {
        let d = dw("fac_rm", 2);
        let o = obj(17, 9, p(2, 2));
        d.do_set(17, 9, o.clone());
        d.remove_standard_object(Some(&o));
        assert!(o.needs_removed());
        assert!(d.do_contains(17, 9));
        let bh = obj(24, 10, p(2, 2));
        d.remove_standard_object(Some(&bh));
        assert!(!bh.needs_removed());
        d.remove_standard_object(None);
    }

    /// add_torch bug rule (type != 17 returns when index 17 has an entry).
    #[test]
    fn add_torch_rule() {
        let d = dw("fac_torch", 2);
        let h = H::new();
        set_ops_hooks(h.clone());
        let existing = obj(17, 4, p(5, 5));
        d.register_with_pos(&existing, 17, p(5, 5));
        // other type: returns without adding (no new_torch hook exists -> nothing registered either way)
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(p(5, 5), 47, 0, 0, None, None);
        assert_eq!(d.ta_len(17), 1);
        existing.base().borrow_mut().needs_removed = true;
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(p(5, 5), 17, 0, 0, None, None);
    }

    #[test]
    fn debug_chest_registers() {
        let d = dw("fac_chest", 2);
        let c = obj(46, 12, p(4, 4));
        d.load_debug_chest_at_pos_chest(p(4, 4), &c);
        assert_eq!(d.ta_len(46), 1);
        assert_eq!(d.bp_len(46), 1);
    }

    /// Chest generation hooks recording the fill calls.
    struct T {
        moves: RefCell<Vec<(i32, u32, i32)>>,
    }
    impl crate::dynworld::lookups::DwOpsHooksStub for T {
        fn new_chest(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            _it: i32,
            _f: bool,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            let id = dw.get_next_dynamic_object_id();
            let o: ObjRef = Obj::new(46, id, Rc::new(RefCell::new(vec![])));
            o.base().borrow_mut().pos = pos;
            Some(o)
        }
        fn chest_move_inventory_items_from_array_to_index_count(
            &self,
            _c: &ObjRef,
            coins: &mut Vec<ItemRef>,
            index: i32,
            count: i32,
        ) {
            let first = coins.first().map_or(0, |i| i.borrow().item_type.0);
            self.moves.borrow_mut().push((index, first, count));
            coins.clear();
        }
    }

    /// the treasure fill consumes exactly the draws the spec lists (3 + per slot), in order.
    #[test]
    fn treasure_fill_draw_count() {
        for seed in 1..20u32 {
            let d = dw("fac_treasure", 2);
            let t = Rc::new(T { moves: RefCell::new(vec![]) });
            set_ops_hooks(t.clone());
            let pb: &'static crate::world::state::PhysicalBlock =
                Box::leak(Box::new(crate::world::state::PhysicalBlock::new_fresh()));
            let tile = TileRef::new(pb, 0);
            // slot count from the first three draws
            let r = draws(seed, 3);
            crate::rng::glibc_rand::srand(seed);
            d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&tile, p(5, 5), false, true);
            assert_eq!(tile.contents(), 0x30);
            let moves = t.moves.borrow();
            let fill = r01_from(r[0]) * r01_from(r[1]) * r01_from(r[2]);
            let f16 = 16.0f32 * fill;
            let n = (if 15.0 <= f16 { 15.0 } else { f16 } + 1.0) as i32;
            assert_eq!(moves.len() as i32, n);
            let mut consumed = 3;
            for (i, (idx, first, count)) in moves.iter().enumerate() {
                assert_eq!(*idx, i as i32);
                assert!(*count >= 1);
                consumed += match *first {
                    166 | 167 => 5 + if *count == 99 { 1 } else { 0 },
                    260 => 6,
                    87 | 86 | 76 | 75 | 88 => 4,
                    72 | 329 => 3,
                    other => panic!("unexpected item {other}"),
                };
            }
            drop(moves);
            assert_eq!(rand(), draws(seed, consumed + 1)[consumed], "seed {seed}");
            assert_eq!(d.ta_len(46), 1);
            assert_eq!(d.bp_len(46), 1);
        }
    }

    #[test]
    fn clamp_matches() {
        assert_eq!(clampf(0.0, 1.0, 99.0), 1.0);
        assert_eq!(clampf(120.0, 1.0, 99.0), 99.0);
        assert_eq!(clampf(50.0, 1.0, 99.0), 50.0);
    }
}
