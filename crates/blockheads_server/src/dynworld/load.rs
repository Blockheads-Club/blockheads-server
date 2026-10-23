//! Loading and saving of dynamic objects: global state, blockheads, per-macro-tile object files, world-generation
//! spawning, save-version migrations, surface blocks, and the initial net data of a macro tile.
//!
//! `saveGameWithWorldData:...`, `saveDynamicObjectsForMacroTile:...`,...); (`loadDynamicObjectsOfType:...`,
//! `loadClientOwnedDynamicObjectsForClient:...`, `loadDynamicObjectsForMacroTile:includeSurfaceBlocks:`).
//! `client` is always nil on the dedicated server (decision 1): client branches are dropped.

use std::collections::HashSet;

use super::state::{DwWorldGen, DynamicWorld, NetBlob, SaveLoadKind, MAX_DYNAMIC_OBJECT_TYPES};
use crate::fmath::cconv::{f32_to_i32, f64_to_i32};
use crate::fmath::vector::clamp;
use crate::foundation::exception::{invalid_argument, ns_raise};
use crate::foundation::gsdict::GsDict;
use crate::foundation::gzip::gzip_deflate;
use crate::foundation::nsnumber::{NsNumber, NsNumberKey};
use crate::foundation::paths::append_path_component;
use crate::nslog;
use crate::objects::base::number_with_unsigned_long;
use crate::objects::handle::{ptr_eq, ObjRef};
use crate::objects::type_tables::{
    dynamic_object_type_for_interaction_object_type, object_type_can_be_loaded_only_when_client_owner_online,
    object_type_has_static_position, object_type_is_interaction_object, object_type_is_plant, object_type_is_tree,
    object_type_requires_partial_update, DYNAMIC_OBJECT_TYPE_BLOCKHEAD, DYNAMIC_OBJECT_TYPE_CHEST,
    DYNAMIC_OBJECT_TYPE_FREE_BLOCK, DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT,
};
use crate::plist::helpers::{
    data_from_property_list, data_from_property_list_hooked, property_list_from_data_opt, property_list_with_data_raw,
};
use crate::plist::keyed_archiver::{archive_index_set, unarchive_index_set_opt};
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand::rand_tagged;
use crate::types::enums::TreeType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::growth::tree_type_for_tree_promise;
use crate::world::helpers::{tile_is_air_or_snow, tile_is_solid, tile_is_water, tile_requires_glow_block};
use crate::world::state::{MacroTileRef, PhysicalBlockRef, TileRef};
use crate::world::tiles::{
    macro_index_at_macro_position, macro_tile_at_macro_postion, tile_at_world_position, tile_at_world_position_loaded,
};

/// Result of `initialDynamicObjectsNetDataForMacroTileIndex:wireForClient:`: the `creationData` / `updateData`
/// dictionaries with `NSNumber(unsigned)` keys, as vectors in GSIMap enumeration order (contract 5.9).
#[derive(Default, Clone, Debug, PartialEq)]
pub struct InitialNetData {
    pub creation_data: Vec<(u32, Vec<NetBlob>)>,
    pub update_data: Vec<(u32, Vec<NetBlob>)>,
}

/// `999999.875f` (0x497423fe): the adult max age of generated trees.
pub const ADULT_MAX_AGE: f32 = 999999.875f32;

/// `clampi255(v)`: `v < 1 -> 0; v < 255 -> v; else 255`.
pub fn clampi255(v: i32) -> i32 {
    if v < 1 {
        0
    } else if v < 255 {
        v
    } else {
        255
    }
}

/// `geneByte(n) = clampi255((int)(127.0f + 127.0f * clamp(0.5f*(float)n + 0.5f, 0, 1)))`.
pub fn gene_byte(n: f64) -> i32 {
    let c = clamp(0.5f32 * (n as f32) + 0.5f32, 0.0f32, 1.0f32);
    clampi255(f32_to_i32(127.0f32 + 127.0f32 * c))
}

/// `a = (uint16)(int)(n+n); if a > 1: a = 2`.
fn tulip_ab(n: f32) -> i32 {
    let mut a = f32_to_i32(n + n) as u16;
    if a > 1 {
        a = 2;
    }
    a as i32
}

/// Tulip genes, "dual-noise" form (types 8, version < 7 migration, post-surface tulip).
/// Returns `(maxAgeGene, growthRateGene)`.
pub fn tulip_genes_dual(div1: f64, div2: f64) -> (i32, i32) {
    let n1 = (div1 as f32).abs();
    let mut r_t = 3;
    if n1 > 0.2f32 {
        r_t = 4;
        if n1 > 0.4f32 {
            r_t = 5;
            if n1 > 0.7f32 {
                r_t = 2;
            }
        }
    }
    let n2 = (div2 as f32).abs();
    let mut d_t = 3;
    if n2 > 0.3f32 {
        d_t = 4;
        if n2 > 0.7f32 {
            d_t = 5;
        }
    }
    let a = tulip_ab(n1);
    let b = tulip_ab(n2);
    let max_age = (r_t | d_t << 12 | r_t << 8 | d_t << 4) as i16 as i32;
    let growth = (a << 12 | b << 8) as i16 as i32;
    (max_age, growth)
}

/// Tulip rarity variant (tile-loop promises 0x7e/0x8e/0x8f; `rarity` 0/1/2).
pub fn tulip_genes_rarity(rarity: i32, rare1: f64, rare2: f64) -> (i32, i32) {
    let mut d = if rarity == 0 { 3 } else { 1 };
    let mut r = d;
    let n1 = (rare1 as f32).abs();
    if n1 > 0.2f32 {
        r = if rarity == 0 { 4 } else { 5 };
        if n1 > 0.4f32 && rarity == 2 {
            r = 7;
        }
    }
    let n2 = (rare2 as f32).abs();
    if n2 > 0.3f32 {
        d = if rarity == 0 { 4 } else { 5 };
        if n2 > 0.7f32 && rarity != 0 {
            d = 7;
        }
    }
    let a = tulip_ab(n1);
    let b = tulip_ab(n2);
    let max_age = (r | d << 12 | r << 8 | d << 4) as i16 as i32;
    let growth = (a << 12 | b << 8) as i16 as i32;
    (max_age, growth)
}

/// Generation depth of clownfish/sharks (path A): `t = (double)((float)r / 2^31f); t *= 0.9; d = (double)(0x200-y) * t;
/// y' = (int)((double)y + d)`.
pub fn gen_fish_depth(y: i32, r: i32) -> i32 {
    let t = ((r as f32) / 2147483648.0f32) as f64;
    let t = t * 0.9f64;
    let d = ((0x200 - y) as f64) * t;
    f64_to_i32(y as f64 + d)
}

/// Migration (version < 2) depth: float32-first form.
pub fn migration_fish_depth(y: i32, r: i32) -> i32 {
    let f = ((0x200 - y) as f32) * ((r as f32) / 2147483648.0f32);
    f64_to_i32(y as f64 + (f as f64) * 0.9f64)
}

/// Build `{key: value,...}` as a mutable dictionary.
fn dict_of(entries: Vec<(&str, PlistValue)>) -> PlistValue {
    let mut v = PlistValue::mutable_dict();
    let PlistValue::Dict(d) = &mut v else { unreachable!() };
    for (k, val) in entries {
        d.map.insert(k.to_string(), val);
    }
    v
}

/// `[NSNumber numberWithInt:]`
fn num_i32(v: i32) -> PlistValue {
    PlistValue::int(v as i64)
}

impl DynamicWorld {
    // ======================================================================================================
    // persistence helpers
    // ======================================================================================================

    fn world_db_put(&self, key: &str, data: &[u8]) {
        self.world_database.put(key, data);
    }

    fn dw_db_put(&self, key: &str, data: &[u8]) {
        self.dynamic_object_database.put(key, data);
    }

    /// `[NSData dataWithContentsOfFile:[worldSaveDirectory stringByAppendingPathComponent:name]]` when the file exists.
    fn read_save_file(&self, name: &str) -> Option<Vec<u8>> {
        let p = append_path_component(&self.world_save_directory, name);
        if std::path::Path::new(&p).exists() {
            std::fs::read(&p).ok()
        } else {
            None
        }
    }

    // ======================================================================================================
    // loadDynamicObjects:repositionBlockheadLoadFailures:
    // ======================================================================================================

