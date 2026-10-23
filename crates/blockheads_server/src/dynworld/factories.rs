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

    /// `-sowPlantNearParent:`: (the client guard is gone: there is no client here) two attempts, one `rand()` each; the plant-type loop
    /// stops at the first match.
    pub fn sow_plant_near_parent(self: &Rc<Self>, parent: &ObjRef) {
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
        if let Some(o) = self.by_pos_get(17, wi) {
            // UB-PARITY
            if ty != 17 || !o.needs_removed() {
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
        self.load_standard_dynamic_object_of_type_at_pos(26, pos);
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
        made: RefCell<Vec<String>>,
        next_id: Cell<u64>,
    }
    impl H {
        fn new() -> Rc<H> {
            Rc::new(H { made: RefCell::new(vec![]), next_id: Cell::new(1000) })
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
                assert!(
                    matches!(*first, 166 | 167 | 260 | 87 | 86 | 76 | 75 | 88 | 72 | 329),
                    "unexpected item {first}"
                );
                consumed += match *first {
                    166 | 167 => 5 + if *count == 99 { 1 } else { 0 },
                    260 => 6,
                    87 | 86 | 76 | 75 | 88 => 4,
                    _ => 3,
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

/// Scenario tests over a small hilly world with scripted constructors.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooksStub};
    use crate::dynworld::state::testutil::Obj;
    use crate::rng::glibc_rand::srand;
    use crate::trees::tree::testenv::{env, ip, tile, GROUND};
    use crate::world::state::World;
    use std::cell::{Cell, RefCell};

    type Log = Rc<RefCell<Vec<String>>>;

    /// Constructors that return plain objects of a fixed type and log what they were asked for.
    struct Ops {
        log: Log,
        soil: Cell<bool>,
        plant_ty: Cell<i32>,
        refuse: Cell<bool>,
        item: Cell<i32>,
        tap: Cell<bool>,
        breed: Cell<bool>,
        wb_type: Cell<i32>,
    }
    impl Ops {
        fn make(&self, dw: &Rc<DynamicWorld>, ty: i32, pos: IntPair, what: &str) -> Option<ObjRef> {
            self.log.borrow_mut().push(format!("{what}@{},{}", pos.x, pos.y));
            if self.refuse.get() {
                return None;
            }
            let o: ObjRef = Obj::new(ty, dw.get_next_dynamic_object_id(), self.log.clone());
            o.base().borrow_mut().pos = pos;
            Some(o)
        }
    }
    impl DwOpsHooksStub for Ops {
        fn new_tree(
            &self,
            dw: &Rc<DynamicWorld>,
            ty: i32,
            pos: IntPair,
            _m: i16,
            _g: i16,
            _a: bool,
            _x: f32,
        ) -> Option<ObjRef> {
            self.make(dw, 1, pos, &format!("tree{ty}"))
        }
        fn new_plant(
            &self,
            dw: &Rc<DynamicWorld>,
            ty: i32,
            pos: IntPair,
            _a: i16,
            _g: i16,
            _ad: bool,
        ) -> Option<ObjRef> {
            self.make(dw, 10, pos, &format!("plant{ty}"))
        }
        fn new_npc(
            &self,
            dw: &Rc<DynamicWorld>,
            ty: i32,
            pos: IntPair,
            _sd: Option<&PlistDict>,
            _a: bool,
            _w: bool,
            _c: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 13, pos, &format!("npc{ty}"))
        }
        fn new_torch(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            ty: i32,
            _a: u16,
            _b: u16,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 17, pos, &format!("torch{ty}"))
        }
        fn new_egg(&self, dw: &Rc<DynamicWorld>, pos: IntPair, _sd: Option<&PlistDict>) -> Option<ObjRef> {
            self.make(dw, 30, pos, "egg")
        }
        fn new_painting(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            ty: i32,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 52, pos, &format!("painting{ty}"))
        }
        fn new_rail(&self, dw: &Rc<DynamicWorld>, pos: IntPair, ty: i32, _o: bool) -> Option<ObjRef> {
            self.make(dw, 40, pos, &format!("rail{ty}"))
        }
        fn new_boat(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 27, pos, "boat")
        }
        fn new_train_car(
            &self,
            dw: &Rc<DynamicWorld>,
            ty: i32,
            pos: IntPair,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 42, pos, &format!("car{ty}"))
        }
        fn new_workbench(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            ty: i32,
            _f: bool,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 45, pos, &format!("workbench{ty}"))
        }
        fn new_trade_portal(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            ty: i32,
            _f: bool,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 50, pos, &format!("portal{ty}"))
        }
        fn new_chest(
            &self,
            dw: &Rc<DynamicWorld>,
            pos: IntPair,
            ty: i32,
            _f: bool,
            _sd: Option<&PlistDict>,
            _c: Option<&str>,
            _n: Option<&str>,
        ) -> Option<ObjRef> {
            self.make(dw, 46, pos, &format!("chest{ty}"))
        }
        fn new_glow_block(&self, dw: &Rc<DynamicWorld>, pos: IntPair, _t: TileRef) -> Option<ObjRef> {
            self.make(dw, 18, pos, "glow")
        }
        fn new_blockhead(&self, dw: &Rc<DynamicWorld>, pos: IntPair, n: i32, _c: &ObjRef, uid: u64) -> Option<ObjRef> {
            let o = self.make(dw, 24, pos, &format!("blockhead{n}"))?;
            o.base().borrow_mut().unique_id = uid;
            Some(o)
        }
        fn tree_is_required_soil_type(&self, _t: &ObjRef, _s: u8) -> bool {
            self.soil.get()
        }
        fn tree_type(&self, _t: &ObjRef) -> i32 {
            1
        }
        fn plant_type(&self, _p: &ObjRef) -> i32 {
            self.plant_ty.get()
        }
        fn plant_is_required_soil_type(&self, _p: &ObjRef, _s: u8) -> bool {
            self.soil.get()
        }
        fn plant_can_breed(&self, _p: &ObjRef) -> bool {
            self.breed.get()
        }
        fn tap_is_within_body_radius(&self, _o: &ObjRef, _t: crate::types::structs::Vector2) -> bool {
            self.tap.get()
        }
        fn blockhead_teleport_to_pos(&self, _b: &ObjRef, _p: IntPair) -> bool {
            !self.refuse.get()
        }
        fn object_update_in_time_since_saved(&self, o: &ObjRef) {
            self.log.borrow_mut().push(format!("caught_up({})", o.unique_id()));
        }
        fn elevator_shaft_open(&self, s: &ObjRef) {
            self.log.borrow_mut().push(format!("open({})", s.unique_id()));
        }
        fn world_remove_tile_at_world_x_world_y(&self, x: i32, y: i32) {
            self.log.borrow_mut().push(format!("remove_tile({x},{y})"));
        }
        fn world_ownership_sign_was_placed_or_changed_at_pos(
            &self,
            p: IntPair,
            _o: Option<&str>,
            _w: i32,
            _h: i32,
            r: bool,
        ) {
            self.log.borrow_mut().push(format!("ownership({},{},{r})", p.x, p.y));
        }
        fn train_car_rail_or_station_name_changed(&self, c: &ObjRef) {
            self.log.borrow_mut().push(format!("renamed({})", c.unique_id()));
        }
    }

    impl crate::dynworld::state::DwHooksStub for Ops {
        fn item_type(&self, _o: &ObjRef) -> i32 {
            self.item.get()
        }
        fn workbench_type(&self, _w: &ObjRef) -> i32 {
            self.wb_type.get()
        }
    }

    fn fixture(name: &str) -> (Rc<World>, Rc<DynamicWorld>, Rc<Ops>, Log) {
        crate::objects::registry::install();
        let (w, d) = env(name);
        let log: Log = Rc::new(RefCell::new(vec![]));
        let ops = Rc::new(Ops {
            log: log.clone(),
            soil: Cell::new(true),
            plant_ty: Cell::new(4),
            refuse: Cell::new(false),
            item: Cell::new(0),
            tap: Cell::new(false),
            breed: Cell::new(true),
            wb_type: Cell::new(0),
        });
        set_ops_hooks(ops.clone());
        d.set_hooks(ops.clone());
        (w, d, ops, log)
    }

    fn set_tile(w: &World, x: i32, y: i32, f: impl FnOnce(&mut crate::types::tile::Tile)) {
        tile_at_world_position_loaded(x, y, w).unwrap().update(f);
    }

    /// Column heights differ so that parents see ground at their own row, below them and far above them; some
    /// columns have a pond on top.
    fn hills(w: &World) {
        for x in 0..64 {
            let h = 3 + (x * 5) % 11;
            for y in 0..=20 {
                set_tile(w, x, y, |t| {
                    if y <= h {
                        t.type_index = 6;
                    } else if y == h + 1 && x % 7 == 0 {
                        t.type_index = 3;
                    } else {
                        t.type_index = 2;
                        t.back_wall_type_index = 2;
                    }
                });
            }
        }
    }

    fn obj(ty: i32, id: u64, pos: IntPair, log: &Log) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, log.clone());
        o.base().borrow_mut().pos = pos;
        o
    }

    fn forget_added(d: &DynamicWorld) {
        for t in 0..65 {
            d.dynamic_objects_to_add.borrow_mut()[t].clear();
            d.dynamic_objects_by_world_pos_index.borrow_mut()[t].clear();
        }
    }

    #[test]
    fn sowing_trees_over_hills() {
        let (w, d, ops, log) = fixture("fs_trees");
        hills(&w);
        for py in [0, 3, 8, 12, 25, 40] {
            let parent = obj(1, 1, ip(32, py), &log);
            for seed in 1..60u32 {
                srand(seed);
                d.sow_tree_near_parent_adult_adult_max_age(&parent, seed % 2 == 0, 5.0);
                forget_added(&d);
            }
        }
        assert!(log.borrow().iter().any(|l| l.starts_with("tree1@")));
        // soil that the parent cannot grow in, and a crowded neighbourhood
        let n = log.borrow().len();
        ops.soil.set(false);
        let parent = obj(1, 1, ip(32, 12), &log);
        for seed in 1..30u32 {
            srand(seed);
            d.sow_tree_near_parent_adult_adult_max_age(&parent, false, 5.0);
        }
        assert_eq!(log.borrow().len(), n);
        ops.soil.set(true);
        for x in (0..64).step_by(2) {
            d.do_set(1, 100 + x as u64, obj(1, 100 + x as u64, ip(x, 20), &log));
        }
        for seed in 1..30u32 {
            srand(seed);
            d.sow_tree_near_parent_adult_adult_max_age(&parent, false, 5.0);
        }
        assert_eq!(log.borrow().len(), n);
    }

    #[test]
    fn sowing_plants_over_hills() {
        let (w, d, ops, log) = fixture("fs_plants");
        hills(&w);
        for pt in [4, 6, 7] {
            ops.plant_ty.set(pt);
            for py in [0, 3, 8, 12, 25, 40] {
                let parent = obj(10, 1, ip(32, py), &log);
                for seed in 1..60u32 {
                    srand(seed);
                    d.sow_plant_near_parent(&parent);
                    forget_added(&d);
                }
            }
        }
        assert!(log.borrow().iter().any(|l| l.starts_with("plant4@")));
        assert!(log.borrow().iter().any(|l| l.starts_with("plant6@")));
        assert!(log.borrow().iter().any(|l| l.starts_with("plant7@")));
        // a crowded row blocks every attempt
        let n = log.borrow().len();
        for x in 0..64 {
            d.do_set(10, 500 + x as u64, obj(10, 500 + x as u64, ip(x, 40), &log));
        }
        let parent = obj(10, 1, ip(32, 12), &log);
        for seed in 1..30u32 {
            srand(seed);
            d.sow_plant_near_parent(&parent);
        }
        assert_eq!(log.borrow().len(), n);
    }

    #[test]
    fn sowing_by_seed_item() {
        let (_w, d, _ops, log) = fixture("fs_seed");
        use crate::types::item_type::ItemType as I;
        d.sow_tree_or_plant_at_position_item_type_max_height_growth_rate(ip(5, 12), I::ITEM_MAPLE_SEED.0 as i32, 3, 4);
        d.sow_tree_or_plant_at_position_item_type_max_height_growth_rate(ip(9, 12), I::ITEM_FLAX_SEED.0 as i32, 3, 4);
        let l = log.borrow().clone();
        assert!(l.iter().any(|s| s.starts_with("tree")) && l.iter().any(|s| s.starts_with("plant")), "{l:?}");
    }

    /// An object that occupies one of the three tile layers.
    struct Layer {
        base: RefCell<crate::objects::base::DynamicObjectBase>,
        ty: i32,
        layer: u8,
    }
    fn layer_obj(ty: i32, id: u64, pos: IntPair, layer: u8) -> ObjRef {
        let o = crate::objects::handle::alloc_obj(|w| Layer {
            base: RefCell::new(crate::objects::base::DynamicObjectBase::alloc_zeroed_with(w)),
            ty,
            layer,
        });
        o.base.borrow_mut().unique_id = id;
        o.base.borrow_mut().pos = pos;
        o
    }
    impl crate::objects::base::DynamicObject for Layer {
        fn base(&self) -> &RefCell<crate::objects::base::DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static crate::objects::class_info::ClassInfo {
            &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn object_type(&self) -> i32 {
            self.ty
        }
        fn occupies_normal_contents(&self) -> bool {
            self.layer == 0
        }
        fn occupies_foreground_contents(&self) -> bool {
            self.layer == 1
        }
        fn occupies_background_contents(&self) -> bool {
            self.layer == 2
        }
    }

    #[test]
    fn standard_objects_fire_snow_and_surface() {
        let (w, d, _ops, log) = fixture("fs_std");
        let p = ip(5, 12);
        assert!(d.load_standard_dynamic_object_of_type_at_pos(16, p).is_some());
        assert!(d.load_standard_dynamic_object_of_type_at_pos(16, p).is_none());
        assert!(d.load_standard_dynamic_object_of_type_at_pos(0, p).is_none());
        d.load_surface_block_at_pos(ip(6, 12));
        d.load_snow_surface_block_at_pos_load_snow(ip(7, 12), false);
        d.load_snow_surface_block_at_pos_load_snow(ip(8, 12), true);
        d.load_snow_surface_block_at_pos_load_snow(ip(8, 12), true);
        assert!(log.borrow().iter().any(|l| l.starts_with("caught_up")));
        d.load_gather_block_at_pos(ip(9, 12));
        // fire: needs a tile, refuses burning tiles and stale plant sprites
        assert!(d.place_fire_at_position(ip(10, 12)).is_some());
        assert_eq!(tile(&w, 10, 12).on_fire, 1);
        assert!(d.place_fire_at_position(ip(10, 12)).is_none());
        set_tile(&w, 11, 12, |t| t.foreground_contents = 0x43);
        assert!(d.place_fire_at_position(ip(11, 12)).is_none());
        assert_eq!(tile(&w, 11, 12).foreground_contents, 0);
        assert!(d.place_fire_at_position(ip(11, -5)).is_none());
        // a second fire at an occupied slot is not lit
        set_tile(&w, 12, 12, |t| t.on_fire = 0);
        d.by_pos_set(16, d.world_index(ip(12, 12)), &obj(16, 777, ip(12, 12), &log));
        assert!(d.place_fire_at_position(ip(12, 12)).is_none());
    }

    #[test]
    fn npc_placement_limits() {
        let (_w, d, _ops, log) = fixture("fs_npc");
        let dt = dynamic_object_type_for_npc_type(NPCType(1)).0 as i32;
        assert!(d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
            ip(5, 12),
            1,
            None,
            true,
            true,
            None
        ));
        // crowded: more than one npc around the spot
        d.do_set(13, 2001, obj(13, 2001, ip(20, 12), &log));
        d.do_set(13, 2002, obj(13, 2002, ip(21, 12), &log));
        assert!(!d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
            ip(20, 12),
            1,
            None,
            true,
            false,
            None
        ));
        assert!(d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
            ip(20, 12),
            1,
            None,
            true,
            true,
            None
        ));
        // too many of the type
        for i in 0..520u64 {
            d.do_set(dt as usize, 5000 + i, obj(dt, 5000 + i, ip(40, 12), &log));
        }
        assert!(!d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
            ip(1, 12),
            1,
            None,
            true,
            false,
            None
        ));
    }

    #[test]
    fn placed_workbenches_chests_and_signs() {
        let (_w, d, ops, log) = fixture("fs_place");
        assert!(d
            .workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(ip(5, 12), 1, None, None, None)
            .is_some());
        assert!(!d.workbench_has_been_crafted.get());
        assert!(d
            .workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(ip(7, 12), 3, None, None, None)
            .is_some());
        assert!(d.workbench_has_been_crafted.get());
        ops.refuse.set(true);
        assert!(d
            .workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(ip(9, 12), 3, None, None, None)
            .is_none());
        ops.refuse.set(false);
        use crate::types::item_type::ItemType as I;
        for (n, t) in [I::ITEM_OWNERSHIP_SIGN, I::ITEM_TRAIN_STATION, I::ITEM_SIGN, I::ITEM_CHEST, I::ITEM_WOOD_BED]
            .iter()
            .enumerate()
        {
            let it = InventoryItem::new(*t, 0, 0, None, None);
            let r = d.interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
                ip(12 + 3 * n as i32, 12),
                &it.borrow(),
                false,
                None,
                None,
                None,
            );
            assert!(r.is_some(), "{t:?}");
        }
        let nothing = InventoryItem::new(ItemType(1), 0, 0, None, None);
        assert!(d
            .interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
                ip(30, 12),
                &nothing.borrow(),
                false,
                None,
                None,
                None
            )
            .is_none());
        let l = log.borrow().clone();
        assert!(l.iter().any(|s| s.starts_with("ownership(")), "{l:?}");
    }

    #[test]
    fn removing_standard_objects_clears_the_layer() {
        let (w, d, _ops, log) = fixture("fs_remove");
        for (layer, id) in [(0u8, 1u64), (1, 2), (2, 3), (3, 4)] {
            let pos = ip(5 + id as i32, 12);
            set_tile(&w, pos.x, pos.y, |t| {
                t.contents = 0x30;
                t.foreground_contents = 0x61;
                t.background_contents = 0x46;
            });
            let o = layer_obj(33, id, pos, layer);
            assert_eq!(o.class_info().name, crate::objects::class_info::DYNAMIC_OBJECT_CLASS.name);
            assert!(o.as_any().is::<Layer>());
            d.remove_standard_object(Some(&o));
            assert!(o.needs_removed());
            let t = tile(&w, pos.x, pos.y);
            match layer {
                0 => assert_eq!((t.contents, t.foreground_contents, t.background_contents), (0, 0x61, 0x46)),
                1 => assert_eq!((t.contents, t.foreground_contents, t.background_contents), (0x30, 0, 0x46)),
                2 => assert_eq!((t.contents, t.foreground_contents, t.background_contents), (0x30, 0x61, 0)),
                _ => assert_eq!((t.contents, t.foreground_contents, t.background_contents), (0x30, 0x61, 0x46)),
            }
        }
        // objects of the protected types only log
        let chest = obj(46, 9, ip(3, 12), &log);
        d.remove_standard_object(Some(&chest));
        assert!(!chest.needs_removed());
        // tiles that cannot be loaded are skipped
        let far = layer_obj(33, 10, ip(3, 5000), 0);
        d.remove_standard_object(Some(&far));
        let far = layer_obj(33, 11, ip(3, 5000), 1);
        d.remove_standard_object(Some(&far));
        let far = layer_obj(33, 12, ip(3, 5000), 2);
        d.remove_standard_object(Some(&far));
    }

    #[test]
    fn eggs_paintings_rails_boats_and_torches() {
        let (_w, d, ops, log) = fixture("fs_misc");
        d.add_egg_at_pos_save_dict(ip(5, 12), None);
        ops.refuse.set(true);
        d.add_egg_at_pos_save_dict(ip(6, 12), None);
        d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(ip(6, 12), 3, None, None, None);
        d.add_rail_at_pos_of_type_owned_by_station(ip(6, 12), 3, false);
        d.place_boat_in_water_at_pos_save_dict_placed_by_client(ip(6, 12), None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(6, 12), 204, None, None);
        d.load_new_blockhead_at_pos_craftable_item_object_unique_id(ip(6, 12), &obj(1, 1, ip(0, 0), &log), 5);
        ops.refuse.set(false);
        d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(ip(5, 12), 3, None, None, None);
        // a second painting at the same spot is ignored until the first is removed
        d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(ip(5, 12), 4, None, None, None);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("painting")).count(), 2);
        d.painting_at_pos(ip(5, 12)).unwrap().base().borrow_mut().needs_removed = true;
        d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(ip(5, 12), 4, None, None, None);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("painting")).count(), 3);
        d.add_rail_at_pos_of_type_owned_by_station(ip(7, 12), 3, false);
        d.add_rail_at_pos_of_type_owned_by_station(ip(7, 12), 3, false);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("rail")).count(), 2);
        d.get_rail_at_pos(ip(7, 12)).unwrap().base().borrow_mut().needs_removed = true;
        d.add_rail_at_pos_of_type_owned_by_station(ip(7, 12), 3, false);
        d.place_boat_in_water_at_pos_save_dict_placed_by_client(ip(8, 12), None, None);
        // torches: same type replaces a removed torch, other types never replace
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(ip(9, 12), 17, 0, 0, None, None);
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(ip(9, 12), 17, 0, 0, None, None);
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(ip(9, 12), 47, 0, 0, None, None);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("torch")).count(), 1);
        assert!(d.remove_torch_at_pos(ip(9, 12)).is_some());
        d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(ip(9, 12), 17, 0, 0, None, None);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("torch")).count(), 2);
    }

    #[test]
    fn remove_wrappers_flag_the_registered_objects() {
        let (_w, d, _ops, log) = fixture("fs_wrap");
        let p = ip(5, 12);
        for ty in [17, 30, 52, 19, 53, 54, 38, 56, 55, 40, 31] {
            let o = obj(ty, 100 + ty as u64, p, &log);
            d.register_with_pos(&o, ty, p);
        }
        let car = obj(42, 900, ip(50, 12), &log);
        d.do_set(42, 900, car);
        assert!(d.remove_torch_at_pos(p).is_some());
        assert!(d.remove_egg_at_pos(p).is_some());
        assert!(d.remove_painting_at_pos(p).is_some());
        assert!(d.remove_ladder_at_pos(p).is_some());
        assert!(d.remove_column_at_pos(p).is_some());
        assert!(d.remove_stairs_at_pos(p).is_some());
        assert!(d.remove_wire_at_pos(p).is_some());
        assert!(d.remove_elevator_shaft_at_pos(p).is_some());
        assert!(d.remove_elevator_motor_at_pos(p).is_some());
        assert!(d.remove_rail_at_pos(p).is_some());
        assert!(d.remove_window_at_pos(p).is_some());
        assert!(d.remove_window_at_pos(p).is_none());
        assert!(log.borrow().contains(&"renamed(900)".to_string()));
        // elevator shafts open from the clicked cell and the one above
        d.register_with_pos(&obj(56, 700, ip(8, 12), &log), 56, ip(8, 12));
        d.register_with_pos(&obj(56, 701, ip(8, 13), &log), 56, ip(8, 13));
        d.open_elevator_at_pos(ip(8, 12));
        let l = log.borrow().clone();
        assert!(l.contains(&"open(700)".to_string()) && l.contains(&"open(701)".to_string()));
    }

    #[test]
    fn doors_set_their_tiles_and_remove_both_halves() {
        let (w, d, ops, _log) = fixture("fs_doors");
        // door types: tall wooden (52), tall (164), single (69, 165), gate (304) and another
        for (n, ty) in [52, 164, 69, 165, 304, 9].iter().enumerate() {
            let x = 5 + 2 * n as i32;
            d.add_door_at_pos_of_type_save_dict_placed_by_client(ip(x, 12), *ty, None, None);
            let t = tile(&w, x, 12);
            match ty {
                52 | 164 => assert_eq!((t.background_contents, tile(&w, x, 13).background_contents), (0x46, 0x46)),
                69 | 165 => assert_eq!(t.background_contents, 0x4b),
                304 => assert_eq!(t.contents, 0x92),
                _ => {}
            }
        }
        // removal: from the lower half, the upper half, a single door and a gate
        ops.item.set(52);
        assert!(d.remove_door_at_pos(ip(5, 12)).is_some());
        assert_eq!(tile(&w, 5, 13).background_contents, 0);
        assert!(d.remove_door_at_pos(ip(7, 13)).is_some());
        assert_eq!(tile(&w, 7, 12).background_contents, 0);
        ops.item.set(69);
        assert!(d.remove_door_at_pos(ip(9, 12)).is_some());
        ops.item.set(304);
        assert!(d.remove_door_at_pos(ip(13, 12)).is_some());
        assert_eq!(tile(&w, 13, 12).contents, 0);
        // no door and no door below
        assert!(d.remove_door_at_pos(ip(30, 12)).is_none());
        d.remove_standard_object(d.object_of_type_at_pos(20, ip(11, 12)).as_ref());
        // the extra wrappers
        d.add_ladder_at_pos_of_type_save_dict_placed_by_client(ip(20, 12), 100, None, None);
        d.add_column_at_pos_of_type_save_dict_placed_by_client(ip(21, 12), 100, None, None);
        d.add_stairs_at_pos_of_type_save_dict_placed_by_client(ip(22, 12), 100, None, None);
        d.add_wire_at_pos_of_type_save_dict_placed_by_client(ip(23, 12), 100, None, None);
        d.add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(ip(24, 12), 100, None, None);
        d.add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(ip(25, 12), 100, None, None);
        d.add_window_at_pos_of_type_save_dict_placed_by_client(ip(26, 12), 100, None, None);
    }

    /// Solid rock up to row 22 around x 0..40, a lit-by-nothing cave (back wall 1) from row 23 to 26 for x 6..=22.
    fn cave(w: &World) {
        for x in 0..40 {
            for y in 10..=30 {
                set_tile(w, x, y, |t| {
                    t.contents = 0;
                    t.foreground_contents = 0;
                    if y <= 22 {
                        t.type_index = 6;
                    } else if (6..=22).contains(&x) && y <= 26 {
                        t.type_index = 2;
                        t.back_wall_type_index = 1;
                    } else {
                        t.type_index = 6;
                    }
                });
            }
        }
    }

    fn treasure(d: &Rc<DynamicWorld>, w: &World, pos: IntPair, troll: bool, loot: bool) {
        let t = tile_at_world_position_loaded(pos.x, pos.y, w).unwrap();
        d.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&t, pos, troll, loot);
        forget_added(d);
    }

    #[test]
    fn troll_caves_and_treasure_loot() {
        let (w, d, _ops, log) = fixture("fs_cave");
        cave(&w);
        let count = |what: &str| log.borrow().iter().filter(|l| l.starts_with(what)).count();
        // the full set: troll, two torches, trade portal
        treasure(&d, &w, ip(10, 24), true, false);
        assert_eq!((count("npc6"), count("torch183"), count("portal210")), (1, 2, 1));
        // a spot on the right half of a macro tile mirrors the offsets
        cave(&w);
        treasure(&d, &w, ip(20, 24), true, false);
        assert_eq!(count("npc6"), 2);
        // loot only, over many seeds, with and without a troll
        for seed in 1..400u32 {
            cave(&w);
            srand(seed);
            treasure(&d, &w, ip(10, 24), seed % 3 == 0, true);
        }
        assert!(count("chest1072") > 300);
        // an interaction object already there: nothing happens
        let n = log.borrow().len();
        d.register_with_pos(&obj(46, 4242, ip(10, 24), &log), 46, ip(10, 24));
        treasure(&d, &w, ip(10, 24), true, true);
        assert_eq!(log.borrow().len(), n);
        d.dynamic_objects_by_world_pos_index.borrow_mut()[46].clear();
        // the ground under the troll spot is missing: no troll
        cave(&w);
        set_tile(&w, 12, 22, |t| t.type_index = 2);
        treasure(&d, &w, ip(10, 24), true, false);
        // climbing: one and two solid cells in the troll column
        for solid in [1, 2] {
            cave(&w);
            for k in 0..solid {
                set_tile(&w, 12, 23 + k, |t| t.type_index = 6);
            }
            let before = count("npc6");
            treasure(&d, &w, ip(10, 24), true, false);
            assert_eq!(count("npc6"), before + 1, "solid {solid}");
        }
        // a blocked or unlit troll cell, and a blocked cell above it
        cave(&w);
        set_tile(&w, 12, 23, |t| t.back_wall_type_index = 2);
        let before = count("npc6");
        treasure(&d, &w, ip(10, 24), true, false);
        cave(&w);
        set_tile(&w, 12, 24, |t| t.contents = 0x30);
        treasure(&d, &w, ip(10, 24), true, false);
        assert_eq!(count("npc6"), before);
        // torches only where there is room, portals only on supported ground
        cave(&w);
        set_tile(&w, 11, 24, |t| t.foreground_contents = 0x61);
        set_tile(&w, 13, 24, |t| t.type_index = 6);
        treasure(&d, &w, ip(10, 24), true, false);
        for (ptile, a1, a2, floor) in [
            (true, false, false, true),
            (false, true, false, true),
            (false, false, true, true),
            (false, false, false, false),
        ] {
            cave(&w);
            if ptile {
                set_tile(&w, 14, 22, |t| t.type_index = 6);
                set_tile(&w, 14, 23, |t| t.type_index = 6);
                set_tile(&w, 14, 24, |t| t.type_index = 6);
            }
            if a1 {
                set_tile(&w, 14, 24, |t| t.contents = 0x30);
            }
            if a2 {
                set_tile(&w, 14, 25, |t| t.contents = 0x30);
            }
            if !floor {
                set_tile(&w, 14, 21, |t| t.type_index = 2);
            }
            treasure(&d, &w, ip(10, 24), true, false);
        }
        // a refused trade portal and a refused chest
        cave(&w);
        _ops.refuse.set(true);
        treasure(&d, &w, ip(10, 24), true, true);
        _ops.refuse.set(false);
        // too high up for trolls
        cave(&w);
        treasure(&d, &w, ip(10, 300), true, false);
    }

    #[test]
    fn train_cars_snap_to_rails_and_neighbours() {
        let (w, d, ops, log) = fixture("fs_cars");
        for x in 15..=30 {
            set_tile(&w, x, 10, |t| t.foreground_contents = 98);
        }
        // sloped rail pieces: one level up and one level down
        set_tile(&w, 31, 11, |t| t.foreground_contents = 98);
        set_tile(&w, 31, 10, |t| t.type_index = 6);
        set_tile(&w, 14, 9, |t| t.foreground_contents = 98);
        // snapping down onto the rail, a hand car on the spot, plain cars
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(20, 11), 204, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(20, 10), 205, None, None);
        assert!(log.borrow().iter().any(|l| l == "car204@20,10" || l == "car204@20,11"));
        // a car next to an existing one is pushed away from it; the cars answer taps through the hook
        let neighbour = obj(42, 800, ip(21, 10), &log);
        d.do_set(42, 800, neighbour);
        ops.tap.set(true);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(20, 10), 205, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(31, 10), 205, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(15, 10), 205, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(14, 10), 205, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(10, 5000), 205, None, None);
    }

    #[test]
    fn glow_blocks_load_once_per_cell() {
        let (w, d, _ops, log) = fixture("fs_glow");
        let t = tile_at_world_position_loaded(5, 12, &w).unwrap();
        let mt = w.macro_tiles().get(0).unwrap();
        mt.dynamic_objects_loaded.set(false);
        d.load_glow_block_if_needed_at_pos_tile(ip(5, 12), &t);
        assert!(!log.borrow().iter().any(|l| l.starts_with("glow")));
        mt.dynamic_objects_loaded.set(true);
        d.load_glow_block_if_needed_at_pos_tile(ip(5, 12), &t);
        d.load_glow_block_if_needed_at_pos_tile(ip(5, 12), &t);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("glow")).count(), 1);
        d.currently_loading_macro_blocks.borrow_mut()[18].insert(0);
        d.load_glow_block_if_needed_at_pos_tile(ip(6, 12), &t);
        d.currently_loading_macro_blocks.borrow_mut()[18].clear();
        d.currently_adding_glow_blocks.borrow_mut().insert(wi_key(d.world_index(ip(7, 12))));
        d.load_glow_block_if_needed_at_pos_tile(ip(7, 12), &t);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("glow")).count(), 1);
        // outside the world
        d.load_glow_block_if_needed_at_pos_tile(ip(7, -40), &t);
        // the constructor may refuse
        _ops.refuse.set(true);
        d.load_glow_block_if_needed_at_pos_tile(ip(8, 12), &t);
        let mt1 = w.macro_tiles().get(1).unwrap();
        *mt1.physical_block.borrow_mut() = None;
        d.load_glow_block_if_needed_at_pos_tile(ip(40, 12), &t);
    }

    #[test]
    fn blockheads_teleport_and_arrive_with_particles() {
        let (_w, d, ops, log) = fixture("fs_bh");
        let bh = obj(24, 1, ip(1, 12), &log);
        let wb = obj(45, 2, ip(8, 12), &log);
        assert!(d.teleport_blockhead_to_workbench(&bh, Some(&wb)));
        assert!(d.teleport_blockhead_to_workbench(&bh, None));
        ops.refuse.set(true);
        assert!(!d.teleport_blockhead_to_workbench(&bh, Some(&wb)));
        ops.refuse.set(false);
        let cio = obj(1, 1, ip(0, 0), &log);
        d.load_new_blockhead_at_pos_craftable_item_object_unique_id(ip(3, 12), &cio, 77);
        d.load_new_blockhead_at_pos_craftable_item_object_unique_id(ip(4, 12), &cio, 78);
        assert_eq!(d.blockheads.borrow().len(), 2);
        assert_eq!(d.blockheads.borrow()[1].unique_id(), 78);
    }

    #[test]
    fn breeding_partners_must_be_willing_and_close() {
        let (_w, d, ops, log) = fixture("fs_breed");
        let me = obj(10, 1, ip(10, 12), &log);
        d.do_set(10, 1, me.clone());
        d.do_set(10, 2, obj(10, 2, ip(12, 12), &log));
        ops.breed.set(false);
        assert!(d.find_breeding_plant_near_plant(&me).is_none());
        ops.breed.set(true);
        assert_eq!(d.find_breeding_plant_near_plant(&me).unwrap().unique_id(), 2);
    }

    #[test]
    fn sowing_plants_needs_matching_soil() {
        let (w, d, ops, log) = fixture("fs_plant_soil");
        hills(&w);
        let parent = obj(10, 1, ip(32, 12), &log);
        ops.soil.set(false);
        for pt in [4, 7] {
            ops.plant_ty.set(pt);
            for seed in 1..40u32 {
                srand(seed);
                d.sow_plant_near_parent(&parent);
            }
        }
        assert!(log.borrow().iter().all(|l| !l.starts_with("plant")));
    }

    #[test]
    fn occupied_slots_and_removed_objects_are_replaced() {
        let (w, d, ops, log) = fixture("fs_occupied");
        let first = d.load_standard_dynamic_object_of_type_at_pos(16, ip(5, 12)).unwrap();
        assert!(d.load_standard_dynamic_object_of_type_at_pos(16, ip(5, 12)).is_none());
        first.base().borrow_mut().needs_removed = true;
        assert!(d.load_standard_dynamic_object_of_type_at_pos(16, ip(5, 12)).is_some());
        // add_*: a live object blocks the slot, a removed one does not
        d.add_ladder_at_pos_of_type_save_dict_placed_by_client(ip(6, 12), 100, None, None);
        let n = d.ta_len(19);
        d.add_ladder_at_pos_of_type_save_dict_placed_by_client(ip(6, 12), 100, None, None);
        assert_eq!(d.ta_len(19), n);
        d.ladder_at_pos(ip(6, 12)).unwrap().base().borrow_mut().needs_removed = true;
        d.add_ladder_at_pos_of_type_save_dict_placed_by_client(ip(6, 12), 100, None, None);
        assert_eq!(d.ta_len(19), n + 1);
        // portals count as the start portal
        ops.wb_type.set(13);
        d.workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(ip(9, 12), 1, None, None, None);
        assert!(d.portal_positions.borrow().contains_index(d.world_index_key(ip(9, 12))));
        // doors at the edge of the world have no tile to paint, doors below that are no tall doors stay
        d.add_door_at_pos_of_type_save_dict_placed_by_client(ip(10, 5000), 52, None, None);
        d.register_with_pos(&obj(20, 800, ip(12, 12), &log), 20, ip(12, 12));
        ops.item.set(69);
        assert!(d.remove_door_at_pos(ip(12, 13)).is_none());
        d.register_with_pos(&obj(20, 801, ip(14, 5000), &log), 20, ip(14, 5000));
        assert!(d.remove_door_at_pos(ip(14, 5000)).is_some());
        let _ = w;
    }

    #[test]
    fn train_cars_next_to_cars_move_over() {
        let (w, d, ops, log) = fixture("fs_cars2");
        ops.tap.set(true);
        d.do_set(42, 800, obj(42, 800, ip(0, 0), &log));
        // no rail below the click: the car stays; rail one row below: snap
        set_tile(&w, 30, 29, |t| t.foreground_contents = 98);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(30, 30), 205, None, None);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(30, 40), 205, None, None);
        // a rail piece to the left, a car on it: the new car moves right and down when there is a gap below
        for y in 30..32 {
            set_tile(&w, 41, y, |t| t.foreground_contents = 0);
        }
        set_tile(&w, 39, 31, |t| t.foreground_contents = 98);
        set_tile(&w, 40, 31, |t| t.foreground_contents = 0);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(40, 31), 205, None, None);
        // ... and up when the cell is solid
        set_tile(&w, 20, 31, |t| t.foreground_contents = 98);
        set_tile(&w, 21, 31, |t| t.type_index = 6);
        set_tile(&w, 22, 31, |t| t.type_index = 6);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(21, 31), 205, None, None);
        // a car beside a rail on flat ground stays at its height
        set_tile(&w, 50, 10, |t| t.foreground_contents = 98);
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(51, 10), 205, None, None);
        // the bottom of the world has no cell below to look at
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(5, 0), 205, None, None);
        // the cell the car would be pushed into is not loaded
        set_tile(&w, 33, 10, |t| t.foreground_contents = 98);
        *w.macro_tiles().get(0).unwrap().physical_block.borrow_mut() = None;
        d.place_train_car_at_pos_of_type_save_dict_placed_by_client(ip(32, 10), 205, None, None);
        assert!(log.borrow().iter().filter(|l| l.starts_with("car205")).count() >= 6);
    }
}
