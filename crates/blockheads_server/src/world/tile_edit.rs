//! `world_ops` tile edits: the remote edit handlers (`remoteRemoveRequest`, `remoteFillRequest`,...),
//! `removeTile*`, `removeBackWall*`, `removeWater*`, workbench / interaction object placement, `fillTile*`
//! (with the door and trapdoor algorithms), gather send / receive, `paintTile`, `removePaint` and
//! `doRepairForTileAtPos:`.
//!
//! Only the `client == nil` (dedicated server) branches are ported. NULL tile dereferences of the original are
//! reproduced with [`ub_null_deref`] (`// UB-PARITY`).

use super::helpers::*;
use super::misc::{deps, ub_null_deref};
use super::state::{DwObject, TileRef, World};
use super::tiles::{tile_at_world_position, tile_at_world_position_loaded};
use crate::foundation::gzip::gzip_inflate;
use crate::foundation::md5::string_from_md5;
use crate::items::inventory_item::InventoryItem;
use crate::items::misc::item_type_for_workbench_type_and_level;
use crate::items::predicates::*;
use crate::objects::handle::ObjRef;
use crate::plist::helpers::property_list_from_data;
use crate::plist::value::PlistValue;
use crate::rng::glibc_rand::{r01_from, rand_tagged};
use crate::types::enums::{NPCType, WorkbenchType};
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;

// ---------------------------------------------------------------------------------------------------
// constants
// ---------------------------------------------------------------------------------------------------

const TILE_AIR: u8 = 2;
const TILE_WATER: u8 = 3;
const TILE_ICE: u8 = 4;
const TILE_WOODEN_PLATFORM: u8 = 0x20;
const TILE_TIME_CRYSTAL: u8 = 0x10;
const TILE_LAVA: u8 = 0x1f;
const TILE_BASALT_UNMINED: u8 = 0x33;

const I_WATER: u32 = 1044; // 0x414
const I_FREEZE_WATER: u32 = 1058; // 0x422
const I_MELT_WATER: u32 = 1059; // 0x423
const I_ICE: u32 = 1060; // 0x424
const I_BASALT_UNMINED: u32 = 1064; // 0x428
const I_WATER_BUCKET: u32 = 105; // 0x69
const I_BOAT: u32 = 111; // 0x6f
const I_ELEVATOR_SHAFT: u32 = 1087; // 0x43f

/// `pad`-extend a wire payload with zero bytes to `n` (the original reads past the buffer;).
fn pad(p: &[u8], n: usize) -> Vec<u8> {
    let mut v = p.to_vec();
    if v.len() < n {
        v.resize(n, 0);
    }
    v
}
fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn le_u64(b: &[u8], o: usize) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[o..o + 8]);
    u64::from_le_bytes(a)
}
fn le_u16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn clear_paint(t: &mut crate::types::tile::Tile) {
    t.paint_front = 0;
    t.paint_left = 0;
    t.paint_right = 0;
    t.paint_top = 0;
    t.paint_bot = 0;
}

/// `0x1b,0x1c -> 6; 0x31,0x32 -> 0x30` (grass / compost grass die under a placed block).
fn grass_remap(v: u8) -> u8 {
    match v {
        0x1b | 0x1c => 6,
        0x31 | 0x32 => 0x30,
        o => o,
    }
}

/// UB-PARITY: unwrap a tile pointer the original dereferences without a NULL check.
fn must(t: Option<TileRef>, site: &'static str) -> TileRef {
    match t {
        Some(t) => t,
        None => ub_null_deref(site),
    }
}

/// The set of tile `typeIndex` values for which `particleColorForTile(tile).a > 0.5`
/// (`_misc.c`, jump table).
fn particle_type_visible(type_index: u8) -> bool {
    match type_index {
        0 => false,
        2 | 21 | 22 | 23 | 25 | 31 | 33..=47 | 60..=66 => false,
        1..=77 => true,
        _ => false,
    }
}

/// `particleColorForTile(tile).a > 0.5f`: bush / scrub / plant, tree trunk, or a visible tile type. `None` is
/// alpha 0. (The contract names a `&Tile` parameter; the predicates need the live handle.)
pub(crate) fn particle_color_alpha_is_visible(tile: Option<TileRef>) -> bool {
    match tile {
        None => false,
        Some(t) => {
            tile_is_bush_or_scrub(Some(t)) || tile_is_tree_trunk(Some(t)) || particle_type_visible(t.type_index())
        }
    }
}

/// `workbenchTypeIsDoubleHeight` and `itemTypeForWorkbenchTypeAndLevel` live in `items` (item_tables); these are
/// the statics.
#[allow(dead_code)]
pub(crate) fn workbench_type_is_double_height_static(t: WorkbenchType) -> bool {
    workbench_type_is_double_height(t)
}

impl World {
    fn tile_at(&self, x: i32, y: i32) -> Option<TileRef> {
        tile_at_world_position_loaded(x, y, self)
    }

    fn io_at(&self, pos: IntPair) -> Option<DwObject> {
        self.dynamic_world().and_then(|dw| dw.interaction_object_at_pos(pos))
    }

    /// `posY * worldWidthMacro * 32 + posX` as uint32 (int32 arithmetic, wraps).
    fn edit_index(&self, pos_x: u32, pos_y: u32) -> u32 {
        (pos_y as i32).wrapping_mul(self.world_width_macro).wrapping_mul(32).wrapping_add(pos_x as i32) as u32
    }

    /// Decode the gzipped XML plist that trails 0x15 / 0x16 requests.
    fn trailing_gz_plist(payload: &[u8], offset: usize) -> Option<PlistValue> {
        if payload.len() <= offset {
            return None;
        }
        gzip_inflate(&payload[offset..]).and_then(|b| property_list_from_data(&b))
    }

    /// `if (inv) [dynamicWorld clientBlockheadInventoryRecievedForPlayerID:blockheadID:data:]`.
    fn forward_inventory_change(&self, plist: &Option<PlistValue>, client_id: Option<&str>) {
        if let Some(p) = plist {
            if let Some(PlistValue::Data(inv)) = p.object_for_key("blockhead_inventory_change") {
                let id = p.object_for_key("blockhead_uniqueID").map_or(0, |v| v.unsigned_long_value());
                deps().client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(client_id, id, &inv.bytes);
            }
        }
    }

    // =================================================================================================
    // background contents / back walls / water
    // =================================================================================================

    /// `-[World removeAnyBackgroundContentsForTile:atPos:removeBlockhead:]`.
    pub fn remove_any_background_contents_for_tile_at_pos_remove_blockhead(
        &self,
        tile: &TileRef,
        pos: IntPair,
        bh: Option<&ObjRef>,
    ) {
        if tile.background_contents() != 0 {
            let d = deps();
            d.create_background_content_free_block_at_position_for_tile_remove_blockhead(pos, tile, bh);
            tile.set_background_contents(0);
            d.world_changed_at_pos_send_reliably(pos, true);
        }
    }

    /// `-[World remoteRemoveBackWallRequest:fromClient:]`. `payload` excludes the 0x3a type byte.
    /// Returns the world index (`posY*W*32 + posX`) for BHServer's `addFillRequest`.
    pub fn remote_remove_back_wall_request_from_client(&self, payload: &[u8], client_id: Option<&str>) -> u32 {
        let b = pad(payload, 16);
        let remove_blockhead_id = le_u64(&b, 0);
        let (pos_x, pos_y) = (le_u32(&b, 8), le_u32(&b, 0xc));
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        let index = self.edit_index(pos_x, pos_y);
        if self.server().is_some() {
            if let Some(c) = client_id {
                if self.tile_is_protected_at_pos_against_client(pos, Some(c)) {
                    return index;
                }
                // Mods (not in the original): a cancelled `block-break` takes the protected-tile path.
                if let Some(h) = crate::mods::hooks() {
                    let wall = self.tile_at(pos.x, pos.y).map_or(0, |t| t.back_wall_type_index() as u32);
                    if h.block_break(c, pos, true, wall) {
                        return index;
                    }
                }
            }
        }
        let _actor = client_id.filter(|_| crate::mods::hooks().is_some()).map(crate::mods::act_as);
        let bh =
            if remove_blockhead_id != 0 { deps().blockhead_with_id_including_net(remove_blockhead_id) } else { None };
        self.remove_back_wall_at_pos_remove_blockhead(pos, bh.as_ref());
        index
    }