    /// `-loadDynamicObjects:repositionBlockheadLoadFailures:` -> BOOL.
    pub fn load_dynamic_objects_reposition_blockhead_load_failures(
        &self,
        load_success_so_far: bool,
        reposition: bool,
    ) -> bool {
        let hooks = self.hooks();
        let world = self.world();
        // ---- Part A ----
        if load_success_so_far {
            let mut save_dict: Option<PlistValue> = None;
            if let Some(d) = self.world_database.data_for_key(Some("dynamicWorldv2")) {
                save_dict = property_list_from_data_opt(Some(&d));
            }
            if save_dict.is_none() {
                if let Some(d) = self.read_save_file("dynamicWorldv2") {
                    save_dict = property_list_from_data_opt(Some(&d));
                }
            }
            if save_dict.is_none() {
                // legacy `dynamicWorld` (dictionaryWithContentsOfFile:, old-style plist)
                if let Some(d) = self.read_save_file("dynamicWorld") {
                    save_dict = property_list_from_data_opt(Some(&d));
                }
            }
            if let Some(sd) = &save_dict {
                let get = |k: &str| sd.object_for_key(k);
                self.dynamic_object_id_count.set(get("dynamicObjectIDCount").map_or(0, |v| v.unsigned_long_value()));
                self.active_blockhead_index.set(get("activeBlockheadIndex").map_or(0, |v| v.int_value()));
                self.workbench_has_been_crafted.set(get("workbenchHasBeenCrafted").map_or(false, |v| v.bool_value()));
                if let Some(PlistValue::Dict(t)) = get("poleItemTakenTimes") {
                    *self.pole_item_taken_times.borrow_mut() = Some(GsDict::copy_from(&t.map));
                }
                let save_version = get("saveVersion").map_or(0, |v| v.int_value());
                if save_version < 2 {
                    self.build_conversion_list();
                }
                // savedGlowIndices: a keyed archive; missing -> "no portals" (contract 6, open item i)
                if let Some(PlistValue::Data(gd)) = get("savedGlowIndices") {
                    if let Some(idx) = unarchive_index_set_opt(Some(&gd.bytes), "portalPositionIndexSet") {
                        self.portal_positions.borrow_mut().add_indexes(&idx);
                    }
                }
            }
        }
        // ---- Part B (server only) ----
        let blockheads_data: Option<Vec<u8>> =
            self.world_database.data_for_key(Some("blockheads")).or_else(|| self.read_save_file("blockheads"));
        // ---- Part C ----
        if let Some(bd) = blockheads_data {
            let d = property_list_from_data_opt(Some(&bd));
            if let Some(d) = d {
                let mut count = 0;
                let mut all_loaded = true;
                let dicts: Vec<PlistValue> = match d.object_for_key("dynamicObjects") {
                    Some(PlistValue::Array(a)) => a.items.clone(),
                    _ => Vec::new(),
                };
                for object_dict in dicts.iter() {
                    let unique_id = object_dict.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value());
                    let inventory_name = format!("blockhead_{}_inventory", unique_id as i64);
                    let mut inventory_data = self.world_database.data_for_key(Some(&inventory_name));
                    if inventory_data.is_none() {
                        inventory_data = self.read_save_file(&inventory_name);
                    }
                    // blockheads with no inventory data are silently skipped (not counted as failure)
                    if let Some(inv) = inventory_data {
                        if let Some(other) = hooks.blockhead_with_id_including_net(unique_id) {
                            hooks.blockhead_will_be_unloaded(&other);
                            self.remove_from_list(&self.net_blockheads, &other);
                            self.remove_from_list(&self.blockheads, &other);
                        }
                        // NO @try: a corrupt blob may raise
                        let saved_inventory_slots = property_list_with_data_raw(&inv, 0);
                        let PlistValue::Dict(dict) = object_dict else { continue };
                        let bh = hooks.blockhead_from_save_dict(dict, saved_inventory_slots, reposition, None, None);
                        match bh {
                            None => all_loaded = false,
                            Some(bh) => {
                                self.blockheads.borrow_mut().push(bh);
                                count += 1;
                                if count > 4 {
                                    break; // at most 5 blockheads are loaded
                                }
                            }
                        }
                    }
                }
                if !all_loaded {
                    return false;
                }
                for bh in self.blockheads.borrow().clone() {
                    hooks.fully_load_if_needed_around_pos(bh.pos(), 0xFFFF_FFFF, &bh);
                    self.send_net_data_if_needed_for_object_is_creation(&bh, true);
                }
            }
            // unsigned compare of (ulong)(long)idx
            if (self.active_blockhead_index.get() as i64 as u64) >= self.blockheads.borrow().len() as u64 {
                self.active_blockhead_index.set(0);
            }
            self.has_loaded_blockheads.set(true);
            for i in 0..MAX_DYNAMIC_OBJECT_TYPES {
                let a: Vec<ObjRef> = self.dynamic_objects.borrow()[i].values().cloned().collect();
                for o in a {
                    o.blockheads_loaded(&o);
                }
                let b: Vec<ObjRef> = self.dynamic_objects_to_add.borrow()[i].values().cloned().collect();
                for o in b {
                    o.blockheads_loaded(&o);
                }
            }
            let rules = world.custom_rules();
            for bh in self.blockheads.borrow().clone() {
                hooks.blockhead_stop_interacting_if_no_interaction_object(&bh);
                if rules.custom_rules_enabled != 0 && rules.death == 4 {
                    hooks.blockhead_die(&bh);
                }
            }
        }
        true
    }

    /// Part A of `loadDynamicObjects` for `saveVersion < 2`: scan the save directory for `<x>_<y>_dynamicWorld_<n>`
    /// files and arm the conversion.
    fn build_conversion_list(&self) {
        let mut list: GsDict<String, GsDict<String, String>> = GsDict::new();
        if let Ok(rd) = std::fs::read_dir(&self.world_save_directory) {
            for e in rd.flatten() {
                let file_name = e.file_name().to_string_lossy().into_owned();
                if file_name.contains("_dynamicWorld") {
                    let comps: Vec<&str> = file_name.split('_').collect();
                    if comps.len() == 4 {
                        let dir_name = format!("{}_{}", comps[0], comps[1]);
                        let dir_path = append_path_component(&self.world_save_directory, &dir_name);
                        let old_full = append_path_component(&self.world_save_directory, &file_name);
                        let new_full = append_path_component(&dir_path, comps[2]);
                        if list.get(&dir_name).is_none() {
                            list.insert(dir_name.clone(), GsDict::new());
                        }
                        list.get_mut(&dir_name).unwrap().insert(old_full, new_full);
                    }
                }
            }
        }
        if list.is_empty() {
            *self.version_one_to_two_conversion_list.borrow_mut() = None;
        } else {
            let snapshot = GsDict::copy_from(&list);
            *self.version_one_to_two_conversion_list.borrow_mut() = Some(list);
            self.conversion_thread(snapshot);
        }
    }

    // ======================================================================================================
    // loadDynamicObjectsOfType / loadClientOwned / loadDynamicObjectsForMacroTile
    // ======================================================================================================

    /// `-loadDynamicObjectsOfType:fromData:physicalBlock:loadedPortal:clientOwnedID:`.
    pub fn load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
        &self,
        object_type: i32,
        un_compressed_data: Option<&[u8]>,
        pb: PhysicalBlockRef,
        loaded_portal: &mut bool,
        client_owned_id: Option<&str>,
    ) {
        // guard: != 21, != 0, < 65 (and reject negatives: the original indexes out of bounds)
        if object_type == 21 || object_type == 0 || object_type >= 65 || object_type < 0 {
            return;
        }
        let hooks = self.hooks();
        let world = self.world();
        let t = object_type as usize;
        let macro_index = macro_index_at_macro_position(pb.x_pos.get(), pb.y_pos.get(), &world) as u32;
        if client_owned_id.is_none() {
            if self.currently_loading_macro_blocks.borrow()[t].contains(&macro_index) {
                nslog!("loading again, mid load");
                return;
            }
            self.currently_loading_macro_blocks.borrow_mut()[t].insert(macro_index);
        }
        let save_data = un_compressed_data.and_then(|d| property_list_from_data_opt(Some(d)));
        if let Some(save_data) = save_data {
            let dicts: Vec<PlistValue> = match save_data.object_for_key("dynamicObjects") {
                Some(PlistValue::Array(a)) => a.items.clone(),
                _ => Vec::new(),
            };
            let mut shark_count = 0;
            for object_dict in dicts.iter() {
                let PlistValue::Dict(dict) = object_dict else { continue };
                let int_of = |k: &str| object_dict.object_for_key(k).map_or(0, |v| v.int_value());
                let unique_id = object_dict.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value());
                let mut type_to_check = object_type;
                if object_type == 0xF {
                    let tt = dynamic_object_type_for_interaction_object_type(int_of("interactionObjectType"));
                    if tt != 0 {
                        type_to_check = tt;
                    }
                }
                let tc = type_to_check as usize;
                if self.do_contains(tc, unique_id) || self.ta_contains(tc, unique_id) {
                    continue;
                }
                if self.currently_adding_object_ids.borrow()[tc].contains(&unique_id) {
                    nslog!("dupe object id");
                    hooks.dynamic_world_changed_at_pos_object_type(
                        IntPair { x: int_of("pos_x"), y: int_of("pos_y") },
                        type_to_check,
                    );
                    continue;
                }
                if object_type_has_static_position(type_to_check) {
                    let pos = IntPair { x: int_of("pos_x"), y: int_of("pos_y") };
                    let wi = self.world_index_key(pos);
                    if self.bp_contains(tc, wi) {
                        if object_type != 0x12 && object_type != 0x2d {
                            nslog!("dupe static position object {} {}", pos.x, pos.y);
                        }
                        hooks.dynamic_world_changed_at_pos_object_type(pos, type_to_check);
                        continue;
                    }
                }
                self.currently_adding_object_ids.borrow_mut()[tc].insert(unique_id);
                let mut obj: Option<ObjRef> = None;
                if object_type_is_tree(object_type) {
                    obj = hooks.construct_from_save_dict(SaveLoadKind::TreeOrPlant, object_type, dict, None);
                    if pb.version.get() < 4 {
                        if let Some(o) = &obj {
                            hooks.tree_update_all_owned_tiles_to_new_id_size(o);
                        }
                    }
                } else if object_type_is_plant(object_type) {
                    obj = hooks.construct_from_save_dict(SaveLoadKind::TreeOrPlant, object_type, dict, None);
                } else if object_type == 0x24 {
                    if shark_count < 16 {
                        obj = hooks.construct_from_save_dict(SaveLoadKind::Shark, object_type, dict, None);
                        if obj.is_some() {
                            shark_count += 1;
                        }
                    }
                } else if object_type == DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT {
                    let key = format!("trainchest_{}", unique_id as i64);
                    let mut chest_data = self.dynamic_object_database.data_for_key(Some(&key));
                    if chest_data.is_none() && !world.has_finished_database_migration_to17() {
                        chest_data = self.read_save_file(&key);
                    }
                    let chest_dict = chest_data.and_then(|d| property_list_from_data_opt(Some(&d)));
                    if chest_dict.is_none() {
                        nslog!("ERROR LOADING CHEST FOR CARGO TRAIN");
                    }
                    let cd = match &chest_dict {
                        Some(PlistValue::Dict(d)) => Some(d),
                        _ => None,
                    };
                    obj = hooks.construct_from_save_dict(SaveLoadKind::FreightCar, object_type, dict, cd);
                } else if object_type == 0xF || object_type_is_interaction_object(object_type) {
                    if object_type == 0xF {
                        let p = IntPair { x: int_of("pos_x"), y: int_of("pos_y") };
                        hooks.dynamic_world_changed_at_pos_object_type(p, type_to_check);
                        hooks.dynamic_world_changed_at_pos_object_type(p, 0xF);
                    }
                    let ito = int_of("interactionObjectType");
                    if ito == 1 {
                        let wb_type = int_of("workbenchType");
                        obj = hooks.construct_from_save_dict(SaveLoadKind::Workbench, object_type, dict, None);
                        if wb_type == 1 || wb_type == 0xd {
                            let base_pos = obj.as_ref().map_or(IntPair { x: 0, y: 0 }, |o| o.pos());
                            if wb_type == 1 {
                                *loaded_portal = true;
                                // tile conversion: level 0..5 -> typeIndex on tile (base.x, base.y-1)
                                if let Some(tile) = tile_at_world_position_loaded(base_pos.x, base_pos.y - 1, &world) {
                                    let level = obj.as_ref().map_or(0, |o| hooks.workbench_level(o));
                                    let ti: Option<u8> = match level {
                                        0 => Some(0x19),
                                        1 => Some(0x21),
                                        2 => Some(0x22),
                                        3 => Some(0x23),
                                        4 => Some(0x24),
                                        5 => Some(0x25),
                                        _ => None,
                                    };
                                    if let Some(ti) = ti {
                                        tile.set_type_index(ti);
                                    }
                                }
                            }
                            let key = self.world_index_key(base_pos);
                            self.portal_positions.borrow_mut().add_index(key);
                        }
                        if wb_type == 2 && pb.version.get() < 6 {
                            if let Some(o) = &obj {
                                if hooks.workbench_level(o) == 2 {
                                    hooks.workbench_set_level_silently(o, 3);
                                }
                            }
                        }
                    } else {
                        obj = hooks.construct_from_save_dict(
                            SaveLoadKind::InteractionObject(ito),
                            object_type,
                            dict,
                            None,
                        );
                    }
                } else if object_type != 0x16 {
                    // surface blocks are never loaded from the DB
                    obj = hooks.construct_from_save_dict(SaveLoadKind::Generic, object_type, dict, None);
                }
                match obj {
                    None => {
                        self.currently_adding_object_ids.borrow_mut()[tc].remove(&unique_id);
                    }
                    Some(obj) => {
                        let ot = obj.object_type() as usize;
                        // statement order after a successful init (contract 5.6)
                        self.ta_set(ot, unique_id, obj.clone());
                        if self.has_loaded_blockheads.get() {
                            obj.blockheads_loaded(&obj);
                        }
                        if object_type_has_static_position(ot as i32) {
                            let key = self.world_index_key(obj.pos());
                            self.bp_set(ot, key, obj.clone());
                        }
                        if ot as i32 == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
                            let key = self.world_index_key(obj.pos());
                            self.free_blocks_insert(key, &obj);
                        }
                        if let Some(cid) = client_owned_id {
                            if let Some(sc) = self.server_client(cid) {
                                if let Some(set) = sc.world_indices_containing_tamed_animals().borrow_mut().get_mut(ot)
                                {
                                    set.insert(macro_index);
                                }
                            }
                        }
                    }
                }
            }
        }
        if client_owned_id.is_none() {
            self.currently_loading_macro_blocks.borrow_mut()[t].remove(&macro_index);
        }
    }

    /// `-loadClientOwnedDynamicObjectsForClient:physicalBlock:`.
    pub fn load_client_owned_dynamic_objects_for_client_physical_block(&self, client_id: &str, pb: PhysicalBlockRef) {
        let world = self.world();
        let mut loaded_portal = false;
        for i in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            if object_type_can_be_loaded_only_when_client_owner_online(i) {
                let key = format!("{}_{}_{}/{}", client_id, pb.x_pos.get(), pb.y_pos.get(), i);
                if let Some(data) = self.dynamic_object_database.data_for_key(Some(&key)) {
                    let mt = macro_tile_at_macro_postion(pb.x_pos.get(), pb.y_pos.get(), &world.macro_tiles(), &world);
                    if let Some(mt) = mt {
                        if mt.physical_block().map_or(false, |p| std::ptr::eq(p, pb)) {
                            if !mt.dynamic_objects_loaded.get() {
                                if let Some(h) = world.ops_hooks() {
                                    h.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
                                        &world, &mt, true,
                                    );
                                }
                            } else {
                                self.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
                                    i,
                                    Some(&data),
                                    pb,
                                    &mut loaded_portal,
                                    Some(client_id),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // ======================================================================================================
    // macro block population (clean-room rewrite from the behavioural specification)
    // ======================================================================================================

    /// Tree-density noise sampled at `((x + offset) / W, y)`; zero when no noise source is installed.
    fn noise(&self, x: i32, offset: f32, y: f64, octaves: i32) -> f64 {
        let width_cells = (self.world().world_width_macro() << 5) as f32;
        let sample_x = (((x as f32) + offset) / width_cells) as f64;
        self.tree_density_noise_function.as_ref().map_or(0.0, |n| n.get_x_y_octaves(sample_x, y, octaves))
    }

    /// Genes of generated trees: (height, growth).
    fn tree_genes(&self, x: i32) -> (i32, i32) {
        (gene_byte(self.noise(x, 0.0, 0.7641, 8)), gene_byte(self.noise(x, 563.0, 0.4341, 8)))
    }

    /// Genes of ordinary plants: (A, B).
    fn plant_genes(&self, x: i32) -> (i32, i32) {
        (gene_byte(self.noise(x, 4678.0, 0.4651, 8)), gene_byte(self.noise(x, 56483.0, 0.5641, 8)))
    }

    /// Tulip genes of the "dual" form.
    fn dual_tulip_genes(&self, x: i32) -> (i32, i32) {
        tulip_genes_dual(self.noise(x, 4678.0, 0.4651, 4), self.noise(x, 2678.0, 0.4251, 4))
    }

    /// Tulip genes of the "rarity" form.
    fn rarity_tulip_genes(&self, x: i32, rarity: i32) -> (i32, i32) {
        tulip_genes_rarity(rarity, self.noise(x, 4378.0, 0.4651, 4), self.noise(x, 2371.0, 0.4251, 4))
    }

    /// Turns a gem promise in the foreground into a gem and lights a torch there. False for anything else.
    fn convert_gem_promise(&self, tile: TileRef, pos: IntPair) -> bool {
        let (gem, torch) = match tile.foreground_contents() {
            0x34 => (0x33, 0x4b),
            0x36 => (0x35, 0x4c),
            0x38 => (0x37, 0x56),
            0x3a => (0x39, 0x57),
            0x3c => (0x3b, 0x58),
            _ => return false,
        };
        tile.set_foreground_contents(gem);
        self.hooks().add_torch_at_pos_of_type(pos, torch);
        true
    }

    /// Glow block if the tile needs one, otherwise a gem-promise conversion.
    fn glow_or_gem(&self, tile: TileRef, pos: IntPair) {
        if tile_requires_glow_block(tile) {
            self.hooks().load_glow_block_if_needed_at_pos_tile(pos, tile);
        } else {
            self.convert_gem_promise(tile, pos);
        }
    }

    /// Entry point: populate one macro block with its dynamic objects.
    pub fn load_dynamic_objects_for_macro_tile_include_surface_blocks(
        &self,
        mt: &MacroTileRef,
        include_surface_blocks: bool,
    ) {
        let Some(block) = mt.physical_block() else {
            nslog!("trying to load dynamic objects without physical block");
            return;
        };
        if block.version.get() == 0 {
            self.populate_fresh_block(block);
        } else {
            self.populate_from_save(block);
        }
        if include_surface_blocks {
            self.populate_surface_blocks(block);
        }
    }

    // ---------------------------------------------------------------- first-time generation

    fn populate_fresh_block(&self, block: PhysicalBlockRef) {
        block.version.set(8);
        let (bx, by) = (block.x_pos.get(), block.y_pos.get());
        self.hooks().light_changed_at_macro_pos_send_reliably(IntPair { x: bx, y: by }, true);
        let Some(loader) = self.world_tile_loader.borrow().clone() else { return };
        let pending = Pending::of(&*loader);
        let width = self.world().world_width_macro() * 32;
        for lx in 0..32 {
            let x = bx * 32 + lx;
            let surface = loader.max_of_rock_and_dirt_height_for_x(x);
            if let Some(hat) = pole_hat(x, width) {
                self.place_pole_hat(&*loader, block, x, surface, hat);
                continue;
            }
            if surface < by * 32 {
                self.scan_sky_column(block, lx, x);
            } else {
                self.scan_lit_column(block, lx, x);
            }
            if (by - 1) * 32 <= surface && surface <= (by + 1) * 32 {
                self.spawn_on_surface(&pending, x, surface, by);
            }
        }
    }

    /// Hovering free block above the pole column, when its height falls in this block.
    fn place_pole_hat(&self, loader: &dyn DwWorldGen, block: PhysicalBlockRef, x: i32, surface: i32, hat: ItemType) {
        let top = surface.max(loader.lake_height_for_x(x));
        let y = top.max(512) + 16;
        let by = block.y_pos.get();
        if (by * 32..(by + 1) * 32).contains(&y) {
            self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                IntPair { x, y }, hat, 0, 0, None, None, true, 0, None,
            );
        }
    }

    /// Cells of a column that lies completely above the terrain, bottom to top.
    fn scan_sky_column(&self, block: PhysicalBlockRef, lx: i32, x: i32) {
        let hooks = self.hooks();
        for ly in 0..32 {
            let pos = IntPair { x, y: block.y_pos.get() * 32 + ly };
            let tile = block.tile_ref((ly * 32 + lx) as usize);
            let tree = tree_type_for_tree_promise(tile.contents() as i32);
            if tree != TreeType::TREE_NOTHING {
                tile.set_contents(0);
                let (height, growth) = self.tree_genes(x);
                hooks.load_tree_at_position(pos, tree.0 as i32, height, growth, true, ADULT_MAX_AGE);
                let companion = if tree == TreeType::TREE_APPLE {
                    1
                } else if tree == TreeType::TREE_CACTUS {
                    7
                } else {
                    continue;
                };
                hooks.load_npc_at_position(pos, companion, true, false);
                continue;
            }
            if tile_requires_glow_block(tile) {
                hooks.load_glow_block_if_needed_at_pos_tile(pos, tile);
                continue;
            }
            if self.convert_gem_promise(tile, pos) {
                continue;
            }
            let contents = tile.contents();
            if matches!(contents, 0x5e | 0x90 | 0x91) {
                tile.set_contents(0);
                if tile.type_index() == 2 {
                    hooks.create_treasure_chest_or_troll_at_tile(tile, pos, contents != 0x91, contents != 0x90);
                }
                continue;
            }
            match tile.foreground_contents() {
                0x7f => {
                    tile.set_foreground_contents(0);
                    let (a, b) = self.plant_genes(x);
                    hooks.load_plant_at_position(pos, 7, a, b, true);
                }
                promise @ (0x7e | 0x8e | 0x8f) => {
                    tile.set_foreground_contents(0);
                    let rarity = match promise {
                        0x7e => 0,
                        0x8e => 1,
                        _ => 2,
                    };
                    let (a, b) = self.rarity_tulip_genes(x, rarity);
                    hooks.load_plant_at_position(pos, 8, a, b, true);
                }
                _ => {}
            }
        }
    }

    /// Cells of a column that touches or underlies the terrain: only artificially lit cells matter.
    fn scan_lit_column(&self, block: PhysicalBlockRef, lx: i32, x: i32) {
        for ly in 0..32 {
            let tile = block.tile_ref((ly * 32 + lx) as usize);
            let t = tile.get();
            if t.artificial_light_r | t.artificial_light_g | t.artificial_light_b != 0 {
                self.glow_or_gem(tile, IntPair { x, y: block.y_pos.get() * 32 + ly });
            }
        }
    }

    /// Pending tree / NPC / plant spawns for the column's surface point.
    fn spawn_on_surface(&self, pending: &Pending, x: i32, surface: i32, by: i32) {
        let (tree, npc, plant) = (pending.trees.get(x), pending.npcs.get(x), pending.plants.get(x));
        if tree == 0 && npc == 0 && plant == 0 {
            return;
        }
        let hooks = self.hooks();
        let local = surface - by * 32;
        let in_block = (0..32).contains(&local);
        let mut at = IntPair { x, y: surface };
        if tree != 0 {
            if !in_block {
                return;
            }
            pending.trees.set(x, 0);
            let (height, growth) = self.tree_genes(x);
            hooks.load_tree_at_position(at, tree, height, growth, true, ADULT_MAX_AGE);
        }
        if npc != 0 {
            let is_fish = npc == 4 || npc == 5;
            let eligible = match npc {
                5 => at.y < 498 && by > 14,
                _ => in_block,
            };
            if !eligible {
                return;
            }
            if is_fish {
                at.y = gen_fish_depth(at.y, rand_tagged("dynworld::populate_fresh_block:fish_depth"));
            }
            pending.npcs.set(x, 0);
            hooks.load_npc_at_position(at, npc, true, false);
        }
        if plant != 0 && in_block {
            // `at` may carry the fish depth from above.
            let (a, b) = if plant == 8 {
                self.dual_tulip_genes(x)
            } else {
                pending.plants.set(x, 0);
                self.plant_genes(x)
            };
            hooks.load_plant_at_position(at, plant, a, b, true);
        }
    }

    // ---------------------------------------------------------------- loading from the save

    fn populate_from_save(&self, block: PhysicalBlockRef) {
        let (bx, by) = (block.x_pos.get(), block.y_pos.get());
        let dir = format!("{}_{}", bx, by);
        self.apply_pending_conversion(&dir);
        let mut seen: HashSet<String> = HashSet::new();
        let mut portal_loaded = false;
        self.load_object_blobs(block, &dir, &mut seen, &mut portal_loaded);
        self.load_legacy_files(block, &dir, &mut seen, &mut portal_loaded);
        if self.has_server() {
            for client in self.server_client_keys() {
                self.load_client_owned_dynamic_objects_for_client_physical_block(&client, block);
            }
        }
        self.repair_start_portal(bx, by, portal_loaded);
        self.run_migrations(block);
        if block.version.get() < 8 {
            self.hooks().update_physical_block_to_latest_version(block);
        }
        block.version.set(8);
    }

    /// Performs the pending version 1 -> 2 file renames of this directory, if any.
    fn apply_pending_conversion(&self, dir: &str) {
        let Some(renames) =
            self.version_one_to_two_conversion_list.borrow().as_ref().and_then(|l| l.get(&dir.to_string()).cloned())
        else {
            return;
        };
        let _ = std::fs::create_dir_all(append_path_component(&self.world_save_directory, dir));
        for (old, new) in renames.iter() {
            let _ = std::fs::rename(old, new);
        }
        // the renames came from this very list
        let mut list = self.version_one_to_two_conversion_list.borrow_mut();
        let l = list.as_mut().expect("conversion list");
        l.remove(&dir.to_string());
        if l.is_empty() {
            *list = None;
        }
    }

    /// Per-type blobs from the object database.
    fn load_object_blobs(
        &self,
        block: PhysicalBlockRef,
        dir: &str,
        seen: &mut HashSet<String>,
        portal_loaded: &mut bool,
    ) {
        for ty in 0..MAX_DYNAMIC_OBJECT_TYPES as i32 {
            let key = format!("{dir}/{ty}");
            if let Some(blob) = self.dynamic_object_database.data_for_key(Some(&key)) {
                seen.insert(key);
                self.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
                    ty,
                    Some(&blob),
                    block,
                    portal_loaded,
                    None,
                );
            }
        }
    }

    /// Per-type files of worlds that have not finished the database migration.
    fn load_legacy_files(
        &self,
        block: PhysicalBlockRef,
        dir: &str,
        seen: &mut HashSet<String>,
        portal_loaded: &mut bool,
    ) {
        if self.world().has_finished_database_migration_to17() {
            return;
        }
        let folder = append_path_component(&self.world_save_directory, dir);
        let Ok(entries) = std::fs::read_dir(&folder) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains('_') {
                continue;
            }
            let ty = crate::foundation::nsstring::int_value(&name);
            if !seen.insert(format!("{dir}/{ty}")) {
                continue;
            }
            let bytes = std::fs::read(append_path_component(&folder, &name)).ok();
            self.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
                ty,
                bytes.as_deref(),
                block,
                portal_loaded,
                None,
            );
        }
    }

    /// Re-creates the start portal when its block is loaded but the portal is gone.
    fn repair_start_portal(&self, bx: i32, by: i32, portal_loaded: bool) {
        let world = self.world();
        let hooks = self.hooks();
        let at = world.start_portal_pos();
        if at.x / 32 != bx || at.y / 32 != by || portal_loaded || hooks.workbench_at_pos(at).is_some() {
            return;
        }
        for y in [at.y, at.y + 1] {
            if let Some(t) = tile_at_world_position_loaded(at.x, y, &world) {
                t.update(|t| {
                    t.background_contents = 0;
                    t.foreground_contents = 0;
                    t.contents = 0;
                    t.type_index = 2;
                    t.back_wall_type_index = 2;
                });
            }
        }
        hooks.place_basic_portal_workbench(at);
        if let Some(t) = tile_at_world_position_loaded(at.x, at.y - 1, &world) {
            t.update(|t| {
                t.type_index = 0x19;
                t.contents = 0;
                t.zone_type_index = 1;
            });
        }
    }

    /// Surface point of column `x` when it lies within this block's rows.
    fn surface_in_block(loader: &dyn DwWorldGen, x: i32, by: i32) -> Option<IntPair> {
        let local = loader.max_of_rock_and_dirt_height_for_x(x) - by * 32;
        (0..32).contains(&local).then_some(IntPair { x, y: by * 32 + local })
    }

    /// Spawns that older save versions never stored.
    fn run_migrations(&self, block: PhysicalBlockRef) {
        let Some(loader) = self.world_tile_loader.borrow().clone() else { return };
        let pending = Pending::of(&*loader);
        let (bx, by) = (block.x_pos.get(), block.y_pos.get());
        let version = block.version.get();
        let columns = (bx * 32)..(bx * 32 + 32);
        let steps: [(u8, fn(&DynamicWorld, &Pending, i32, Option<IntPair>)); 4] = [
            (2, DynamicWorld::migrate_v2),
            (6, DynamicWorld::migrate_v6),
            (7, DynamicWorld::migrate_v7),
            (8, DynamicWorld::migrate_v8),
        ];
        for (below, step) in steps {
            if version < below {
                for x in columns.clone() {
                    step(self, &pending, x, Self::surface_in_block(&*loader, x, by));
                }
            }
        }
    }

    /// Plant 6, then fish.
    fn migrate_v2(&self, pending: &Pending, x: i32, surface: Option<IntPair>) {
        let hooks = self.hooks();
        if pending.plants.get(x) == 6 {
            let Some(at) = surface else { return };
            pending.plants.set(x, 0);
            let (a, b) = self.plant_genes(x);
            hooks.load_plant_at_position(at, 6, a, b, true);
        }
        let npc = pending.npcs.get(x);
        if let (4 | 5, Some(mut at)) = (npc, surface) {
            if npc == 4 || at.y < 498 {
                at.y = migration_fish_depth(at.y, rand_tagged("dynworld::populate_from_save:migration_fish_depth"));
                pending.npcs.set(x, 0);
                hooks.load_npc_at_position(at, npc, true, false);
            }
        }
    }

    /// NPC 7 is always consumed, spawned only when the surface is in this block.
    fn migrate_v6(&self, pending: &Pending, x: i32, surface: Option<IntPair>) {
        if pending.npcs.get(x) == 7 {
            pending.npcs.set(x, 0);
            if let Some(at) = surface {
                self.hooks().load_npc_at_position(at, 7, true, false);
            }
        }
    }

    /// Tulips (not consumed).
    fn migrate_v7(&self, pending: &Pending, x: i32, surface: Option<IntPair>) {
        if pending.plants.get(x) != 8 {
            return;
        }
        if let Some(at) = surface.filter(|&at| self.is_meadow(at)) {
            let (a, b) = self.dual_tulip_genes(x);
            self.hooks().load_plant_at_position(at, 8, a, b, true);
        }
    }

    /// NPC 8 and plants 9 / 10.
    fn migrate_v8(&self, pending: &Pending, x: i32, surface: Option<IntPair>) {
        let (npc, plant) = (pending.npcs.get(x), pending.plants.get(x));
        if npc != 8 && plant != 9 && plant != 10 {
            return;
        }
        let Some(at) = surface.filter(|&at| self.is_meadow(at)) else { return };
        let hooks = self.hooks();
        if npc == 8 {
            pending.npcs.set(x, 0);
            hooks.load_npc_at_position(at, 8, true, false);
        }
        if plant == 9 || plant == 10 {
            pending.plants.set(x, 0);
            let (a, b) = self.plant_genes(x);
            hooks.load_plant_at_position(at, plant, a, b, true);
        }
    }

    /// Air or snow with a back wall of type 2 standing on dirt, grass or frozen grass.
    fn is_meadow(&self, at: IntPair) -> bool {
        let world = self.world();
        let Some(tile) = tile_at_world_position_loaded(at.x, at.y, &world) else { return false };
        if !tile_is_air_or_snow(Some(tile)) || tile.back_wall_type_index() != 2 {
            return false;
        }
        tile_at_world_position_loaded(at.x, at.y - 1, &world)
            .is_some_and(|below| matches!(below.type_index(), 6 | 0x1b | 0x1c))
    }

    // ---------------------------------------------------------------- surface blocks

    fn populate_surface_blocks(&self, block: PhysicalBlockRef) {
        let world = self.world();
        let hooks = self.hooks();
        let (bx, by) = (block.x_pos.get(), block.y_pos.get());
        let peek = |x: i32, y: i32| tile_at_world_position(x, y, &world.macro_tiles(), &world);
        let water_level = |t: Option<TileRef>| t.filter(|&t| tile_is_water(Some(t))).map(|t| t.partial_content_left());
        for x in (bx * 32)..(bx * 32 + 32) {
            for y in (by * 32)..(by * 32 + 32) {
                let at = IntPair { x, y };
                let tile = tile_at_world_position_loaded(x, y, &world);
                if water_level(tile).is_some_and(|l| l < 255) {
                    hooks.load_surface_block_at_pos(at);
                } else if !tile_is_air_or_snow(tile) {
                    if tile.is_some_and(|t| t.type_index() == 4) {
                        hooks.load_snow_surface_block_at_pos_load_snow(at, true);
                    }
                } else {
                    let below = peek(x, y - 1);
                    if tile_is_solid(below) || tile_is_water(below) {
                        hooks.load_snow_surface_block_at_pos_load_snow(at, true);
                    }
                    let wet = water_level(peek(x, y + 1)).is_some_and(|l| l != 0)
                        || water_level(peek(x - 1, y)).is_some_and(|l| l >= 2)
                        || water_level(peek(x + 1, y)).is_some_and(|l| l >= 2);
                    if wet {
                        hooks.load_surface_block_at_pos(at);
                    }
                }
            }
        }
    }

    // ======================================================================================================
    // initial net data / unload
    // ======================================================================================================

    /// `-initialDynamicObjectsNetDataForMacroTileIndex:wireForClient:`.
    /// Returns `None` iff `mt->physicalBlock` was NULL when read (BEFORE the two load calls).
    pub fn initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(
        &self,
        macro_tile_index: i32,
        client: &str,
    ) -> Option<InitialNetData> {
        let world = self.world();
        let mt = world.macro_tiles().get(macro_tile_index.max(0) as usize)?;
        let pb = mt.physical_block();
        // a tile exists, so the world is at least one macro tile wide
        let w = world.world_width_macro();
        let (mpx, mpy) = (macro_tile_index % w, macro_tile_index / w);
        let _ = tile_at_world_position_loaded(mpx << 5, mpy << 5, &world);
        if let Some(h) = world.ops_hooks() {
            h.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&world, &mt, true);
        }
        if pb.is_none() {
            return None;
        }
        let mut creation: GsDict<NsNumberKey, Vec<NetBlob>> = GsDict::new();
        let mut update: GsDict<NsNumberKey, Vec<NetBlob>> = GsDict::new();
        let objects: Vec<ObjRef> = mt.dynamic_objects.borrow().clone().unwrap_or_default();
        if !objects.is_empty() {
            let sc = self.server_client(client);
            let key = |t: u32| NsNumberKey(NsNumber::UInt(t as u64));
            for object in objects {
                if object.object_type() != DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    let t = object.object_type() as u32;
                    if let Some(nd) = object.creation_net_data_for_client(&object, Some(client)) {
                        if creation.get(&key(t)).is_none() {
                            creation.insert(key(t), Vec::new());
                        }
                        creation.get_mut(&key(t)).unwrap().push(nd);
                        if let Some(sc) = &sc {
                            sc.wire_dynamic_object(object.unique_id());
                        }
                    }
                } else {
                    let k = key(DYNAMIC_OBJECT_TYPE_BLOCKHEAD as u32);
                    if creation.get(&k).is_none() {
                        creation.insert(k.clone(), Vec::new());
                    }
                    let Some(nd) = object.creation_net_data_for_client(&object, Some(client)) else {
                        invalid_argument("Tried to add nil to array");
                    };
                    creation.get_mut(&k).unwrap().push(nd);
                    if update.get(&k).is_none() {
                        update.insert(k.clone(), Vec::new());
                    }
                    let Some(ud) = object.update_net_data_for_client(&object, Some(client)) else {
                        invalid_argument("Tried to add nil to array");
                    };
                    update.get_mut(&k).unwrap().push(ud);
                    if let Some(sc) = &sc {
                        sc.wire_dynamic_object(object.unique_id());
                    }
                }
            }
        }
        let conv = |g: &GsDict<NsNumberKey, Vec<NetBlob>>| -> Vec<(u32, Vec<NetBlob>)> {
            g.iter()
                .map(|(k, v)| {
                    // the keys are always `numberWithUnsignedInt:`
                    let n = if let NsNumber::UInt(u) = k.0 { u as u32 } else { 0 };
                    (n, v.clone())
                })
                .collect()
        };
        Some(InitialNetData { creation_data: conv(&creation), update_data: conv(&update) })
    }

    /// `-removeDynamicObjectsForMacroTile:`: unload all dynamic objects of a macro tile from the bookkeeping.
    pub fn remove_dynamic_objects_for_macro_tile(&self, mt: &MacroTileRef) {
        let pb = mt.physical_block();
        let objects: Vec<ObjRef> = mt.dynamic_objects.borrow().clone().unwrap_or_default();
        if pb.is_some() && !objects.is_empty() {
            for object in objects {
                let t = object.object_type();
                if t == DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    // client == nil: blockheads are left alone
                } else if t != 21 {
                    let id = object.unique_id();
                    let tu = t as usize;
                    self.do_erase(tu, id);
                    self.ta_erase(tu, id);
                    self.currently_adding_object_ids.borrow_mut()[tu].remove(&id);
                    if t == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
                        let key = self.world_index_key(object.pos());
                        self.free_blocks_by_position.borrow_mut().remove(&key);
                    }
                    if object_type_has_static_position(t) {
                        let key = self.world_index_key(object.pos());
                        self.bp_erase(tu, key);
                    }
                    if object_type_requires_partial_update(t) {
                        self.partial_update_ordered_objects.borrow_mut()[tu].retain(|o| !ptr_eq(o, &object));
                    }
                }
            }
        }
        *mt.dynamic_objects.borrow_mut() = None;
        mt.dynamic_objects_loaded.set(false);
    }

    // ======================================================================================================
    // saving
    // ======================================================================================================

    /// `-sendLightblocksToClients`.
    pub fn send_lightblocks_to_clients(&self) {
        let hooks = self.hooks();
        let world = self.world();
        for i in 0..32usize {
            loop {
                let front = {
                    let v = self.light_changed_send_unreliably_macro_positions_single_client.borrow();
                    v[i].first().copied()
                };
                let Some(this_pos) = front else { break };
                let mt = macro_tile_at_macro_postion(this_pos.x, this_pos.y, &world.macro_tiles(), &world);
                if let Some(mt) = mt {
                    let mut send_now = true;
                    if self.world_changed_macro_positions.borrow().iter().any(|p| *p == this_pos) {
                        send_now = false;
                    } else if self.world_changed_send_unreliably_macro_positions.borrow().iter().any(|p| *p == this_pos)
                    {
                        send_now = false;
                    }
                    hooks.save_light_block_for_client_light_block_index(i as i32, mt.physical_block(), send_now);
                }
                let mut v = self.light_changed_send_unreliably_macro_positions_single_client.borrow_mut();
                if !v[i].is_empty() {
                    v[i].remove(0);
                }
            }
        }
    }

    /// `-saveDynamicObjects`: blockheads (type 24) are never saved here, but their dirty list is cleared.
    pub fn save_dynamic_objects(&self) {
        let world = self.world();
        for object_type in 0..MAX_DYNAMIC_OBJECT_TYPES {
            let v: Vec<IntPair> = self.dynamic_world_changed_macro_positions.borrow()[object_type].clone();
            if !v.is_empty() {
                if object_type as i32 != DYNAMIC_OBJECT_TYPE_BLOCKHEAD {
                    let macro_tiles = world.macro_tiles();
                    for pos in v {
                        if let Some(mt) = macro_tile_at_macro_postion(pos.x, pos.y, &macro_tiles, &world) {
                            self.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(
                                &mt,
                                object_type as i32,
                                pos.x,
                                pos.y,
                            );
                        }
                    }
                }
                self.dynamic_world_changed_macro_positions.borrow_mut()[object_type].clear();
            }
        }
    }

    /// `-saveGameWithWorldData:signOwnershipData:`.
    pub fn save_game_with_world_data_sign_ownership_data(
        &self,
        world_save_data: Option<&[u8]>,
        sign_ownership_data: Option<&[u8]>,
    ) {
        let hooks = self.hooks();
        let world = self.world();
        let glow = archive_index_set(&self.portal_positions.borrow(), "portalPositionIndexSet");
        let mut entries: Vec<(&str, PlistValue)> = vec![
            ("dynamicObjectIDCount", number_with_unsigned_long(self.dynamic_object_id_count.get())),
            ("activeBlockheadIndex", num_i32(self.active_blockhead_index.get())),
            ("workbenchHasBeenCrafted", PlistValue::boolean(self.workbench_has_been_crafted.get())),
            ("saveVersion", num_i32(if self.version_one_to_two_conversion_list.borrow().is_none() { 8 } else { 1 })),
            // [[CrystalManager instance] commitSaveIfNeeded] is an empty method (F9)
            ("savedGlowIndices", PlistValue::data(glow)),
        ];
        if let Some(PlistValue::Dict(d)) =
            self.pole_item_taken_times.borrow().as_ref().map(|d| PlistValue::dict(GsDict::copy_from(d)))
        {
            entries.push(("poleItemTakenTimes", PlistValue::Dict(d)));
        }
        let save_dict = dict_of(entries);
        let save_data = data_from_property_list(&save_dict);
        if let (Some(sd), Some(wsd)) = (&save_data, world_save_data) {
            self.world_db_put("dynamicWorldv2", sd);
            self.world_db_put("worldv2", wsd);
            // appDatabase is nil on the dedicated server (decision 4): `None` skips
            if let Some(app) = &self.app_database {
                app.put(&format!("{}_worldv2", world.save_id().unwrap_or_default()), wsd);
            }
        }
        if let Some(sod) = sign_ownership_data {
            self.world_db_put("signOwnershipData", sod);
        }
        let to_save: Vec<(String, Vec<u8>)> =
            self.client_blockhead_inventories_to_save.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        for (k, v) in to_save {
            self.world_db_put(&k, &v);
        }
        self.client_blockhead_inventories_to_save.borrow_mut().remove_all();
        for bh in self.blockheads.borrow().clone() {
            hooks.blockhead_prepare_inventory_for_saving(&bh);
            if hooks.blockhead_inventory_needs_saving(&bh) {
                self.save_blockhead_inventory(&bh);
                hooks.blockhead_set_inventory_needs_saving(&bh, false);
            }
        }
        for bh in self.net_blockheads_with_disconnected_clients.borrow().clone() {
            hooks.blockhead_prepare_inventory_for_saving(&bh);
            if hooks.blockhead_inventory_needs_saving(&bh) {
                self.save_blockhead_inventory(&bh);
                hooks.blockhead_set_inventory_needs_saving(&bh, false);
            }
        }
        if !self.dynamic_world_changed_macro_positions.borrow()[DYNAMIC_OBJECT_TYPE_BLOCKHEAD as usize].is_empty() {
            self.save_blockheads();
        }
        self.save_dynamic_objects();
        // physical blocks: (sendReliably, dontSend, onlySaveIfClientsNeedIt) = (1,0,0) (0,0,0) (0,1,0) (0,0,0)
        if !self.world_changed_macro_positions.borrow().is_empty() {
            let list: Vec<IntPair> = self.world_changed_macro_positions.borrow().clone();
            for pos in list {
                if let Some(mt) = macro_tile_at_macro_postion(pos.x, pos.y, &world.macro_tiles(), &world) {
                    hooks.save_physical_block_for_macro_tile(&mt, true, false, false);
                }
            }
            self.world_changed_macro_positions.borrow_mut().clear();
        }
        self.save_front_of(&self.world_changed_send_unreliably_macro_positions, false, false);
        self.save_front_of(&self.world_changed_dont_send_macro_positions, false, true);
        self.save_front_of(&self.snow_changed_macro_positions, false, false);
    }

    /// "ONE entry per call": save the front macro position and erase it even when `mt == NULL`.
    fn save_front_of(&self, v: &std::cell::RefCell<Vec<IntPair>>, send_reliably: bool, dont_send: bool) {
        let front = v.borrow().first().copied();
        if let Some(pos) = front {
            let world = self.world();
            if let Some(mt) = macro_tile_at_macro_postion(pos.x, pos.y, &world.macro_tiles(), &world) {
                self.hooks().save_physical_block_for_macro_tile(&mt, send_reliably, dont_send, false);
            }
            let mut b = v.borrow_mut();
            if !b.is_empty() {
                b.remove(0);
            }
        }
    }

    /// `-saveAndSendOnlyBlocksThatNeedToBeSent`: throttled incremental flush (server only).
    pub fn save_and_send_only_blocks_that_need_to_be_sent(&self) {
        if !self.has_server() {
            return;
        }
        let hooks = self.hooks();
        let world = self.world();
        if !self.world_changed_macro_positions.borrow().is_empty() {
            let macro_tiles = world.macro_tiles();
            let mut send_limiter = 0;
            let mut i = 0usize;
            loop {
                let pos = {
                    let v = self.world_changed_macro_positions.borrow();
                    if i >= v.len() {
                        break;
                    }
                    v[i]
                };
                let mut advance = true;
                if let Some(mt) = macro_tile_at_macro_postion(pos.x, pos.y, &macro_tiles, &world) {
                    let ok = hooks.save_physical_block_for_macro_tile(&mt, true, false, true);
                    if ok {
                        let mut v = self.world_changed_macro_positions.borrow_mut();
                        if i < v.len() {
                            v.remove(i);
                        }
                        advance = false;
                        send_limiter += 1;
                        if send_limiter > 3 {
                            break; // max 4 blocks per call
                        }
                    }
                }
                if advance {
                    i += 1;
                }
            }
        }
        let front = self.world_changed_send_unreliably_macro_positions.borrow().first().copied();
        if let Some(pos) = front {
            if let Some(mt) = macro_tile_at_macro_postion(pos.x, pos.y, &world.macro_tiles(), &world) {
                let ok = hooks.save_physical_block_for_macro_tile(&mt, false, false, true);
                if ok {
                    let mut v = self.world_changed_send_unreliably_macro_positions.borrow_mut();
                    if !v.is_empty() {
                        v.remove(0);
                    }
                }
            }
        }
    }

    /// `-saveBlockheads`.
    pub fn save_blockheads(&self) {
        let hooks = self.hooks();
        let mut dicts: Vec<PlistValue> = Vec::new();
        for bh in self.blockheads.borrow().clone() {
            dicts.push(hooks.blockhead_save_dict_including_workbench_or_interaction_object(&bh, false));
        }
        let save_data_dict = dict_of(vec![("dynamicObjects", PlistValue::array(dicts))]);
        // SERVER: uncompressed XML plist
        self.world_db_put(
            "blockheads",
            &serialize_or_raise(&save_data_dict, "Failed to save blockheads.", false, || {}),
        );
        let cache_nonempty =
            self.disconnected_clients_cached_save_dicts_not_done.borrow().as_ref().map_or(false, |c| !c.is_empty());
        if cache_nonempty {
            for bh in self.net_blockheads_with_disconnected_clients.borrow().clone() {
                if let Some(dir) = hooks.blockhead_client_save_dir(&bh) {
                    let sd = hooks.blockhead_save_dict_including_workbench_or_interaction_object(&bh, true);
                    let mut cache = self.disconnected_clients_cached_save_dicts_not_done.borrow_mut();
                    let cache = cache.as_mut().unwrap();
                    if cache.get(&dir).is_none() {
                        cache.insert(dir.clone(), Vec::new());
                    }
                    let arr = cache.get_mut(&dir).unwrap();
                    let uid = bh.unique_id();
                    let idx = arr
                        .iter()
                        .position(|d| d.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value()) == uid);
                    match idx {
                        None => arr.push(sd),
                        Some(i) => arr[i] = sd,
                    }
                }
            }
            let dirs: Vec<String> =
                self.disconnected_clients_cached_save_dicts_not_done.borrow().as_ref().unwrap().keys_in_order();
            for dir in dirs.iter() {
                let arr = self
                    .disconnected_clients_cached_save_dicts_not_done
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .get(dir)
                    .cloned()
                    .unwrap_or_default();
                let sd2 = dict_of(vec![("dynamicObjects", PlistValue::array(arr))]);
                let xml = serialize_or_raise(&sd2, "Failed to save disconnected blockheads.", false, || {});
                self.world_db_put(&format!("{}_blockheads", dir), &gzip_deflate(&xml));
            }
            let mut unactive: Vec<String> = dirs;
            for bh in self.net_blockheads_with_disconnected_clients.borrow().clone() {
                if let Some(d) = hooks.blockhead_client_save_dir(&bh) {
                    unactive.retain(|k| *k != d);
                }
            }
            let mut cache = self.disconnected_clients_cached_save_dicts_not_done.borrow_mut();
            for k in unactive {
                cache.as_mut().unwrap().remove(&k);
            }
        }
    }

    /// `-saveBlockheadInventory:`.
    pub fn save_blockhead_inventory(&self, blockhead: &ObjRef) {
        let hooks = self.hooks();
        let slots = hooks.blockhead_save_item_slots_array(blockhead);
        let data = serialize_or_raise(&slots, "Failed to save blockhead inventory.", false, || {});
        let uid = blockhead.unique_id() as i64;
        match hooks.blockhead_client_save_dir(blockhead) {
            None => self.world_db_put(&format!("blockhead_{}_inventory", uid), &data),
            Some(dir) => self.world_db_put(&format!("{}_blockhead_{}_inventory", dir, uid), &gzip_deflate(&data)),
        }
    }

    /// `-removePortalFromListAtPos:`: `removeIndex:(NSUInteger)(long)worldIndex` (-1 -> huge index -> no-op).
    pub fn remove_portal_from_list_at_pos(&self, pos: IntPair) {
        let idx = self.world_index(pos) as i64 as u64;
        self.portal_positions.borrow_mut().remove_index(idx);
    }

    /// `-safeRemoveFromDynamicObjectDatabase:`: remove the key and the legacy file (error ignored).
    pub fn safe_remove_from_dynamic_object_database(&self, key: &str) {
        self.dynamic_object_database.remove(key);
        let _ = std::fs::remove_file(append_path_component(&self.world_save_directory, key));
    }

    fn chest_key(&self, chest: &ObjRef) -> String {
        let pos = chest.pos();
        format!("{}_{}/chest_{}", pos.x / 32, pos.y / 32, chest.unique_id())
    }

    /// `-removeSavedInventoryForChest:`.
    pub fn remove_saved_inventory_for_chest(&self, chest: &ObjRef) {
        let key = self.chest_key(chest);
        self.safe_remove_from_dynamic_object_database(&key);
    }

    /// `-loadLocalInventoryDataForChest:`.
    pub fn load_local_inventory_data_for_chest(&self, chest: &ObjRef) -> Option<Vec<u8>> {
        let key = self.chest_key(chest);
        let mut data = self.dynamic_object_database.data_for_key(Some(&key));
        if data.is_none() && !self.world().has_finished_database_migration_to17() {
            data = self.read_save_file(&key);
        }
        data
    }

    /// `-saveDynamicObjectsForMacroTile:objectType:xPos:yPos:` (`xPos`/`yPos` are MACRO coordinates).
    pub fn save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(
        &self,
        macro_tile: &MacroTileRef,
        object_type: i32,
        x_pos: i32,
        y_pos: i32,
    ) {
        if object_type == 15 {
            nslog!("saving dep");
        }
        if object_type == 21 || !macro_tile.dynamic_objects_loaded.get() || object_type < 0 || object_type >= 65 {
            return;
        }
        let hooks = self.hooks();
        let world = self.world();
        let ot = object_type as usize;
        let mut dicts: Vec<PlistValue> = Vec::new();
        let mut per_client: Option<GsDict<String, Vec<PlistValue>>> = None;
        let macro_index = macro_index_at_macro_position(x_pos, y_pos, &world) as u32;
        if object_type_can_be_loaded_only_when_client_owner_online(object_type) {
            let mut pc: GsDict<String, Vec<PlistValue>> = GsDict::new();
            for client_id in self.server_client_keys() {
                let sc = self.server_client(&client_id).expect("key of the client map");
                let has = sc.world_indices_containing_tamed_animals().borrow()[ot].contains(&macro_index);
                if has && pc.get(&client_id).is_none() {
                    pc.insert(client_id.clone(), Vec::new());
                }
            }
            per_client = Some(pc);
        }
        let objects: Vec<ObjRef> = macro_tile.dynamic_objects.borrow().clone().unwrap_or_default();
        if !objects.is_empty() {
            let mut free_block_count = 0;
            for object in objects {
                if object.object_type() == object_type {
                    // `++freeBlockCount < 5000` only increments when objectType == FREE_BLOCK (short-circuit)
                    if object_type == DYNAMIC_OBJECT_TYPE_FREE_BLOCK {
                        free_block_count += 1;
                        if free_block_count >= 5000 {
                            continue;
                        }
                    }
                    let mut save_dict: Option<PlistValue> = None;
                    if object_type == DYNAMIC_OBJECT_TYPE_TRAIN_CAR_FREIGHT {
                        if hooks.freight_needs_chest_save(&object) {
                            hooks.freight_set_needs_chest_save(&object, false);
                            let d = hooks.freight_chest_save_dict(&object);
                            match d.as_ref().and_then(data_from_property_list) {
                                None => ns_raise("DataSaveException", "Failed to save train chest data."),
                                Some(data) => {
                                    self.dw_db_put(&format!("trainchest_{}", object.unique_id() as i64), &data)
                                }
                            }
                        }
                    } else if object_type == DYNAMIC_OBJECT_TYPE_CHEST {
                        // a portal chest (chestType 4) writes neither inventory nor saveDict-without-inventory
                        if hooks.chest_type(&object) != 4 {
                            if let Some(inv) = hooks.chest_inventory_data_allowing_empty(&object) {
                                self.dw_db_put(&format!("{}_{}/chest_{}", x_pos, y_pos, object.unique_id()), &inv);
                            }
                            save_dict = hooks.chest_save_dict_excluding_inventory(&object);
                        }
                    }
                    let save_dict = save_dict.unwrap_or_else(|| PlistValue::Dict(object.get_save_dict(&object)));
                    let owner_id = object.client_id_for_saving_seperately_and_only_loading_while_player_online();
                    let sc = owner_id.as_deref().and_then(|o| self.server_client(o));
                    match owner_id.as_deref() {
                        Some(oid) if oid != "server" => {
                            if let Some(sc) = sc {
                                if let Some(pc) = per_client.as_mut() {
                                    if pc.get(&oid.to_string()).is_none() {
                                        pc.insert(oid.to_string(), Vec::new());
                                    }
                                    pc.get_mut(&oid.to_string()).unwrap().push(save_dict);
                                }
                                // UB-PARITY (quirk 2): the insert runs even when `per_client` is nil (non-NPC type);
                                // the object is then dropped from every file.
                                if let Some(set) = sc.world_indices_containing_tamed_animals().borrow_mut().get_mut(ot)
                                {
                                    set.insert(macro_index);
                                }
                            }
                            // owning client offline: NOT written anywhere in this pass
                        }
                        _ => dicts.push(save_dict),
                    }
                }
            }
        }
        let pl = dict_of(vec![("dynamicObjects", PlistValue::array(dicts))]);
        let data = serialize_or_raise(&pl, "Failed to save dynamic world object data.", true, || {});
        self.dw_db_put(&format!("{}_{}/{}", x_pos, y_pos, object_type), &data);
        if let Some(pc) = per_client {
            for (client_id, arr) in pc.iter() {
                let empty = arr.is_empty();
                let pl2 = dict_of(vec![("dynamicObjects", PlistValue::array(arr.clone()))]);
                if empty {
                    // per-client files exist only for registered clients
                    let sc = self.server_client(client_id).expect("registered client");
                    sc.world_indices_containing_tamed_animals().borrow_mut()[ot].remove(&macro_index);
                }
                let data2 = serialize_or_raise(&pl2, "Failed to save dynamic world object data.", true, || {});
                self.dw_db_put(&format!("{}_{}_{}/{}", client_id, x_pos, y_pos, object_type), &data2);
            }
        }
    }
}

/// `dataFromPropertyList:` of a save record; `DataSaveException` when it fails. `hook` runs inside the `@try` of the
/// serializer (the seam the tests use to make it fail). `log` adds the "error compressing" line some callers print.
fn serialize_or_raise(v: &PlistValue, reason: &str, log: bool, hook: impl FnOnce()) -> Vec<u8> {
    match data_from_property_list_hooked(v, hook) {
        Some(d) => d,
        None => {
            if log {
                nslog!("error compressing property list to save objects");
            }
            ns_raise("DataSaveException", reason)
        }
    }
}

/// Hat item hovering above the pole column `x`, if `x` is a pole column of a world `width` cells wide.
fn pole_hat(x: i32, width: i32) -> Option<ItemType> {
    if x == 0 {
        Some(ItemType::ITEM_SANTA_HAT)
    } else if x == width / 4 {
        Some(ItemType::ITEM_FRUIT_HAT)
    } else if x == width / 2 {
        Some(ItemType::ITEM_SOUTH_POLE_BOOTS)
    } else if x == (width / 4) * 3 {
        Some(ItemType::ITEM_GRASS_SKIRT)
    } else {
        None
    }
}

/// One shared, mutable per-column "pending spawn" array (0 = nothing pending).
struct Column(std::rc::Rc<std::cell::RefCell<Vec<i32>>>);

impl Column {
    fn get(&self, x: i32) -> i32 {
        usize::try_from(x).ok().and_then(|i| self.0.borrow().get(i).copied()).unwrap_or(0)
    }
    fn set(&self, x: i32, v: i32) {
        let mut cells = self.0.borrow_mut();
        if let Some(slot) = usize::try_from(x).ok().and_then(|i| cells.get_mut(i)) {
            *slot = v;
        }
    }
}

/// The three pending-spawn arrays of the world tile loader.
struct Pending {
    trees: Column,
    npcs: Column,
    plants: Column,
}