    /// `-[World remoteRemoveRequest:fromClient:]`. `payload` excludes the 0x11 type byte.
    pub fn remote_remove_request_from_client(&self, payload: &[u8], client_id: Option<&str>) -> u32 {
        let b = pad(payload, 0x18);
        let remove_blockhead_id = le_u64(&b, 0);
        let (pos_x, pos_y) = (le_u32(&b, 8), le_u32(&b, 0xc));
        let (cc, fc) = (b[0x10], b[0x11]);
        let (is_water, only_c, only_fg, z_index) = (b[0x12], b[0x13], b[0x14], b[0x15]);
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        let index = self.edit_index(pos_x, pos_y);
        let d = deps();
        if self.server().is_some() {
            if let Some(c) = client_id {
                if self.tile_is_protected_at_pos_against_client(pos, Some(c)) {
                    return index;
                }
                // Mods (not in the original): a cancelled `block-break` takes the protected-tile path.
                if let Some(h) = crate::mods::hooks() {
                    let tile = self.tile_at(pos.x, pos.y).map_or(0, |t| t.type_index() as u32);
                    if h.block_break(c, pos, false, tile) {
                        return index;
                    }
                }
                if z_index == 1 && !d.player_is_admin_with_id(Some(c)) {
                    // resync only; the request is NOT rejected
                    d.update_player(c);
                }
            }
        }
        let _actor = client_id.filter(|_| crate::mods::hooks().is_some()).map(crate::mods::act_as);
        let bh = if remove_blockhead_id != 0 { d.blockhead_with_id_including_net(remove_blockhead_id) } else { None };
        if is_water == 0 {
            self.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                pos_x as i32,
                pos_y as i32,
                cc as i32,
                fc as i32,
                bh.as_ref(),
                only_c != 0,
                only_fg != 0,
            );
            d.world_changed_at_pos_send_reliably(pos, true);
        } else {
            self.remove_water_tile_at_pos(pos);
        }
        index
    }

    /// `-[World removeTileAtWorldX:worldY:createContentsFreeblockCount:createForegroundContentsFreeblockCount:removeBlockhead:]`
    /// (5-arg wrapper).
    pub fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
        &self,
        x: i32,
        y: i32,
        cc: i32,
        fc: i32,
        bh: Option<&ObjRef>,
    ) {
        self.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents_send_world_changed_notifcation_dont_remove_contents(
            x, y, cc, fc, bh, false, false, true, false,
        );
    }

    /// 7-arg wrapper.
    pub fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
        &self,
        x: i32,
        y: i32,
        cc: i32,
        fc: i32,
        bh: Option<&ObjRef>,
        only_c: bool,
        only_fg: bool,
    ) {
        self.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents_send_world_changed_notifcation_dont_remove_contents(
            x, y, cc, fc, bh, only_c, only_fg, true, false,
        );
    }

    /// `-[World shouldBeCrystalBlockAtPos:]`: `clientTileLoader` is nil on the server, so always NO.
    pub fn should_be_crystal_block_at_pos(&self, _pos: IntPair) -> bool {
        false
    }

    /// `CrystalManager` accounting of a mined time crystal (inert on the server; the `sound x cc` calls bump
    /// `freeBlockSoundDelay`).
    fn crystal_mined_accounting(&self, pos: IntPair, cc: i32, bh: Option<&ObjRef>) {
        let d = deps();
        d.crystal_manager_set_count_watcher_none();
        let a = d.crystal_manager_amount();
        if cc < 50001 {
            let s = string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", a.wrapping_add(cc).wrapping_add(73)));
            d.crystal_manager_modify_modify_string(cc.wrapping_neg(), &s);
        }
        for _ in 0..cc {
            d.play_time_crystal_received_sound_at_pos(pos);
        }
        if let Some(b) = bh {
            d.bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(b, ItemType::ITEM_TIME_CRYSTAL, 0);
        }
    }

    /// The fluid wake-up block shared by `removeTile`, `removeWater` and `doRepair`: after a tile became air,
    /// below solid / water -> snow surface; above water / left or right water >= 2 -> surface block.
    fn wake_fluid_around_air_tile(&self, pos: IntPair) {
        let d = deps();
        let (x, y) = (pos.x, pos.y);
        let b = self.tile_at(x, y - 1);
        if b.is_some() && (tile_is_solid(b) || (tile_is_water(b) && b.unwrap().partial_content_left() != 0)) {
            d.load_snow_surface_block_at_pos_load_snow(pos, false);
        }
        let a = self.tile_at(x, y + 1);
        if a.is_some() && tile_is_water(a) && a.unwrap().partial_content_left() != 0 {
            d.load_surface_block_at_pos(pos);
        } else {
            let l = self.tile_at(x - 1, y);
            if l.is_some() && tile_is_water(l) && l.unwrap().partial_content_left() >= 2 {
                d.load_surface_block_at_pos(pos);
            } else {
                let r = self.tile_at(x + 1, y);
                if r.is_some() && tile_is_water(r) && r.unwrap().partial_content_left() >= 2 {
                    d.load_surface_block_at_pos(pos);
                }
            }
        }
    }

    /// `-[World removeTileAtWorldX:worldY:createContentsFreeblockCount:createForegroundContentsFreeblockCount:removeBlockhead:onlyRemoveCOntents:onlyRemoveForegroundContents:sendWorldChangedNotifcation:dontRemoveContents:]`
    /// : the authoritative "mine a tile" implementation (server branch).
    pub fn remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents_send_world_changed_notifcation_dont_remove_contents(
        &self,
        x: i32,
        y: i32,
        cc: i32,
        fc: i32,
        bh: Option<&ObjRef>,
        only_c: bool,
        only_fg: bool,
        send: bool,
        dont_remove: bool,
    ) {
        let Some(tile) = self.tile_at(x, y) else { return };
        let pos = IntPair { x, y };
        let d = deps();

        let door_tile = tile_contains_door(Some(tile));
        let trap_tile = tile_contains_trap_door(Some(tile));
        let mut net_crystal = false;
        let mut server_controlled = false;
        if self.server().is_some() && tile.type_index() == TILE_TIME_CRYSTAL {
            if let Some(b) = bh {
                if d.bh_is_net(b) {
                    net_crystal = true;
                } else {
                    server_controlled = d.bh_is_client_blockhead_being_controlled_by_server(b);
                }
            }
        }
        if !door_tile && !trap_tile && !net_crystal {
            if tile.type_index() == TILE_TIME_CRYSTAL && cc > 0 {
                if server_controlled {
                    if let Some(b) = bh {
                        d.bh_add_to_crystal_discrepency(b, cc);
                    }
                } else if !bh.is_some_and(|b| d.bh_is_net(b)) {
                    self.crystal_mined_accounting(pos, cc, bh);
                }
            } else {
                for _ in 0..(fc as u32) {
                    d.create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
                        pos, true, &tile, bh,
                    );
                }
                for _ in 0..(cc as u32) {
                    d.create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
                        pos, false, &tile, bh,
                    );
                }
            }
        }
        let air_like = tile_is_air_water_or_snow(Some(tile));

        if !only_fg && !matches!(tile.type_index(), 1 | 0xc | 0xe | 0x11 | 0x13 | 0x1d) {
            if !air_like || tile.contents() == 0 {
                self.remove_any_background_contents_for_tile_at_pos_remove_blockhead(&tile, pos, bh);
            }
            let above = self.tile_at(x, y + 1);
            let t = tile.get();
            if !air_like
                && back_wall_is_mutable(Some(tile))
                && (above.is_none()
                    || above.unwrap().back_wall_type_index() == TILE_AIR
                    || above.unwrap().back_wall_type_index() == 0x20
                    || (t.type_index == TILE_WOODEN_PLATFORM
                        && t.back_wall_type_index != TILE_AIR
                        && t.back_wall_type_index != TILE_WATER))
                && (t.type_index != TILE_WOODEN_PLATFORM || t.back_wall_type_index == 0x20)
            {
                tile.update(|t| {
                    t.back_wall_type_index = TILE_AIR;
                    clear_paint(t);
                });
            }
            if !air_like && tile.back_wall_type_index() == TILE_AIR {
                let mut y2 = y - 1;
                while y2 >= 0 && y2 >= y - 16 {
                    let t2 = self.tile_at(x, y2);
                    if !tile_is_air_water_or_snow(t2)
                        || t2.unwrap().back_wall_type_index() == TILE_AIR
                        || !back_wall_is_mutable(t2)
                        || t2.unwrap().type_index() == TILE_WOODEN_PLATFORM
                        || tile.type_index() == TILE_WOODEN_PLATFORM
                    {
                        break;
                    }
                    let t2 = t2.unwrap();
                    let bw = tile.back_wall_type_index();
                    t2.update(|t| {
                        t.back_wall_type_index = bw;
                        if t.back_wall_type_index == TILE_AIR {
                            clear_paint(t);
                        }
                    });
                    if tile.back_wall_type_index() == TILE_AIR {
                        self.remove_any_background_contents_for_tile_at_pos_remove_blockhead(
                            &t2,
                            IntPair { x, y: y2 },
                            bh,
                        );
                    }
                    if send {
                        d.world_changed_at_pos_send_reliably(IntPair { x, y: y2 }, true);
                    }
                    y2 -= 1;
                }
            }
        }

        if !only_fg {
            if !only_c {
                if tile.contents() != 0x67 && !dont_remove {
                    tile.set_contents(0);
                }
                if !matches!(tile.type_index(), 5 | 3 | 0x1f | 0x19 | 0x21..=0x25) {
                    tile.update(|t| {
                        t.type_index = TILE_AIR;
                        t.partial_content_left = 0;
                    });
                }
                tile.update(|t| {
                    t.gather_progress = 0;
                    t.dynamic_object_owner_old = 0;
                    t.dynamic_object_owner = 0;
                });
                if tile_is_air(Some(tile)) {
                    self.wake_fluid_around_air_tile(pos);
                }
            } else if !dont_remove {
                tile.set_contents(0);
            }
        } else if !dont_remove {
            tile.set_foreground_contents(0);
        }

        if send {
            if !air_like && !only_fg {
                d.world_changed_at_pos_send_reliably(pos, true);
                d.update_sun_light_for_tile_at_pos_world(&tile, pos);
            } else {
                d.world_contents_changed_at_pos(pos);
            }
        }
        // reloadDrawBlockGeometryForTile: empty on the server
    }

    /// `-[World removeBackWallAtPos:removeBlockhead:]`.
    pub fn remove_back_wall_at_pos_remove_blockhead(&self, pos: IntPair, _bh: Option<&ObjRef>) {
        let tile = self.tile_at(pos.x, pos.y);
        if let Some(tile) = tile {
            if !tile_is_solid(Some(tile)) {
                tile.update(|t| {
                    t.back_wall_type_index = TILE_AIR;
                    clear_paint(t);
                });
                let d = deps();
                d.world_changed_at_pos_send_reliably(pos, true);
                d.update_sun_light_for_tile_at_pos_world(&tile, pos);
            }
        }
    }

    /// `-[World removeWaterTileAtPos:]`.
    pub fn remove_water_tile_at_pos(&self, pos: IntPair) {
        if let Some(tile) = self.tile_at(pos.x, pos.y) {
            tile.update(|t| {
                t.type_index = TILE_AIR;
                t.partial_content_left = 0;
            });
            let d = deps();
            if tile_is_air(Some(tile)) {
                self.wake_fluid_around_air_tile(pos);
            }
            d.water_changed_at_pos_full_block(pos, true);
            d.update_sun_light_for_tile_at_pos_world(&tile, pos);
        }
    }

    // =================================================================================================
    // workbenches / interaction objects
    // =================================================================================================

    /// `-[World updateTileGraphicForWorkbenchOfType:atPos:level:]`.
    pub fn update_tile_graphic_for_workbench_of_type_at_pos_level(&self, t: WorkbenchType, pos: IntPair, _level: i32) {
        if let Some(tile) = self.tile_at(pos.x, pos.y) {
            if matches!(t.0, 3 | 1 | 13 | 24 | 26 | 29) {
                tile.set_contents(0x2f);
                if matches!(t.0, 1 | 13 | 24 | 26) {
                    if let Some(up) = self.tile_at(pos.x, pos.y + 1) {
                        up.set_contents(0x2f);
                    }
                }
            } else {
                tile.set_contents(0x2e);
                tile.set_gather_progress(0);
            }
        }
        deps().world_changed_at_pos_send_reliably(pos, true);
    }

    /// `-[World remotePlaceWorkbenchRequest:fromPeer:]`. `payload` excludes the 0x15 type byte.
    pub fn remote_place_workbench_request_from_peer(&self, payload: &[u8], client_id: Option<&str>) {
        let b = pad(payload, 0x10);
        let (pos_x, pos_y) = (le_u32(&b, 0), le_u32(&b, 4));
        let wb_type = b[8];
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        if self.server().is_some() {
            if let Some(c) = client_id {
                if self.tile_is_protected_at_pos_against_client(pos, Some(c)) {
                    return;
                }
                // Mods (not in the original): a cancelled `object-place` is dropped like a protected tile.
                if crate::mods::hooks().is_some_and(|h| h.object_place(c, pos, true, wb_type as u32)) {
                    return;
                }
            }
        }
        let plist = Self::trailing_gz_plist(payload, 0x10);
        let save_dict = plist.as_ref().and_then(|p| p.object_for_key("saveDict")).cloned();
        self.forward_inventory_change(&plist, client_id);
        let alias = deps().player_alias_for_peer_id(client_id);
        self.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType(wb_type as u32),
            pos,
            save_dict.as_ref(),
            client_id,
            None,
            alias.as_deref(),
        );
    }

    /// `-[World placeWorkbenchOfType:atPos:saveDict:placedByClient:placedByBlockhead:placedByClientName:]`
    ///  (server branch).
    pub fn place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        t: WorkbenchType,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        _bh: Option<&ObjRef>,
        client_name: Option<&str>,
    ) {
        let d = deps();
        let mut bail = false;
        let mut pos = pos;
        if t == WorkbenchType::WORKBENCH_PLACED_PORTAL {
            let base = self.tile_at(pos.x, pos.y);
            if let Some(b) = base {
                if b.contents() != 0 || tile_is_solid(Some(b)) {
                    bail = true;
                }
            }
            // the portal object sits one tile higher
            pos.y += 1;
        }
        let tile = self.tile_at(pos.x, pos.y);
        if let Some(tile) = tile {
            if self.io_at(pos).is_some() {
                bail = true;
            } else if tile.contents() == 0 && !tile_is_solid(Some(tile)) {
                if workbench_type_is_double_height(t) {
                    let up = self.tile_at(pos.x, pos.y + 1);
                    if let Some(u) = up {
                        if u.contents() != 0 || tile_is_solid(Some(u)) {
                            bail = true;
                        }
                    }
                }
            } else {
                bail = true;
            }
            let level = save_dict.and_then(|s| s.object_for_key("level")).map_or(0, |v| v.int_value());
            if !bail {
                self.update_tile_graphic_for_workbench_of_type_at_pos_level(t, pos, level);
                d.workbench_placed_at_position_of_type_save_dict_placed_by_client_client_name(
                    pos,
                    t,
                    save_dict,
                    client_id,
                    client_name,
                );
            } else {
                d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    pos,
                    item_type_for_workbench_type_and_level(t, level),
                    0,
                    0,
                    None,
                    save_dict,
                    false,
                    0,
                    None,
                );
            }
        }
    }

    /// `-[World remotePlaceInteractionObjectRequest:fromPeer:]`. `payload` excludes the 0x16 type byte.
    pub fn remote_place_interaction_object_request_from_peer(&self, payload: &[u8], client_id: Option<&str>) {
        let b = pad(payload, 8);
        let (pos_x, pos_y) = (le_u32(&b, 0), le_u32(&b, 4));
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        let d = deps();
        if self.server().is_some() {
            if let Some(c) = client_id {
                if self.tile_is_protected_at_pos_against_client(pos, Some(c)) {
                    return;
                }
            }
        }
        let plist = Self::trailing_gz_plist(payload, 8);
        let item_data: Vec<u8> = match plist.as_ref().and_then(|p| p.object_for_key("item")) {
            Some(PlistValue::Data(dd)) => dd.bytes.clone(),
            _ => Vec::new(),
        };
        let item = InventoryItem::from_save_data(&item_data);
        let save_dict = plist.as_ref().and_then(|p| p.object_for_key("saveDict")).cloned();
        let item_type = item.borrow().item_type;
        if self.server().is_some() {
            if let Some(c) = client_id {
                if item_type == ItemType::ITEM_OWNERSHIP_SIGN && !d.player_is_admin_with_id(Some(c)) {
                    return;
                }
                // Mods (not in the original): a cancelled `object-place` is dropped like a protected tile.
                if crate::mods::hooks().is_some_and(|h| h.object_place(c, pos, false, item_type.0)) {
                    return;
                }
            }
        }
        self.forward_inventory_change(&plist, client_id);
        let alias = d.player_alias_for_peer_id(client_id);
        let item_b = item.borrow();
        self.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            &item_b,
            pos,
            save_dict.as_ref(),
            client_id,
            None,
            alias.as_deref(),
        );
    }

    /// `-[World placeInteractionObjectWithItem:atPos:saveDict:placedByClient:placedByBlockhead:placedByClientName:]`
    ///  (server path). The tile pointer is not NULL-checked in the original.
    pub fn place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
        &self,
        item: &InventoryItem,
        pos: IntPair,
        save_dict: Option<&PlistValue>,
        client_id: Option<&str>,
        bh: Option<&ObjRef>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        const SITE: &str =
            "placeInteractionObjectWithItem:atPos:saveDict:placedByClient:placedByBlockhead:placedByClientName:";
        let d = deps();
        let mut bail = false;
        let ty = item.item_type;
        let mut pos = pos;
        if ty == ItemType::ITEM_TRADE_PORTAL {
            let base = self.tile_at(pos.x, pos.y);
            if let Some(b) = base {
                if b.contents() != 0 || tile_is_solid(Some(b)) {
                    bail = true;
                }
            }
            pos.y += 1;
        }
        let (x, y) = (pos.x, pos.y);
        let tile = must(self.tile_at(x, y), SITE);
        if self.io_at(pos).is_some() {
            bail = true;
        }
        let mut flipped = false;
        if item_type_is_two_blocks_wide(ty) {
            let r = self.tile_at(x + 1, y);
            let rb = self.tile_at(x + 1, y - 1);
            if (r.is_some() && (tile_is_solid(r) || r.unwrap().contents() != 0))
                || (!tile_is_solid(rb) && (rb.is_none() || rb.unwrap().foreground_contents() != 0x64))
            {
                flipped = true;
                let l = self.tile_at(x - 1, y);
                if let Some(l) = l {
                    if tile_is_solid(Some(l)) || l.contents() != 0 {
                        bail = true;
                    } else {
                        let lb = self.tile_at(x - 1, y - 1);
                        if !tile_is_solid(lb) && (lb.is_none() || lb.unwrap().foreground_contents() != 0x64) {
                            bail = true;
                        }
                    }
                }
            }
        }
        if item_type_occupies_foreground_contents(ty) && tile.foreground_contents() != 0 {
            bail = true;
        }
        if ty == ItemType::ITEM_TRAIN_STATION {
            if tile.foreground_contents() == 0 {
                let n = |dx: i32, dy: i32| must(self.tile_at(x + dx, y + dy), SITE);
                let a = n(-1, 0);
                if a.contents() != 0 || a.foreground_contents() != 0 {
                    bail = true;
                }
                if n(-1, 1).contents() != 0 {
                    bail = true;
                }
                if n(0, 1).contents() != 0 {
                    bail = true;
                }
                let c = n(1, 0);
                if c.contents() != 0 || c.foreground_contents() != 0 {
                    bail = true;
                }
                if n(1, 1).contents() != 0 {
                    bail = true;
                }
            } else {
                bail = true;
            }
        } else if ty == ItemType::ITEM_TRADE_PORTAL || ty == ItemType::ITEM_MIRROR {
            if must(self.tile_at(x, y + 1), SITE).contents() != 0 {
                bail = true;
            }
        }
        if interaction_object_item_type_occupies_foreground(ty) {
            if tile.foreground_contents() != 0 {
                bail = true;
            }
        } else if tile.contents() != 0 {
            bail = true;
        }
        if bail {
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos,
                ty,
                item.data_a,
                item.data_b,
                item.sub_items.as_deref(),
                save_dict,
                false,
                0,
                bh,
            );
            return None;
        }
        if interaction_object_item_type_occupies_foreground(ty) {
            tile.set_foreground_contents(0x30);
        } else {
            tile.set_contents(0x30);
        }
        if item_type_is_two_blocks_wide(ty) {
            let n = if flipped { self.tile_at(x - 1, y) } else { self.tile_at(x + 1, y) };
            if let Some(n) = n {
                n.set_contents(0x30);
                n.set_gather_progress(0);
            }
        }
        let obj = d.interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(
            pos,
            item,
            flipped,
            save_dict,
            client_id,
            client_name,
        );
        d.world_changed_at_pos_send_reliably(pos, true);
        if ty == ItemType::ITEM_TRAIN_STATION {
            tile.set_foreground_contents(0x62);
            let set = |dx: i32, dy: i32, fg: bool| {
                // the five neighbours were all fetched with `must` above
                let Some(t) = self.tile_at(x + dx, y + dy) else { unreachable!() };
                t.set_contents(0x30);
                if fg {
                    t.set_foreground_contents(0x62);
                }
            };
            set(-1, 0, true);
            set(-1, 1, false);
            set(0, 1, false);
            set(1, 0, true);
            set(1, 1, false);
            for (dx, dy) in [(-1, 0), (1, 0), (0, 1), (-1, 1), (1, 1)] {
                d.world_changed_at_pos_send_reliably(IntPair { x: x + dx, y: y + dy }, true);
            }
        } else if ty == ItemType::ITEM_TRADE_PORTAL || ty == ItemType::ITEM_MIRROR {
            tile.set_contents(0x30);
            if let Some(t) = self.tile_at(x, y + 1) {
                t.set_contents(0x30);
            }
        }
        obj
    }

    // =================================================================================================
    // fill
    // =================================================================================================

    /// `-[World remoteFillRequest:placedByClient:]`. `payload` excludes the 0x10 type byte; the trailing
    /// plist is NOT gzipped. Returns the world index for `addFillRequest`.
    pub fn remote_fill_request_placed_by_client(&self, payload: &[u8], client_id: Option<&str>) -> u32 {
        let b = pad(payload, 0x10);
        let (pos_x, pos_y) = (le_u32(&b, 0), le_u32(&b, 4));
        let (ty, data_a, data_b) = (le_u16(&b, 8), le_u16(&b, 0xa), le_u16(&b, 0xc));
        let z_index = b[0xe];
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        let index = self.edit_index(pos_x, pos_y);
        let d = deps();
        let mut save_dict: Option<PlistValue> = None;
        if payload.len() > 0x10 {
            let plist = property_list_from_data(&payload[0x10..]);
            save_dict = plist.as_ref().and_then(|p| p.object_for_key("placeSaveDict")).cloned();
            self.forward_inventory_change(&plist, client_id);
        }
        let item_type = ItemType(ty as u32);
        if self.server().is_some() {
            if let Some(c) = client_id {
                let prot = self.tile_is_protected_at_pos_against_client(pos, Some(c));
                // train cars and boats may be placed in protected areas
                let reject = prot && !item_type_is_train_car(item_type) && ty != 0x6f;
                if reject {
                    return index;
                }
                // Mods (not in the original): a cancelled `block-place` takes the protected-tile path.
                if crate::mods::hooks().is_some_and(|h| h.block_place(c, pos, ty as u32, data_a, data_b)) {
                    return index;
                }
                if z_index == 1 && !d.player_is_admin_with_id(Some(c)) {
                    d.update_player(c);
                }
            }
        }
        let _actor = client_id.filter(|_| crate::mods::hooks().is_some()).map(crate::mods::act_as);
        let tile = self.tile_at(pos.x, pos.y);
        let alias = d.player_alias_for_peer_id(client_id);
        self.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
            tile.as_ref(),
            pos,
            item_type,
            data_a,
            data_b,
            client_id,
            save_dict.as_ref(),
            alias.as_deref(),
        );
        d.world_changed_at_pos_send_reliably(pos, true);
        index
    }

    /// `-[World remotePaintRequest:fromClient:]`. `payload` excludes the 0x19 type byte.
    pub fn remote_paint_request_from_client(&self, payload: &[u8], client_id: Option<&str>) {
        let b = pad(payload, 0x18);
        let paint_blockhead_id = le_u64(&b, 0);
        let (pos_x, pos_y) = (le_u32(&b, 8), le_u32(&b, 0xc));
        let color_index = le_u16(&b, 0x10);
        let face_index = b[0x12];
        let pos = IntPair { x: pos_x as i32, y: pos_y as i32 };
        if self.server().is_some() {
            if let Some(c) = client_id {
                if self.tile_is_protected_at_pos_against_client(pos, Some(c)) {
                    return;
                }
            }
        }
        let bh =
            if paint_blockhead_id != 0 { deps().blockhead_with_id_including_net(paint_blockhead_id) } else { None };
        let tile = must(self.tile_at(pos.x, pos.y), "paintTile:atPos:colorIndex:faceIndex:paintBlockhead:");
        self.paint_tile_at_pos_color_index_face_index_paint_blockhead(&tile, pos, color_index, face_index, bh.as_ref());
    }

    /// `-[World fillTile:atPos:withType:]`.
    pub fn fill_tile_at_pos_with_type(&self, tile: Option<&TileRef>, pos: IntPair, item_type: ItemType) {
        self.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(tile, pos, item_type, 0, 0, None, None, None, None);
    }

    /// `-[World fillTile:atPos:withType:dataA:dataB:placedByClient:saveDict:placedByClientName:]`.
    pub fn fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
        &self,
        tile: Option<&TileRef>,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        client_id: Option<&str>,
        save_dict: Option<&PlistValue>,
        client_name: Option<&str>,
    ) {
        self.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
            tile, pos, item_type, data_a, data_b, client_id, save_dict, None, client_name,
        );
    }

    /// `-[World fillTile:atPos:withType:dataA:dataB:placedByClient:saveDict:placedByBlockhead:placedByClientName:]`
    /// : place a block / item into the world (server branch, sections A-G). `tile` is dereferenced for
    /// every non-sowable type: `None` is UB-PARITY.
    pub fn fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(
        &self,
        tile: Option<&TileRef>,
        pos: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        client_id: Option<&str>,
        save_dict: Option<&PlistValue>,
        _bh: Option<&ObjRef>,
        client_name: Option<&str>,
    ) {
        const SITE: &str =
            "fillTile:atPos:withType:dataA:dataB:placedByClient:saveDict:placedByBlockhead:placedByClientName:";
        let d = deps();
        let (x, y) = (pos.x, pos.y);
        // A. sowable shortcut
        if item_type_is_sowable(item_type) {
            d.sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
                pos,
                item_type,
                data_a as i16 as i32,
                data_b as i16 as i32,
            );
            return;
        }
        let mut ty = item_type.0;
        // C. freeze-water precondition
        if ty == I_FREEZE_WATER {
            if let Some(t) = tile {
                let tt = Some(*t);
                if (t.foreground_contents() != 0 && !tile_is_plant(tt))
                    || (t.contents() != 0 && !tile_is_tree(tt) && !tile_is_dead_tree(tt) && !tile_is_bush(tt))
                    || t.background_contents() != 0
                {
                    return;
                }
            }
        }
        let tile: TileRef = match tile {
            Some(t) => *t,
            None => ub_null_deref(SITE),
        };
        let old_type = tile.type_index();
        let mut displaced: u8 =
            if old_type == TILE_WATER && ty != I_FREEZE_WATER { tile.partial_content_left() } else { 0 };

        // D. placement conflict check
        // `&&` short-circuits: `tileAtWorldPositionLoaded(x, y-1)` (which loads / creates the block below) is only
        // evaluated for a boat on a non-water tile.
        let boat_not_on_water = ty == I_BOAT
            && tile.type_index() != TILE_WATER
            && self.tile_at(x, y - 1).is_some_and(|b| b.type_index() != TILE_WATER);
        let mut conflict = false;
        if item_type_occupies_background_contents(item_type) {
            conflict = tile.background_contents() != 0 || tile.back_wall_type_index() == TILE_AIR;
        } else if item_type_occupies_foreground_contents(item_type) {
            conflict = tile.foreground_contents() != 0;
        }
        if boat_not_on_water {
            conflict = true;
        } else {
            let io = self.io_at(pos);
            if io.as_ref().is_some_and(|o| o.obj.object_type() == 0x32) {
                conflict = true;
            } else {
                let mut skip_common = false;
                if tile_is_workbench(Some(tile)) {
                    let wt = d.workbench_at_pos(pos).map_or(0, |w| d.workbench_type(&w).0);
                    if wt == 1 || wt == 0xd {
                        conflict = true;
                        skip_common = true;
                    }
                }
                if !skip_common {
                    if item_type_is_block(item_type)
                        && ty != I_ELEVATOR_SHAFT
                        && !tile_allows_placing_of_solid_blocks(Some(tile))
                    {
                        conflict = true;
                    } else if tile_is_solid(Some(tile)) && !matches!(ty, I_MELT_WATER | 13 | 58 | 174 | 178) {
                        conflict = true;
                    } else if item_type_is_placable_on_trees(item_type) && tile.foreground_contents() != 0 {
                        conflict = true;
                    }
                }
            }
        }
        if conflict {
            if ty == I_MELT_WATER || ty == I_FREEZE_WATER {
                return;
            }
            d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                pos, item_type, data_a, data_b, None, None, false, 0, None,
            );
            return;
        }

        // E. switch
        if !(item_type_is_valid_fill_item(item_type) && (13..=1105).contains(&ty)) {
            return;
        }
        let mut handled = false;
        let mut tile2: Option<TileRef> = None;
        let block = |v: u8, handled: &mut bool| {
            tile.set_type_index(v);
            *handled = true;
        };
        let torch = |fg: u8| {
            tile.set_foreground_contents(fg);
            d.add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(
                pos, item_type, data_a, data_b, save_dict, client_id,
            );
        };
        match ty {
            1048 => block(6, &mut handled),
            1062 => block(0x30, &mut handled),
            1051 => block(7, &mut handled),
            1075 => block(0x3a, &mut handled),
            1024 => block(0xa, &mut handled),
            1026 => block(0xb, &mut handled),
            1094 => block(0x46, &mut handled),
            1028 => block(0xd, &mut handled),
            1103 => block(0x4c, &mut handled),
            1105 => block(0x4d, &mut handled),
            1030 => block(0xf, &mut handled),
            1045 => block(0x1a, &mut handled),
            1098..=1102 => block(0x47 + (ty - 1098) as u8, &mut handled),
            1089 => block(0x43, &mut handled),
            1091 => block(0x44, &mut handled),
            1090 => block(0x45, &mut handled),
            1066..=1070 => block(0x35 + (ty - 1066) as u8, &mut handled),
            1049 => block(9, &mut handled),
            1057 => block(0x20, &mut handled),
            1036 => block(0x12, &mut handled),
            1038 => block(0x14, &mut handled),
            1042 => block(0x18, &mut handled),
            1076 => block(0x3b, &mut handled),
            1054 => block(0x1e, &mut handled),
            1063 => block(0x34, &mut handled),
            1064 => block(TILE_BASALT_UNMINED, &mut handled),
            I_WATER => block(TILE_WATER, &mut handled),
            I_FREEZE_WATER => block(TILE_ICE, &mut handled),
            I_ICE => {
                let below = self.tile_at(x, y - 1);
                if below.is_some() && below.unwrap().type_index() == TILE_LAVA {
                    block(TILE_BASALT_UNMINED, &mut handled);
                } else {
                    block(TILE_ICE, &mut handled);
                }
            }
            I_WATER_BUCKET => {
                let season = super::climate::season_for_world_x(x, self.world_time.get(), Some(self));
                let day_fraction = self.get_day_night_fraction_for_x_at_world_time(x as f32, self.world_time.get());
                let weather_fraction = self.get_weather_fraction_for_pos(pos);
                let temp = super::climate::current_temperature_for_tile_at_world_pos(
                    &tile.get(),
                    pos,
                    season,
                    day_fraction,
                    weather_fraction,
                    Some(self),
                );
                if temp < 0.0f32 {
                    ty = I_ICE;
                    block(TILE_ICE, &mut handled);
                } else {
                    let below = self.tile_at(x, y - 1);
                    if below.is_some() && below.unwrap().type_index() == TILE_LAVA {
                        ty = I_BASALT_UNMINED;
                        block(TILE_BASALT_UNMINED, &mut handled);
                    } else {
                        block(TILE_WATER, &mut handled);
                    }
                }
            }
            I_MELT_WATER => {
                let saved = tile.partial_content_left();
                self.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents_send_world_changed_notifcation_dont_remove_contents(
                    x, y, 0, 0, None, false, false, false, true,
                );
                tile.set_type_index(TILE_WATER);
                tile.set_partial_content_left(saved);
                handled = true;
            }
            17 => torch(0x31),
            183 => torch(0x61),
            145..=149 => torch(0x52 + (ty - 145) as u8),
            75 => torch(0x33),
            76 => torch(0x35),
            86 => torch(0x37),
            87 => torch(0x39),
            88 => torch(0x3b),
            47 => torch(0x32),
            150 => torch(0x57),
            254 => torch(0x66),
            258 => torch(0x69),
            157 => torch(0x58),
            91 => {
                tile.set_foreground_contents(0x4e);
                d.add_egg_at_pos_save_dict(pos, save_dict);
            }
            212..=220 => {
                tile.set_foreground_contents(0x63);
                d.add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
                    pos,
                    item_type,
                    save_dict,
                    client_id,
                    client_name,
                );
            }
            58 => {
                tile.set_background_contents(0x45);
                d.add_window_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            174 => {
                tile.set_background_contents(0x5f);
                d.add_window_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            53 => {
                tile.set_foreground_contents(0x42);
                d.add_ladder_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            178 => {
                if tile.contents() != 0 && tile.foreground_contents() == 0 {
                    tile.set_foreground_contents(0x60);
                } else {
                    tile.set_contents(0x60);
                }
                d.add_wire_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            I_ELEVATOR_SHAFT => {
                tile.set_contents(0x67);
                d.add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            1088 => {
                tile.set_contents(0x68);
                d.add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            199 => {
                tile.set_foreground_contents(0x62);
                d.add_rail_at_pos_of_type_owned_by_station(pos, item_type, data_a == 1);
            }
            222..=228 | 236..=244 | 264 | 266 | 268 | 282 | 284 | 332 | 334..=338 => {
                tile.set_foreground_contents(0x64);
                d.add_column_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            229..=235 | 245..=253 | 263 | 265 | 267 | 281 | 283 | 333 | 339..=343 => {
                tile.set_contents(0x65);
                d.add_stairs_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
            }
            52 | 164 => {
                let door_val: u8 = if ty == 164 { 0x38 } else { 9 };
                tile.set_type_index(door_val);
                let t2 = self.tile_at(x, y + 1);
                tile2 = t2;
                let t3 = self.tile_at(x, y + 2);
                if let Some(t2) = t2 {
                    let mut bg: u8 = 2;
                    let mut found = false;
                    let not_fluid = |v: u8| !matches!(v, 2 | 5 | 3);
                    if ty == 52 {
                        if let Some(t3) = t3 {
                            if tile_is_solid(Some(t3))
                                && !tile_is_unplaced_block_minable_with_pickaxe(Some(t3))
                                && !tile_type_is_portal_base_stone(t3.type_index() as i32)
                                && t3.type_index() != 0x1f
                                && t3.type_index() != 0x10
                            {
                                bg = t3.type_index();
                                if not_fluid(bg) {
                                    found = true;
                                }
                            }
                        }
                        if !found && bg == 2 {
                            if back_wall_is_mutable(Some(t2))
                                && !tile_type_is_portal_base_stone(t2.type_index() as i32)
                                && t2.type_index() != 0x1f
                                && t2.type_index() != 0x10
                            {
                                bg = t2.back_wall_type_index();
                                if not_fluid(bg) {
                                    found = true;
                                }
                            }
                        }
                    } else if let Some(t3) = t3 {
                        if matches!(t3.type_index(), 0x39 | 0x45 | 0x44) {
                            bg = t3.type_index();
                            found = true;
                        }
                    }
                    if !found {
                        if let Some(b) = self.tile_at(x, y - 1) {
                            if tile_is_solid(Some(b))
                                && !tile_is_unplaced_block_minable_with_pickaxe(Some(b))
                                && !tile_type_is_portal_base_stone(b.type_index() as i32)
                                && b.type_index() != 0x10
                                && b.type_index() != 0x1f
                            {
                                bg = b.type_index();
                                if not_fluid(bg) {
                                    found = true;
                                }
                            }
                        }
                    }
                    if found {
                        if matches!(bg, 0x1b | 0x1c) {
                            bg = 6;
                        } else if matches!(bg, 0x31 | 0x32) {
                            bg = 0x30;
                        }
                        tile.set_type_index(bg);
                        t2.set_type_index(bg);
                    } else {
                        t2.set_type_index(door_val);
                    }
                }
                d.add_door_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
                handled = true;
            }
            69 | 165 => {
                let val: u8 = if ty == 165 { 0x38 } else { 9 };
                tile.set_type_index(val);
                for nb in [self.tile_at(x + 1, y), self.tile_at(x - 1, y)] {
                    if let Some(n) = nb {
                        if tile_is_solid(Some(n))
                            && !tile_is_unplaced_block_minable_with_pickaxe(Some(n))
                            && (ty != 165 || matches!(n.type_index(), 0x39 | 0x45 | 0x44))
                            && !tile_type_is_portal_base_stone(n.type_index() as i32)
                            && !matches!(n.type_index(), 0x1f | 0x30 | 0x31 | 0x32 | 0x10)
                        {
                            tile.set_type_index(n.type_index());
                            break;
                        }
                    }
                }
                d.add_door_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id);
                handled = true;
            }
            304 => d.add_door_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id),
            111 => d.place_boat_in_water_at_pos_save_dict_placed_by_client(pos, save_dict, client_id),
            204 | 205 | 206 | 208 => {
                d.place_train_car_at_pos_of_type_save_dict_placed_by_client(pos, item_type, save_dict, client_id)
            }
            13 => d.place_fire_at_position(pos),
            303 | 324 | 325 | 326 | 327 | 330 => {
                let npc = npc_type_from_cage_item_type(item_type);
                if npc != NPCType::NPC_NOTHING {
                    d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                        pos, npc, save_dict, false, true, client_id,
                    );
                }
            }
            158 | 159 => {
                let npc = if ty == 158 { NPCType(4) } else { NPCType(5) };
                d.load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                    pos, npc, save_dict, false, true, client_id,
                );
            }
            _ => return, // default: no F / G
        }

        // F. post-processing when the tile type was written
        if handled {
            tile.update(clear_paint);
            let mut fill_bw = false;
            if back_wall_is_mutable(Some(tile)) && !matches!(ty, I_WATER | I_WATER_BUCKET | I_MELT_WATER) {
                if matches!(tile.back_wall_type_index(), 3 | 5 | 2) {
                    tile.update(|t| t.back_wall_type_index = t.type_index);
                    fill_bw = true;
                }
            }
            if ty != I_FREEZE_WATER && ty != I_MELT_WATER {
                tile.update(|t| {
                    t.partial_content_left = 0xff;
                    t.gather_progress = 0;
                });
            }
            if let Some(t2) = tile2 {
                if back_wall_is_mutable(Some(t2)) && matches!(t2.back_wall_type_index(), 3 | 5 | 2) {
                    t2.update(|t| t.back_wall_type_index = t.type_index);
                }
                t2.update(|t| {
                    t.partial_content_left = 0xff;
                    t.gather_progress = 0;
                });
            }
            if !matches!(ty, I_WATER | I_WATER_BUCKET | I_MELT_WATER) {
                if let Some(below) = self.tile_at(x, y - 1) {
                    below.update(|t| {
                        t.type_index = grass_remap(t.type_index);
                        t.back_wall_type_index = grass_remap(t.back_wall_type_index);
                    });
                }
            }
            if fill_bw
                && !matches!(ty, I_WATER | I_WATER_BUCKET | I_MELT_WATER)
                && tile.type_index() != TILE_WOODEN_PLATFORM
            {
                // phase 1: is there anything to hold the back wall?
                let mut found = false;
                let mut y2 = y - 1;
                while y2 >= y - 10 && y2 >= 0 {
                    let t = self.tile_at(x, y2);
                    if tile_is_solid(t) || t.is_some_and(|t| !matches!(t.back_wall_type_index(), 2 | 3 | 5)) {
                        found = true;
                        break;
                    }
                    y2 -= 1;
                }
                if found {
                    // phase 2
                    let mut y2 = y - 1;
                    while y2 >= y - 10 && y2 >= 0 {
                        let t = self.tile_at(x, y2);
                        if y2 == y - 1 && t.is_some_and(|t| t.foreground_contents() == 0x64) {
                            break;
                        }
                        let Some(t) = t else { break };
                        if !tile_is_solid(Some(t))
                            && back_wall_is_mutable(Some(t))
                            && !(tile.type_index() == TILE_ICE && t.type_index() == TILE_WATER)
                            && matches!(t.back_wall_type_index(), 2 | 3 | 5)
                        {
                            let bw = tile.back_wall_type_index();
                            t.update(|t| t.back_wall_type_index = grass_remap(bw));
                        } else {
                            // Note: the remap field is the back wall.
                            t.update(|t| t.back_wall_type_index = grass_remap(t.back_wall_type_index));
                        }
                        y2 -= 1;
                    }
                }
            }
        }

        // G. common world-change notification
        if tile.type_index() == TILE_WATER {
            d.water_changed_at_pos_full_block(pos, true);
            if ty == I_WATER_BUCKET || ty == I_MELT_WATER {
                for nb in [IntPair { x: x + 1, y }, IntPair { x: x - 1, y }, IntPair { x, y: y - 1 }] {
                    if tile_is_air_or_snow(self.tile_at(nb.x, nb.y)) {
                        d.load_surface_block_at_pos(nb);
                    }
                }
            }
        } else if ty != I_MELT_WATER && ty != I_FREEZE_WATER {
            if tile_is_air_water_or_snow(Some(tile)) {
                d.world_contents_changed_at_pos(pos);
            } else {
                d.world_changed_at_pos_send_reliably(pos, true);
            }
            if tile2.is_some() {
                d.world_changed_at_pos_send_reliably(IntPair { x, y: y + 1 }, true);
            }
        }
        if tile_is_solid(Some(tile)) || (tile_is_water(Some(tile)) && tile.partial_content_left() > 0) {
            let off = if tile2.is_some() { 2 } else { 1 };
            if tile_is_air(self.tile_at(x, y + off)) {
                d.load_snow_surface_block_at_pos_load_snow(IntPair { x, y: y + off }, false);
            }
        }
        if tile.type_index() == TILE_ICE {
            d.load_snow_surface_block_at_pos_load_snow(pos, false);
        }
        if item_type_is_light_emitting_block(ItemType(ty)) {
            d.load_glow_block_if_needed_at_pos_tile(pos, &tile);
        }
        if tile.type_index() != TILE_AIR && handled {
            // push displaced water upward
            let mut y2 = y + 1;
            while displaced > 0 {
                let Some(t) = self.tile_at(x, y2) else { break };
                if t.type_index() == TILE_WATER {
                    let add = displaced.min(255 - t.partial_content_left());
                    t.set_partial_content_left(t.partial_content_left() + add);
                    displaced -= add;
                    y2 += 1;
                    continue;
                }
                if t.type_index() == TILE_AIR {
                    t.set_type_index(TILE_WATER);
                    d.water_changed_at_pos_full_block(IntPair { x, y: y2 }, true);
                    for nb in [IntPair { x: x + 1, y: y2 }, IntPair { x: x - 1, y: y2 }, IntPair { x, y: y2 - 1 }] {
                        if tile_is_air_or_snow(self.tile_at(nb.x, nb.y)) {
                            d.load_surface_block_at_pos(nb);
                        }
                    }
                    t.set_partial_content_left(displaced);
                    displaced = 0;
                    let _ = displaced;
                    break;
                }
                break; // any other tile type stops the push
            }
            if !((old_type == TILE_ICE && tile.type_index() == TILE_WATER)
                || (old_type == TILE_WATER && tile.type_index() == TILE_ICE))
            {
                d.update_sun_light_removed_for_tile_at_pos_world(&tile, pos);
                if let Some(t2) = tile2 {
                    d.update_sun_light_removed_for_tile_at_pos_world(&t2, IntPair { x, y: y + 1 });
                }
            }
        } else if ty == 58 || ty == 174 {
            d.update_sun_light_for_tile_at_pos_world(&tile, pos);
        }
    }

    // =================================================================================================
    // gather / paint / repair
    // =================================================================================================

    /// `-[World sendGatherNotificationForTile:atPos:]`: broadcast `[0x1a] + GatherNetRequest` to every
    /// connected client.
    pub fn send_gather_notification_for_tile_at_pos(&self, tile: &TileRef, pos: IntPair) {
        if self.server().is_none() {
            return;
        }
        if self.server_clients_count() == 0 {
            return;
        }
        let mut data = Vec::with_capacity(17);
        data.push(0x1au8);
        data.extend_from_slice(&(pos.x as u32).to_le_bytes());
        data.extend_from_slice(&(pos.y as u32).to_le_bytes());
        data.push(tile.gather_progress());
        data.extend_from_slice(&[0u8; 7]); // padding bytes: project rule is zeros
        deps().send_network_data_to_peers_reliable(&data, None, true);
    }

    /// `-[World remoteGatherRequest:]`. `payload` excludes the 0x1a type byte; BHServer forwards the
    /// original packet afterwards. The particle block only consumes 32 `rand()` draws.
    pub fn remote_gather_request(&self, payload: &[u8]) {
        let b = pad(payload, 16);
        let (pos_x, pos_y) = (le_u32(&b, 0), le_u32(&b, 4));
        let gather_progress = b[8];
        let tile = tile_at_world_position(pos_x as i32, pos_y as i32, &self.macro_tiles(), self);
        let d = deps();
        if let Some(t) = tile {
            if t.gather_progress() < gather_progress && (tile_is_solid(Some(t)) || t.contents() != 0) {
                t.set_gather_progress(gather_progress);
                if particle_color_alpha_is_visible(Some(t)) {
                    let hit = [pos_x as i32 as f32 + 0.5f32, 0.5f32 + pos_y as i32 as f32, -0.5f32];
                    for _ in 0..8 {
                        // ParticleEmitter.instance() is fetched before the four draws
                        let r1 = r01_from(rand_tagged("gather.particle"));
                        let r2 = r01_from(rand_tagged("gather.particle"));
                        let r3 = r01_from(rand_tagged("gather.particle"));
                        let r4 = r01_from(rand_tagged("gather.particle"));
                        let offset = [r1 - 0.5f32, r2 - 0.5f32, 0.0f32];
                        let velocity = [2.0f32 * (r3 - 0.5f32), r4, 0.0f32];
                        let p = [hit[0] + offset[0], hit[1] + offset[1], hit[2] + offset[2]];
                        // the colour is presentation only; the tile colour table is client data
                        d.particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(
                            p,
                            velocity,
                            [1.0, 1.0, 1.0, 1.0],
                            1,
                            1.0,
                            4.0,
                        );
                    }
                }
            }
        }
        if !tile_is_unplaced_block_minable_with_pickaxe(tile)
            || tile.is_some_and(|t| t.type_index() == TILE_ROCK_CRYSTAL)
        {
            d.load_gather_block_at_pos(IntPair { x: pos_x as i32, y: pos_y as i32 });
        }
    }

    /// `-[World paintTile:atPos:colorIndex:faceIndex:paintBlockhead:]` (server branch).
    pub fn paint_tile_at_pos_color_index_face_index_paint_blockhead(
        &self,
        tile: &TileRef,
        pos: IntPair,
        color_index: u16,
        face_index: u8,
        bh: Option<&ObjRef>,
    ) {
        let d = deps();
        let dw = self.dynamic_world();
        // `if (bh == nil || [obj canBeRemovedByBlockhead:bh]) { [obj paint:c]; done = YES }`; a nil object
        // answers NO to the gate and ignores `paint:`.
        let try_paint = |o: Option<&DwObject>| -> bool {
            match o {
                Some(o) => {
                    if bh.is_none() || d.obj_can_be_removed_by_blockhead(&o.obj, bh.unwrap()) {
                        d.obj_paint(&o.obj, color_index);
                        true
                    } else {
                        false
                    }
                }
                None => bh.is_none(),
            }
        };
        let mut done = false;
        if tile.foreground_contents() == 0x64 {
            let col = dw.as_ref().and_then(|dw| dw.column_at_pos(pos));
            done = try_paint(col.as_ref());
        } else if tile.foreground_contents() == 0x42 {
            let l = dw.as_ref().and_then(|dw| dw.ladder_at_pos(pos));
            done = try_paint(l.as_ref());
        }
        if !done && (tile.foreground_contents() == 0x30 || (!tile_is_solid(Some(*tile)) && tile.contents() == 0x30)) {
            let io = dw.as_ref().and_then(|dw| dw.interaction_object_at_pos(pos));
            if let Some(io) = io.as_ref() {
                if d.io_is_paintable(&io.obj) {
                    done = try_paint(Some(io));
                }
            }
        }
        if !done {
            if tile.contents() == 0x65 {
                let s = dw.as_ref().and_then(|dw| dw.stairs_at_pos(pos));
                done = try_paint(s.as_ref());
            } else if !tile_is_solid(Some(*tile)) && tile.contents() == 0x67 {
                let sh = dw.as_ref().and_then(|dw| dw.elevator_shaft_at_pos(pos));
                done = try_paint(sh.as_ref());
            }
        }
        if !done {
            tile.update(|t| match face_index {
                0 => t.paint_front = color_index,
                1 => t.paint_left = color_index,
                2 => t.paint_right = color_index,
                3 => t.paint_top = color_index,
                4 => t.paint_bot = color_index,
                _ => t.paint_front = color_index,
            });
            d.world_changed_at_pos_send_reliably(pos, true);
        }
    }

    /// `-[World removePaintAtTile:atPos:faceIndex:paintBlockhead:]`.
    pub fn remove_paint_at_tile_at_pos_face_index_paint_blockhead(
        &self,
        tile: &TileRef,
        pos: IntPair,
        face_index: u8,
        bh: Option<&ObjRef>,
    ) {
        self.paint_tile_at_pos_color_index_face_index_paint_blockhead(tile, pos, 0, face_index, bh);
    }

    /// `-[World doRepairForTileAtPos:]`: resets a tile to its natural state. Server only.
    pub fn do_repair_for_tile_at_pos(&self, pos: IntPair) {
        if self.server().is_none() {
            return;
        }
        let Some(tile) = self.tile_at(pos.x, pos.y) else { return };
        let d = deps();
        d.dynamic_world_do_repair_for_tile_at_pos(pos);
        if tile_is_not_unmined_block_so_can_be_removed_on_repair(Some(tile)) {
            tile.update(|t| {
                t.contents = 0;
                t.type_index = TILE_AIR;
            });
            if back_wall_removes_with_repair_tool(Some(tile)) {
                tile.set_back_wall_type_index(TILE_AIR);
            }
        }
        tile.update(|t| {
            t.foreground_contents = 0;
            t.background_contents = 0;
            clear_paint(t);
            t.partial_content_left = 0;
            t.gather_progress = 0;
            t.on_fire = 0;
            t.dynamic_object_owner_old = 0;
            t.dynamic_object_owner = 0;
        });
        // start-portal column restoration
        let sp = self.start_portal_pos.get();
        if pos.x == sp.x {
            let py = sp.y;
            if pos.y < py || pos.y > py + 1 {
                if pos.y == py - 1 {
                    tile.set_type_index(0x19);
                    let mut wb = d.workbench_at_pos(sp);
                    if wb.is_none() {
                        self.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                            WorkbenchType::WORKBENCH_BASIC_PORTAL,
                            sp,
                            None,
                            None,
                            None,
                            None,
                        );
                        wb = d.workbench_at_pos(sp);
                    }
                    let level = wb.as_ref().map_or(0, |w| d.workbench_level(w));
                    match level {
                        0 => tile.set_type_index(0x19),
                        1 => tile.set_type_index(0x21),
                        2 => tile.set_type_index(0x22),
                        3 => tile.set_type_index(0x23),
                        4 => tile.set_type_index(0x24),
                        5 => tile.set_type_index(0x25),
                        _ => {}
                    }
                }
            } else {
                tile.set_contents(0x2f);
            }
        }
        if tile_is_air(Some(tile)) {
            self.wake_fluid_around_air_tile(pos);
        }
        d.update_sun_light_for_tile_at_pos_world(&tile, pos);
        d.world_changed_at_pos_send_reliably(pos, true);
    }
}