impl Pending {
    fn of(loader: &dyn DwWorldGen) -> Pending {
        Pending {
            trees: Column(loader.tree_positions()),
            npcs: Column(loader.npc_positions()),
            plants: Column(loader.plant_positions()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// contract 8 item 16: gene_byte boundaries.
    #[test]
    fn gene_byte_boundaries() {
        assert_eq!(gene_byte(-1.0), 127);
        assert_eq!(gene_byte(0.0), 190);
        assert_eq!(gene_byte(1.0), 254);
        assert_eq!(gene_byte(2.0), 254);
        assert_eq!(gene_byte(-5.0), 127);
    }

    /// contract 8 item 16: fish depth for rand() = 0 and 2^31-1.
    #[test]
    fn fish_depth_formula() {
        assert_eq!(gen_fish_depth(100, 0), 100);
        // (float)(2^31-1) rounds to 2^31 -> t = 1.0 -> 0.9
        assert_eq!(gen_fish_depth(100, 0x7fff_ffff), 100 + (((0x200 - 100) as f64) * 0.9) as i32);
        assert_eq!(migration_fish_depth(100, 0), 100);
        assert_eq!(migration_fish_depth(100, 0x7fff_ffff), (100.0f64 + ((0x200 - 100) as f32 as f64) * 0.9) as i32);
    }

    #[test]
    fn tulip_gene_forms() {
        // all noise 0: rT = 3, dT = 3, a = b = 0
        assert_eq!(tulip_genes_dual(0.0, 0.0), (3 | 3 << 12 | 3 << 8 | 3 << 4, 0));
        // |n1| > 0.7 -> rT = 2; |n2| > 0.7 -> dT = 5; a = b = 2
        let (m, g) = tulip_genes_dual(1.2, -1.2);
        assert_eq!(m, (2 | 5 << 12 | 2 << 8 | 5 << 4) as i16 as i32);
        assert_eq!(g, (2 << 12 | 2 << 8) as i16 as i32);
        // rarity variant, rarity 2, large noise
        let (m, _) = tulip_genes_rarity(2, 0.5, 0.8);
        assert_eq!(m, 7 | 7 << 12 | 7 << 8 | 7 << 4);
        // rarity 0 never exceeds 4
        let (m, _) = tulip_genes_rarity(0, 0.9, 0.9);
        assert_eq!(m, 4 | 4 << 12 | 4 << 8 | 4 << 4);
    }

    // ---------------------------------------------------------------------------------------------------
    // persistence / generation tests (fake hooks)
    // ---------------------------------------------------------------------------------------------------
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwNoise, DwWorldGen, DynamicWorldInit, FreeBlockInit};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Gen {
        trees: Rc<RefCell<Vec<i32>>>,
        npcs: Rc<RefCell<Vec<i32>>>,
        plants: Rc<RefCell<Vec<i32>>>,
        height: i32,
    }
    impl DwWorldGen for Gen {
        fn tree_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            self.trees.clone()
        }
        fn npc_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            self.npcs.clone()
        }
        fn plant_positions(&self) -> Rc<RefCell<Vec<i32>>> {
            self.plants.clone()
        }
        fn max_of_rock_and_dirt_height_for_x(&self, _x: i32) -> i32 {
            self.height
        }
        fn lake_height_for_x(&self, _x: i32) -> i32 {
            0
        }
    }
    struct Rec {
        log: Rc<RefCell<Vec<String>>>,
        next: std::cell::Cell<u64>,
    }
    impl crate::dynworld::state::DwHooksStub for Rec {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.log
                .borrow_mut()
                .push(format!("freeblock({},{},{})", init.item_type, init.position.x, init.position.y));
            self.next.set(self.next.get() + 1);
            let o: ObjRef = Obj::new(14, self.next.get(), self.log.clone());
            o.base().borrow_mut().pos = init.position;
            Some(o)
        }
        fn load_npc_at_position(&self, pos: IntPair, npc_type: i32, _adult: bool, _placed: bool) {
            self.log.borrow_mut().push(format!("npc({},{},{})", npc_type, pos.x, pos.y));
        }
        fn light_changed_at_macro_pos_send_reliably(&self, p: IntPair, r: bool) {
            self.log.borrow_mut().push(format!("light({},{},{})", p.x, p.y, r));
        }
        fn construct_from_save_dict(
            &self,
            _k: SaveLoadKind,
            t: i32,
            d: &crate::plist::value::PlistDict,
            _c: Option<&crate::plist::value::PlistDict>,
        ) -> Option<ObjRef> {
            let id = d.map.get(&"uniqueID".to_string()).map_or(0, |v| v.unsigned_long_value());
            let o: ObjRef = Obj::new(t, id, self.log.clone());
            let x = d.map.get(&"pos_x".to_string()).map_or(0, |v| v.int_value());
            let y = d.map.get(&"pos_y".to_string()).map_or(0, |v| v.int_value());
            o.base().borrow_mut().pos = IntPair { x, y };
            Some(o)
        }
    }
    use crate::objects::base::DynamicObject;

    fn new_dw_on(
        name: &str,
        main: Rc<crate::storage::lmdb::Database>,
        dwdb: Rc<crate::storage::lmdb::Database>,
    ) -> Rc<DynamicWorld> {
        let w = world(1);
        std::mem::forget(w.clone());
        DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(scratch(name)),
        })
    }

    /// contract 8 item 3: `dynamicWorldv2` keys and values; reload restores them (item 5, in-process).
    #[test]
    fn save_game_dynamic_world_v2_and_reload() {
        let d = dw("ld_save", 1);
        d.dynamic_object_id_count.set(115);
        d.portal_positions.borrow_mut().add_index(5 * 32 + 7);
        d.save_game_with_world_data_sign_ownership_data(Some(b"worlddata"), None);
        let data = d.world_database.get("dynamicWorldv2").expect("dynamicWorldv2");
        let xml = String::from_utf8_lossy(&data).into_owned();
        assert!(
            xml.starts_with(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//GNUstep//DTD plist 0.9//EN\""
            ),
            "{xml}"
        );
        let v = crate::plist::helpers::property_list_from_data(&data).unwrap();
        assert_eq!(v.object_for_key("dynamicObjectIDCount").unwrap().unsigned_long_value(), 115);
        assert_eq!(v.object_for_key("saveVersion").unwrap().int_value(), 8);
        assert_eq!(v.object_for_key("activeBlockheadIndex").unwrap().int_value(), 0);
        assert!(!v.object_for_key("workbenchHasBeenCrafted").unwrap().bool_value());
        assert!(v.object_for_key("poleItemTakenTimes").is_none());
        let glow = v.object_for_key("savedGlowIndices").unwrap();
        assert!(matches!(glow, PlistValue::Data(b) if b.bytes.starts_with(b"bplist00")), "{glow:?}");
        assert_eq!(d.world_database.get("worldv2").unwrap(), b"worlddata");
        // reload into a fresh DynamicWorld on the same databases
        let d2 = new_dw_on("ld_save2", d.world_database.clone(), d.dynamic_object_database.clone());
        assert!(d2.load_dynamic_objects_reposition_blockhead_load_failures(true, true));
        assert_eq!(d2.dynamic_object_id_count.get(), 115);
        assert!(d2.portal_positions.borrow().contains_index(5 * 32 + 7));
        assert!(!d2.has_loaded_blockheads.get());
    }

    /// contract 8 item 4: empty `blockheads` record.
    #[test]
    fn save_blockheads_empty_record() {
        let d = dw("ld_bh", 1);
        d.save_blockheads();
        let data = d.world_database.get("blockheads").unwrap();
        let xml = String::from_utf8_lossy(&data).into_owned();
        assert!(xml.contains("<key>dynamicObjects</key>"), "{xml}");
        assert!(xml.contains("<array>\n    </array>"), "{xml}");
        // a reload with this record sets hasLoadedBlockheads
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(true, true));
        assert!(d.has_loaded_blockheads.get());
    }

    /// contract 8 item 14: a macro tile with 5000+ free blocks writes exactly 4999.
    #[test]
    fn free_block_cap_4999() {
        let d = dw("ld_cap", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let world = d.world();
        let mt = world.macro_tiles().get(3).unwrap();
        let mut objs: Vec<ObjRef> = Vec::new();
        for i in 0..5003u64 {
            objs.push(Obj::new(14, i + 1, log.clone()));
        }
        *mt.dynamic_objects.borrow_mut() = Some(objs);
        mt.dynamic_objects_loaded.set(true);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 14, 3, 0);
        let data = d.dynamic_object_database.get("3_0/14").unwrap();
        let v = crate::plist::helpers::property_list_from_data(&data).unwrap();
        let objects = v.object_for_key("dynamicObjects").unwrap();
        assert!(matches!(objects, PlistValue::Array(a) if a.items.len() == 4999));
        // not loaded -> nothing written; a record is written even when empty
        mt.dynamic_objects_loaded.set(false);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 13, 3, 0);
        assert!(d.dynamic_object_database.get("3_0/13").is_none());
        mt.dynamic_objects_loaded.set(true);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 13, 3, 0);
        assert!(d.dynamic_object_database.get("3_0/13").is_some());
    }

    /// remove_dynamic_objects_for_macro_tile unloads bookkeeping (not blockheads, not artificial lights).
    #[test]
    fn remove_macro_tile_objects() {
        let d = dw("ld_rm", 1);
        let log = Rc::new(RefCell::new(vec![]));
        let world = d.world();
        let mt = world.macro_tiles().get(0).unwrap();
        let pb = world.take_physical_block();
        *mt.physical_block.borrow_mut() = Some(pb);
        let a: ObjRef = Obj::new(14, 1, log.clone());
        a.base().borrow_mut().pos = IntPair { x: 2, y: 3 };
        let bh: ObjRef = Obj::new(24, 2, log.clone());
        let light: ObjRef = Obj::new(21, 3, log.clone());
        d.do_set(14, 1, a.clone());
        d.do_set(24, 2, bh.clone());
        d.do_set(21, 3, light.clone());
        d.free_blocks_insert(d.world_index_key(IntPair { x: 2, y: 3 }), &a);
        *mt.dynamic_objects.borrow_mut() = Some(vec![a, bh, light]);
        mt.dynamic_objects_loaded.set(true);
        d.remove_dynamic_objects_for_macro_tile(&mt);
        assert!(!d.do_contains(14, 1));
        assert!(d.do_contains(24, 2));
        assert!(d.do_contains(21, 3));
        assert!(d.free_blocks_by_position.borrow().is_empty());
        assert!(mt.dynamic_objects.borrow().is_none());
        assert!(!mt.dynamic_objects_loaded.get());
    }

    /// contract 8 items 7/8: hats only in special columns and no rand() for them; fish depth uses exactly one rand().
    #[test]
    fn generation_hats_and_fish() {
        let d = dw("ld_gen", 1);
        let log = Rc::new(RefCell::new(vec![]));
        d.set_hooks(Rc::new(Rec { log: log.clone(), next: std::cell::Cell::new(1000) }));
        let npcs = Rc::new(RefCell::new(vec![0i32; 32]));
        let trees = Rc::new(RefCell::new(vec![0i32; 32]));
        let plants = Rc::new(RefCell::new(vec![0i32; 32]));
        npcs.borrow_mut()[5] = 4; // clownfish
        npcs.borrow_mut()[8] = 4; // special column: must be ignored
        trees.borrow_mut()[8] = 1;
        *d.world_tile_loader.borrow_mut() =
            Some(Rc::new(Gen { trees: trees.clone(), npcs: npcs.clone(), plants, height: 520 }));
        let world = d.world();
        let mt = world.macro_tiles().get(16).unwrap(); // macro (0,16): index my*W+mx with W=1
        let pb = world.take_physical_block();
        pb.x_pos.set(0);
        pb.y_pos.set(16);
        *mt.physical_block.borrow_mut() = Some(pb);
        crate::rng::glibc_rand::srand(42);
        let expect_r = crate::rng::glibc_rand::rand();
        crate::rng::glibc_rand::srand(42);
        d.load_dynamic_objects_for_macro_tile_include_surface_blocks(&mt, false);
        let got = log.borrow().clone();
        assert_eq!(got[0], "light(0,16,true)");
        // hats at x = 0, 8, 16, 24 in this order, y = 520 + 16
        let hats: Vec<&String> = got.iter().filter(|s| s.starts_with("freeblock")).collect();
        assert_eq!(
            hats,
            vec!["freeblock(142,0,536)", "freeblock(140,8,536)", "freeblock(143,16,536)", "freeblock(141,24,536)"]
        );
        let npc: Vec<&String> = got.iter().filter(|s| s.starts_with("npc")).collect();
        assert_eq!(npc, vec![&format!("npc(4,5,{})", gen_fish_depth(520, expect_r))]);
        // special column 8: no tree, npc untouched
        assert!(!got.iter().any(|s| s.starts_with("tree")));
        assert_eq!(npcs.borrow()[8], 4);
        assert_eq!(npcs.borrow()[5], 0);
        assert_eq!(pb.version.get(), 8);
        // exactly one rand() was consumed by the generation
        assert_eq!(crate::rng::glibc_rand::rand(), {
            crate::rng::glibc_rand::srand(42);
            let _ = crate::rng::glibc_rand::rand();
            crate::rng::glibc_rand::rand()
        });
    }

    /// loadDynamicObjectsOfType: dupes by id are skipped, objects go to dynamicObjectsToAdd, bookkeeping erased.
    #[test]
    fn load_of_type_dedupes() {
        let d = dw("ld_type", 1);
        let log = Rc::new(RefCell::new(vec![]));
        d.set_hooks(Rc::new(Rec { log: log.clone(), next: std::cell::Cell::new(0) }));
        let world = d.world();
        let pb = world.take_physical_block();
        pb.x_pos.set(2);
        pb.y_pos.set(0);
        let mk = |id: i64| {
            let mut m = PlistValue::mutable_dict();
            if let PlistValue::Dict(dd) = &mut m {
                dd.map.insert("uniqueID".to_string(), PlistValue::int(id));
                dd.map.insert("pos_x".to_string(), PlistValue::int(70));
                dd.map.insert("pos_y".to_string(), PlistValue::int(9));
            }
            m
        };
        let blob =
            data_from_property_list(&dict_of(vec![("dynamicObjects", PlistValue::array(vec![mk(7), mk(7), mk(8)]))]))
                .unwrap();
        let mut portal = false;
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(d.ta_len(13), 2);
        assert!(d.currently_loading_macro_blocks.borrow()[13].is_empty());
        // loading the same blob again: both exist in to_add -> skipped
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(d.ta_len(13), 2);
        // guards
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            21,
            Some(&blob),
            pb,
            &mut portal,
            None,
        );
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            -3,
            Some(&blob),
            pb,
            &mut portal,
            None,
        );
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            65,
            Some(&blob),
            pb,
            &mut portal,
            None,
        );
    }

    /// saveAndSend: the one-entry-per-call rule and the erase when the macro tile is NULL.
    #[test]
    fn save_front_entry_erased_even_without_tile() {
        let d = dw("ld_front", 1);
        d.world_changed_dont_send_macro_positions.borrow_mut().push(IntPair { x: 0, y: 99 }); // y out of range -> NULL tile
        d.world_changed_dont_send_macro_positions.borrow_mut().push(IntPair { x: 0, y: 1 });
        d.save_game_with_world_data_sign_ownership_data(None, None);
        assert_eq!(d.world_changed_dont_send_macro_positions.borrow().len(), 1);
    }
}

/// Decision-table tests of the macro-block population (generation, load from save, surface blocks).
#[cfg(test)]
mod population_tests {
    use super::fakes::{Sc, Srv};
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwNoise, DynamicWorldInit, FreeBlockInit, ServerClientMap};
    use crate::foundation::gsdict::GsDict;
    use crate::rng::glibc_rand::{rand, srand};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<String>>>;
    type Arr = Rc<RefCell<Vec<i32>>>;

    // ------------------------------------------------------------------------------------ fakes

    struct Spy {
        log: Log,
        next: Cell<u64>,
        workbench: Cell<bool>,
    }
    impl crate::dynworld::state::DwHooksStub for Spy {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.log.borrow_mut().push(format!("hat({},{},{})", init.item_type, init.position.x, init.position.y));
            self.next.set(self.next.get() + 1);
            let o: ObjRef = Obj::new(14, self.next.get(), self.log.clone());
            o.base().borrow_mut().pos = init.position;
            Some(o)
        }
        fn load_npc_at_position(&self, pos: IntPair, ty: i32, adult: bool, placed: bool) {
            self.log.borrow_mut().push(format!("npc({},{},{},{},{})", ty, pos.x, pos.y, adult, placed));
        }
        fn load_tree_at_position(&self, pos: IntPair, t: i32, mh: i32, gr: i32, adult: bool, age: f32) {
            self.log.borrow_mut().push(format!("tree({},{},{},{},{},{},{})", t, pos.x, pos.y, mh, gr, adult, age));
        }
        fn load_plant_at_position(&self, pos: IntPair, t: i32, a: i32, g: i32, adult: bool) {
            self.log.borrow_mut().push(format!("plant({},{},{},{},{},{})", t, pos.x, pos.y, a, g, adult));
        }
        fn add_torch_at_pos_of_type(&self, pos: IntPair, t: i32) {
            self.log.borrow_mut().push(format!("torch({},{},{})", pos.x, pos.y, t));
        }
        fn create_treasure_chest_or_troll_at_tile(&self, _t: TileRef, pos: IntPair, troll: bool, treasure: bool) {
            self.log.borrow_mut().push(format!("chest({},{},{},{})", pos.x, pos.y, troll, treasure));
        }
        fn load_glow_block_if_needed_at_pos_tile(&self, pos: IntPair, _t: TileRef) {
            self.log.borrow_mut().push(format!("glow({},{})", pos.x, pos.y));
        }
        fn load_surface_block_at_pos(&self, pos: IntPair) {
            self.log.borrow_mut().push(format!("surf({},{})", pos.x, pos.y));
        }
        fn load_snow_surface_block_at_pos_load_snow(&self, pos: IntPair, snow: bool) {
            self.log.borrow_mut().push(format!("snow({},{},{})", pos.x, pos.y, snow));
        }
        fn light_changed_at_macro_pos_send_reliably(&self, p: IntPair, r: bool) {
            self.log.borrow_mut().push(format!("light({},{},{})", p.x, p.y, r));
        }
        fn place_basic_portal_workbench(&self, pos: IntPair) {
            self.log.borrow_mut().push(format!("portal({},{})", pos.x, pos.y));
        }
        fn update_physical_block_to_latest_version(&self, pb: PhysicalBlockRef) {
            self.log.borrow_mut().push(format!("upgrade({})", pb.version.get()));
        }
        fn workbench_at_pos(&self, _pos: IntPair) -> Option<ObjRef> {
            self.workbench.get().then(|| -> ObjRef { Obj::new(45, 1, self.log.clone()) })
        }
        fn construct_from_save_dict(
            &self,
            _k: SaveLoadKind,
            t: i32,
            d: &crate::plist::value::PlistDict,
            _c: Option<&crate::plist::value::PlistDict>,
        ) -> Option<ObjRef> {
            let id = d.map.get(&"uniqueID".to_string()).map_or(0, |v| v.unsigned_long_value());
            let o: ObjRef = Obj::new(t, id, self.log.clone());
            let x = d.map.get(&"pos_x".to_string()).map_or(0, |v| v.int_value());
            let y = d.map.get(&"pos_y".to_string()).map_or(0, |v| v.int_value());
            o.base().borrow_mut().pos = IntPair { x, y };
            Some(o)
        }
    }

    struct Gen {
        trees: Arr,
        npcs: Arr,
        plants: Arr,
        height: Cell<i32>,
        lake: Cell<i32>,
        per_column: RefCell<Vec<(i32, i32)>>,
    }
    impl DwWorldGen for Gen {
        fn tree_positions(&self) -> Arr {
            self.trees.clone()
        }
        fn npc_positions(&self) -> Arr {
            self.npcs.clone()
        }
        fn plant_positions(&self) -> Arr {
            self.plants.clone()
        }
        fn max_of_rock_and_dirt_height_for_x(&self, x: i32) -> i32 {
            let own = self.per_column.borrow().iter().find(|(c, _)| *c == x).map(|(_, h)| *h);
            own.unwrap_or(self.height.get())
        }
        fn lake_height_for_x(&self, _x: i32) -> i32 {
            self.lake.get()
        }
    }

    /// Returns a value per noise `y` and records each sample.
    struct Noise {
        samples: RefCell<Vec<(f64, f64, i32)>>,
        by_y: Vec<(f64, f64)>,
    }
    impl DwNoise for Noise {
        fn get_x_y_octaves(&self, x: f64, y: f64, o: i32) -> f64 {
            self.samples.borrow_mut().push((x, y, o));
            self.by_y.iter().find(|(k, _)| *k == y).map_or(0.0, |(_, v)| *v)
        }
    }

    struct Fx {
        dw: Rc<DynamicWorld>,
        log: Log,
        spy: Rc<Spy>,
        gen: Rc<Gen>,
        noise: Rc<Noise>,
    }

    fn fx_with(name: &str, noise: Vec<(f64, f64)>, server: bool) -> Fx {
        let (main, dwdb) = dbs(name);
        let w = world(1);
        std::mem::forget(w.clone());
        let noise = Rc::new(Noise { samples: RefCell::new(vec![]), by_y: noise });
        let clients: ServerClientMap = Rc::new(RefCell::new(GsDict::new()));
        clients.borrow_mut().insert("c1".to_string(), Rc::new(Sc::new("c1")));
        let dw = DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: if server { Some(Rc::new(Srv)) } else { None },
            server_clients: if server { Some(clients) } else { None },
            cache: None,
            tree_density_noise_function: Some(noise.clone()),
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(scratch(&format!("{name}_save"))),
        });
        let log: Log = Rc::new(RefCell::new(vec![]));
        let spy = Rc::new(Spy { log: log.clone(), next: Cell::new(1000), workbench: Cell::new(false) });
        dw.set_hooks(spy.clone());
        let gen = Rc::new(Gen {
            trees: Rc::new(RefCell::new(vec![0; 32])),
            npcs: Rc::new(RefCell::new(vec![0; 32])),
            plants: Rc::new(RefCell::new(vec![0; 32])),
            height: Cell::new(100),
            lake: Cell::new(0),
            per_column: RefCell::new(vec![]),
        });
        Fx { dw, log, spy, gen, noise }
    }
    fn fx(name: &str) -> Fx {
        fx_with(name, vec![], false)
    }
    impl Fx {
        fn with_loader(&self) {
            *self.dw.world_tile_loader.borrow_mut() = Some(self.gen.clone());
        }
        fn block(&self, my: i32, version: u8) -> PhysicalBlockRef {
            let world = self.dw.world();
            let pb = world.take_physical_block();
            pb.x_pos.set(0);
            pb.y_pos.set(my);
            pb.version.set(version);
            *world.macro_tiles().get(my as usize).unwrap().physical_block.borrow_mut() = Some(pb);
            pb
        }
        fn run(&self, pb: PhysicalBlockRef, surface: bool) {
            let mt = self.dw.world().macro_tiles().get(pb.y_pos.get() as usize).unwrap();
            self.dw.load_dynamic_objects_for_macro_tile_include_surface_blocks(&mt, surface);
        }
        fn lines(&self) -> Vec<String> {
            self.log.borrow().iter().filter(|s| !s.starts_with("light(")).cloned().collect()
        }
        fn set(&self, which: &str, x: i32, v: i32) {
            let arr = match which {
                "tree" => &self.gen.trees,
                "npc" => &self.gen.npcs,
                _ => &self.gen.plants,
            };
            arr.borrow_mut()[x as usize] = v;
        }
        fn get(&self, which: &str, x: i32) -> i32 {
            let arr = match which {
                "tree" => &self.gen.trees,
                "npc" => &self.gen.npcs,
                _ => &self.gen.plants,
            };
            let v = arr.borrow()[x as usize];
            v
        }
    }
    fn cell(pb: PhysicalBlockRef, lx: i32, ly: i32) -> TileRef {
        pb.tile_ref((ly * 32 + lx) as usize)
    }
    fn age() -> f32 {
        ADULT_MAX_AGE
    }

    // ------------------------------------------------------------------------------------ entry

    #[test]
    fn entry_without_physical_block_does_nothing() {
        let f = fx("pop_nopb");
        let mt = f.dw.world().macro_tiles().get(3).unwrap();
        f.dw.load_dynamic_objects_for_macro_tile_include_surface_blocks(&mt, true);
        assert!(f.log.borrow().is_empty());
    }

    #[test]
    fn generation_without_loader_only_sets_version_and_light() {
        let f = fx("pop_noloader");
        let pb = f.block(3, 0);
        f.run(pb, false);
        assert_eq!(*f.log.borrow(), vec!["light(0,3,true)".to_string()]);
        assert_eq!(pb.version.get(), 8);
    }

    // ------------------------------------------------------------------------------------ poles

    #[test]
    fn pole_hats_height_rules() {
        // surface below 512 -> 528; lake above surface raises; block rows decide placement
        let f = fx("pop_poles");
        f.with_loader();
        f.gen.height.set(10);
        f.run(f.block(16, 0), false);
        assert_eq!(f.lines(), vec!["hat(142,0,528)", "hat(140,8,528)", "hat(143,16,528)", "hat(141,24,528)"]);

        let f = fx("pop_poles_lake");
        f.with_loader();
        f.gen.height.set(10);
        f.gen.lake.set(520);
        f.run(f.block(16, 0), false);
        assert_eq!(f.lines().len(), 4);
        assert!(f.lines().iter().all(|l| l.ends_with(",536)")));

        // lake pushes the hats out of the block -> none
        let f = fx("pop_poles_out");
        f.with_loader();
        f.gen.height.set(10);
        f.gen.lake.set(540);
        f.run(f.block(16, 0), false);
        assert!(f.lines().is_empty());
        // wrong block row
        let f = fx("pop_poles_row");
        f.with_loader();
        f.run(f.block(15, 0), false);
        assert!(f.lines().is_empty());
        // y exactly at the block's upper edge (high terrain) is outside
        let f = fx("pop_poles_edge");
        f.with_loader();
        f.gen.height.set(623);
        f.run(f.block(19, 0), false);
        assert_eq!(f.lines().len(), 4); // 639 is the last row of block 19
        let f = fx("pop_poles_edge2");
        f.with_loader();
        f.gen.height.set(624);
        f.run(f.block(19, 0), false);
        assert!(f.lines().is_empty()); // 640
    }

    #[test]
    fn pole_columns_do_nothing_else() {
        let f = fx("pop_polecol");
        f.with_loader();
        f.gen.height.set(10);
        let pb = f.block(16, 0);
        for x in [0, 8, 16, 24] {
            cell(pb, x, 0).set_contents(140); // tree promise
            cell(pb, x, 1).update(|t| t.type_index = 0x10);
            f.set("tree", x, 1);
            f.set("npc", x, 4);
            f.set("plant", x, 7);
        }
        srand(5);
        f.run(pb, false);
        assert_eq!(f.lines().len(), 4); // hats only
        assert_eq!(rand(), {
            srand(5);
            rand()
        });
        assert_eq!(cell(pb, 0, 0).contents(), 140);
        assert_eq!(f.get("npc", 8), 4);
    }

    // ------------------------------------------------------------------------------------ sky columns

    #[test]
    fn sky_tree_promises_and_companions() {
        let f = fx("pop_sky_tree");
        f.with_loader();
        let pb = f.block(20, 0);
        cell(pb, 5, 0).set_contents(132); // apple
        cell(pb, 5, 1).set_contents(136); // cactus
        cell(pb, 5, 2).set_contents(140); // another tree
        cell(pb, 5, 3).set_contents(140);
        cell(pb, 5, 3).update(|t| t.type_index = 0x10); // tree wins over glow
        f.run(pb, false);
        let a = age();
        assert_eq!(
            f.lines(),
            vec![
                format!("tree(1,5,640,190,190,true,{a})"),
                "npc(1,5,640,true,false)".to_string(),
                format!("tree(5,5,641,190,190,true,{a})"),
                "npc(7,5,641,true,false)".to_string(),
                format!("tree(3,5,642,190,190,true,{a})"),
                format!("tree(3,5,643,190,190,true,{a})"),
            ]
        );
        assert_eq!(cell(pb, 5, 0).contents(), 0);
        assert_eq!(cell(pb, 5, 3).contents(), 0);
    }

    #[test]
    fn sky_cells_decision_order() {
        let f = fx("pop_sky_cells");
        f.with_loader();
        let pb = f.block(20, 0);
        let c = |ly| cell(pb, 5, ly);
        c(0).update(|t| t.type_index = 0x10); // glow
        c(1).set_foreground_contents(0x34); // gem promise
        c(2).set_contents(0x4D); // glow by contents
        c(3).update(|t| t.type_index = 0x10);
        c(3).set_foreground_contents(0x34); // glow wins, no conversion
        c(4).update(|t| {
            t.type_index = 2;
            t.contents = 0x5e;
        });
        c(5).update(|t| {
            t.type_index = 2;
            t.contents = 0x90;
        });
        c(6).update(|t| {
            t.type_index = 2;
            t.contents = 0x91;
        });
        c(7).update(|t| {
            t.type_index = 3;
            t.contents = 0x5e; // not air: cleared, nothing created
        });
        c(8).set_foreground_contents(0x7f);
        c(9).set_foreground_contents(0x7e);
        c(10).set_foreground_contents(0x8e);
        c(11).set_foreground_contents(0x8f);
        c(12).update(|t| {
            t.type_index = 2;
            t.contents = 0x5e;
            t.foreground_contents = 0x7f; // chest branch wins, plant promise stays
        });
        c(13).set_foreground_contents(0x20); // unrelated
        f.run(pb, false);
        let (pa, pbv) = (190, 190);
        let t0 = tulip_genes_rarity(0, 0.0, 0.0);
        let t1 = tulip_genes_rarity(1, 0.0, 0.0);
        let t2 = tulip_genes_rarity(2, 0.0, 0.0);
        assert_eq!(
            f.lines(),
            vec![
                "glow(5,640)".to_string(),
                "torch(5,641,75)".to_string(),
                "glow(5,642)".to_string(),
                "glow(5,643)".to_string(),
                "chest(5,644,true,true)".to_string(),
                "chest(5,645,true,false)".to_string(),
                "chest(5,646,false,true)".to_string(),
                format!("plant(7,5,648,{pa},{pbv},true)"),
                format!("plant(8,5,649,{},{},true)", t0.0, t0.1),
                format!("plant(8,5,650,{},{},true)", t1.0, t1.1),
                format!("plant(8,5,651,{},{},true)", t2.0, t2.1),
                "chest(5,652,true,true)".to_string(),
            ]
        );
        assert_eq!(c(1).foreground_contents(), 0x33);
        assert_eq!(c(3).foreground_contents(), 0x34);
        assert_eq!(c(7).contents(), 0);
        assert_eq!(c(8).foreground_contents(), 0);
        assert_eq!(c(12).foreground_contents(), 0x7f);
        assert_eq!(c(13).foreground_contents(), 0x20);
    }

    #[test]
    fn gem_promises_all_five() {
        let f = fx("pop_gems");
        f.with_loader();
        let pb = f.block(20, 0);
        for (i, p) in [0x34u8, 0x36, 0x38, 0x3a, 0x3c].into_iter().enumerate() {
            cell(pb, 5, i as i32).set_foreground_contents(p);
        }
        f.run(pb, false);
        assert_eq!(
            f.lines(),
            vec!["torch(5,640,75)", "torch(5,641,76)", "torch(5,642,86)", "torch(5,643,87)", "torch(5,644,88)"]
        );
        for (i, g) in [0x33u8, 0x35, 0x37, 0x39, 0x3b].into_iter().enumerate() {
            assert_eq!(cell(pb, 5, i as i32).foreground_contents(), g);
        }
    }

    #[test]
    fn gene_sources_use_the_documented_noise_samples() {
        let f = fx_with(
            "pop_genes",
            vec![(0.7641, 1.0), (0.4341, -1.0), (0.4651, 0.5), (0.5641, 1.0), (0.4251, 0.8)],
            false,
        );
        f.with_loader();
        let pb = f.block(20, 0);
        cell(pb, 5, 0).set_contents(132);
        cell(pb, 5, 1).set_foreground_contents(0x7f);
        cell(pb, 5, 2).set_foreground_contents(0x8f);
        f.run(pb, false);
        let l = f.lines();
        assert_eq!(l[0], format!("tree(1,5,640,254,127,true,{})", age()));
        assert_eq!(l[2], format!("plant(7,5,641,{},254,true)", gene_byte(0.5)));
        let (m, g) = tulip_genes_rarity(2, 0.5, 0.8);
        assert_eq!(l[3], format!("plant(8,5,642,{m},{g},true)"));
        let s = f.noise.samples.borrow();
        let w = |off: f32| (((5.0f32) + off) / 32.0f32) as f64;
        for expect in [
            (w(0.0), 0.7641, 8),
            (w(563.0), 0.4341, 8),
            (w(4678.0), 0.4651, 8),
            (w(56483.0), 0.5641, 8),
            (w(4378.0), 0.4651, 4),
            (w(2371.0), 0.4251, 4),
        ] {
            assert!(s.contains(&expect), "{expect:?} in {s:?}");
        }
    }

    #[test]
    fn absent_noise_source_gives_zero_noise() {
        let (main, dwdb) = dbs("pop_nonoise");
        let w = world(1);
        std::mem::forget(w.clone());
        let dw = DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(scratch("pop_nonoise_save")),
        });
        assert_eq!(dw.tree_genes(3), (190, 190));
        assert_eq!(dw.dual_tulip_genes(3), tulip_genes_dual(0.0, 0.0));
    }

    // ------------------------------------------------------------------------------------ lit columns

    #[test]
    fn lit_column_only_lit_cells_matter() {
        let f = fx("pop_lit");
        f.with_loader();
        f.gen.height.set(100);
        let pb = f.block(3, 0);
        let c = |ly| cell(pb, 5, ly);
        c(0).update(|t| {
            t.artificial_light_r = 1;
            t.type_index = 0x10;
        });
        c(1).update(|t| {
            t.artificial_light_g = 1;
            t.foreground_contents = 0x36;
        });
        c(2).update(|t| {
            t.artificial_light_b = 1;
            t.contents = 0x4D;
        });
        c(3).update(|t| t.type_index = 0x10); // unlit
        c(4).update(|t| t.foreground_contents = 0x38); // unlit gem promise
        c(5).update(|t| t.artificial_light_r = 1); // lit, nothing to do
        c(6).update(|t| {
            t.artificial_light_r = 1;
            t.contents = 140; // trees are not made here
        });
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["glow(5,96)", "torch(5,97,76)", "glow(5,98)"]);
        assert_eq!(c(1).foreground_contents(), 0x35);
        assert_eq!(c(4).foreground_contents(), 0x38);
        assert_eq!(c(6).contents(), 140);
    }

    #[test]
    fn surface_exactly_at_block_floor_is_not_sky() {
        let f = fx("pop_floor");
        f.with_loader();
        let pb = f.block(3, 0);
        cell(pb, 5, 0).set_contents(140);
        f.gen.height.set(96);
        f.run(pb, false);
        assert!(f.lines().is_empty());
        let f = fx("pop_floor_sky");
        f.with_loader();
        let pb = f.block(3, 0);
        cell(pb, 5, 0).set_contents(140);
        f.gen.height.set(95);
        f.run(pb, false);
        assert_eq!(f.lines().len(), 1);
    }

    // ------------------------------------------------------------------------------------ surface spawns

    /// Runs generation of block row `by` with column 5 at height `h` and the given pending entries.
    fn spawn(by: i32, h: i32, tree: i32, npc: i32, plant: i32) -> (Fx, Vec<String>) {
        spawn_seeded(1, by, h, tree, npc, plant)
    }
    /// Like [`spawn`], with the C random stream seeded right before the block is populated.
    fn spawn_seeded(seed: u32, by: i32, h: i32, tree: i32, npc: i32, plant: i32) -> (Fx, Vec<String>) {
        let f = fx("pop_spawn");
        f.with_loader();
        f.gen.height.set(h);
        f.set("tree", 5, tree);
        f.set("npc", 5, npc);
        f.set("plant", 5, plant);
        let pb = f.block(by, 0);
        srand(seed);
        f.run(pb, false);
        let l = f.lines();
        (f, l)
    }
    fn pending(f: &Fx) -> (i32, i32, i32) {
        (f.get("tree", 5), f.get("npc", 5), f.get("plant", 5))
    }

    #[test]
    fn spawns_need_pending_entries_and_the_band() {
        let (_, l) = spawn(3, 100, 0, 0, 0);
        assert!(l.is_empty());
        // band: (by-1)*32 <= h <= (by+1)*32 (by=3 -> 64..=128)
        for h in [63, 129] {
            let (f, l) = spawn(3, h, 1, 7, 7);
            assert!(l.is_empty(), "h={h}: {l:?}");
            assert_eq!(pending(&f), (1, 7, 7));
        }
    }

    #[test]
    fn spawn_tree_npc_plant_in_order() {
        let (f, l) = spawn(3, 100, 2, 9, 7);
        assert_eq!(
            l,
            vec![
                format!("tree(2,5,100,190,190,true,{})", age()),
                "npc(9,5,100,true,false)".to_string(),
                "plant(7,5,100,190,190,true)".to_string(),
            ]
        );
        assert_eq!(pending(&f), (0, 0, 0));
    }

    #[test]
    fn spawn_tree_out_of_range_skips_the_column() {
        // h = (by+1)*32 -> local 32 ; h = (by-1)*32 -> local -32
        for h in [128, 64] {
            let (f, l) = spawn(3, h, 1, 9, 7);
            assert!(l.is_empty());
            assert_eq!(pending(&f), (1, 9, 7));
        }
    }

    #[test]
    fn spawn_plain_npc_out_of_range_skips_plant() {
        let (f, l) = spawn(3, 128, 0, 9, 7);
        assert!(l.is_empty());
        assert_eq!(pending(&f), (0, 9, 7));
        let (f, l) = spawn(3, 64, 0, 9, 7);
        assert!(l.is_empty());
        assert_eq!(pending(&f), (0, 9, 7));
    }

    #[test]
    fn spawn_clownfish_draws_one_rand_and_moves_the_plant_too() {
        srand(77);
        let r = rand();
        let (f, l) = spawn_seeded(77, 3, 100, 0, 4, 7);
        let y = gen_fish_depth(100, r);
        assert_eq!(l, vec![format!("npc(4,5,{y},true,false)"), format!("plant(7,5,{y},190,190,true)")]);
        assert_eq!(pending(&f), (0, 0, 0));
        assert_eq!(rand(), {
            srand(77);
            rand();
            rand()
        });
    }

    #[test]
    fn spawn_clownfish_out_of_range_draws_nothing() {
        let (f, l) = spawn_seeded(78, 3, 128, 0, 4, 7);
        assert!(l.is_empty());
        assert_eq!(pending(&f), (0, 4, 7));
        assert_eq!(rand(), {
            srand(78);
            rand()
        });
    }

    #[test]
    fn spawn_shark_eligibility() {
        srand(79);
        let r = rand();
        // eligible: y < 498 and by > 14
        let (_, l) = spawn_seeded(79, 15, 497, 0, 5, 0);
        assert_eq!(l, vec![format!("npc(5,5,{},true,false)", gen_fish_depth(497, r))]);
        // y == 498 is not eligible
        let (f, l) = spawn(15, 498, 0, 5, 7);
        assert!(l.is_empty());
        assert_eq!(pending(&f), (0, 5, 7));
        // block row 14 is not eligible
        let (_, l) = spawn(14, 460, 0, 5, 0);
        assert!(l.is_empty());
        // deep surface is not eligible
        let (_, l) = spawn(17, 560, 0, 5, 0);
        assert!(l.is_empty());
        // the block row need not contain the surface: eligible, but the plant is outside the block
        let (f, l) = spawn_seeded(79, 15, 470, 0, 5, 7);
        assert_eq!(l, vec![format!("npc(5,5,{},true,false)", gen_fish_depth(470, r))]);
        assert_eq!(pending(&f), (0, 0, 7));
    }

    #[test]
    fn spawn_plants_tulip_is_not_cleared() {
        let (f, l) = spawn(3, 100, 0, 0, 8);
        let (m, g) = tulip_genes_dual(0.0, 0.0);
        assert_eq!(l, vec![format!("plant(8,5,100,{m},{g},true)")]);
        assert_eq!(pending(&f), (0, 0, 8));
        let (f, l) = spawn(3, 100, 0, 0, 9);
        assert_eq!(l, vec!["plant(9,5,100,190,190,true)".to_string()]);
        assert_eq!(pending(&f), (0, 0, 0));
        for h in [128, 64] {
            let (f, l) = spawn(3, h, 0, 0, 9);
            assert!(l.is_empty());
            assert_eq!(pending(&f), (0, 0, 9));
        }
    }

    // ------------------------------------------------------------------------------------ load from save

    fn blob(ids: &[(i64, i32, i32, Option<(i32, i32)>)]) -> Vec<u8> {
        let items = ids
            .iter()
            .map(|(id, x, y, ito)| {
                let mut e = vec![
                    ("uniqueID", PlistValue::int(*id)),
                    ("pos_x", PlistValue::int(*x as i64)),
                    ("pos_y", PlistValue::int(*y as i64)),
                ];
                if let Some((i, w)) = ito {
                    e.push(("interactionObjectType", PlistValue::int(*i as i64)));
                    e.push(("workbenchType", PlistValue::int(*w as i64)));
                }
                dict_of(e)
            })
            .collect();
        data_from_property_list(&dict_of(vec![("dynamicObjects", PlistValue::array(items))])).unwrap()
    }

    #[test]
    fn load_reads_database_blobs_and_legacy_files_once() {
        let f = fx("pop_blobs");
        let pb = f.block(3, 8);
        f.dw.dw_db_put("0_3/13", &blob(&[(1, 2, 100, None)]));
        let dir = format!("{}0_3/", f.dw.world_save_directory);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}13"), blob(&[(2, 3, 100, None)])).unwrap(); // same type: skipped
        std::fs::write(format!("{dir}28"), blob(&[(3, 4, 100, None)])).unwrap(); // legacy only
        std::fs::write(format!("{dir}35_extra"), blob(&[(4, 5, 100, None)])).unwrap(); // has '_'
        std::fs::create_dir_all(format!("{dir}39")).unwrap(); // unreadable -> empty data
        f.run(pb, false);
        assert_eq!(f.dw.ta_len(13), 1);
        assert_eq!(f.dw.ta_len(28), 1);
        assert_eq!(f.dw.ta_len(35), 0);
        assert_eq!(f.dw.ta_len(39), 0);

        // migration finished: legacy files are ignored
        let f = fx("pop_blobs2");
        f.dw.world().set_has_finished_database_migration_to17(true);
        let pb = f.block(3, 8);
        let dir = format!("{}0_3/", f.dw.world_save_directory);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(format!("{dir}28"), blob(&[(3, 4, 100, None)])).unwrap();
        f.run(pb, false);
        assert_eq!(f.dw.ta_len(28), 0);
    }

    #[test]
    fn load_applies_the_pending_directory_conversion() {
        let f = fx("pop_conv");
        let pb = f.block(3, 8);
        let old = format!("{}old_13", f.dw.world_save_directory);
        std::fs::write(&old, blob(&[(9, 2, 100, None)])).unwrap();
        let new = format!("{}0_3/13", f.dw.world_save_directory);
        let mut inner = GsDict::new();
        inner.insert(old.clone(), new.clone());
        let mut list = GsDict::new();
        list.insert("0_3".to_string(), inner);
        list.insert("0_4".to_string(), GsDict::new());
        *f.dw.version_one_to_two_conversion_list.borrow_mut() = Some(list);
        f.run(pb, false);
        assert!(std::path::Path::new(&new).exists());
        assert!(!std::path::Path::new(&old).exists());
        assert_eq!(f.dw.ta_len(13), 1);
        let left = f.dw.version_one_to_two_conversion_list.borrow().as_ref().map(|l| l.len());
        assert_eq!(left, Some(1));
        // a block without an entry leaves the list alone; the last entry drops the list
        let pb2 = f.block(5, 8);
        f.run(pb2, false);
        assert!(f.dw.version_one_to_two_conversion_list.borrow().is_some());
        let pb4 = f.block(4, 8);
        f.run(pb4, false);
        assert!(f.dw.version_one_to_two_conversion_list.borrow().is_none());
    }

    #[test]
    fn load_visits_connected_clients_on_a_server() {
        let f = fx_with("pop_clients", vec![], true);
        let pb = f.block(3, 8);
        let mt = f.dw.world().macro_tiles().get(3).unwrap();
        mt.dynamic_objects_loaded.set(true);
        f.dw.dw_db_put("c1_0_3/13", &blob(&[(5, 2, 100, None)]));
        f.run(pb, false);
        assert_eq!(f.dw.ta_len(13), 1);
        // not on a server: the key is ignored
        let g = fx("pop_clients2");
        let pb = g.block(3, 8);
        g.dw.dw_db_put("c1_0_3/13", &blob(&[(5, 2, 100, None)]));
        g.run(pb, false);
        assert_eq!(g.dw.ta_len(13), 0);
    }

    // ------------------------------------------------------------------------------------ start portal

    fn portal_fx(name: &str) -> (Fx, PhysicalBlockRef) {
        let f = fx(name);
        let pb = f.block(3, 8);
        f.dw.world().set_start_portal_pos(IntPair { x: 7, y: 100 });
        for y in [99, 100, 101] {
            cell(pb, 7, y - 96).update(|t| {
                t.type_index = 1;
                t.background_contents = 9;
                t.foreground_contents = 9;
                t.contents = 9;
                t.back_wall_type_index = 1;
            });
        }
        (f, pb)
    }

    #[test]
    fn missing_start_portal_is_rebuilt() {
        let (f, pb) = portal_fx("pop_portal");
        f.run(pb, false);
        for ly in [4, 5] {
            let t = cell(pb, 7, ly).get();
            assert_eq!((t.type_index, t.back_wall_type_index), (2, 2));
            assert_eq!((t.background_contents, t.foreground_contents, t.contents), (0, 0, 0));
        }
        let below = cell(pb, 7, 3).get();
        assert_eq!((below.type_index, below.contents, below.zone_type_index), (0x19, 0, 1));
        assert_eq!(f.lines(), vec!["portal(7,100)"]);
    }

    #[test]
    fn start_portal_not_rebuilt_when_present_elsewhere_or_other_block() {
        // a workbench exists
        let (f, pb) = portal_fx("pop_portal_wb");
        f.spy.workbench.set(true);
        f.run(pb, false);
        assert!(f.lines().is_empty());
        assert_eq!(cell(pb, 7, 4).get().type_index, 1);
        // another block row
        let (f, pb) = portal_fx("pop_portal_row");
        f.dw.world().set_start_portal_pos(IntPair { x: 7, y: 4 * 32 });
        f.run(pb, false);
        assert!(f.lines().is_empty());
        // another block column
        let (f, pb) = portal_fx("pop_portal_col");
        f.dw.world().set_start_portal_pos(IntPair { x: 33, y: 100 });
        f.run(pb, false);
        assert!(f.lines().is_empty());
        // a portal was loaded from the save
        let (f, pb) = portal_fx("pop_portal_loaded");
        f.dw.dw_db_put("0_3/15", &blob(&[(77, 7, 100, Some((1, 1)))]));
        f.run(pb, false);
        assert!(f.lines().is_empty());
        assert_eq!(f.dw.ta_len(15), 1);
    }

    #[test]
    fn start_portal_at_block_top_tolerates_a_missing_neighbour_block() {
        let (f, pb) = portal_fx("pop_portal_top");
        f.dw.world().set_start_portal_pos(IntPair { x: 7, y: 127 });
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["portal(7,127)"]);
        assert_eq!(cell(pb, 7, 31).get().type_index, 2);
    }

    // ------------------------------------------------------------------------------------ migrations

    /// Block row 15 (480..511): columns get distinct surface heights.
    fn mig(version: u8) -> (Fx, PhysicalBlockRef) {
        let f = fx("pop_mig");
        f.with_loader();
        let pb = f.block(15, version);
        (f, pb)
    }
    fn h(f: &Fx, x: i32, h: i32) {
        f.gen.per_column.borrow_mut().push((x, h));
    }
    /// Makes the surface cell of column x a meadow cell.
    fn meadow(pb: PhysicalBlockRef, x: i32, h: i32) {
        let ly = h - 480;
        cell(pb, x, ly).update(|t| {
            t.type_index = 2;
            t.back_wall_type_index = 2;
        });
        cell(pb, x, ly - 1).update(|t| t.type_index = 6);
    }

    #[test]
    fn migration_below_2_plants_and_fish() {
        let (f, pb) = mig(1);
        // x=1 plant 6 in range + fish
        h(&f, 1, 490);
        f.set("plant", 1, 6);
        f.set("npc", 1, 4);
        // x=2 plant 6 out of range: the npc part is skipped as well
        h(&f, 2, 600);
        f.set("plant", 2, 6);
        f.set("npc", 2, 4);
        // x=3 shark above 498 / x=4 shark at 498 / x=5 fish out of range / x=6 another npc
        h(&f, 3, 497);
        f.set("npc", 3, 5);
        h(&f, 4, 498);
        f.set("npc", 4, 5);
        h(&f, 5, 600);
        f.set("npc", 5, 4);
        h(&f, 6, 490);
        f.set("npc", 6, 9);
        srand(31);
        let (r1, r2, r3) = (rand(), rand(), rand());
        srand(31);
        f.run(pb, false);
        let l = f.lines();
        assert_eq!(
            l,
            vec![
                "plant(6,1,490,190,190,true)".to_string(),
                format!("npc(4,1,{},true,false)", migration_fish_depth(490, r1)),
                format!("npc(5,3,{},true,false)", migration_fish_depth(497, r2)),
                "upgrade(1)".to_string(),
            ]
        );
        let _ = r3;
        assert_eq!((f.get("plant", 1), f.get("npc", 1)), (0, 0));
        assert_eq!((f.get("plant", 2), f.get("npc", 2)), (6, 4));
        assert_eq!(f.get("npc", 3), 0);
        assert_eq!(f.get("npc", 4), 5);
        assert_eq!(f.get("npc", 5), 4);
        assert_eq!(f.get("npc", 6), 9);
        assert_eq!(rand(), {
            srand(31);
            rand();
            rand();
            rand()
        });
        assert_eq!(pb.version.get(), 8);
    }

    #[test]
    fn migration_below_6_cactus_npc() {
        let (f, pb) = mig(5);
        h(&f, 1, 490);
        f.set("npc", 1, 7);
        h(&f, 2, 600);
        f.set("npc", 2, 7);
        f.set("plant", 3, 6); // not touched: version >= 2
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["npc(7,1,490,true,false)", "upgrade(5)"]);
        assert_eq!((f.get("npc", 1), f.get("npc", 2), f.get("plant", 3)), (0, 0, 6));
        // version 6: nothing to do for npc 7
        let (f, pb) = mig(6);
        h(&f, 1, 490);
        f.set("npc", 1, 7);
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["upgrade(6)"]);
        assert_eq!(f.get("npc", 1), 7);
    }

    #[test]
    fn migration_below_7_tulips_need_a_meadow() {
        let (f, pb) = mig(6);
        let (m, g) = tulip_genes_dual(0.0, 0.0);
        // x=1 grass on dirt (air), x=2 snow on frozen grass, x=3 snow on grass-frozen variant 0x1b
        for (x, top, below) in [(1, 2u8, 6u8), (2, 5, 0x1b), (3, 2, 0x1c)] {
            h(&f, x, 490);
            f.set("plant", x, 8);
            meadow(pb, x, 490);
            cell(pb, x, 10).update(|t| t.type_index = top);
            cell(pb, x, 9).update(|t| t.type_index = below);
        }
        // failures: solid top, wrong back wall, wrong ground, out of range, no tile below the block
        h(&f, 4, 490);
        f.set("plant", 4, 8);
        meadow(pb, 4, 490);
        cell(pb, 4, 10).update(|t| t.type_index = 1);
        h(&f, 5, 490);
        f.set("plant", 5, 8);
        meadow(pb, 5, 490);
        cell(pb, 5, 10).update(|t| t.back_wall_type_index = 1);
        h(&f, 6, 490);
        f.set("plant", 6, 8);
        meadow(pb, 6, 490);
        cell(pb, 6, 9).update(|t| t.type_index = 7);
        h(&f, 7, 600);
        f.set("plant", 7, 8);
        h(&f, 8, 480);
        f.set("plant", 8, 8);
        cell(pb, 8, 0).update(|t| {
            t.type_index = 2;
            t.back_wall_type_index = 2;
        });
        f.run(pb, false);
        let mut expect: Vec<String> = (1..=3).map(|x| format!("plant(8,{x},490,{m},{g},true)")).collect();
        expect.push("upgrade(6)".into());
        assert_eq!(f.lines(), expect);
        assert_eq!(f.get("plant", 1), 8); // tulips are never cleared
                                          // version 7 skips this step
        let (f, pb) = mig(7);
        h(&f, 1, 490);
        f.set("plant", 1, 8);
        meadow(pb, 1, 490);
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["upgrade(7)"]);
    }

    #[test]
    fn migration_below_8_animals_and_flowers() {
        let (f, pb) = mig(7);
        // x=1 npc 8 + plant 9; x=2 plant 10; x=3 npc 8 only; x=4 not a meadow; x=5 out of range; x=6 other values
        for x in [1, 2, 3] {
            h(&f, x, 490);
            meadow(pb, x, 490);
        }
        f.set("npc", 1, 8);
        f.set("plant", 1, 9);
        f.set("plant", 2, 10);
        f.set("npc", 3, 8);
        h(&f, 4, 490);
        f.set("npc", 4, 8);
        f.set("plant", 4, 9);
        h(&f, 5, 600);
        f.set("plant", 5, 10);
        h(&f, 6, 490);
        meadow(pb, 6, 490);
        f.set("npc", 6, 3);
        f.set("plant", 6, 11);
        f.run(pb, false);
        assert_eq!(
            f.lines(),
            vec![
                "npc(8,1,490,true,false)".to_string(),
                "plant(9,1,490,190,190,true)".to_string(),
                "plant(10,2,490,190,190,true)".to_string(),
                "npc(8,3,490,true,false)".to_string(),
                "upgrade(7)".to_string(),
            ]
        );
        assert_eq!((f.get("npc", 1), f.get("plant", 1), f.get("plant", 2), f.get("npc", 3)), (0, 0, 0, 0));
        assert_eq!((f.get("npc", 4), f.get("plant", 4), f.get("plant", 5)), (8, 9, 10));
        assert_eq!((f.get("npc", 6), f.get("plant", 6)), (3, 11));
    }

    #[test]
    fn current_version_blocks_are_not_migrated_or_upgraded() {
        let (f, pb) = mig(8);
        h(&f, 1, 490);
        f.set("npc", 1, 7);
        f.run(pb, false);
        assert!(f.lines().is_empty());
        assert_eq!(f.get("npc", 1), 7);
        assert_eq!(pb.version.get(), 8);
    }

    #[test]
    fn migrations_need_a_world_tile_loader() {
        let f = fx("pop_mig_noloader");
        let pb = f.block(15, 3);
        f.run(pb, false);
        assert_eq!(f.lines(), vec!["upgrade(3)"]);
        assert_eq!(pb.version.get(), 8);
    }

    // ------------------------------------------------------------------------------------ surface blocks

    /// A block of air in row `by` with the cell (10, by*32+10) selected.
    fn air_block(f: &Fx, by: i32) -> PhysicalBlockRef {
        let pb = f.block(by, 8);
        for i in 0..1024 {
            pb.tile_ref(i).set_type_index(2);
        }
        pb
    }
    fn surface_lines(f: &Fx) -> Vec<String> {
        f.lines().into_iter().filter(|l| l.starts_with("surf") || l.starts_with("snow")).collect()
    }
    fn water(pb: PhysicalBlockRef, lx: i32, ly: i32, level: u8) {
        let c = cell(pb, lx, ly);
        c.set_type_index(3);
        c.set_partial_content_left(level);
    }

    fn sb(name: &str, by: i32, setup: impl FnOnce(PhysicalBlockRef)) -> Vec<String> {
        let f = fx(name);
        let pb = air_block(&f, by);
        setup(pb);
        f.run(pb, true);
        surface_lines(&f)
    }
    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn surface_blocks_solid_and_snow_cells() {
        let l = sb("sb_solid", 3, |pb| cell(pb, 10, 10).set_type_index(1));
        assert_eq!(l, strs(&["snow(10,107,true)"]));
        let l = sb("sb_snow4", 3, |pb| cell(pb, 10, 10).set_type_index(4));
        assert_eq!(l, strs(&["snow(10,106,true)", "snow(10,107,true)"]));
        // snow (type 5) over solid ground gets a snow block; the air above snow does not
        let l = sb("sb_snow5", 3, |pb| {
            cell(pb, 10, 9).set_type_index(1);
            cell(pb, 10, 10).set_type_index(5);
        });
        assert_eq!(l, strs(&["snow(10,106,true)"]));
        // untouched blocks, including absent neighbours above and below, produce nothing
        assert!(sb("sb_empty", 3, |_| {}).is_empty());
    }

    #[test]
    fn surface_blocks_water_levels() {
        let l = sb("sb_w100", 3, |pb| water(pb, 10, 10, 100));
        assert_eq!(l, strs(&["surf(9,106)", "surf(10,105)", "surf(10,106)", "snow(10,107,true)", "surf(11,106)"]));
        // full water is not a surface block itself, but its neighbours see it
        let l = sb("sb_w255", 3, |pb| water(pb, 10, 10, 255));
        assert_eq!(l, strs(&["surf(9,106)", "surf(10,105)", "snow(10,107,true)", "surf(11,106)"]));
        // level 1: only the cell above notices (side neighbours need >= 2)
        let l = sb("sb_w1", 3, |pb| water(pb, 10, 10, 1));
        assert_eq!(l, strs(&["surf(10,105)", "surf(10,106)", "snow(10,107,true)"]));
        // level 0: not even the cell above
        let l = sb("sb_w0", 3, |pb| water(pb, 10, 10, 0));
        assert_eq!(l, strs(&["surf(10,106)", "snow(10,107,true)"]));
        // a side neighbour at level exactly 2 counts, the right one too
        let l = sb("sb_w2", 3, |pb| water(pb, 10, 10, 2));
        assert!(l.contains(&"surf(9,106)".to_string()) && l.contains(&"surf(11,106)".to_string()));
    }

    #[test]
    fn surface_blocks_wrap_edges_and_chain() {
        let l = sb("sb_wrap", 3, |pb| water(pb, 31, 10, 5));
        assert_eq!(l, strs(&["surf(0,106)", "surf(30,106)", "surf(31,105)", "surf(31,106)", "snow(31,107,true)"]));
        // the world's bottom row: nothing below
        let l = sb("sb_bottom", 0, |pb| water(pb, 10, 0, 100));
        assert!(l.contains(&"surf(10,0)".to_string()) && l.contains(&"snow(10,1,true)".to_string()));
        // the world's top row: nothing above
        let l = sb("sb_top", 31, |pb| water(pb, 10, 31, 100));
        assert!(l.contains(&"surf(10,1023)".to_string()) && l.contains(&"surf(10,1022)".to_string()));
        // water above AND beside: still a single surface block (else-if chain)
        let l = sb("sb_chain", 3, |pb| {
            water(pb, 10, 11, 100);
            water(pb, 11, 10, 100);
        });
        assert_eq!(l.iter().filter(|s| *s == "surf(10,106)").count(), 1);
        // left water and right water on the same cell: once
        let l = sb("sb_both", 3, |pb| {
            water(pb, 9, 10, 100);
            water(pb, 11, 10, 100);
        });
        assert_eq!(l.iter().filter(|s| *s == "surf(10,106)").count(), 1);
    }

    #[test]
    fn surface_blocks_only_when_requested() {
        let f = fx("sb_flag");
        let pb = air_block(&f, 3);
        water(pb, 10, 10, 100);
        f.run(pb, false);
        assert!(surface_lines(&f).is_empty());
    }

    #[test]
    fn column_arrays_tolerate_out_of_range_columns() {
        let c = Column(Rc::new(RefCell::new(vec![1, 2, 3])));
        assert_eq!(c.get(1), 2);
        assert_eq!((c.get(-1), c.get(3)), (0, 0));
        c.set(1, 9);
        c.set(-1, 9);
        c.set(3, 9);
        assert_eq!(*c.0.borrow(), vec![1, 9, 3]);
    }
}