/// `tile->typeIndex == 0x10` in `remoteGatherRequest`'s `loadGatherBlockAtPos` condition.
const TILE_ROCK_CRYSTAL: u8 = TILE_TIME_CRYSTAL;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::helpers::testutil::{dw_obj, new_obj, FakeDw};
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::{BHServer, DynamicWorld, WorldTileLoader};
    use std::rc::Rc;

    pub(super) fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// A world whose macro blocks (0..=2, 0..=2) are loaded and filled with air (back wall air too).
    pub(super) fn air_world(rec: &Rc<testkit::Rec>) -> (Rc<World>, Rc<FakeDw>) {
        let w = World::alloc(32);
        let fake = Rc::new(FakeDw::default());
        *w.dynamic_world.borrow_mut() = Some(Rc::new(DynamicWorld::new(Box::new(fake.clone()))));
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
        *rec.world.borrow_mut() = Some(Rc::downgrade(&w));
        rec.load_makes_valid.set(true);
        super::super::blocks::install_ops_hooks(&w);
        for my in 0..3 {
            for mx in 0..3 {
                let mt = w.macro_tiles().get(my * 32 + mx).unwrap();
                w.load_physical_block_for_macro_tile_at_x_y_load_surrounding_blocks_create_if_not_created(
                    &mt, mx as i32, my as i32, false, true,
                );
                let pb = mt.physical_block().unwrap();
                for i in 0..1024 {
                    pb.tile_ref(i).update(|t| {
                        t.type_index = TILE_AIR;
                        t.back_wall_type_index = TILE_AIR;
                    });
                }
            }
        }
        rec.take();
        (w, fake)
    }

    pub(super) fn at(w: &World, x: i32, y: i32) -> TileRef {
        tile_at_world_position_loaded(x, y, w).unwrap()
    }

    /// T1: particle alpha table.
    #[test]
    fn particle_alpha_table() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let t = at(&w, 40, 40);
        for ty in 0..=80u8 {
            t.update(|x| {
                x.type_index = ty;
                x.contents = 0;
                x.foreground_contents = 0;
            });
            let expect = matches!(ty, 1 | 3..=20 | 24 | 26..=30 | 32 | 48..=59 | 67..=77);
            assert_eq!(particle_color_alpha_is_visible(Some(t)), expect, "type {}", ty);
        }
        assert!(!particle_color_alpha_is_visible(None));
        // bush / plant / trunk contents on an air tile
        t.update(|x| {
            x.type_index = 2;
            x.contents = 0x07; // trunk
        });
        assert!(particle_color_alpha_is_visible(Some(t)));
        t.update(|x| {
            x.contents = 0x03; // crown => bush
        });
        assert!(particle_color_alpha_is_visible(Some(t)));
        t.update(|x| {
            x.contents = 0;
            x.foreground_contents = 0x43; // plant
        });
        assert!(particle_color_alpha_is_visible(Some(t)));
        install_deps(Rc::new(NullDeps));
    }

    /// T2.
    #[test]
    fn workbench_item_table() {
        let f = item_type_for_workbench_type_and_level;
        assert_eq!(f(WorkbenchType(1), 3), ItemType(137));
        assert_eq!(f(WorkbenchType(13), 0), ItemType(134));
        assert_eq!(f(WorkbenchType(3), 0), ItemType(15));
        assert_eq!(f(WorkbenchType(24), 9), ItemType(221));
        assert_eq!(f(WorkbenchType(26), 9), ItemType(270));
        assert_eq!(f(WorkbenchType(29), 9), ItemType(300));
        assert_eq!(f(WorkbenchType(99), 9), ItemType(0));
        assert!(
            workbench_type_is_double_height_static(WorkbenchType(26))
                && !workbench_type_is_double_height_static(WorkbenchType(29))
        );
    }

    /// The numeric fill table of -E matches the item names of the generated enum.
    #[test]
    fn fill_table_ids_match_names() {
        assert_eq!(ItemType::ITEM_DIRT.0, 1048);
        assert_eq!(ItemType::ITEM_FREEZE_WATER.0, I_FREEZE_WATER);
        assert_eq!(ItemType::ITEM_MELT_WATER.0, I_MELT_WATER);
        assert_eq!(ItemType::ITEM_ICE.0, I_ICE);
        assert_eq!(ItemType::ITEM_BASALT_UNMINED.0, I_BASALT_UNMINED);
        assert_eq!(ItemType::ITEM_WATER_BUCKET.0, I_WATER_BUCKET);
        assert_eq!(ItemType::ITEM_BOAT.0, I_BOAT);
        assert_eq!(ItemType::ITEM_ELEVATOR_SHAFT.0, I_ELEVATOR_SHAFT);
        assert_eq!(ItemType::ITEM_WATER.0, I_WATER);
    }

    /// T7: block fill table rows.
    #[test]
    fn fill_block_types() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let rows: [(u32, u8); 12] = [
            (1048, 6),
            (1062, 0x30),
            (1051, 7),
            (1075, 0x3a),
            (1024, 0xa),
            (1026, 0xb),
            (1094, 0x46),
            (1049, 9),
            (1057, 0x20),
            (1042, 0x18),
            (1066, 0x35),
            (1102, 0x4b),
        ];
        for (item, tile_type) in rows {
            let (x, y) = (40, 40);
            let t = at(&w, x, y);
            t.update(|v| {
                *v = Default::default();
                v.type_index = TILE_AIR;
                v.back_wall_type_index = TILE_AIR;
            });
            let _ = rec.take();
            w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType(item));
            let tv = t.get();
            assert_eq!(tv.type_index, tile_type, "item {}", item);
            assert_eq!(tv.partial_content_left, 0xff);
            assert_eq!(tv.gather_progress, 0);
            // the new block extends its back wall: air back wall takes the block type
            if item != 1057 {
                assert_eq!(tv.back_wall_type_index, tile_type, "bw item {}", item);
            }
            let log = rec.take();
            assert!(log.contains(&"worldChanged(40,40)true".to_string()), "{:?}", log);
            assert!(log.iter().any(|l| l == "sunLightRemoved(40,40)"), "{:?}", log);
            // solid with air above: snow surface wake-up
            assert!(log.contains(&"loadSnowSurface(40,41)false".to_string()), "{:?}", log);
        }
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn fill_conflicts_and_special_cases() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let (x, y) = (40, 40);
        let t = at(&w, x, y);
        // solid target: free block instead
        t.set_type_index(6);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_DIRT);
        let log = rec.take();
        assert_eq!(log, vec!["freeBlock(40,40) type=1048 a=0 b=0 hovers=false snd=0 bh=false"]);
        assert_eq!(t.type_index(), 6);
        // freeze water onto a solid tile is a conflict that returns silently (melt water is exempt from the solid test)
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_FREEZE_WATER);
        assert!(rec.take().is_empty());
        // non-fill types do nothing at all
        t.set_type_index(TILE_AIR);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_FLINT);
        assert!(rec.take().is_empty());
        // boat not on water => free block
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_BOAT);
        assert_eq!(rec.take(), vec!["freeBlock(40,40) type=111 a=0 b=0 hovers=false snd=0 bh=false"]);
        // boat on water => placeBoat
        t.set_type_index(TILE_WATER);
        w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
            Some(&t),
            ip(x, y),
            ItemType::ITEM_BOAT,
            0,
            0,
            Some("c1"),
            None,
            None,
        );
        assert_eq!(rec.take()[0], "placeBoat(40,40) client=Some(\"c1\")");
        // sowable: i16 conversions of dataA / dataB, no deref of the tile
        w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
            None,
            ip(x, y),
            ItemType::ITEM_APPLE,
            0xffff,
            5,
            None,
            None,
            None,
        );
        assert_eq!(rec.take(), vec!["sow(40,40) type=21 h=-1 g=5"]);
        // torch
        t.update(|v| {
            v.type_index = TILE_AIR;
            v.foreground_contents = 0;
        });
        w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
            Some(&t),
            ip(x, y),
            ItemType::ITEM_BASIC_TORCH,
            1,
            2,
            None,
            None,
            None,
        );
        assert_eq!(t.foreground_contents(), 0x31);
        let log = rec.take();
        assert_eq!(log[0], "addTorch(40,40) type=17 a=1 b=2 client=None");
        assert!(log.contains(&"contentsChanged(40,40)".to_string()), "{:?}", log);
        // ladder / column / window
        t.set_foreground_contents(0);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_LADDER);
        assert_eq!(t.foreground_contents(), 0x42);
        t.set_foreground_contents(0);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_STONE_COLUMN);
        assert_eq!(t.foreground_contents(), 0x64);
        t.set_foreground_contents(0);
        t.set_background_contents(0);
        t.set_back_wall_type_index(6);
        rec.take();
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_WINDOW);
        assert_eq!(t.background_contents(), 0x45);
        let log = rec.take();
        assert!(log.contains(&"sunLight(40,40)".to_string()), "{:?}", log);
        install_deps(Rc::new(NullDeps));
    }

    /// T7: doors take the type of the block behind them; trapdoors of the neighbour.
    #[test]
    fn doors_and_trapdoors() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let (x, y) = (40, 40);
        // stone above the door pair (x, y+2): the door pair takes its type (cobblestone 0xa)
        at(&w, x, y + 2).set_type_index(0xa);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_DOOR);
        assert_eq!(t.type_index(), 0xa);
        assert_eq!(at(&w, x, y + 1).type_index(), 0xa);
        let log = rec.take();
        assert!(log.contains(&"addDoor(40,40) type=52 client=None".to_string()), "{:?}", log);
        assert!(log.contains(&"worldChanged(40,41)true".to_string()), "{:?}", log);
        // nothing behind: wooden door tiles
        let (x, y) = (50, 40);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_DOOR);
        assert_eq!((t.type_index(), at(&w, x, y + 1).type_index()), (9, 9));
        // iron door with steel block above takes 0x39; without => iron block 0x38
        let (x, y) = (60, 40);
        at(&w, x, y + 2).set_type_index(0x39);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_IRON_DOOR);
        assert_eq!((t.type_index(), at(&w, x, y + 1).type_index()), (0x39, 0x39));
        let (x, y) = (70, 40);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_IRON_DOOR);
        assert_eq!((t.type_index(), at(&w, x, y + 1).type_index()), (0x38, 0x38));
        // trapdoor: right neighbour first
        let (x, y) = (45, 50);
        at(&w, x + 1, y).set_type_index(6);
        at(&w, x - 1, y).set_type_index(0xa);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_TRAPDOOR);
        assert_eq!(t.type_index(), 6);
        let (x, y) = (55, 50);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_TRAPDOOR);
        assert_eq!(t.type_index(), 9);
        // iron trapdoor only accepts steel / carbon / titanium neighbours
        let (x, y) = (65, 50);
        at(&w, x + 1, y).set_type_index(6);
        at(&w, x - 1, y).set_type_index(0x44);
        let t = at(&w, x, y);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_IRON_TRAPDOOR);
        assert_eq!(t.type_index(), 0x44);
        install_deps(Rc::new(NullDeps));
    }

    /// T7: water bucket / melt / freeze / displaced water.
    #[test]
    fn water_handling() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let (x, y) = (40, 40);
        let t = at(&w, x, y);
        // bucket: ice if cold, basalt if lava below, else water
        let temp = {
            let season = crate::world::climate::season_for_world_x(x, 0.0, Some(&w));
            let day = w.get_day_night_fraction_for_x_at_world_time(x as f32, 0.0);
            let wf = w.get_weather_fraction_for_pos(ip(x, y));
            crate::world::climate::current_temperature_for_tile_at_world_pos(
                &t.get(),
                ip(x, y),
                season,
                day,
                wf,
                Some(&w),
            )
        };
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_WATER_BUCKET);
        assert_eq!(t.type_index(), 3);
        let log = rec.take();
        assert!(temp >= 0.0);
        {
            assert!(log.contains(&"waterChanged(40,40)true".to_string()), "{:?}", log);
            // right, left, below neighbours that are air get a surface block, in that order
            let idx: Vec<_> = ["loadSurface(41,40)", "loadSurface(39,40)", "loadSurface(40,39)"]
                .iter()
                .map(|s| log.iter().position(|l| l == s))
                .collect();
            assert!(idx.iter().all(|i| i.is_some()) && idx[0] < idx[1] && idx[1] < idx[2], "{:?}", log);
        }
        // lava below with a warm tile: basalt unmined
        let (x, y) = (50, 40);
        let t = at(&w, x, y);
        at(&w, x, y - 1).set_type_index(TILE_LAVA);
        {
            w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_WATER_BUCKET);
            assert_eq!(t.type_index(), TILE_BASALT_UNMINED);
            rec.take();
        }
        // displaced water: pour dirt onto water, the water moves up into air
        let (x, y) = (60, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = TILE_WATER;
            v.partial_content_left = 100;
        });
        // a water tile is not solid: dirt can be placed
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_DIRT);
        assert_eq!(t.type_index(), 6);
        let above = at(&w, x, y + 1);
        assert_eq!((above.type_index(), above.partial_content_left()), (TILE_WATER, 100));
        let log = rec.take();
        assert!(log.contains(&"waterChanged(60,41)true".to_string()), "{:?}", log);
        // water onto ice swap needs no sun update
        let (x, y) = (70, 40);
        let t = at(&w, x, y);
        t.update(|v| v.type_index = TILE_ICE);
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_FREEZE_WATER);
        // ice is solid => conflict => silently returned
        assert_eq!(t.type_index(), TILE_ICE);
        // melt water: tile becomes water, keeps its partial content, no worldChanged from removeTile
        let (x, y) = (80, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = TILE_AIR;
            v.partial_content_left = 77;
            v.contents = 0;
        });
        w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType::ITEM_MELT_WATER);
        assert_eq!((t.type_index(), t.partial_content_left()), (TILE_WATER, 77));
        install_deps(Rc::new(NullDeps));
    }

    /// T8: removeTile.
    #[test]
    fn remove_tile_rules() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let rm = |x: i32, y: i32, cc: i32, fc: i32, oc: bool, ofg: bool| {
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(
                x, y, cc, fc, None, oc, ofg,
            );
        };
        // dirt tile with dirt back wall: tile becomes air, back wall air, free blocks first (fg then contents)
        let (x, y) = (40, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = 6;
            v.back_wall_type_index = 6;
            v.contents = 0;
        });
        rm(x, y, 1, 2, false, false);
        let log = rec.take();
        assert_eq!(log[0], "freeBlockForTile(40,40) fg=true bh=false");
        assert_eq!(log[1], "freeBlockForTile(40,40) fg=true bh=false");
        assert_eq!(log[2], "freeBlockForTile(40,40) fg=false bh=false");
        assert_eq!(t.type_index(), TILE_AIR);
        assert_eq!(t.back_wall_type_index(), TILE_AIR);
        assert!(
            log.contains(&"worldChanged(40,40)true".to_string()) && log.contains(&"sunLight(40,40)".to_string()),
            "{:?}",
            log
        );
        // natural stone types: no back wall logic
        let (x, y) = (50, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = 1;
            v.back_wall_type_index = 1;
        });
        rm(x, y, 1, 0, false, false);
        assert_eq!(t.type_index(), TILE_AIR);
        assert_eq!(t.back_wall_type_index(), 1);
        // protected types keep their type; contents cleared
        for keep in [5u8, 3, 0x1f, 0x19, 0x21, 0x25] {
            let (x, y) = (60, 40);
            let t = at(&w, x, y);
            t.update(|v| {
                v.type_index = keep;
                v.contents = 9;
                v.partial_content_left = 3;
            });
            rm(x, y, 0, 0, false, false);
            assert_eq!((t.type_index(), t.contents()), (keep, 0), "keep {}", keep);
        }
        // onlyRemoveContents keeps the block; onlyRemoveForeground clears only the foreground
        let (x, y) = (70, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = 6;
            v.contents = 5;
            v.foreground_contents = 7;
        });
        rm(x, y, 0, 0, true, false);
        assert_eq!((t.type_index(), t.contents(), t.foreground_contents()), (6, 0, 7));
        t.set_contents(5);
        rm(x, y, 0, 0, false, true);
        assert_eq!((t.type_index(), t.contents(), t.foreground_contents()), (6, 5, 0));
        // elevator shaft contents survive
        t.update(|v| v.contents = 0x67);
        rm(x, y, 0, 0, false, false);
        assert_eq!(t.contents(), 0x67);
        // time crystal: no free block, accounting through the crystal manager (cc sounds)
        let (x, y) = (80, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = TILE_TIME_CRYSTAL;
            v.contents = 0;
        });
        rec.take();
        rm(x, y, 2, 0, false, false);
        let log = rec.take();
        assert!(!log.iter().any(|l| l.starts_with("freeBlock")), "{:?}", log);
        assert_eq!(log[0], "crystal.setCountWatcher(nil)");
        assert_eq!(
            log[1],
            format!(
                "crystal.modify(-2,{})",
                string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", 1000 + 2 + 73))
            )
        );
        assert_eq!(log.iter().filter(|l| l.starts_with("crystalSound")).count(), 2);
        // door tiles: no free blocks
        let (x, y) = (90, 40);
        let t = at(&w, x, y);
        t.update(|v| {
            v.type_index = 9;
            v.background_contents = 0;
        });
        rec.take();
        install_deps(Rc::new(NullDeps));
    }

    pub(super) fn server_world(rec: &Rc<testkit::Rec>) -> (Rc<World>, Rc<FakeDw>) {
        let (w, fake) = air_world(rec);
        *w.server.borrow_mut() = Some(Rc::new(BHServer { api: None }));
        (w, fake)
    }

    pub(super) fn remove_payload(x: u32, y: u32, cc: u8, fc: u8, water: u8, oc: u8, ofg: u8, z: u8) -> Vec<u8> {
        let mut b = vec![0u8; 0x18];
        b[8..12].copy_from_slice(&x.to_le_bytes());
        b[12..16].copy_from_slice(&y.to_le_bytes());
        b[0x10] = cc;
        b[0x11] = fc;
        b[0x12] = water;
        b[0x13] = oc;
        b[0x14] = ofg;
        b[0x15] = z;
        b
    }

    /// W9: remove request: index return value, protection, zIndex resync.
    #[test]
    fn remote_remove_request() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let t = at(&w, 40, 40);
        t.update(|v| {
            v.type_index = 6;
            v.back_wall_type_index = 6;
        });
        let idx = w.remote_remove_request_from_client(&remove_payload(40, 40, 1, 0, 0, 0, 0, 1), Some("bob"));
        assert_eq!(idx, 40 * 32 * 32 + 40);
        assert_eq!(t.type_index(), TILE_AIR);
        let log = rec.take();
        assert!(log.contains(&"updatePlayer(bob)".to_string()), "{:?}", log);
        assert!(log.iter().filter(|l| *l == "worldChanged(40,40)true").count() >= 2, "{:?}", log);
        // protected (ADMIN_ONLY): ignored, the index is still returned
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 1;
        w.custom_rules.set(cr);
        t.set_type_index(6);
        let idx = w.remote_remove_request_from_client(&remove_payload(40, 40, 1, 0, 0, 0, 0, 0), Some("bob"));
        assert_eq!(idx, 40 * 1024 + 40);
        assert_eq!(t.type_index(), 6);
        // admin passes
        rec.admin.borrow_mut().push("root".into());
        w.remote_remove_request_from_client(&remove_payload(40, 40, 0, 0, 0, 0, 0, 1), Some("root"));
        assert_eq!(t.type_index(), TILE_AIR);
        // water removal flag
        t.update(|v| {
            v.type_index = TILE_WATER;
            v.partial_content_left = 9;
        });
        rec.take();
        w.remote_remove_request_from_client(&remove_payload(40, 40, 0, 0, 1, 0, 0, 1), Some("root"));
        assert_eq!((t.type_index(), t.partial_content_left()), (TILE_AIR, 0));
        assert!(rec.take().contains(&"waterChanged(40,40)true".to_string()));
        // short payload reads as zeros (UB-PARITY: no panic)
        let _ = w.remote_remove_request_from_client(&[], Some("root"));
        install_deps(Rc::new(NullDeps));
    }

    pub(super) fn fill_payload(x: u32, y: u32, ty: u16, a: u16, b: u16, z: u8) -> Vec<u8> {
        let mut v = vec![0u8; 16];
        v[0..4].copy_from_slice(&x.to_le_bytes());
        v[4..8].copy_from_slice(&y.to_le_bytes());
        v[8..10].copy_from_slice(&ty.to_le_bytes());
        v[10..12].copy_from_slice(&a.to_le_bytes());
        v[12..14].copy_from_slice(&b.to_le_bytes());
        v[14] = z;
        v
    }

    #[test]
    fn remote_fill_request() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        rec.alias.borrow_mut().insert("bob".into(), "Bobby".into());
        let idx = w.remote_fill_request_placed_by_client(&fill_payload(40, 41, 1048, 0, 0, 0), Some("bob"));
        assert_eq!(idx, 41 * 1024 + 40);
        assert_eq!(at(&w, 40, 41).type_index(), 6);
        // protected: rejected unless train car / boat
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 1;
        w.custom_rules.set(cr);
        w.remote_fill_request_placed_by_client(&fill_payload(40, 42, 1048, 0, 0, 0), Some("bob"));
        assert_eq!(at(&w, 40, 42).type_index(), TILE_AIR);
        rec.take();
        w.remote_fill_request_placed_by_client(&fill_payload(40, 42, 204, 0, 0, 1), Some("bob"));
        let log = rec.take();
        // zIndex == 1 from a non-admin => resync; the handcar is placed in the protected area
        assert!(log.contains(&"updatePlayer(bob)".to_string()), "{:?}", log);
        // sow: shortcut; tile None is fine
        w.remote_fill_request_placed_by_client(&fill_payload(40, 5000, 21, 3, 4, 0), None);
        install_deps(Rc::new(NullDeps));
    }

    /// W7: workbench placement.
    #[test]
    fn workbench_placement() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        rec.alias.borrow_mut().insert("bob".into(), "Bobby".into());
        // campfire on free air
        let t = at(&w, 40, 40);
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType::WORKBENCH_CAMPFIRE,
            ip(40, 40),
            None,
            Some("bob"),
            None,
            Some("Bobby"),
        );
        assert_eq!(t.contents(), 0x2f);
        let log = rec.take();
        assert_eq!(
            log,
            vec!["worldChanged(40,40)true", "workbenchPlaced(40,40) type=3 client=Some(\"bob\") name=Some(\"Bobby\")"]
        );
        // occupied tile => the item drops instead
        t.set_contents(5);
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType::WORKBENCH_CAMPFIRE,
            ip(40, 40),
            None,
            None,
            None,
            None,
        );
        assert_eq!(rec.take(), vec!["freeBlock(40,40) type=15 a=0 b=0 hovers=false snd=0 bh=false"]);
        // basic portal is double height: needs the tile above to be free; marks both sprites
        t.set_contents(0);
        at(&w, 40, 41).set_contents(0);
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType::WORKBENCH_BASIC_PORTAL,
            ip(40, 40),
            None,
            None,
            None,
            None,
        );
        assert_eq!((t.contents(), at(&w, 40, 41).contents()), (0x2f, 0x2f));
        rec.take();
        // placed portal: the base tile must be free, the object sits one tile higher
        let base = at(&w, 50, 40);
        base.set_contents(0);
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType::WORKBENCH_PLACED_PORTAL,
            ip(50, 40),
            None,
            None,
            None,
            None,
        );
        assert_eq!(at(&w, 50, 41).contents(), 0x2f);
        assert!(rec.take().iter().any(|l| l.starts_with("workbenchPlaced(50,41)")));
        // other types: sprite 0x2e
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType(2),
            ip(60, 40),
            None,
            None,
            None,
            None,
        );
        assert_eq!(at(&w, 60, 40).contents(), 0x2e);
        // tile above a double-height bench is solid => item with the saved level (level 3 portal -> 137)
        at(&w, 70, 41).set_type_index(1);
        let mut d = crate::foundation::gsdict::GsDict::new();
        d.insert("level".to_string(), PlistValue::int(3));
        let sd = PlistValue::dict(d);
        rec.take();
        w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
            WorkbenchType::WORKBENCH_BASIC_PORTAL,
            ip(70, 40),
            Some(&sd),
            None,
            None,
            None,
        );
        assert_eq!(rec.take(), vec!["freeBlock(70,40) type=137 a=0 b=0 hovers=false snd=0 bh=false"]);
        install_deps(Rc::new(NullDeps));
    }

    /// W7: interaction objects.
    #[test]
    fn interaction_object_placement() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        let chest = InventoryItem::new(ItemType::ITEM_CHEST, 0, 0, None, None);
        let t = at(&w, 40, 40);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&chest.borrow(), ip(40, 40), None, Some("bob"), None, None);
        assert_eq!(t.contents(), 0x30);
        let log = rec.take();
        assert_eq!(
            log,
            vec!["ioPlaced(40,40) type=1043 flipped=false client=Some(\"bob\")", "worldChanged(40,40)true"]
        );
        // occupied: free block, no state change
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&chest.borrow(), ip(40, 40), None, None, None, None);
        assert_eq!(rec.take(), vec!["freeBlock(40,40) type=1043 a=0 b=0 hovers=false snd=0 bh=false"]);
        // a known interaction object on the tile also blocks
        fake.io.borrow_mut().insert((50, 40), dw_obj(1043, 2, 0, false, true));
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&chest.borrow(), ip(50, 40), None, None, None, None);
        assert!(rec.take()[0].starts_with("freeBlock(50,40)"));
        // bed: two blocks wide, right side free => not flipped, neighbour to the right gets contents 0x30
        let bed = InventoryItem::new(ItemType::ITEM_WOOD_BED, 0, 0, None, None);
        at(&w, 61, 39).set_foreground_contents(0x64);
        at(&w, 60, 39).set_foreground_contents(0x64);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&bed.borrow(), ip(60, 40), None, None, None, None);
        assert_eq!((at(&w, 60, 40).contents(), at(&w, 61, 40).contents()), (0x30, 0x30));
        assert!(rec.take()[0].contains("flipped=false"));
        // right side blocked => flipped to the left
        at(&w, 71, 40).set_type_index(6);
        at(&w, 70, 39).set_foreground_contents(0x64);
        at(&w, 69, 39).set_foreground_contents(0x64);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&bed.borrow(), ip(70, 40), None, None, None, None);
        assert_eq!((at(&w, 70, 40).contents(), at(&w, 69, 40).contents()), (0x30, 0x30));
        assert!(rec.take()[0].contains("flipped=true"));
        // train station: five worldChanged calls in the documented order
        let ts = InventoryItem::new(ItemType::ITEM_TRAIN_STATION, 0, 0, None, None);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&ts.borrow(), ip(90, 40), None, None, None, None);
        let log = rec.take();
        let wc: Vec<&String> = log.iter().filter(|l| l.starts_with("worldChanged")).collect();
        assert_eq!(
            wc,
            vec![
                "worldChanged(90,40)true",
                "worldChanged(89,40)true",
                "worldChanged(91,40)true",
                "worldChanged(90,41)true",
                "worldChanged(89,41)true",
                "worldChanged(91,41)true"
            ]
        );
        assert_eq!(at(&w, 90, 40).foreground_contents(), 0x62);
        assert_eq!(at(&w, 89, 40).foreground_contents(), 0x62);
        // mirror marks two tiles
        let mirror = InventoryItem::new(ItemType::ITEM_MIRROR, 0, 0, None, None);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&mirror.borrow(), ip(100, 40), None, None, None, None);
        assert_eq!((at(&w, 100, 40).contents(), at(&w, 100, 41).contents()), (0x30, 0x30));
        install_deps(Rc::new(NullDeps));
    }

    pub(super) fn gather_payload(x: u32, y: u32, g: u8) -> Vec<u8> {
        let mut v = vec![0u8; 16];
        v[0..4].copy_from_slice(&x.to_le_bytes());
        v[4..8].copy_from_slice(&y.to_le_bytes());
        v[8] = g;
        v
    }

    /// W8: gather: 32 rand() draws for visible tiles, loadGatherBlock rules.
    #[test]
    fn gather_request_rng_and_load() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let next_after = |n: usize| {
            crate::rng::glibc_rand::srand(5);
            for _ in 0..n {
                crate::rng::glibc_rand::rand();
            }
            crate::rng::glibc_rand::rand()
        };
        // dirt (visible, not "minable with pickaxe"): 32 draws, gather block loaded
        let t = at(&w, 40, 40);
        t.update(|v| {
            v.type_index = 6;
            v.gather_progress = 3;
        });
        crate::rng::glibc_rand::srand(5);
        w.remote_gather_request(&gather_payload(40, 40, 9));
        assert_eq!(t.gather_progress(), 9);
        assert_eq!(crate::rng::glibc_rand::rand(), next_after(32));
        let log = rec.take();
        assert_eq!(log, vec!["loadGather(40,40)"]);
        // lower progress: nothing (no draws)
        crate::rng::glibc_rand::srand(5);
        w.remote_gather_request(&gather_payload(40, 40, 4));
        assert_eq!(t.gather_progress(), 9);
        assert_eq!(crate::rng::glibc_rand::rand(), next_after(0));
        rec.take();
        // air tile with no contents: progress unchanged, still loadGather
        let a = at(&w, 41, 40);
        crate::rng::glibc_rand::srand(5);
        w.remote_gather_request(&gather_payload(41, 40, 9));
        assert_eq!(a.gather_progress(), 0);
        assert_eq!(rec.take(), vec!["loadGather(41,40)"]);
        assert_eq!(crate::rng::glibc_rand::rand(), next_after(0));
        // rock (minable): draws, but no loadGather; time crystal always loads
        let r = at(&w, 42, 40);
        r.set_type_index(1);
        crate::rng::glibc_rand::srand(5);
        w.remote_gather_request(&gather_payload(42, 40, 1));
        assert_eq!(rec.take(), Vec::<String>::new());
        assert_eq!(crate::rng::glibc_rand::rand(), next_after(32));
        let c = at(&w, 43, 40);
        c.set_type_index(TILE_TIME_CRYSTAL);
        w.remote_gather_request(&gather_payload(43, 40, 1));
        assert_eq!(rec.take(), vec!["loadGather(43,40)"]);
        // portal base stone (alpha 0): no draws
        let p = at(&w, 44, 40);
        p.set_type_index(0x19);
        crate::rng::glibc_rand::srand(5);
        w.remote_gather_request(&gather_payload(44, 40, 1));
        assert_eq!(crate::rng::glibc_rand::rand(), next_after(0));
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn gather_notification_broadcast() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let t = at(&w, 40, 40);
        t.set_gather_progress(7);
        // no server clients: nothing
        w.send_gather_notification_for_tile_at_pos(&t, ip(40, 40));
        assert!(rec.take().is_empty());
        let mut m: crate::foundation::gsdict::GsDict<String, Rc<crate::world::state::ServerClient>> =
            crate::foundation::gsdict::GsDict::new();
        m.insert("a".into(), Rc::new(crate::world::state::ServerClient {}));
        *w.server_clients.borrow_mut() = Some(Rc::new(std::cell::RefCell::new(m)));
        w.send_gather_notification_for_tile_at_pos(&t, ip(40, 40));
        let mut expect = vec![0x1au8, 40, 0, 0, 0, 40, 0, 0, 0, 7];
        expect.extend_from_slice(&[0u8; 7]);
        assert_eq!(rec.take(), vec![format!("send({:?},None,true)", expect)]);
        install_deps(Rc::new(NullDeps));
    }

    /// W7: paint.
    #[test]
    fn paint_faces_and_objects() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        let t = at(&w, 40, 40);
        t.set_type_index(6);
        for (face, col) in [(0u8, 11u16), (1, 12), (2, 13), (3, 14), (4, 15), (9, 16)] {
            w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), col, face, None);
        }
        let v = t.get();
        assert_eq!((v.paint_left, v.paint_right, v.paint_top, v.paint_bot, v.paint_front), (12, 13, 14, 15, 16));
        assert_eq!(rec.take().iter().filter(|l| *l == "worldChanged(40,40)true").count(), 6);
        w.remove_paint_at_tile_at_pos_face_index_paint_blockhead(&t, ip(40, 40), 1, None);
        assert_eq!(t.get().paint_left, 0);
        // column: painted through the column object, the tile is untouched
        t.set_foreground_contents(0x64);
        fake.column.borrow_mut().insert((40, 40), dw_obj(222, 0, 0, true, true));
        rec.take();
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 20, 0, None);
        assert_eq!(t.get().paint_front, 16);
        assert!(rec.take().is_empty());
        // gate fails for a blockhead that may not remove it => falls through to the face paint
        fake.column.borrow_mut().insert((40, 40), dw_obj(222, 0, 0, true, true));
        let bh = new_obj();
        // `Rec` answers canBeRemovedByBlockhead with the trait default (true): done
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 21, 0, Some(&bh));
        assert_eq!(t.get().paint_front, 16);
        install_deps(Rc::new(NullDeps));
    }

    /// Repair tool.
    #[test]
    fn repair_tile() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        w.start_portal_pos.set(ip(40, 40));
        // ordinary tile: contents removed, becomes air
        let t = at(&w, 60, 60);
        t.update(|v| {
            v.type_index = 6;
            v.contents = 9;
            v.foreground_contents = 4;
            v.paint_front = 3;
            v.on_fire = 1;
        });
        w.do_repair_for_tile_at_pos(ip(60, 60));
        let v = t.get();
        assert_eq!((v.type_index, v.contents, v.foreground_contents, v.paint_front, v.on_fire), (TILE_AIR, 0, 0, 0, 0));
        let log = rec.take();
        assert_eq!(log.last().unwrap(), "worldChanged(60,60)true");
        assert!(log.contains(&"sunLight(60,60)".to_string()));
        // unmined natural stone survives
        let t = at(&w, 61, 60);
        t.update(|v| {
            v.type_index = 1;
            v.contents = 9;
        });
        w.do_repair_for_tile_at_pos(ip(61, 60));
        assert_eq!(t.type_index(), 1);
        // start portal column: the tile below the portal is the base stone; the two portal tiles get the sprite
        let below = at(&w, 40, 39);
        w.do_repair_for_tile_at_pos(ip(40, 39));
        assert_eq!(below.type_index(), 0x19);
        w.do_repair_for_tile_at_pos(ip(40, 41));
        assert_eq!(at(&w, 40, 41).contents(), 0x2f);
        // no server: nothing happens
        let rec2 = testkit::install();
        let (w2, _) = air_world(&rec2);
        let t2 = at(&w2, 60, 60);
        t2.set_type_index(6);
        w2.do_repair_for_tile_at_pos(ip(60, 60));
        assert_eq!(t2.type_index(), 6);
        install_deps(Rc::new(NullDeps));
    }

    /// UB-PARITY: fill / paint / gather-notification on a missing tile die like the original (here: panic without
    /// `faithful-crashes`; with it the test would raise SIGSEGV, so it is gated off).
    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    #[should_panic(expected = "UB-PARITY")]
    fn fill_on_null_tile_is_ub() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        w.fill_tile_at_pos_with_type(None, ip(1, 5000), ItemType::ITEM_DIRT);
    }
}