/// A server client with real tamed-animal sets and a server that knows nobody, shared by the load/save tests.
#[cfg(test)]
mod fakes {
    use super::*;
    use crate::dynworld::state::{DwServer, DwServerClient};
    use std::cell::RefCell;

    pub(super) struct Sc {
        id: String,
        pub(super) tamed: RefCell<Vec<HashSet<u32>>>,
        pub(super) wired: RefCell<Vec<u64>>,
    }
    impl Sc {
        pub(super) fn new(id: &str) -> Sc {
            Sc { id: id.into(), tamed: RefCell::new(vec![HashSet::new(); 65]), wired: RefCell::new(vec![]) }
        }
    }
    impl DwServerClient for Sc {
        fn connected(&self) -> bool {
            true
        }
        fn client_id(&self) -> String {
            self.id.clone()
        }
        fn light_block_index(&self) -> i32 {
            0
        }
        fn block_is_wired(&self, _m: i32) -> bool {
            false
        }
        fn dynamic_object_is_wired(&self, _u: u64) -> bool {
            false
        }
        fn wire_dynamic_object(&self, u: u64) {
            self.wired.borrow_mut().push(u);
        }
        fn unwire_dynamic_object(&self, _u: u64) {}
        fn add_creation_object_data_to_send_of_type_object_id(&self, _d: &NetBlob, _t: u8, _i: u64) {}
        fn add_creation_data_update_object_data_to_send_of_type(&self, _d: &NetBlob, _t: u8) {}
        fn add_update_object_data_to_send_of_type_reliable(&self, _d: &NetBlob, _t: u8, _r: bool) {}
        fn add_removal_object_data_to_send_of_type_object_id(&self, _d: &NetBlob, _t: u8, _i: u64) {}
        fn send_all_object_data(&self) {}
        fn world_indices_containing_tamed_animals(&self) -> &RefCell<Vec<HashSet<u32>>> {
            &self.tamed
        }
    }

    pub(super) struct Srv;
    impl DwServer for Srv {
        fn player_info_for_peer_id(&self, _p: &str) -> Option<PlistValue> {
            None
        }
        fn player_is_admin_with_id(&self, _c: &str) -> bool {
            false
        }
        fn player_update(&self) -> Option<String> {
            None
        }
        fn notify_players_changed(&self) {}
    }

    /// The fakes answer the way a quiet connected client does.
    #[test]
    fn fake_client_and_server_are_inert() {
        let sc = Sc::new("c9");
        assert!(sc.connected());
        assert_eq!(sc.client_id(), "c9");
        assert_eq!(sc.light_block_index(), 0);
        assert!(!sc.block_is_wired(1));
        assert!(!sc.dynamic_object_is_wired(2));
        sc.wire_dynamic_object(3);
        sc.unwire_dynamic_object(3);
        let blob: NetBlob = vec![1];
        sc.add_creation_object_data_to_send_of_type_object_id(&blob, 1, 2);
        sc.add_creation_data_update_object_data_to_send_of_type(&blob, 1);
        sc.add_update_object_data_to_send_of_type_reliable(&blob, 1, true);
        sc.add_removal_object_data_to_send_of_type_object_id(&blob, 1, 2);
        sc.send_all_object_data();
        assert_eq!(*sc.wired.borrow(), vec![3]);
        assert_eq!(sc.world_indices_containing_tamed_animals().borrow().len(), 65);
        assert!(Srv.player_info_for_peer_id("p").is_none());
        assert!(!Srv.player_is_admin_with_id("c"));
        assert!(Srv.player_update().is_none());
        Srv.notify_players_changed();
    }
}

/// Branch coverage of the save/load paths: legacy files, blockhead loading limits, per-type constructors, saves of
/// chests, freight cars and owner-bound animals, and the initial net data of a macro tile.
#[cfg(test)]
mod load_save_tests {
    use super::fakes::{Sc, Srv};
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DynamicWorldInit, ServerClientMap};
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    type Log = Rc<RefCell<Vec<String>>>;

    /// An object with scriptable net data, owner and `blockheadsLoaded` counting.
    struct TObj {
        base: RefCell<DynamicObjectBase>,
        ty: i32,
        log: Log,
        creation: Option<Vec<u8>>,
        update: Option<Vec<u8>>,
        owner: Option<String>,
    }
    fn tobj(
        ty: i32,
        id: u64,
        log: &Log,
        creation: Option<&[u8]>,
        update: Option<&[u8]>,
        owner: Option<&str>,
    ) -> ObjRef {
        let o = crate::objects::handle::alloc_obj(|w| TObj {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            ty,
            log: log.clone(),
            creation: creation.map(|c| c.to_vec()),
            update: update.map(|c| c.to_vec()),
            owner: owner.map(str::to_string),
        });
        o.base.borrow_mut().unique_id = id;
        o
    }
    impl DynamicObject for TObj {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn object_type(&self) -> i32 {
            self.ty
        }
        fn creation_net_data_for_client(&self, _me: &ObjRef, _c: Option<&str>) -> Option<Vec<u8>> {
            self.creation.clone()
        }
        fn update_net_data_for_client(&self, _me: &ObjRef, _c: Option<&str>) -> Option<Vec<u8>> {
            self.update.clone()
        }
        fn client_id_for_saving_seperately_and_only_loading_while_player_online(&self) -> Option<String> {
            self.owner.clone()
        }
        fn blockheads_loaded(&self, _me: &ObjRef) {
            self.log.borrow_mut().push(format!("loaded({})", self.base.borrow().unique_id));
        }
    }

    /// Scriptable hooks: what the constructors return and which flags the blockhead/chest/freight hooks report.
    struct Hk {
        log: Log,
        existing: RefCell<Option<ObjRef>>,
        fail_blockhead: Cell<bool>,
        fail_construct: Cell<bool>,
        level: Cell<i32>,
        save_dir: RefCell<Option<String>>,
        chest_type: Cell<i32>,
        chest_chain: Cell<bool>,
        freight_dict: Cell<bool>,
        freight_flag: Cell<bool>,
        no_dir: RefCell<Vec<u64>>,
        save_ok: Cell<bool>,
    }
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn blockhead_with_id_including_net(&self, _id: u64) -> Option<ObjRef> {
            self.existing.borrow().clone()
        }
        fn blockhead_will_be_unloaded(&self, b: &ObjRef) {
            self.log.borrow_mut().push(format!("unload({})", b.unique_id()));
        }
        fn blockhead_from_save_dict(
            &self,
            d: &crate::plist::value::PlistDict,
            _slots: Option<PlistValue>,
            _repos: bool,
            _dir: Option<&str>,
            _local: Option<PlistValue>,
        ) -> Option<ObjRef> {
            if self.fail_blockhead.get() {
                return None;
            }
            let id = d.map.get(&"uniqueID".to_string()).map_or(0, |v| v.unsigned_long_value());
            Some(tobj(24, id, &self.log, None, None, None))
        }
        fn fully_load_if_needed_around_pos(&self, _p: IntPair, _i: u32, b: &ObjRef) {
            self.log.borrow_mut().push(format!("fullload({})", b.unique_id()));
        }
        fn blockhead_stop_interacting_if_no_interaction_object(&self, b: &ObjRef) {
            self.log.borrow_mut().push(format!("stop({})", b.unique_id()));
        }
        fn blockhead_die(&self, b: &ObjRef) {
            self.log.borrow_mut().push(format!("die({})", b.unique_id()));
        }
        fn blockhead_prepare_inventory_for_saving(&self, b: &ObjRef) {
            self.log.borrow_mut().push(format!("prep({})", b.unique_id()));
        }
        fn blockhead_inventory_needs_saving(&self, _b: &ObjRef) -> bool {
            true
        }
        fn blockhead_set_inventory_needs_saving(&self, b: &ObjRef, v: bool) {
            self.log.borrow_mut().push(format!("needs({},{})", b.unique_id(), v));
        }
        fn blockhead_client_save_dir(&self, b: &ObjRef) -> Option<String> {
            if self.no_dir.borrow().contains(&b.unique_id()) {
                return None;
            }
            self.save_dir.borrow().clone()
        }
        fn save_physical_block_for_macro_tile(&self, mt: &MacroTileRef, rel: bool, dont: bool, only: bool) -> bool {
            self.log.borrow_mut().push(format!("savepb({},{},{}){}", rel, dont, only, mt.dynamic_objects_loaded.get()));
            self.save_ok.get()
        }
        fn save_light_block_for_client_light_block_index(&self, i: i32, pb: Option<PhysicalBlockRef>, now: bool) {
            self.log.borrow_mut().push(format!("lightblock({},{},{})", i, pb.is_some(), now));
        }
        fn blockhead_save_item_slots_array(&self, _b: &ObjRef) -> PlistValue {
            PlistValue::array(vec![PlistValue::int(1)])
        }
        fn blockhead_save_dict_including_workbench_or_interaction_object(&self, b: &ObjRef, inc: bool) -> PlistValue {
            dict_of(vec![("uniqueID", PlistValue::int(b.unique_id() as i64)), ("inc", PlistValue::boolean(inc))])
        }
        fn workbench_level(&self, _w: &ObjRef) -> i32 {
            self.level.get()
        }
        fn workbench_set_level_silently(&self, _w: &ObjRef, l: i32) {
            self.log.borrow_mut().push(format!("level({l})"));
        }
        fn tree_update_all_owned_tiles_to_new_id_size(&self, t: &ObjRef) {
            self.log.borrow_mut().push(format!("treetiles({})", t.unique_id()));
        }
        fn dynamic_world_changed_at_pos_object_type(&self, p: IntPair, t: i32) {
            self.log.borrow_mut().push(format!("changed({},{},{})", p.x, p.y, t));
        }
        fn chest_type(&self, _c: &ObjRef) -> i32 {
            self.chest_type.get()
        }
        fn chest_inventory_data_allowing_empty(&self, _c: &ObjRef) -> Option<Vec<u8>> {
            self.chest_chain.get().then(|| b"inv".to_vec())
        }
        fn chest_save_dict_excluding_inventory(&self, _c: &ObjRef) -> Option<PlistValue> {
            self.chest_chain.get().then(|| dict_of(vec![("chest", PlistValue::boolean(true))]))
        }
        fn freight_needs_chest_save(&self, _c: &ObjRef) -> bool {
            self.freight_flag.get()
        }
        fn freight_set_needs_chest_save(&self, c: &ObjRef, v: bool) {
            self.log.borrow_mut().push(format!("freightflag({},{})", c.unique_id(), v));
        }
        fn freight_chest_save_dict(&self, _c: &ObjRef) -> Option<PlistValue> {
            self.freight_dict.get().then(|| dict_of(vec![("items", PlistValue::int(3))]))
        }
        fn construct_from_save_dict(
            &self,
            kind: SaveLoadKind,
            t: i32,
            d: &crate::plist::value::PlistDict,
            chest: Option<&crate::plist::value::PlistDict>,
        ) -> Option<ObjRef> {
            self.log.borrow_mut().push(format!("construct({:?},{},{})", kind, t, chest.is_some()));
            if self.fail_construct.get() {
                return None;
            }
            let id = d.map.get(&"uniqueID".to_string()).map_or(0, |v| v.unsigned_long_value());
            let o = tobj(t, id, &self.log, None, None, None);
            let x = d.map.get(&"pos_x".to_string()).map_or(0, |v| v.int_value());
            let y = d.map.get(&"pos_y".to_string()).map_or(0, |v| v.int_value());
            o.base().borrow_mut().pos = IntPair { x, y };
            Some(o)
        }
    }

    fn hk(log: &Log) -> Rc<Hk> {
        Rc::new(Hk {
            log: log.clone(),
            existing: RefCell::new(None),
            fail_blockhead: Cell::new(false),
            fail_construct: Cell::new(false),
            level: Cell::new(0),
            save_dir: RefCell::new(None),
            chest_type: Cell::new(0),
            chest_chain: Cell::new(false),
            freight_dict: Cell::new(true),
            freight_flag: Cell::new(true),
            no_dir: RefCell::new(vec![]),
            save_ok: Cell::new(true),
        })
    }

    fn fixture(name: &str) -> (Rc<DynamicWorld>, Log, Rc<Hk>) {
        let d = dw(name, 1);
        let log: Log = Rc::new(RefCell::new(vec![]));
        let h = hk(&log);
        d.set_hooks(h.clone());
        (d, log, h)
    }

    /// A server-backed world with one connected client `c1`.
    fn served(name: &str, app: bool) -> (Rc<DynamicWorld>, Log, Rc<Hk>, Rc<Sc>) {
        let (main, dwdb) = dbs(name);
        let w = world(1);
        std::mem::forget(w.clone());
        let log: Log = Rc::new(RefCell::new(vec![]));
        let sc = Rc::new(Sc::new("c1"));
        let clients: ServerClientMap = Rc::new(RefCell::new(GsDict::new()));
        clients.borrow_mut().insert("c1".to_string(), sc.clone());
        clients.borrow_mut().insert("c2".to_string(), Rc::new(Sc::new("c2")));
        let d = DynamicWorld::new(DynamicWorldInit {
            world: w,
            world_tile_loader: None,
            server: Some(Rc::new(Srv)),
            server_clients: Some(clients),
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: app.then(|| dbs(&format!("{name}_app")).0),
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(scratch(&format!("{name}_save"))),
        });
        let h = hk(&log);
        d.set_hooks(h.clone());
        (d, log, h, sc)
    }

    fn dict_with(entries: Vec<(&str, i64)>) -> PlistValue {
        dict_of(entries.into_iter().map(|(k, v)| (k, PlistValue::int(v))).collect())
    }
    fn plist_bytes(v: &PlistValue) -> Vec<u8> {
        data_from_property_list(v).unwrap()
    }

    #[test]
    fn small_helpers() {
        assert_eq!(clampi255(-4), 0);
        assert_eq!(clampi255(77), 77);
        assert_eq!(clampi255(300), 255);
    }

    /// Part A reads `dynamicWorldv2` and then the legacy `dynamicWorld` from the save directory; a version-1 save
    /// arms the conversion of the old per-tile files.
    #[test]
    fn legacy_save_files_arm_the_conversion() {
        let (d, _log, _h) = fixture("lsf_legacy");
        let mut times = PlistValue::mutable_dict();
        if let PlistValue::Dict(t) = &mut times {
            t.map.insert("3".to_string(), PlistValue::int(9));
        }
        let sd = dict_of(vec![
            ("dynamicObjectIDCount", PlistValue::int(41)),
            ("activeBlockheadIndex", PlistValue::int(2)),
            ("workbenchHasBeenCrafted", PlistValue::boolean(true)),
            ("poleItemTakenTimes", times),
        ]);
        let dir = d.world_save_directory.clone();
        std::fs::write(format!("{dir}dynamicWorld"), plist_bytes(&sd)).unwrap();
        for f in ["3_4_dynamicWorld_7", "3_4_dynamicWorld_8", "5_6_dynamicWorld_9", "junk_dynamicWorld", "other"] {
            std::fs::write(format!("{dir}{f}"), b"x").unwrap();
        }
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(true, false));
        assert_eq!(d.dynamic_object_id_count.get(), 41);
        assert!(d.workbench_has_been_crafted.get());
        assert!(d.pole_item_taken_times.borrow().is_some());
        let list = d.version_one_to_two_conversion_list.borrow();
        let list = list.as_ref().expect("conversion armed");
        assert_eq!(list.len(), 2);
        assert_eq!(list.get(&"3_4".to_string()).unwrap().len(), 2);
        drop(list);
        // the save written while converting is still version 1
        d.save_game_with_world_data_sign_ownership_data(Some(b"w"), Some(b"signs"));
        let v =
            crate::plist::helpers::property_list_from_data(&d.world_database.get("dynamicWorldv2").unwrap()).unwrap();
        assert_eq!(v.object_for_key("saveVersion").unwrap().int_value(), 1);
        assert!(v.object_for_key("poleItemTakenTimes").is_some());
        assert_eq!(d.world_database.get("signOwnershipData").unwrap(), b"signs");
    }

    #[test]
    fn old_save_without_old_files_has_nothing_to_convert() {
        let (d, _log, _h) = fixture("lsf_noconv");
        let dir = d.world_save_directory.clone();
        std::fs::write(format!("{dir}dynamicWorldv2"), plist_bytes(&dict_with(vec![("saveVersion", 1)]))).unwrap();
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(true, false));
        assert!(d.version_one_to_two_conversion_list.borrow().is_none());
    }

    /// Part C: at most five blockheads are loaded, missing inventories are skipped, an already-loaded blockhead
    /// is unloaded first and custom-rules death mode kills everybody.
    #[test]
    fn blockhead_loading_limits_and_death_rule() {
        let (d, log, h) = fixture("lsf_bhs");
        let mut rules = d.world().custom_rules();
        rules.custom_rules_enabled = 1;
        rules.death = 4;
        d.world().set_custom_rules(rules);
        let dicts: Vec<PlistValue> = (1..=8).map(|i| dict_with(vec![("uniqueID", i)])).collect();
        d.world_database.put("blockheads", &plist_bytes(&dict_of(vec![("dynamicObjects", PlistValue::array(dicts))])));
        let inv = plist_bytes(&PlistValue::array(vec![]));
        for i in [1, 2, 4, 5, 6, 7, 8] {
            d.world_database.put(&format!("blockhead_{i}_inventory"), &inv);
        }
        // the inventory of 3 only exists as a legacy file; 2 is already loaded elsewhere
        std::fs::write(format!("{}blockhead_3_inventory", d.world_save_directory), &inv).unwrap();
        let old = tobj(24, 2, &log, None, None, None);
        *h.existing.borrow_mut() = Some(old.clone());
        d.blockheads.borrow_mut().push(old.clone());
        d.net_blockheads.borrow_mut().push(old);
        d.active_blockhead_index.set(40);
        let o1 = tobj(14, 90, &log, None, None, None);
        d.do_set(14, 90, o1);
        let o2 = tobj(14, 91, &log, None, None, None);
        d.ta_set(14, 91, o2);
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(true, true));
        let ids: Vec<u64> = d.blockheads.borrow().iter().map(|b| b.unique_id()).collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        assert_eq!(d.active_blockhead_index.get(), 0);
        assert!(d.has_loaded_blockheads.get());
        assert!(d.net_blockheads.borrow().is_empty());
        let l = log.borrow().clone();
        assert!(l.contains(&"unload(2)".to_string()));
        assert!(l.contains(&"loaded(90)".to_string()) && l.contains(&"loaded(91)".to_string()));
        assert!(l.contains(&"fullload(5)".to_string()));
        assert!(l.contains(&"stop(3)".to_string()) && l.contains(&"die(3)".to_string()));
    }

    #[test]
    fn blockhead_load_failure_aborts() {
        let (d, _log, h) = fixture("lsf_bhfail");
        h.fail_blockhead.set(true);
        d.world_database.put(
            "blockheads",
            &plist_bytes(&dict_of(vec![("dynamicObjects", PlistValue::array(vec![dict_with(vec![("uniqueID", 4)])]))])),
        );
        d.world_database.put("blockhead_4_inventory", &plist_bytes(&PlistValue::array(vec![])));
        assert!(!d.load_dynamic_objects_reposition_blockhead_load_failures(true, true));
        // a record without the dynamicObjects key loads nothing but still counts as loaded
        let (d2, _l2, _h2) = fixture("lsf_bhnone");
        d2.world_database.put("blockheads", &plist_bytes(&dict_with(vec![("other", 1)])));
        assert!(d2.load_dynamic_objects_reposition_blockhead_load_failures(true, true));
        assert!(d2.has_loaded_blockheads.get());
    }

    fn mk_dict(id: i64, x: i64, y: i64, extra: Vec<(&str, i64)>) -> PlistValue {
        let mut e = vec![("uniqueID", id), ("pos_x", x), ("pos_y", y)];
        e.extend(extra);
        dict_with(e)
    }
    fn blob(dicts: Vec<PlistValue>) -> Vec<u8> {
        plist_bytes(&dict_of(vec![("dynamicObjects", PlistValue::array(dicts))]))
    }
    fn block(d: &DynamicWorld, mx: i32, my: i32) -> PhysicalBlockRef {
        let world = d.world();
        let pb = world.take_physical_block();
        pb.x_pos.set(mx);
        pb.y_pos.set(my);
        *world.macro_tiles().get(my as usize).unwrap().physical_block.borrow_mut() = Some(pb);
        pb
    }

    /// Duplicates, static positions and the per-type constructors of `loadDynamicObjectsOfType`.
    #[test]
    fn load_of_type_constructors_and_duplicates() {
        let (d, log, h) = fixture("lsf_types");
        let pb = block(&d, 0, 0);
        pb.version.set(3);
        let mut portal = false;
        // dupe id already being added: reported as a change at its position
        d.currently_adding_object_ids.borrow_mut()[13].insert(5);
        let b = blob(vec![mk_dict(5, 7, 8, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"changed(7,8,13)".to_string()));
        // static position already occupied (chest type 46 is an interaction object)
        let occupied = tobj(16, 1, &log, None, None, None);
        let key = d.world_index_key(IntPair { x: 3, y: 4 });
        d.bp_set(16, key, occupied.clone());
        let b = blob(vec![mk_dict(6, 3, 4, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            16,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"changed(3,4,16)".to_string()));
        // the same for an interaction object type (no dupe-position log line for 0x12)
        d.bp_set(18, key, occupied);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            18,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        // trees below version 4 get their tiles upgraded, plants do not
        log.borrow_mut().clear();
        let b = blob(vec![mk_dict(20, 9, 9, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            1,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"treetiles(20)".to_string()));
        pb.version.set(5);
        let b = blob(vec![mk_dict(21, 10, 9, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            2,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(!log.borrow().contains(&"treetiles(21)".to_string()));
        let b = blob(vec![mk_dict(22, 11, 9, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            10,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        // sharks: at most 16 per call
        log.borrow_mut().clear();
        let sharks: Vec<PlistValue> = (0..20).map(|i| mk_dict(100 + i, i, 3, vec![])).collect();
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            0x24,
            Some(&blob(sharks)),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(log.borrow().iter().filter(|s| s.starts_with("construct(Shark")).count(), 16);
        // a constructor that returns nil forgets the id again
        h.fail_construct.set(true);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob(vec![mk_dict(30, 1, 1, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert!(!d.currently_adding_object_ids.borrow()[13].contains(&30));
        h.fail_construct.set(false);
        // generic types and the surface block type
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob(vec![mk_dict(31, 2, 2, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"construct(Generic,13,false)".to_string()));
        log.borrow_mut().clear();
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            22,
            Some(&blob(vec![mk_dict(32, 2, 2, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().is_empty());
        // loaded objects know when the blockheads were loaded already
        d.has_loaded_blockheads.set(true);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob(vec![mk_dict(33, 2, 3, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"loaded(33)".to_string()));
    }

    /// A load of a type that is still being loaded for the same macro block is dropped.
    #[test]
    fn load_of_type_mid_load_is_dropped() {
        let (d, log, _h) = fixture("lsf_mid");
        let pb = block(&d, 0, 0);
        d.currently_loading_macro_blocks.borrow_mut()[13].insert(0);
        let mut portal = false;
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob(vec![mk_dict(5, 7, 8, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(d.ta_len(13), 0);
        assert!(log.borrow().is_empty());
    }

    /// Freight cars read their chest record from the database or the legacy file; workbenches convert portal tiles.
    #[test]
    fn load_of_type_freight_and_workbench() {
        let (d, log, h) = fixture("lsf_wb");
        let pb = block(&d, 0, 0);
        let mut portal = false;
        d.dynamic_object_database.put("trainchest_50", &plist_bytes(&dict_with(vec![("a", 1)])));
        let b = blob(vec![mk_dict(50, 1, 1, vec![]), mk_dict(51, 2, 1, vec![])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            43,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        let l = log.borrow().clone();
        assert!(l.contains(&"construct(FreightCar,43,true)".to_string()));
        assert!(l.contains(&"construct(FreightCar,43,false)".to_string()));
        drop(l);
        // portal workbench: the tile below changes to the type matching the level
        let tile = tile_at_world_position_loaded(3, 4, &d.world()).unwrap();
        for (level, want) in [
            (0, Some(0x19u8)),
            (1, Some(0x21)),
            (2, Some(0x22)),
            (3, Some(0x23)),
            (4, Some(0x24)),
            (5, Some(0x25)),
            (9, None),
        ] {
            h.level.set(level);
            tile.set_type_index(7);
            let id = 200 + level as i64;
            let b = blob(vec![mk_dict(id, 3, 5, vec![("interactionObjectType", 1), ("workbenchType", 1)])]);
            d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
                45,
                Some(&b),
                pb,
                &mut portal,
                None,
            );
            assert_eq!(tile.type_index(), want.unwrap_or(7), "level {level}");
            d.dynamic_objects_to_add.borrow_mut()[45].clear();
            d.dynamic_objects_by_world_pos_index.borrow_mut()[45].clear();
            d.currently_adding_object_ids.borrow_mut()[45].clear();
        }
        assert!(portal);
        // a type-13 workbench (0xd) only registers the portal; old type-2 workbenches at level 2 become 3
        pb.version.set(6);
        h.level.set(2);
        let b = blob(vec![mk_dict(300, 20, 9, vec![("interactionObjectType", 1), ("workbenchType", 2)])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            45,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(!log.borrow().contains(&"level(3)".to_string()));
        pb.version.set(5);
        let b = blob(vec![mk_dict(301, 21, 9, vec![("interactionObjectType", 1), ("workbenchType", 0xd)])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            45,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(d.portal_positions.borrow().contains_index(d.world_index_key(IntPair { x: 21, y: 9 })));
        pb.version.set(5);
        for v in [5u8, 2] {
            pb.version.set(v);
            let b =
                blob(vec![mk_dict(310 + v as i64, 22, 9, vec![("interactionObjectType", 1), ("workbenchType", 2)])]);
            d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
                45,
                Some(&b),
                pb,
                &mut portal,
                None,
            );
            d.dynamic_objects_by_world_pos_index.borrow_mut()[45].clear();
        }
        assert!(log.borrow().contains(&"level(3)".to_string()));
        // other interaction objects, and the generic interaction object slot 0xF re-reports its position
        let b = blob(vec![mk_dict(400, 5, 5, vec![("interactionObjectType", 2)])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            46,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        assert!(log.borrow().contains(&"construct(InteractionObject(2),46,false)".to_string()));
        log.borrow_mut().clear();
        let b = blob(vec![mk_dict(401, 6, 5, vec![("interactionObjectType", 3)])]);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            0xF,
            Some(&b),
            pb,
            &mut portal,
            None,
        );
        let l = log.borrow().clone();
        assert!(l.contains(&"changed(6,5,23)".to_string()) && l.contains(&"changed(6,5,15)".to_string()));
    }

    fn raised(f: impl FnOnce()) -> crate::foundation::exception::NsException {
        crate::foundation::exception::catch_ns(f).expect_err("expected an NSException")
    }

    /// Fix-ups for blockheads without inventory, unreadable records and a missing save directory.
    #[test]
    fn load_skips_missing_inventory_and_bad_records() {
        let (d, log, _h) = fixture("lsf_skip");
        let dicts: Vec<PlistValue> = (1..=8).map(|i| dict_with(vec![("uniqueID", i)])).collect();
        d.world_database.put("blockheads", &plist_bytes(&dict_of(vec![("dynamicObjects", PlistValue::array(dicts))])));
        let inv = plist_bytes(&PlistValue::array(vec![]));
        for i in [1, 3, 4, 5, 6, 7, 8] {
            d.world_database.put(&format!("blockhead_{i}_inventory"), &inv);
        }
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(false, false));
        let ids: Vec<u64> = d.blockheads.borrow().iter().map(|b| b.unique_id()).collect();
        assert_eq!(ids, vec![1, 3, 4, 5, 6]);
        assert!(log.borrow().contains(&"fullload(6)".to_string()));
        // an unparsable record is ignored but the blockheads still count as loaded
        let (d2, _l2, _h2) = fixture("lsf_garbage");
        d2.world_database.put("blockheads", b"not a plist at all");
        assert!(d2.load_dynamic_objects_reposition_blockhead_load_failures(false, false));
        assert!(d2.has_loaded_blockheads.get());
        // a version-1 save whose directory vanished has nothing to convert
        let (d3, _l3, _h3) = fixture("lsf_nodir");
        d3.world_database.put("dynamicWorldv2", &plist_bytes(&dict_with(vec![("saveVersion", 1)])));
        std::fs::remove_dir_all(&d3.world_save_directory).unwrap();
        assert!(d3.load_dynamic_objects_reposition_blockhead_load_failures(true, false));
        assert!(d3.version_one_to_two_conversion_list.borrow().is_none());
    }

    /// An object blob without the `dynamicObjects` key loads nothing; a missing client leaves the sets alone.
    #[test]
    fn load_of_type_without_objects_or_client() {
        let (d, _log, _h) = fixture("lsf_nokey");
        let pb = block(&d, 0, 0);
        let mut portal = false;
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&plist_bytes(&dict_with(vec![("other", 1)]))),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(d.ta_len(13), 0);
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            13,
            Some(&blob(vec![mk_dict(5, 1, 1, vec![])])),
            pb,
            &mut portal,
            Some("nobody"),
        );
        assert_eq!(d.ta_len(13), 1);
        // a portal workbench at the bottom of the world has no tile below it; a failed constructor adds nothing
        let (d2, log2, h2) = fixture("lsf_nowb");
        let pb2 = block(&d2, 0, 0);
        let b = blob(vec![mk_dict(8, 4, 0, vec![("interactionObjectType", 1), ("workbenchType", 1)])]);
        d2.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            45,
            Some(&b),
            pb2,
            &mut portal,
            None,
        );
        assert_eq!(d2.ta_len(45), 1);
        pb2.version.set(2);
        h2.fail_construct.set(true);
        let b = blob(vec![mk_dict(9, 5, 7, vec![("interactionObjectType", 1), ("workbenchType", 2)])]);
        d2.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            45,
            Some(&b),
            pb2,
            &mut portal,
            None,
        );
        assert_eq!(d2.ta_len(45), 1);
        assert!(!log2.borrow().contains(&"level(3)".to_string()));
    }

    /// The conversion of old per-tile files happens when the block is first populated, one directory at a time.
    #[test]
    fn conversion_renames_files_when_the_block_loads() {
        let (d, _log, _h) = fixture("lsf_conv");
        let dir = d.world_save_directory.clone();
        d.world_database.put("dynamicWorldv2", &plist_bytes(&dict_with(vec![("saveVersion", 1)])));
        std::fs::write(format!("{dir}0_0_dynamicWorld_13"), blob(vec![])).unwrap();
        std::fs::write(format!("{dir}0_1_dynamicWorld_13"), blob(vec![])).unwrap();
        assert!(d.load_dynamic_objects_reposition_blockhead_load_failures(true, false));
        assert_eq!(d.version_one_to_two_conversion_list.borrow().as_ref().unwrap().len(), 2);
        for my in 0..2 {
            let pb = block(&d, 0, my);
            pb.version.set(2);
            let mt = d.world().macro_tiles().get(my as usize).unwrap();
            d.load_dynamic_objects_for_macro_tile_include_surface_blocks(&mt, false);
            assert!(std::path::Path::new(&format!("{dir}0_{my}/dynamicWorld")).exists());
            assert_eq!(d.version_one_to_two_conversion_list.borrow().as_ref().map(|l| l.len()), (my == 0).then_some(1));
        }
    }

    /// Blockhead inventories are saved for local and disconnected blockheads, disconnected clients keep a cache of
    /// their dicts that is rewritten and pruned on every save, and the app database gets a copy of the world.
    #[test]
    fn saves_blockheads_inventories_and_disconnected_cache() {
        let (d, log, h, _sc) = served("lsf_save_bh", true);
        let local = tobj(24, 1, &log, None, None, None);
        let away = tobj(24, 2, &log, None, None, None);
        let new_away = tobj(24, 3, &log, None, None, None);
        d.blockheads.borrow_mut().push(local);
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(away);
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(new_away);
        *h.save_dir.borrow_mut() = Some("cl".to_string());
        let mut cache: GsDict<String, Vec<PlistValue>> = GsDict::new();
        cache.insert("cl".to_string(), vec![dict_with(vec![("uniqueID", 2)]), dict_with(vec![("uniqueID", 99)])]);
        cache.insert("stale".to_string(), vec![]);
        *d.disconnected_clients_cached_save_dicts_not_done.borrow_mut() = Some(cache);
        d.client_blockhead_inventories_to_save.borrow_mut().insert("extra".to_string(), b"inv".to_vec());
        d.dynamic_world_changed_macro_positions.borrow_mut()[24].push(IntPair { x: 0, y: 0 });
        d.save_game_with_world_data_sign_ownership_data(Some(b"world"), None);
        assert_eq!(d.world_database.get("extra").unwrap(), b"inv");
        assert!(
            d.world_database.get("_worldv2").is_some() || d.app_database.as_ref().unwrap().get("_worldv2").is_some()
        );
        assert!(d.world_database.get("cl_blockhead_2_inventory").is_some());
        assert!(d.world_database.get("cl_blockhead_3_inventory").is_some());
        assert!(d.world_database.get("cl_blockhead_1_inventory").is_some());
        let gz = d.world_database.get("cl_blockheads").unwrap();
        let v = crate::plist::helpers::property_list_from_data(&crate::foundation::gzip::gzip_inflate(&gz).unwrap())
            .unwrap();
        let PlistValue::Array(a) = v.object_for_key("dynamicObjects").unwrap() else { panic!() };
        assert_eq!(a.items.len(), 3);
        let cache = d.disconnected_clients_cached_save_dicts_not_done.borrow();
        assert_eq!(cache.as_ref().unwrap().keys_in_order(), vec!["cl".to_string()]);
        assert!(log.borrow().contains(&"needs(2,false)".to_string()));
    }

    #[test]
    fn serializer_failures_raise_data_save_exceptions() {
        let v = dict_with(vec![("a", 1)]);
        let ex = raised(|| {
            serialize_or_raise(&v, "Failed to save blockheads.", false, || {
                crate::foundation::exception::ns_raise("NSInvalidArgumentException", "boom")
            });
        });
        assert_eq!((ex.name.as_str(), ex.reason_or_null()), ("DataSaveException", "Failed to save blockheads."));
        let ex = raised(|| {
            serialize_or_raise(&v, "Failed to save dynamic world object data.", true, || {
                crate::foundation::exception::ns_raise("NSInvalidArgumentException", "boom")
            });
        });
        assert_eq!(ex.reason_or_null(), "Failed to save dynamic world object data.");
        assert!(!serialize_or_raise(&v, "unused", true, || {}).is_empty());
    }

    /// The incremental flush needs a server; unknown macro positions are skipped without being dropped.
    #[test]
    fn incremental_flush_needs_a_server() {
        let (d, _log, _h) = fixture("lsf_flush0");
        d.world_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y: 1 });
        d.save_and_send_only_blocks_that_need_to_be_sent();
        assert_eq!(d.world_changed_macro_positions.borrow().len(), 1);
        let (d, _log, _h, _sc) = served("lsf_flush1", false);
        d.world_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y: 99 });
        d.world_changed_send_unreliably_macro_positions.borrow_mut().push(IntPair { x: 0, y: 99 });
        d.save_and_send_only_blocks_that_need_to_be_sent();
        assert_eq!(d.world_changed_macro_positions.borrow().len(), 1);
        assert_eq!(d.world_changed_send_unreliably_macro_positions.borrow().len(), 1);
    }

    /// Owner-bound animals go to per-client files, everything else to the macro tile file; train chests and
    /// chests write their inventories separately.
    #[test]
    fn macro_tile_save_splits_owned_animals_chests_and_freight() {
        let (d, log, h, sc) = served("lsf_tilesave", false);
        let world = d.world();
        let mt = world.macro_tiles().get(0).unwrap();
        mt.dynamic_objects_loaded.set(true);
        // a client that owns animals in this macro tile but has none left: its file is rewritten empty
        sc.tamed.borrow_mut()[13].insert(0);
        *mt.dynamic_objects.borrow_mut() = Some(vec![]);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 13, 0, 0);
        assert!(!sc.tamed.borrow()[13].contains(&0));
        let empty = d.dynamic_object_database.get("c1_0_0/13").unwrap();
        assert!(String::from_utf8_lossy(&empty).contains("dynamicObjects"));
        // owned, server-owned and orphaned animals
        let pet = tobj(13, 1, &log, None, None, Some("c1"));
        let farm = tobj(13, 2, &log, None, None, Some("server"));
        let ghost = tobj(13, 3, &log, None, None, Some("ghost"));
        let plain = tobj(13, 4, &log, None, None, None);
        let odd = tobj(14, 5, &log, None, None, Some("c1"));
        *mt.dynamic_objects.borrow_mut() = Some(vec![pet, farm, ghost, plain, odd]);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 13, 0, 0);
        assert!(sc.tamed.borrow()[13].contains(&0));
        let read = |k: &str| {
            let v = crate::plist::helpers::property_list_from_data(&d.dynamic_object_database.get(k).unwrap()).unwrap();
            let PlistValue::Array(a) = v.object_for_key("dynamicObjects").unwrap() else { panic!() };
            a.items.len()
        };
        assert_eq!(read("0_0/13"), 2);
        assert_eq!(read("c1_0_0/13"), 1);
        // a non-animal type with an owner is dropped from every file but remembered in the client's set
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 14, 0, 0);
        assert_eq!(read("0_0/14"), 0);
        assert!(sc.tamed.borrow()[14].contains(&0));
        // chests
        let chest = tobj(46, 6, &log, None, None, None);
        let portal_chest = tobj(46, 7, &log, None, None, None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![chest, portal_chest]);
        h.chest_chain.set(true);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 46, 0, 0);
        assert_eq!(d.dynamic_object_database.get("0_0/chest_6").unwrap(), b"inv");
        h.chest_chain.set(false);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 46, 0, 0);
        h.chest_type.set(4);
        d.dynamic_object_database.remove("0_0/chest_7");
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 46, 0, 0);
        assert!(d.dynamic_object_database.get("0_0/chest_7").is_none());
        // freight cars save their chest only when it changed
        let car = tobj(43, 8, &log, None, None, None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![car]);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 43, 0, 0);
        assert!(d.dynamic_object_database.get("trainchest_8").is_some());
        assert!(log.borrow().contains(&"freightflag(8,false)".to_string()));
        h.freight_dict.set(false);
        let ex = raised(|| d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 43, 0, 0));
        assert_eq!(ex.reason_or_null(), "Failed to save train chest data.");
        // the debug line for the interaction slot 15
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 15, 0, 0);
    }

    /// The first net data of a macro tile: normal objects, blockheads with both blobs, and the nil-blob exceptions.
    #[test]
    fn initial_net_data_of_a_macro_tile() {
        let (d, log, _h, sc) = served("lsf_initial", false);
        let world = d.world();
        assert!(d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(100_000, "c1").is_none());
        // no physical block yet
        assert!(d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "c1").is_none());
        let pb = block(&d, 0, 0);
        let _ = pb;
        let mt = world.macro_tiles().get(0).unwrap();
        let tree = tobj(1, 10, &log, Some(b"tree"), None, None);
        let silent = tobj(2, 11, &log, None, None, None);
        let bh = tobj(24, 12, &log, Some(b"bh-create"), Some(b"bh-update"), None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![tree, silent, bh]);
        let got = d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "c1").unwrap();
        assert_eq!(got.creation_data.len(), 2);
        let mut creation = got.creation_data.clone();
        creation.sort();
        assert_eq!(creation, vec![(1, vec![b"tree".to_vec()]), (24, vec![b"bh-create".to_vec()])]);
        assert_eq!(got.update_data, vec![(24, vec![b"bh-update".to_vec()])]);
        let mut wired = sc.wired.borrow().clone();
        wired.sort();
        assert_eq!(wired, vec![10, 12]);
        // without a known client nothing is wired
        let got = d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "stranger").unwrap();
        assert_eq!(got.creation_data.len(), 2);
        // blockheads must have both blobs
        let no_create = tobj(24, 13, &log, None, Some(b"u"), None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![no_create]);
        let ex = raised(|| {
            d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "c1");
        });
        assert_eq!(ex.reason_or_null(), "Tried to add nil to array");
        let no_update = tobj(24, 14, &log, Some(b"c"), None, None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![no_update]);
        let ex = raised(|| {
            d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "c1");
        });
        assert_eq!(ex.name, "NSInvalidArgumentException");
    }

    /// Unloading a macro tile forgets static positions and partial-update lists; light changes of unknown tiles
    /// are dropped.
    #[test]
    fn unloading_and_light_flush_edge_cases() {
        let (d, log, _h) = fixture("lsf_unload");
        let world = d.world();
        let mt = world.macro_tiles().get(0).unwrap();
        *mt.physical_block.borrow_mut() = Some(world.take_physical_block());
        let rail = tobj(29, 1, &log, None, None, None);
        rail.base().borrow_mut().pos = IntPair { x: 4, y: 5 };
        let key = d.world_index_key(IntPair { x: 4, y: 5 });
        d.do_set(29, 1, rail.clone());
        d.bp_set(29, key, rail.clone());
        d.partial_update_ordered_objects.borrow_mut()[29].push(rail.clone());
        *mt.dynamic_objects.borrow_mut() = Some(vec![rail]);
        d.remove_dynamic_objects_for_macro_tile(&mt);
        assert!(!d.bp_contains(29, key));
        assert!(d.partial_update_ordered_objects.borrow()[29].is_empty());
        d.light_changed_send_unreliably_macro_positions_single_client.borrow_mut()[0].push(IntPair { x: 0, y: 99 });
        d.send_lightblocks_to_clients();
        assert!(d.light_changed_send_unreliably_macro_positions_single_client.borrow()[0].is_empty());
    }

    /// Small persistence helpers: portal list, chest records and the legacy files they may still live in.
    #[test]
    fn portal_and_chest_record_helpers() {
        let (d, log, _h) = fixture("lsf_chest");
        d.portal_positions.borrow_mut().add_index(d.world_index_key(IntPair { x: 40, y: 41 }));
        d.remove_portal_from_list_at_pos(IntPair { x: 40, y: 41 });
        assert!(d.portal_positions.borrow().is_empty());
        let chest = tobj(46, 6, &log, None, None, None);
        chest.base().borrow_mut().pos = IntPair { x: 70, y: 40 };
        let key = "2_1/chest_6";
        d.dynamic_object_database.put(key, b"db");
        assert_eq!(d.load_local_inventory_data_for_chest(&chest).unwrap(), b"db");
        d.dynamic_object_database.remove(key);
        std::fs::create_dir_all(format!("{}2_1", d.world_save_directory)).unwrap();
        std::fs::write(format!("{}{key}", d.world_save_directory), b"legacy").unwrap();
        assert_eq!(d.load_local_inventory_data_for_chest(&chest).unwrap(), b"legacy");
        d.remove_saved_inventory_for_chest(&chest);
        assert!(d.load_local_inventory_data_for_chest(&chest).is_none());
    }

    struct Ops(Cell<u32>);
    impl crate::world::state::WorldOpsHooks for Ops {
        fn load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &self,
            _w: &crate::world::state::World,
            _mt: &MacroTileRef,
            _mx: i32,
            _my: i32,
            _s: bool,
            _c: bool,
        ) -> bool {
            false
        }
        fn load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(
            &self,
            _w: &crate::world::state::World,
            _mt: &MacroTileRef,
            _s: bool,
        ) {
            self.0.set(self.0.get() + 1);
        }
        fn tile_is_protected_at_pos_against_blockhead(
            &self,
            _w: &crate::world::state::World,
            _p: IntPair,
            _b: &ObjRef,
        ) -> bool {
            false
        }
    }

    /// Animals owned by a client come back when the client's block is loaded; an unloaded macro tile is loaded first.
    #[test]
    fn client_owned_objects_load_with_the_block() {
        let (d, _log, _h, sc) = served("lsf_owned", false);
        let world = d.world();
        let ops = Rc::new(Ops(Cell::new(0)));
        world.set_ops_hooks(Some(ops.clone()));
        let pb = block(&d, 0, 0);
        let mt = world.macro_tiles().get(0).unwrap();
        d.dynamic_object_database.put("c1_0_0/13", &blob(vec![mk_dict(5, 1, 1, vec![])]));
        d.load_client_owned_dynamic_objects_for_client_physical_block("c1", pb);
        assert_eq!(ops.0.get(), 1);
        assert_eq!(d.ta_len(13), 0);
        mt.dynamic_objects_loaded.set(true);
        d.load_client_owned_dynamic_objects_for_client_physical_block("c1", pb);
        assert_eq!(d.ta_len(13), 1);
        assert!(sc.tamed.borrow()[13].contains(&0));
        // a block that is not the macro tile's current one is ignored
        let other = world.take_physical_block();
        other.x_pos.set(0);
        other.y_pos.set(0);
        d.load_client_owned_dynamic_objects_for_client_physical_block("c1", other);
        assert_eq!(ops.0.get(), 1);
        world.set_ops_hooks(None);
        mt.dynamic_objects_loaded.set(false);
        d.load_client_owned_dynamic_objects_for_client_physical_block("c1", pb);
        assert_eq!(ops.0.get(), 1);
    }

    /// The gene bands between the thresholds.
    #[test]
    fn tulip_gene_bands() {
        let (m, _) = tulip_genes_dual(0.5, 0.5);
        assert_eq!(m, (5 | 4 << 12 | 5 << 8 | 4 << 4) as i16 as i32);
        let (m, _) = tulip_genes_dual(0.3, 0.1);
        assert_eq!(m, (4 | 3 << 12 | 4 << 8 | 3 << 4) as i16 as i32);
    }

    /// The test doubles answer through their trait objects too.
    #[test]
    fn test_doubles_report_their_state() {
        let log: Log = Rc::new(RefCell::new(vec![]));
        let o = tobj(14, 1, &log, None, None, None);
        assert_eq!(o.class_info().name, DYNAMIC_OBJECT_CLASS.name);
        assert!(o.as_any().is::<TObj>());
        let w = world(1);
        let ops = Ops(Cell::new(0));
        let mt = w.macro_tiles().get(0).unwrap();
        use crate::world::state::WorldOpsHooks;
        assert!(!ops.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
            &w, &mt, 0, 0, false, false
        ));
        assert!(!ops.tile_is_protected_at_pos_against_blockhead(&w, IntPair { x: 0, y: 0 }, &o));
    }

    /// Free blocks loaded from a file are indexed by position.
    #[test]
    fn loaded_free_blocks_are_indexed() {
        let (d, _log, _h) = fixture("lsf_free");
        let pb = block(&d, 0, 0);
        let mut portal = false;
        d.load_dynamic_objects_of_type_from_data_physical_block_loaded_portal_client_owned_id(
            14,
            Some(&blob(vec![mk_dict(5, 12, 13, vec![])])),
            pb,
            &mut portal,
            None,
        );
        assert_eq!(d.free_blocks_by_position.borrow().len(), 1);
    }

    /// Light blocks, dynamic object files and physical blocks are flushed to the world hooks.
    #[test]
    fn save_game_flushes_dirty_lists() {
        let (d, log, _h) = fixture("lsf_flushall");
        let world = d.world();
        let mt = world.macro_tiles().get(1).unwrap();
        mt.dynamic_objects_loaded.set(true);
        *mt.dynamic_objects.borrow_mut() = Some(vec![]);
        d.light_changed_send_unreliably_macro_positions_single_client.borrow_mut()[3].push(IntPair { x: 0, y: 1 });
        d.light_changed_send_unreliably_macro_positions_single_client.borrow_mut()[4].push(IntPair { x: 0, y: 2 });
        d.light_changed_send_unreliably_macro_positions_single_client.borrow_mut()[5].push(IntPair { x: 0, y: 3 });
        d.world_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y: 2 });
        d.world_changed_send_unreliably_macro_positions.borrow_mut().push(IntPair { x: 0, y: 3 });
        d.send_lightblocks_to_clients();
        let l = log.borrow().clone();
        assert!(l.contains(&"lightblock(3,false,true)".to_string()));
        assert!(l.contains(&"lightblock(4,false,false)".to_string()));
        assert!(l.contains(&"lightblock(5,false,false)".to_string()));
        log.borrow_mut().clear();
        d.dynamic_world_changed_macro_positions.borrow_mut()[13].push(IntPair { x: 0, y: 1 });
        d.dynamic_world_changed_macro_positions.borrow_mut()[13].push(IntPair { x: 0, y: 99 });
        d.dynamic_world_changed_macro_positions.borrow_mut()[24].push(IntPair { x: 0, y: 1 });
        d.world_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y: 99 });
        d.save_game_with_world_data_sign_ownership_data(None, None);
        assert!(d.dynamic_object_database.get("0_1/13").is_some());
        assert!(d.dynamic_object_database.get("0_1/24").is_none());
        assert!(d.dynamic_world_changed_macro_positions.borrow()[13].is_empty());
        assert!(d.world_changed_macro_positions.borrow().is_empty());
        let l = log.borrow().clone();
        assert!(l.contains(&"savepb(true,false,false)false".to_string()));
        assert!(l.contains(&"savepb(false,false,false)false".to_string()));
        // dirty tiles that exist reach the hook; the front entry is consumed even when the hook declines
        d.world_changed_dont_send_macro_positions.borrow_mut().push(IntPair { x: 0, y: 1 });
        d.snow_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y: 2 });
        log.borrow_mut().clear();
        d.save_game_with_world_data_sign_ownership_data(None, None);
        assert!(log.borrow().contains(&"savepb(false,true,false)true".to_string()));
        assert!(d.snow_changed_macro_positions.borrow().is_empty());
    }

    /// At most four blocks are sent per call; rejected blocks stay queued.
    #[test]
    fn incremental_flush_sends_at_most_four_blocks() {
        let (d, log, h, _sc) = served("lsf_flush4", false);
        for y in 0..6 {
            d.world_changed_macro_positions.borrow_mut().push(IntPair { x: 0, y });
        }
        d.world_changed_send_unreliably_macro_positions.borrow_mut().push(IntPair { x: 0, y: 7 });
        d.save_and_send_only_blocks_that_need_to_be_sent();
        assert_eq!(d.world_changed_macro_positions.borrow().len(), 2);
        assert!(d.world_changed_send_unreliably_macro_positions.borrow().is_empty());
        h.save_ok.set(false);
        d.world_changed_send_unreliably_macro_positions.borrow_mut().push(IntPair { x: 0, y: 7 });
        log.borrow_mut().clear();
        d.save_and_send_only_blocks_that_need_to_be_sent();
        assert_eq!(d.world_changed_macro_positions.borrow().len(), 2);
        assert_eq!(d.world_changed_send_unreliably_macro_positions.borrow().len(), 1);
        assert_eq!(log.borrow().iter().filter(|l| l.starts_with("savepb")).count(), 3);
    }

    /// Cached dicts of disconnected clients: a new directory, blockheads without a directory, inventories without one.
    #[test]
    fn disconnected_cache_edge_cases() {
        let (d, log, h, _sc) = served("lsf_cache2", false);
        let a = tobj(24, 1, &log, None, None, None);
        let b = tobj(24, 2, &log, None, None, None);
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(a.clone());
        d.net_blockheads_with_disconnected_clients.borrow_mut().push(b);
        *h.save_dir.borrow_mut() = Some("new".to_string());
        h.no_dir.borrow_mut().push(2);
        let mut cache: GsDict<String, Vec<PlistValue>> = GsDict::new();
        cache.insert("old".to_string(), vec![]);
        *d.disconnected_clients_cached_save_dicts_not_done.borrow_mut() = Some(cache);
        d.save_blockheads();
        assert!(d.world_database.get("new_blockheads").is_some());
        assert!(d.world_database.get("old_blockheads").is_some());
        let keys = d.disconnected_clients_cached_save_dicts_not_done.borrow().as_ref().unwrap().keys_in_order();
        assert_eq!(keys, vec!["new".to_string()]);
        // a local blockhead without a client directory keeps its inventory under the plain key
        d.save_blockhead_inventory(&a);
        h.no_dir.borrow_mut().push(1);
        d.save_blockhead_inventory(&a);
        assert!(d.world_database.get("blockhead_1_inventory").is_some());
        assert!(d.world_database.get("new_blockhead_1_inventory").is_some());
    }

    /// A client that owns animals here but is not the owner of any is not part of the per-client files; train
    /// chests whose flag is clear are left alone.
    #[test]
    fn macro_tile_save_second_client_and_clean_freight() {
        let (d, log, h, sc) = served("lsf_tile2", false);
        let world = d.world();
        let mt = world.macro_tiles().get(0).unwrap();
        mt.dynamic_objects_loaded.set(true);
        sc.tamed.borrow_mut()[13].insert(0);
        let car = tobj(43, 8, &log, None, None, None);
        *mt.dynamic_objects.borrow_mut() = Some(vec![car]);
        h.freight_flag.set(false);
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 43, 0, 0);
        assert!(d.dynamic_object_database.get("trainchest_8").is_none());
        d.save_dynamic_objects_for_macro_tile_object_type_x_pos_y_pos(&mt, 13, 0, 0);
        assert!(d.dynamic_object_database.get("c1_0_0/13").is_some());
        assert!(d.dynamic_object_database.get("c2_0_0/13").is_none());
    }

    /// Tiles without a loaded physical block are left alone; the net data call also goes through the world hooks.
    #[test]
    fn unload_and_net_data_of_tiles_without_blocks() {
        let (d, log, _h, _sc) = served("lsf_noblock", false);
        let world = d.world();
        let mt = world.macro_tiles().get(0).unwrap();
        let o = tobj(14, 1, &log, None, None, None);
        d.do_set(14, 1, o.clone());
        *mt.dynamic_objects.borrow_mut() = Some(vec![o]);
        d.remove_dynamic_objects_for_macro_tile(&mt);
        assert!(d.do_contains(14, 1));
        assert!(mt.dynamic_objects.borrow().is_none());
        // an empty tile has empty net data
        let ops = Rc::new(Ops(Cell::new(0)));
        world.set_ops_hooks(Some(ops.clone()));
        block(&d, 0, 0);
        *mt.dynamic_objects.borrow_mut() = Some(vec![]);
        let got = d.initial_dynamic_objects_net_data_for_macro_tile_index_wire_for_client(0, "c1").unwrap();
        assert!(got.creation_data.is_empty() && got.update_data.is_empty());
        assert_eq!(ops.0.get(), 1);
        world.set_ops_hooks(None);
        // a client-owned file for a macro tile outside the world is ignored
        let pb = block(&d, 0, 1);
        pb.y_pos.set(99);
        d.dynamic_object_database.put("c1_0_99/13", &blob(vec![mk_dict(5, 1, 1, vec![])]));
        d.load_client_owned_dynamic_objects_for_client_physical_block("c1", pb);
        assert_eq!(d.ta_len(13), 0);
    }

    #[test]
    fn incremental_flush_with_nothing_queued() {
        let (d, log, _h, _sc) = served("lsf_flush_empty", false);
        d.save_and_send_only_blocks_that_need_to_be_sent();
        assert!(log.borrow().is_empty());
    }
}