/// Broader scenarios: mod hooks, protection, crystals, fluids, every fill item in random surroundings, paint
/// objects and the start portal repair.
#[cfg(test)]
mod more_tests {
    use super::tests::{air_world, at, fill_payload, ip, remove_payload, server_world};
    use super::*;
    use crate::dynworld::state::testutil::ModSpy;
    use crate::foundation::gzip::gzip_deflate;
    use crate::plist::xml_write::write_xml;
    use crate::world::helpers::testutil::{dw_obj, new_obj};
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use std::rc::Rc;

    fn protect(w: &World) {
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.world_modifcation = 1;
        w.custom_rules.set(cr);
    }

    fn back_wall_payload(id: u64, x: u32, y: u32) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend_from_slice(&x.to_le_bytes());
        v.extend_from_slice(&y.to_le_bytes());
        v
    }

    fn gz(entries: Vec<(&str, PlistValue)>) -> Vec<u8> {
        let mut d = crate::foundation::gsdict::GsDict::new();
        for (k, v) in entries {
            d.insert(k.to_string(), v);
        }
        gzip_deflate(&write_xml(&PlistValue::dict(d)))
    }

    #[test]
    fn back_wall_requests() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        assert!(!w.should_be_crystal_block_at_pos(ip(1, 1)));
        let spy = Rc::new(ModSpy::default());
        crate::mods::install(Some(spy.clone()));
        *rec.bh_by_id.borrow_mut() = Some(new_obj());
        let t = at(&w, 40, 40);
        t.set_back_wall_type_index(6);
        // a cancelling mod keeps the wall
        spy.cancel.set(true);
        let idx = w.remote_remove_back_wall_request_from_client(&back_wall_payload(0, 40, 40), Some("bob"));
        assert_eq!(idx, 40 * 1024 + 40);
        assert_eq!(t.back_wall_type_index(), 6);
        // a mod that lets it through, with a blockhead
        spy.cancel.set(false);
        w.remote_remove_back_wall_request_from_client(&back_wall_payload(7, 40, 40), Some("bob"));
        assert_eq!(t.back_wall_type_index(), TILE_AIR);
        assert!(spy.log.borrow().iter().any(|l| l.starts_with("break(bob,40,40,true,")));
        crate::mods::install(None);
        // protected tiles are ignored
        protect(&w);
        t.set_back_wall_type_index(6);
        w.remote_remove_back_wall_request_from_client(&back_wall_payload(0, 40, 40), Some("bob"));
        assert_eq!(t.back_wall_type_index(), 6);
        // background contents of a tile go to a free block
        t.set_background_contents(0x45);
        rec.take();
        w.remove_any_background_contents_for_tile_at_pos_remove_blockhead(&t, ip(40, 40), None);
        assert_eq!(t.background_contents(), 0);
        assert!(rec.take().contains(&"bgFreeBlock(40,40)".to_string()));
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn remove_request_with_mods() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let spy = Rc::new(ModSpy::default());
        crate::mods::install(Some(spy.clone()));
        let t = at(&w, 40, 40);
        t.set_type_index(6);
        spy.cancel.set(true);
        w.remote_remove_request_from_client(&remove_payload(40, 40, 1, 0, 0, 0, 0, 0), Some("bob"));
        assert_eq!(t.type_index(), 6);
        spy.cancel.set(false);
        w.remote_remove_request_from_client(&remove_payload(40, 40, 1, 0, 0, 0, 0, 0), Some("bob"));
        assert_eq!(t.type_index(), TILE_AIR);
        // fills through a mod
        spy.cancel.set(true);
        w.remote_fill_request_placed_by_client(&fill_payload(40, 40, 1048, 0, 0, 0), Some("bob"));
        assert_eq!(t.type_index(), TILE_AIR);
        spy.cancel.set(false);
        w.remote_fill_request_placed_by_client(&fill_payload(40, 40, 1048, 0, 0, 0), Some("bob"));
        assert_eq!(t.type_index(), 6);
        crate::mods::install(None);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn time_crystal_mining_paths() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let bh = new_obj();
        let t = at(&w, 40, 40);
        let mine = |cc: i32, bh: Option<&ObjRef>| {
            t.update(|v| {
                v.type_index = TILE_TIME_CRYSTAL;
                v.contents = 0;
            });
            rec.take();
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(40, 40, cc, 0, bh);
            rec.take()
        };
        // a net blockhead's crystal is not accounted here
        rec.net_bh.set(true);
        let log = mine(2, Some(&bh));
        assert!(!log.iter().any(|l| l.starts_with("crystal") || l.starts_with("freeBlock")), "{:?}", log);
        rec.net_bh.set(false);
        // a server controlled blockhead only adds to the discrepancy
        rec.controlled_bh.set(true);
        let log = mine(3, Some(&bh));
        assert!(log.contains(&"crystalDiscrepency(3)".to_string()), "{:?}", log);
        rec.controlled_bh.set(false);
        // a local blockhead is accounted and checked for warping
        let log = mine(2, Some(&bh));
        assert!(log.iter().any(|l| l.starts_with("crystal.modify")), "{:?}", log);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn fluids_wake_up_around_removed_tiles() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let water = |x: i32, y: i32, amount: u8| {
            at(&w, x, y).update(|v| {
                v.type_index = TILE_WATER;
                v.partial_content_left = amount;
            });
        };
        let remove = |x: i32, y: i32| {
            at(&w, x, y).set_type_index(6);
            rec.take();
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(x, y, 0, 0, None);
            rec.take()
        };
        water(40, 51, 9);
        assert!(remove(40, 50).contains(&"loadSurface(40,50)".to_string()));
        water(44, 50, 3);
        assert!(remove(45, 50).contains(&"loadSurface(45,50)".to_string()));
        water(51, 50, 3);
        assert!(remove(50, 50).contains(&"loadSurface(50,50)".to_string()));
        // water below the air tile: snow surface
        water(60, 49, 9);
        assert!(remove(60, 50).contains(&"loadSnowSurface(60,50)false".to_string()));
        // removing water itself wakes the neighbours too
        water(70, 51, 9);
        at(&w, 70, 50).update(|v| {
            v.type_index = TILE_WATER;
            v.partial_content_left = 4;
        });
        rec.take();
        w.remove_water_tile_at_pos(ip(70, 50));
        assert!(rec.take().contains(&"loadSurface(70,50)".to_string()));
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn back_walls_follow_removed_blocks_downwards() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let setup = |x: i32, top_bw: u8, top_ty: u8, lower: &[(u8, u8)]| {
            let t = at(&w, x, 40);
            t.update(|v| {
                v.type_index = top_ty;
                v.back_wall_type_index = top_bw;
            });
            at(&w, x, 41).update(|v| {
                v.type_index = TILE_AIR;
                v.back_wall_type_index = TILE_AIR;
            });
            for (i, (ty, bw)) in lower.iter().enumerate() {
                at(&w, x, 39 - i as i32).update(|v| {
                    v.type_index = *ty;
                    v.back_wall_type_index = *bw;
                });
            }
            rec.take();
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(x, 40, 0, 0, None);
            (t, rec.take())
        };
        // stone with a wall: wall removed, air tiles below with walls lose them as well
        let (t, log) = setup(40, 6, 6, &[(TILE_AIR, 6), (TILE_AIR, 6), (6, 6)]);
        assert_eq!(t.back_wall_type_index(), TILE_AIR);
        assert_eq!(at(&w, 40, 39).back_wall_type_index(), TILE_AIR);
        assert_eq!(at(&w, 40, 38).back_wall_type_index(), TILE_AIR);
        assert!(log.contains(&"worldChanged(40,39)true".to_string()), "{:?}", log);
        // a background object on those tiles is dropped
        at(&w, 41, 39).set_background_contents(0x45);
        let (_, log) = setup(41, 6, 6, &[(TILE_AIR, 6), (6, 6)]);
        assert!(log.contains(&"bgFreeBlock(41,39)".to_string()), "{:?}", log);
        // an immutable wall below stops it
        let (_, _) = setup(42, 6, 6, &[(TILE_AIR, 1)]);
        assert_eq!(at(&w, 42, 39).back_wall_type_index(), 1);
        // a platform tile stops it
        let (_, _) = setup(43, 0x20, TILE_WOODEN_PLATFORM, &[(TILE_AIR, 6)]);
        assert_eq!(at(&w, 43, 39).back_wall_type_index(), 6);
        // a platform below is not air-like
        let (_, _) = setup(44, 6, 6, &[(TILE_WOODEN_PLATFORM, 6)]);
        // air below with the air wall
        let (_, _) = setup(45, 6, 6, &[(TILE_AIR, TILE_AIR)]);
        // the top of the world: no tile above
        let t = at(&w, 46, 95);
        t.update(|v| {
            v.type_index = 6;
            v.back_wall_type_index = 6;
        });
        w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(46, 95, 0, 0, None);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn workbench_requests() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        rec.alias.borrow_mut().insert("bob".into(), "Bobby".into());
        let spy = Rc::new(ModSpy::default());
        let req = |x: u32, y: u32, ty: u8, plist: Option<Vec<u8>>| {
            let mut v = vec![0u8; 0x10];
            v[0..4].copy_from_slice(&x.to_le_bytes());
            v[4..8].copy_from_slice(&y.to_le_bytes());
            v[8] = ty;
            if let Some(p) = plist {
                v.extend(p);
            }
            v
        };
        // plain request with a trailing plist and an inventory change
        let inv = gz(vec![
            ("saveDict", PlistValue::dict(crate::foundation::gsdict::GsDict::new())),
            ("blockhead_inventory_change", PlistValue::data(vec![1u8, 2, 3])),
            ("blockhead_uniqueID", PlistValue::int(5)),
        ]);
        w.remote_place_workbench_request_from_peer(&req(40, 40, 3, Some(inv)), Some("bob"));
        assert_eq!(at(&w, 40, 40).contents(), 0x2f);
        // no plist at all, a short plist and no client
        w.remote_place_workbench_request_from_peer(&req(41, 40, 3, None), None);
        w.remote_place_workbench_request_from_peer(&req(42, 40, 3, Some(vec![1, 2, 3])), Some("bob"));
        w.remote_place_workbench_request_from_peer(&req(43, 40, 3, Some(gz(vec![("x", PlistValue::int(1))]))), None);
        // mods
        crate::mods::install(Some(spy.clone()));
        spy.cancel.set(true);
        w.remote_place_workbench_request_from_peer(&req(44, 40, 3, None), Some("bob"));
        assert_eq!(at(&w, 44, 40).contents(), 0);
        spy.cancel.set(false);
        w.remote_place_workbench_request_from_peer(&req(44, 40, 3, None), Some("bob"));
        assert_eq!(at(&w, 44, 40).contents(), 0x2f);
        crate::mods::install(None);
        // protection
        protect(&w);
        w.remote_place_workbench_request_from_peer(&req(45, 40, 3, None), Some("bob"));
        assert_eq!(at(&w, 45, 40).contents(), 0);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn workbench_placement_blockers() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let place = |t: WorkbenchType, x: i32, y: i32| {
            w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                t, ip(x, y), None, None, None, None,
            );
        };
        // the placed portal needs a free base
        at(&w, 40, 40).set_contents(5);
        place(WorkbenchType::WORKBENCH_PLACED_PORTAL, 40, 40);
        at(&w, 41, 40).set_type_index(6);
        place(WorkbenchType::WORKBENCH_PLACED_PORTAL, 41, 40);
        // an interaction object on the spot
        let (_, fake) = (0, ());
        let _ = fake;
        // a double height bench with an occupied tile above
        at(&w, 50, 41).set_contents(3);
        place(WorkbenchType::WORKBENCH_BASIC_PORTAL, 50, 40);
        install_deps(Rc::new(NullDeps));
    }

    fn io_request(x: u32, y: u32, item: Option<ItemType>, extra: Vec<(&str, PlistValue)>) -> Vec<u8> {
        let mut v = vec![0u8; 8];
        v[0..4].copy_from_slice(&x.to_le_bytes());
        v[4..8].copy_from_slice(&y.to_le_bytes());
        let mut e = extra;
        if let Some(it) = item {
            let item = InventoryItem::new(it, 0, 0, None, None);
            e.push(("item", PlistValue::data(item.borrow().save_data())));
        }
        if !e.is_empty() {
            v.extend(gz(e));
        }
        v
    }

    #[test]
    fn interaction_object_requests() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        rec.alias.borrow_mut().insert("bob".into(), "Bobby".into());
        let inv = vec![
            ("blockhead_inventory_change", PlistValue::data(vec![1u8, 2, 3])),
            ("blockhead_uniqueID", PlistValue::int(5)),
            ("saveDict", PlistValue::dict(crate::foundation::gsdict::GsDict::new())),
        ];
        w.remote_place_interaction_object_request_from_peer(
            &io_request(40, 40, Some(ItemType::ITEM_CHEST), inv),
            Some("bob"),
        );
        assert_eq!(at(&w, 40, 40).contents(), 0x30);
        // nothing in the plist: an empty item
        w.remote_place_interaction_object_request_from_peer(&io_request(41, 40, None, vec![]), None);
        w.remote_place_interaction_object_request_from_peer(&io_request(42, 40, None, vec![("a", PlistValue::int(1))]), None);
        // an ownership sign needs an admin
        w.remote_place_interaction_object_request_from_peer(
            &io_request(43, 40, Some(ItemType::ITEM_OWNERSHIP_SIGN), vec![]),
            Some("bob"),
        );
        assert_eq!(at(&w, 43, 40).contents(), 0);
        rec.admin.borrow_mut().push("bob".into());
        w.remote_place_interaction_object_request_from_peer(
            &io_request(43, 40, Some(ItemType::ITEM_OWNERSHIP_SIGN), vec![]),
            Some("bob"),
        );
        // mods
        let spy = Rc::new(ModSpy::default());
        crate::mods::install(Some(spy.clone()));
        spy.cancel.set(true);
        w.remote_place_interaction_object_request_from_peer(
            &io_request(44, 40, Some(ItemType::ITEM_CHEST), vec![]),
            Some("bob"),
        );
        assert_eq!(at(&w, 44, 40).contents(), 0);
        crate::mods::install(None);
        protect(&w);
        rec.admin.borrow_mut().clear();
        w.remote_place_interaction_object_request_from_peer(
            &io_request(45, 40, Some(ItemType::ITEM_CHEST), vec![]),
            Some("bob"),
        );
        assert_eq!(at(&w, 45, 40).contents(), 0);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn interaction_object_blockers() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let place = |it: ItemType, x: i32, y: i32| {
            let item = InventoryItem::new(it, 0, 0, None, None);
            let r = w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&item.borrow(), ip(x, y), None, None, None, None);
            rec.take();
            r
        };
        // trade portal: solid base, blocked above
        at(&w, 40, 40).set_type_index(6);
        place(ItemType::ITEM_TRADE_PORTAL, 40, 40);
        place(ItemType::ITEM_TRADE_PORTAL, 41, 40);
        at(&w, 52, 42).set_contents(3);
        place(ItemType::ITEM_TRADE_PORTAL, 52, 40);
        place(ItemType::ITEM_MIRROR, 53, 41);
        at(&w, 54, 42).set_contents(3);
        place(ItemType::ITEM_MIRROR, 54, 41);
        // two blocks wide: both neighbours blocked, the left one solid or with contents
        let bed = ItemType::ITEM_WOOD_BED;
        at(&w, 61, 40).set_type_index(6);
        at(&w, 59, 40).set_type_index(6);
        place(bed, 60, 40);
        at(&w, 59, 40).set_type_index(TILE_AIR);
        at(&w, 59, 40).set_contents(4);
        place(bed, 60, 40);
        at(&w, 59, 40).set_contents(0);
        // left side free but nothing to stand on
        place(bed, 60, 40);
        // foreground occupation
        at(&w, 70, 40).set_foreground_contents(0x30);
        place(ItemType::ITEM_CHEST, 70, 40);
        place(bed, 70, 40);
        place(ItemType::ITEM_TRAIN_STATION, 70, 40);
        // train station: every neighbour blocking
        for (dx, dy) in [(-1, 0), (-1, 1), (0, 1), (1, 0), (1, 1)] {
            at(&w, 80 + dx, 40 + dy).set_contents(3);
            place(ItemType::ITEM_TRAIN_STATION, 80, 40);
            at(&w, 80 + dx, 40 + dy).set_contents(0);
        }
        at(&w, 79, 40).set_foreground_contents(0x30);
        place(ItemType::ITEM_TRAIN_STATION, 80, 40);
        at(&w, 79, 40).set_foreground_contents(0);
        at(&w, 81, 40).set_foreground_contents(0x30);
        place(ItemType::ITEM_TRAIN_STATION, 80, 40);
        install_deps(Rc::new(NullDeps));
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 33
        }
        fn pick<T: Copy>(&mut self, v: &[T]) -> T {
            v[(self.next() % v.len() as u64) as usize]
        }
    }

    fn scramble(w: &World, r: &mut Lcg, cx: i32, cy: i32) {
        const TYPES: [u8; 22] = [2, 2, 2, 3, 4, 6, 6, 1, 0x1f, 0x10, 0x39, 0x45, 0x44, 0x19, 0x1b, 0x1c, 0x31, 0x32, 0x30, 0x20, 0x33, 9];
        const CONTENTS: [u8; 8] = [0, 0, 0, 0x30, 0x65, 0x67, 0x03, 0x07];
        const FG: [u8; 8] = [0, 0, 0, 0x64, 0x30, 0x42, 0x43, 0x62];
        const BG: [u8; 5] = [0, 0, 0, 0x45, 0x5f];
        const BW: [u8; 9] = [2, 2, 3, 5, 6, 1, 0x20, 0x1f, 0x31];
        for dy in -3..=4 {
            for dx in -2..=2 {
                let (ty, c, f, b, bw) = (r.pick(&TYPES), r.pick(&CONTENTS), r.pick(&FG), r.pick(&BG), r.pick(&BW));
                let pl = r.pick(&[0u8, 3, 9, 255]);
                at(w, cx + dx, cy + dy).update(|t| {
                    *t = Default::default();
                    t.type_index = ty;
                    t.contents = c;
                    t.foreground_contents = f;
                    t.background_contents = b;
                    t.back_wall_type_index = bw;
                    t.partial_content_left = pl;
                });
            }
        }
    }

    #[test]
    fn fill_every_item_in_random_surroundings() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let mut r = Lcg(7);
        for round in 0..30 {
            for ty in 0..1110u32 {
                scramble(&w, &mut r, 40, 40);
                let tile = at(&w, 40, 40);
                if round % 3 == 0 {
                    tile.set_type_index(TILE_AIR);
                    tile.set_contents(0);
                    tile.set_foreground_contents(0);
                    tile.set_background_contents(0);
                }
                let da = (r.next() % 3) as u16;
                w.fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_client_name(
                    Some(&tile),
                    ip(40, 40),
                    ItemType(ty),
                    da,
                    0,
                    Some("bob"),
                    None,
                    Some("Bobby"),
                );
                rec.take();
            }
        }
        // the water bucket in the cold
        w.world_time.set(0.0);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn remove_tiles_in_random_surroundings() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let bh = new_obj();
        let mut r = Lcg(11);
        for i in 0..4000 {
            scramble(&w, &mut r, 40, 40);
            let flags = r.next();
            w.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents_send_world_changed_notifcation_dont_remove_contents(
                40,
                40,
                (flags % 3) as i32,
                ((flags >> 2) % 3) as i32,
                if flags & 16 != 0 { Some(&bh) } else { None },
                flags & 32 != 0,
                flags & 64 != 0,
                flags & 128 != 0,
                flags & 256 != 0,
            );
            if i % 7 == 0 {
                w.remove_back_wall_at_pos_remove_blockhead(ip(40, 40), None);
                w.do_repair_for_tile_at_pos(ip(40, 40));
            }
            rec.take();
        }
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn paint_objects_and_gates() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        let bh = new_obj();
        let t = at(&w, 40, 40);
        // a ladder through the ladder object
        t.update(|v| {
            *v = Default::default();
            v.type_index = TILE_AIR;
            v.foreground_contents = 0x42;
        });
        fake.ladder.borrow_mut().insert((40, 40), dw_obj(53, 0, 0, true, true));
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 3, 0, Some(&bh));
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 3, 0, None);
        // an interaction object that cannot be painted, then one that can
        t.update(|v| {
            v.foreground_contents = 0x30;
        });
        fake.io.borrow_mut().insert((40, 40), dw_obj(1043, 2, 0, false, true));
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 4, 1, None);
        rec.paintable.set(true);
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 4, 1, None);
        t.update(|v| {
            v.foreground_contents = 0;
            v.contents = 0x30;
        });
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 4, 1, Some(&bh));
        // a blockhead that may not remove the object
        rec.cannot_remove.set(true);
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 5, 2, Some(&bh));
        rec.cannot_remove.set(false);
        // stairs and shafts
        t.update(|v| {
            v.contents = 0x65;
        });
        fake.stairs.borrow_mut().insert((40, 40), dw_obj(229, 0, 0, true, true));
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 6, 3, None);
        t.update(|v| {
            v.contents = 0x67;
        });
        fake.shaft.borrow_mut().insert((40, 40), dw_obj(1087, 0, 0, true, true));
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 6, 3, None);
        // missing object with a blockhead falls through to the face paint
        t.update(|v| {
            v.contents = 0x65;
        });
        fake.stairs.borrow_mut().clear();
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 7, 4, Some(&bh));
        // paint requests
        let paint = |id: u64, x: u32, y: u32, c: u16, f: u8| {
            let mut v = id.to_le_bytes().to_vec();
            v.extend_from_slice(&x.to_le_bytes());
            v.extend_from_slice(&y.to_le_bytes());
            v.extend_from_slice(&c.to_le_bytes());
            v.push(f);
            v
        };
        *rec.bh_by_id.borrow_mut() = Some(bh.clone());
        w.remote_paint_request_from_client(&paint(0, 41, 40, 9, 1), Some("bob"));
        w.remote_paint_request_from_client(&paint(3, 41, 40, 9, 1), Some("bob"));
        assert_eq!(at(&w, 41, 40).get().paint_left, 9);
        protect(&w);
        w.remote_paint_request_from_client(&paint(0, 42, 40, 9, 1), Some("bob"));
        assert_eq!(at(&w, 42, 40).get().paint_left, 0);
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn repair_restores_the_start_portal_levels() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        w.start_portal_pos.set(ip(40, 40));
        let below = at(&w, 40, 39);
        // no workbench yet: placed through the stub and level 0
        w.do_repair_for_tile_at_pos(ip(40, 39));
        assert_eq!(below.type_index(), 0x19);
        *rec.wb.borrow_mut() = Some(new_obj());
        for (level, ty) in [(1, 0x21u8), (2, 0x22), (3, 0x23), (4, 0x24), (5, 0x25), (6, 0x19)] {
            rec.wb_level.set(level);
            below.set_type_index(6);
            w.do_repair_for_tile_at_pos(ip(40, 39));
            assert_eq!(below.type_index(), ty, "level {level}");
        }
        // the workbench type decides a conflicting block
        rec.wb_type.set(1);
        at(&w, 41, 40).set_type_index(6);
        let item = InventoryItem::new(ItemType::ITEM_DIRT, 0, 0, None, None);
        let _ = item;
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn requests_with_and_without_server_client_and_mods() {
        for server in [false, true] {
            for hooked in [false, true] {
                for client in [None, Some("bob")] {
                    let rec = testkit::install();
                    let (w, _) = if server { server_world(&rec) } else { air_world(&rec) };
                    rec.alias.borrow_mut().insert("bob".into(), "Bobby".into());
                    if hooked {
                        crate::mods::install(Some(Rc::new(ModSpy::default())));
                    }
                    let t = at(&w, 40, 40);
                    t.update(|v| {
                        v.type_index = 6;
                        v.back_wall_type_index = 6;
                    });
                    w.remote_remove_back_wall_request_from_client(&back_wall_payload(0, 40, 40), client);
                    w.remote_remove_request_from_client(&remove_payload(40, 40, 1, 0, 0, 0, 0, 1), client);
                    w.remote_fill_request_placed_by_client(&fill_payload(40, 40, 1048, 0, 0, 1), client);
                    let mut with_plist = fill_payload(41, 40, 1048, 0, 0, 0);
                    with_plist.extend(write_xml(&PlistValue::dict({
                        let mut d = crate::foundation::gsdict::GsDict::new();
                        d.insert("placeSaveDict".to_string(), PlistValue::int(3));
                        d.insert("blockhead_inventory_change".to_string(), PlistValue::data(vec![1u8]));
                        d
                    })));
                    w.remote_fill_request_placed_by_client(&with_plist, client);
                    let mut paint = back_wall_payload(0, 40, 40);
                    paint.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0, 0]);
                    w.remote_paint_request_from_client(&paint, client);
                    w.remote_place_workbench_request_from_peer(
                        &{
                            let mut v = vec![0u8; 0x10];
                            v[0..4].copy_from_slice(&45u32.to_le_bytes());
                            v[4..8].copy_from_slice(&40u32.to_le_bytes());
                            v[8] = 3;
                            v
                        },
                        client,
                    );
                    w.remote_place_interaction_object_request_from_peer(
                        &io_request(46, 40, Some(ItemType::ITEM_CHEST), vec![]),
                        client,
                    );
                    w.send_gather_notification_for_tile_at_pos(&t, ip(40, 40));
                    crate::mods::install(None);
                    install_deps(Rc::new(NullDeps));
                }
            }
        }
    }

    #[test]
    fn edges_of_the_loaded_world() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        // missing tiles
        w.remove_back_wall_at_pos_remove_blockhead(ip(40, 5000), None);
        w.remove_water_tile_at_pos(ip(40, 5000));
        w.update_tile_graphic_for_workbench_of_type_at_pos_level(WorkbenchType(3), ip(40, 5000), 0);
        w.do_repair_for_tile_at_pos(ip(40, 5000));
        // a solid tile keeps its back wall
        at(&w, 40, 40).set_type_index(6);
        at(&w, 40, 40).set_back_wall_type_index(6);
        w.remove_back_wall_at_pos_remove_blockhead(ip(40, 40), None);
        assert_eq!(at(&w, 40, 40).back_wall_type_index(), 6);
        let place = |t: WorkbenchType, x: i32, y: i32| {
            w.place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(
                t, ip(x, y), None, None, None, None,
            );
        };
        // placed portal at the bottom edge: no base tile; a workbench off the loaded world; a double height one at the top
        place(WorkbenchType::WORKBENCH_PLACED_PORTAL, 40, -1);
        place(WorkbenchType(3), 40, 5000);
        place(WorkbenchType::WORKBENCH_BASIC_PORTAL, 50, 1023);
        // an interaction object on the workbench spot
        fake.io.borrow_mut().insert((60, 40), dw_obj(1043, 2, 0, false, true));
        place(WorkbenchType(3), 60, 40);
        // an interaction object that is a sign on an occupied foreground, rails, torches
        let place_item = |it: ItemType, x: i32, y: i32| {
            let item = InventoryItem::new(it, 0, 0, None, None);
            w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&item.borrow(), ip(x, y), None, None, None, None);
            rec.take();
        };
        place_item(ItemType::ITEM_TRADE_PORTAL, 70, -1);
        at(&w, 71, 40).set_foreground_contents(0x30);
        place_item(ItemType::ITEM_SIGN, 71, 40);
        place_item(ItemType::ITEM_OWNERSHIP_SIGN, 71, 40);
        // a two blocks wide bed at the west edge of the world, blocked on the right
        at(&w, 1, 0).set_type_index(6);
        place_item(ItemType::ITEM_WOOD_BED, 0, 0);
        // the same where the tile on the left is in a block that is not loaded
        rec.load_makes_valid.set(false);
        at(&w, 65, 40).set_type_index(6);
        unload(&w, 1, 1);
        place_item(ItemType::ITEM_WOOD_BED, 64, 40);
        rec.load_makes_valid.set(true);
        // a boat or a stairs item on a sign tile
        install_deps(Rc::new(NullDeps));
    }

    fn unload(w: &World, mx: usize, my: usize) {
        *w.macro_tiles().get(my * 32 + mx).unwrap().physical_block.borrow_mut() = None;
    }

    #[test]
    fn fill_edges_climate_and_displaced_water() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        let fill = |it: u32, x: i32, y: i32| {
            let t = at(&w, x, y);
            w.fill_tile_at_pos_with_type(Some(&t), ip(x, y), ItemType(it));
            rec.take();
        };
        // a bucket high up in the cold freezes
        let t = at(&w, 40, 1000);
        t.update(|v| {
            v.type_index = TILE_AIR;
            v.sun_light = 255;
        });
        fill(I_WATER_BUCKET, 40, 1000);
        assert_eq!(t.type_index(), TILE_ICE);
        // a bucket in the warm onto lava turns into basalt, onto the ground into water
        let t = at(&w, 40, 40);
        at(&w, 40, 39).set_type_index(TILE_LAVA);
        fill(I_WATER_BUCKET, 40, 40);
        assert_eq!(t.type_index(), TILE_BASALT_UNMINED);
        t.set_type_index(TILE_AIR);
        at(&w, 40, 39).set_type_index(6);
        fill(I_WATER_BUCKET, 40, 40);
        assert_eq!(t.type_index(), TILE_WATER);
        // a plank of ice
        at(&w, 41, 40).set_type_index(TILE_AIR);
        at(&w, 41, 39).set_type_index(TILE_LAVA);
        fill(I_ICE, 41, 40);
        assert_eq!(at(&w, 41, 40).type_index(), TILE_BASALT_UNMINED);
        // an interaction object of type 0x32 blocks any fill
        fake.io.borrow_mut().insert((50, 40), crate::world::helpers::testutil::dw_obj_of_type(0x32));
        fill(1048, 50, 40);
        assert_eq!(at(&w, 50, 40).type_index(), TILE_AIR);
        // dirt onto water pushes the water up, into water above too
        let t = at(&w, 60, 40);
        t.update(|v| {
            v.type_index = TILE_WATER;
            v.partial_content_left = 200;
        });
        at(&w, 60, 41).update(|v| {
            v.type_index = TILE_WATER;
            v.partial_content_left = 100;
        });
        fill(1048, 60, 40);
        assert_eq!(t.type_index(), 6);
        assert_eq!(at(&w, 60, 41).partial_content_left(), 255);
        assert_eq!(at(&w, 60, 42).type_index(), TILE_WATER);
        // doors at the edges of the world
        for ty in [52u32, 164, 69, 165] {
            for y in [0, 1022, 1023, 40] {
                at(&w, 70, y).set_type_index(TILE_AIR);
                fill(ty, 70, y);
            }
        }
        // a door whose neighbours are not loaded
        rec.load_makes_valid.set(false);
        unload(&w, 1, 1);
        for ty in [69u32, 165] {
            at(&w, 64, 40).set_type_index(TILE_AIR);
            fill(ty, 64, 40);
        }
        unload(&w, 2, 1);
        rec.load_makes_valid.set(true);
        install_deps(Rc::new(NullDeps));
    }

    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    #[should_panic(expected = "UB-PARITY")]
    fn interaction_object_on_a_null_tile_is_ub() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        let item = InventoryItem::new(ItemType::ITEM_CHEST, 0, 0, None, None);
        w.place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&item.borrow(), ip(1, 5000), None, None, None, None);
    }

    #[cfg(not(feature = "faithful-crashes"))]
    #[test]
    #[should_panic(expected = "UB-PARITY")]
    fn freezing_water_on_a_null_tile_is_ub() {
        let rec = testkit::install();
        let (w, _) = air_world(&rec);
        w.fill_tile_at_pos_with_type(None, ip(1, 5000), ItemType::ITEM_FREEZE_WATER);
    }

    #[test]
    fn gather_paint_and_repair_edges() {
        let rec = testkit::install();
        let (w, fake) = server_world(&rec);
        // no tile
        w.remote_gather_request(&super::tests::gather_payload(40, 5000, 3));
        // a sign-like interaction tile without an object
        let t = at(&w, 40, 40);
        t.update(|v| {
            v.foreground_contents = 0x30;
        });
        w.paint_tile_at_pos_color_index_face_index_paint_blockhead(&t, ip(40, 40), 4, 1, None);
        let _ = &fake;
        // repair far from the start portal
        w.start_portal_pos.set(ip(40, 40));
        w.do_repair_for_tile_at_pos(ip(40, 30));
        // the platform of a train station set up by hand
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn gather_payload_edge_cases() {
        let rec = testkit::install();
        let (w, _) = server_world(&rec);
        let spy = fill_payload(40, 40, 1048, 0, 0, 0);
        let _ = spy;
        // the same item on a workbench tile that is a basic portal conflicts
        let t = at(&w, 40, 40);
        t.update(|v| {
            v.type_index = TILE_AIR;
            v.contents = 0x2f;
        });
        *rec.wb.borrow_mut() = Some(new_obj());
        rec.wb_type.set(1);
        w.fill_tile_at_pos_with_type(Some(&t), ip(40, 40), ItemType::ITEM_DIRT);
        assert_eq!(t.type_index(), TILE_AIR);
        install_deps(Rc::new(NullDeps));
    }
}
