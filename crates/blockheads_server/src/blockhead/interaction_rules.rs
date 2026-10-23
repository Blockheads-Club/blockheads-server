//! Blockhead interaction rules: `interactionTypeForTile:...`, `canDigBackWall...`, tool tables, the good/bad preview,
//! gather speed, the `updatePosition:` override, workbench / interaction-object accessors, `stopRiding`,
//! `pickupDynamicObject:`.
//!
//!
//! ## Seam
//! Calls that `blockhead_core` already routes through [`BlockheadHost`] (dynamic-world lookups, door switching,
//! achievements, `fullyLoadIfNeededAroundPos`,...) are made through `host()`; the few `DynamicWorld` calls that seam
//! lacks (`freeBlocksExistAtPos:`, `blockheadAtPos:`, `checkForTrainCarUnderTap:`, glow blocks, torches, treasure
//! chests, `worldContentsChangedAtPos:`,...) go through [`ActionsHost`], defined here with the same
//! pattern (inert defaults, installed once with [`set_actions_host`] by the integrator). Everything that is reachable
//! on the concrete `World` (tiles, flags, `markCircumNavigateX:`,...) is called directly.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::building::bed::{Bed, BED_CLASS};
use crate::fmath::cconv::f32_to_i32;
use crate::interaction::chest::Chest;
use crate::items::inventory_item::ItemRef;
use crate::items::misc::{workbench_requires_electricty, workbench_requires_fuel};
use crate::items::placement::sign_connection_type_and_offset_for_pos;
use crate::items::predicates::*;
use crate::objects::base::base_update_position;
use crate::objects::handle::{downcast, is_kind_of_class, ptr_eq, ObjRef};
use crate::types::enums::{InteractionType, SignConnectionType, TileType};
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, InteractionTestResult, Vector2};
use crate::world::helpers::*;
use crate::world::loot::item_type_from_tile_is_foregorund;
use crate::world::state::{TileRef, World};
use crate::world::tiles::{macro_tile_at_world_postion, tile_at_world_position_loaded};

use super::action::{action_of, ActionRef};
use super::blockhead::{host, kf, va, Blockhead};
use crate::crafting::workbench::Workbench;

// ====================================================================================================
// The seam (what BlockheadHost lacks)
// ====================================================================================================

crate::hooks::hook_trait! {
/// `DynamicWorld` calls used by this module that `BlockheadHost` does not carry. Every method has an inert default (as
/// in the sibling seams); the integrator installs a real adapter with [`set_actions_host`].
#[allow(unused_variables)]
pub trait ActionsHost, stub ActionsHostStub {
    /// `-[DynamicWorld freeBlocksExistAtPos:]`
    fn free_blocks_exist_at_pos(&self, b: &Blockhead, pos: IntPair) -> bool {
        false
    }
    /// `-[DynamicWorld blockheadAtPos:]`
    fn blockhead_at_pos(&self, b: &Blockhead, pos: IntPair) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld checkForTrainCarUnderTap:]`
    fn check_for_train_car_under_tap(&self, b: &Blockhead, tap: Vector2) -> Option<ObjRef> {
        None
    }
    /// `-[DynamicWorld interactionObjectTypeForObjectAtPos:]`
    fn interaction_object_type_for_object_at_pos(&self, b: &Blockhead, pos: IntPair) -> i32 {
        0
    }
    /// `-[[world dynamicWorld] loadGlowBlockIfNeededAtPos:tile:]`
    fn load_glow_block_if_needed_at_pos_tile(&self, b: &Blockhead, pos: IntPair, tile: &TileRef) {}
    /// `-[[world dynamicWorld] addTorchAtPos:ofType:dataA:0 dataB:0 saveDict:nil placedByClient:nil]`
    fn add_torch_at_pos_of_type(&self, b: &Blockhead, pos: IntPair, item_type: i32) {}
    /// `-[[world dynamicWorld] createTreasureChestOrTrollAtTile:atPos:loadTroll:loadTreasure:]`
    fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
        &self,
        b: &Blockhead,
        tile: &TileRef,
        pos: IntPair,
        load_troll: bool,
        load_treasure: bool,
    ) {
    }
    /// `-[DynamicWorld worldContentsChangedAtPos:]`
    fn world_contents_changed_at_pos(&self, b: &Blockhead, pos: IntPair) {}
    /// `-[DynamicWorld removeStandardObject:]`
    fn remove_standard_object(&self, b: &Blockhead, o: &ObjRef) {}
    /// `-[DynamicWorld allBlockheadsIncludingNet]`
    fn all_blockheads_including_net(&self, b: &Blockhead) -> Vec<ObjRef> {
        Vec::new()
    }
    /// `-[obj interactionRenderItemType]` (InteractionObject)
    #[provided]
    fn interaction_render_item_type(&self, o: &ObjRef) -> u32 {
        default_interaction_render_item_type(o)
    }
    /// `-[Donkey class]` membership test is concrete; `-[Blockhead blockheadCanRide:usingItem:]` is `ridable_*` below.
    /// `-[World queueBlockheadAIActionToTile:atPos:forBlockhead:]`'s receiver pre-conditions are the caller's.
    fn dynamic_world_is_client(&self) -> bool {
        false
    }
}
}

/// `-[InteractionObject interactionRenderItemType]` per subclass:
/// Bed -> its own item type; Chest -> `itemTypeFromChestType`; TradingPost / TradePortal -> gold coin; Sign -> sign;
/// OwnershipSign; TrainStation; Mirror; anything else -> `ITEM_CLOTHING_SLOT`.
pub fn default_interaction_render_item_type(o: &ObjRef) -> u32 {
    use crate::interaction::{
        mirror::Mirror, ownership_sign::OwnershipSign, sign::Sign, trade_portal::TradePortal, trading_post::TradingPost,
    };
    use crate::trains::train_station::TrainStation;
    use crate::types::enums::ChestType;
    if let Some(b) = downcast::<Bed>(o) {
        return b.item_type() as u32;
    }
    if let Some(c) = downcast::<Chest>(o) {
        return crate::world::loot::item_type_from_chest_type(ChestType(c.chest_type() as u32)).0;
    }
    if downcast::<TradingPost>(o).is_some() || downcast::<TradePortal>(o).is_some() {
        return 167;
    }
    if downcast::<OwnershipSign>(o).is_some() {
        return 301;
    }
    if downcast::<Sign>(o).is_some() {
        return 163;
    }
    if downcast::<TrainStation>(o).is_some() {
        return 200;
    }
    if downcast::<Mirror>(o).is_some() {
        return 331;
    }
    1
}

/// The default seam: every method inert.
pub struct NullActionsHost;
impl ActionsHostStub for NullActionsHost {}

thread_local! {
    static ACTIONS_HOST: std::cell::RefCell<Rc<dyn ActionsHost>> = std::cell::RefCell::new(Rc::new(NullActionsHost));
}

/// Installs the [`ActionsHost`] (once at startup; tests install recording hosts).
pub fn set_actions_host(h: Rc<dyn ActionsHost>) {
    ACTIONS_HOST.with(|c| *c.borrow_mut() = h);
}

/// The installed [`ActionsHost`].
pub fn actions_host() -> Rc<dyn ActionsHost> {
    ACTIONS_HOST.with(|c| c.borrow().clone())
}

// ====================================================================================================
// Helpers
// ====================================================================================================

/// `SPADE_TYPES` of: `{6, 0x1b, 0x1c, 7, 8, 0x46, 0x3a, 0x30, 0x31, 0x32}`.
pub(crate) fn spade_tile_types() -> [i32; 10] {
    [6, 0x1b, 0x1c, 7, 8, 0x46, 0x3a, 0x30, 0x31, 0x32]
}

/// The integer WRAPX of (`W = worldWidthMacro << 5`; `d >= W/2 -> d -= W`; `d < -W/2 -> d += W`; C `/ 2`).
pub(crate) fn wrap_x_distance(d: i32, world_width_macro: i32) -> i32 {
    let w = world_width_macro.wrapping_shl(5);
    let half = w / 2;
    if d >= half {
        d.wrapping_sub(w)
    } else if d < half.wrapping_neg() {
        d.wrapping_add(w)
    } else {
        d
    }
}

/// The comparator block of: `b < a -> Ascending (Less)`, otherwise `Descending (Greater)`, equal ids
/// give `Greater`.
pub(crate) fn freeblock_uid_desc(a: u64, b: u64) -> Ordering {
    if b < a {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

/// `sortedArrayUsingComparator:` with [`freeblock_uid_desc`]; a stable insertion sort that tolerates the comparator's
/// inconsistency for equal ids.
pub(crate) fn sort_freeblocks_desc(list: &mut Vec<ObjRef>) {
    let mut out: Vec<ObjRef> = Vec::with_capacity(list.len());
    for o in list.drain(..) {
        let id = o.unique_id();
        let mut at = out.len();
        while at > 0 && freeblock_uid_desc(out[at - 1].unique_id(), id) == Ordering::Greater {
            at -= 1;
        }
        out.insert(at, o);
    }
    *list = out;
}

/// `[item itemType]` with nil giving `ITEM_NONE`.
pub(crate) fn item_type_of(item: &Option<ItemRef>) -> ItemType {
    item.as_ref().map_or(ItemType(0), |i| i.borrow().item_type)
}

fn tat(world: &World, x: i32, y: i32) -> Option<TileRef> {
    tile_at_world_position_loaded(x, y, world)
}

/// `[o canBeUsedByBlockhead:b]` (nil or non-interaction object gives NO).
fn can_be_used_by(o: &Option<ObjRef>, b: &ObjRef) -> bool {
    match o {
        Some(o) => o.as_interaction_like().map_or(false, |i| i.can_be_used_by_blockhead(o, b)),
        None => false,
    }
}

/// `[o canBeRemovedByBlockhead:b]` (nil gives NO).
fn can_be_removed_by(o: &Option<ObjRef>, b: &ObjRef) -> bool {
    match o {
        Some(o) => o.can_be_removed_by_blockhead(o, b),
        None => false,
    }
}

fn tile_copy(t: &Option<TileRef>) -> Option<crate::types::tile::Tile> {
    t.as_ref().map(|t| t.get())
}

const WORKBENCH_PORTAL_ITEMS: [u32; 7] = [134, 210, 135, 138, 137, 136, 139];

fn is_portal_item(t: ItemType) -> bool {
    WORKBENCH_PORTAL_ITEMS.contains(&t.0)
}

/// FGSET of P1.
const FGSET: [u8; 21] = [
    0x31, 0x61, 0x32, 0x57, 0x66, 0x69, 0x52, 0x53, 0x54, 0x55, 0x56, 0x42, 0x64, 0x39, 0x37, 0x35, 0x33, 0x3b, 0x63,
    0x62, 0x58,
];

impl Blockhead {
    // ================================================================================================
    // 4.1 interactionTypeForTile
    // ================================================================================================

    /// `-interactionTypeForTile:atPos:item:pickupRejectedDueToInventoryFull:includeActions:faceIndex:allowProtectedActions:`
    /// One function with labelled blocks (trap 1): steps A-K of in order, every `tileAt` made in
    /// the specified order (block loads are observable).
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        tile: Option<&TileRef>,
        tile_pos: IntPair,
        item: Option<&ItemRef>,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> InteractionType {
        let none = InteractionType::INTERACTION_NONE;
        let Some(world) = self.world_rc() else { return none };
        let h = host();
        let ah = actions_host();
        let me = self.me();
        let allow = allow_protected_actions;
        let (x, y) = (tile_pos.x, tile_pos.y);
        let item_opt: Option<ItemRef> = item.cloned();
        let item_t = item_type_of(&item_opt);
        let has_item = item_opt.is_some();
        let count = |t: ItemType| self.count_of_inventory_items_of_type_include_actions(t, include_actions);
        let pickup = |t: ItemType| self.can_pick_up_item_of_type_sub_items(t, None);
        // sets *rejected for -1 and reports whether the pickup is possible
        let can_pickup = |t: ItemType, rejected: &mut bool| -> bool {
            let r = pickup(t);
            if r == 1 {
                return true;
            }
            if r == -1 {
                *rejected = true;
            }
            false
        };

        // ---- Step A: prelude
        let _ = tat(&world, x, y); // A1: result discarded on purpose (block load)
        if let Some(mt) = macro_tile_at_world_postion(x, y, &world.macro_tiles(), &world) {
            world.load_dynamic_objects_if_not_already_loaded_for_macro_tile_include_surface_blocks(&mt, true);
        }
        let Some(tref) = tile.copied() else { return none };
        if y > 0x3f9 {
            return none;
        }
        // A3: ghost plant
        if tile_is_plant(Some(tref)) && h.get_plant_at_pos(tile_pos).is_none() {
            tref.set_foreground_contents(0);
            ah.world_contents_changed_at_pos(self, tile_pos);
        }
        let t = || tref.get();

        // ---- Step B: free-block pickup on a non-solid tile
        if !tile_is_solid(Some(tref)) && self.freeblock_scan(&tref, tile_pos, rejected) {
            return InteractionType::INTERACTION_PICKUP_FREEBLOCK;
        }

        // ---- Step C: paint
        if has_item && allow && count(item_t) > 0 {
            if item_t.0 == 0x67 && tile_is_paintable(Some(tref), tile_pos, &world, Some(&me)) {
                return InteractionType::INTERACTION_PAINT;
            }
            if item_t.0 == 0x68 && tile_is_paintable(Some(tref), tile_pos, &world, Some(&me)) {
                return InteractionType::INTERACTION_STRIP_PAINT;
            }
        }

        // ---- Step D: interaction object on the foreground
        if t().foreground_contents == 0x30 {
            let o = h.interaction_object_at_pos(tile_pos);
            if can_be_used_by(&o, &me) {
                return InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT;
            }
            if o.as_ref().map_or(0, |o| o.object_type()) == 0x3c {
                return none;
            }
        }

        // ---- Step E: dodo egg pickup
        if allow && t().foreground_contents == 0x4e {
            let ty = item_type_from_tile_is_foregorund(tref, tile_pos, true, &world);
            if ty.0 == 0x5b {
                if let Some(egg) = h.egg_at_pos(tile_pos) {
                    let r = self.can_pick_up_item_of_type_sub_items_data_a_data_b(ty, None, 0, h.egg_breed(&egg));
                    if r == 1 {
                        return InteractionType::INTERACTION_PICKUP;
                    }
                    if r == -1 {
                        *rejected = true;
                    }
                }
            }
        }

        // ---- Step F: placement and use of the held item (G1-G7)
        if has_item {
            let ty = item_t;
            let c = count(ty);
            if c > 0 {
                // G1 burn
                if allow
                    && ty.0 == 13
                    && tile_is_burnable(Some(tref), world.dynamic_world().as_ref(), tile_pos)
                    && t().contents != 0x2e
                {
                    return InteractionType::INTERACTION_BURN;
                }
                // G2 train cars
                if item_type_is_train_car(ty) {
                    let mut can_place_train = false;
                    if t().foreground_contents == 0x62 {
                        let tap = Vector2 { x: ((x as f64) + 0.5) as f32, y: ((y as f64) + 1.5) as f32 };
                        can_place_train = ah.check_for_train_car_under_tap(self, tap).is_none();
                    } else if let Some(below) = tat(&world, x, y - 1) {
                        if below.get().foreground_contents == 0x62 {
                            let tap = Vector2 { x: ((x as f64) + 0.5) as f32, y: ((y as f64) + 0.5) as f32 };
                            can_place_train = ah.check_for_train_car_under_tap(self, tap).is_none();
                        }
                    }
                    if can_place_train {
                        let above = tat(&world, x, y + 1);
                        if above.is_some() && tile_is_air_or_snow(above) {
                            if ty.0 == 0xcc {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                            let a2 = tat(&world, x, y + 2);
                            if a2.is_some() && tile_is_air_or_snow(a2) {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                        }
                    }
                }
                // G3 full cage
                if item_type_is_full_cage(ty)
                    && tile_is_air_water_or_snow(Some(tref))
                    && t().foreground_contents == 0
                    && t().contents == 0
                {
                    return InteractionType::INTERACTION_PLACE_ITEM;
                }
                // G4 back-wall placement
                if allow {
                    let tl = t();
                    if item_type_is_placable_on_back_wall(ty)
                        && tl.back_wall_type_index != 2
                        && (tl.type_index != 3 || tl.partial_content_left != 0xff)
                        && item_type_occupies_background_contents(ty)
                        && tl.background_contents == 0
                        && tl.foreground_contents == 0
                        && tl.contents == 0
                    {
                        let rock_bw = matches!(tl.back_wall_type_index, 1 | 0x26 | 0x27 | 0x28 | 0x29);
                        if !(rock_bw && !item_type_can_be_placed_on_background_rock(ty)) && y < 0x3f8 {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                    }
                }
                // G5 sowing and bucket
                if allow {
                    if ty.0 == 0x90 {
                        if tile_is_solid(Some(tref)) {
                            let a = tat(&world, x, y + 1);
                            if tile_is_water(a) && tile_copy(&a).map_or(false, |a| a.foreground_contents == 0) {
                                return InteractionType::INTERACTION_SOW;
                            }
                        }
                    } else if ty.0 == 0x127 {
                        if item_type_is_sowable_in_tile_type(ty, TileType(t().type_index as u32))
                            && t().foreground_contents == 0
                        {
                            return InteractionType::INTERACTION_SOW;
                        }
                    } else if item_type_is_sowable_in_tile_type(ty, TileType(t().type_index as u32)) {
                        let a = tat(&world, x, y + 1);
                        if a.is_some() && tile_is_air_water_or_snow(a) {
                            let at = a.unwrap().get();
                            let v = if sowable_item_type_occupies_foreground(ty) {
                                at.foreground_contents
                            } else {
                                at.contents
                            };
                            if v == 0 {
                                return InteractionType::INTERACTION_SOW;
                            }
                        }
                        if tile_is_plant(a) && tile_is_air_water_or_snow(a) && sowable_item_type_occupies_foreground(ty)
                        {
                            let up = IntPair { x, y: y + 1 };
                            if h.get_plant_at_pos(up).is_none() {
                                a.unwrap().set_foreground_contents(0);
                                ah.world_contents_changed_at_pos(self, up);
                                return InteractionType::INTERACTION_SOW;
                            }
                        }
                    }
                    if ty.0 == 0x66 && tile_is_water(Some(tref)) && t().partial_content_left > 0x20 {
                        return InteractionType::INTERACTION_FILL_BUCKET;
                    }
                }
                // G6 boat
                if ty.0 == 0x6f {
                    if !tile_is_water(Some(tref)) {
                        if tile_is_water(tat(&world, x, y - 1)) && y < 0x3f8 {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                    } else if y < 0x3f8 {
                        return InteractionType::INTERACTION_PLACE_ITEM;
                    }
                }
                // G7 signs
                if allow && (ty.0 == 0xa3 || (ty.0 == 0x12d && world.is_admin())) {
                    if tile_is_solid(Some(tref))
                        && t().foreground_contents == 0
                        && sign_connection_type_and_offset_for_pos(tile_pos, None, &*world, SignConnectionType(0)).0
                            != 0
                        && y < 0x3f8
                    {
                        return InteractionType::INTERACTION_PLACE_ITEM;
                    }
                }
            }
        }

        // ---- Step G: solid-tile specials
        if allow && tile_is_solid(Some(tref)) {
            let tl = t();
            if item_t.0 == 0xb2 && (tl.contents == 0 || (tl.contents != 0x60 && tl.foreground_contents == 0)) {
                return InteractionType::INTERACTION_PLACE_ITEM;
            }
            if t().foreground_contents == 0x60 {
                let w = h.wire_at_pos(tile_pos);
                if can_be_removed_by(&w, &me) && can_pickup(ItemType(0xb2), rejected) {
                    return InteractionType::INTERACTION_PICKUP;
                }
            }
            if t().contents == 0x60 {
                let w = h.wire_at_pos(tile_pos);
                if can_be_removed_by(&w, &me) && can_pickup(ItemType(0xb2), rejected) {
                    return InteractionType::INTERACTION_PICKUP;
                }
            }
            if self.window_pickup(&tref, tile_pos, rejected) {
                return InteractionType::INTERACTION_PICKUP;
            }
        }

        // ---- Step H
        if allow && tile_is_plant(Some(tref)) && !tile_is_solid(Some(tref)) {
            return InteractionType::INTERACTION_DIG_FOREGROUND;
        }

        // ---- Step I
        let blockhead_in_pos = ah.blockhead_at_pos(self, tile_pos);

        // ---- Step J: tile-type dispatch
        'end: {
            let ti = t().type_index;
            if (0x3c..=0x41).contains(&ti) {
                let o = h.interaction_object_at_pos(tile_pos);
                if can_be_used_by(&o, &me) {
                    return InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT;
                }
            } else if matches!(ti, 0x19 | 0x21 | 0x22 | 0x23 | 0x24 | 0x25 | 0x2a | 0x2b | 0x2c | 0x2d | 0x2e | 0x2f) {
                let o = h.interaction_object_at_pos(tile_pos);
                if can_be_used_by(&o, &me) {
                    return InteractionType::INTERACTION_CRAFT;
                }
            }
            if ti == 0x1f {
                return none;
            }
            if allow {
                let dig = matches!(
                    ti,
                    6 | 7 | 8 | 9 | 4 | 0x1b | 0x1c | 0x18 | 0x20 | 0x30 | 0x31 | 0x32 | 0x3a | 0x3b | 0x46
                ) || tile_is_rock(Some(tref));
                if dig {
                    return InteractionType::INTERACTION_DIG;
                }
            }
            if !tile_is_air_water_or_snow(Some(tref)) {
                break 'end;
            }

            // ---- AIR PATH, P1: foreground
            if allow {
                let tl = t();
                if tl.foreground_contents == 0 {
                    if face_index == 3 && has_item && item_t.0 == 199 && count(ItemType(199)) > 0 {
                        return InteractionType::INTERACTION_PLACE_ITEM;
                    }
                } else if tl.foreground_contents == 0x60 {
                    let ty = item_type_from_tile_is_foregorund(tref, tile_pos, true, &world);
                    let mut can = true;
                    if tl.contents == 0x60 {
                        can = can_be_removed_by(&h.wire_at_pos(tile_pos), &me);
                    }
                    if can && can_pickup(ty, rejected) {
                        return InteractionType::INTERACTION_PICKUP;
                    }
                } else if FGSET.contains(&tl.foreground_contents) {
                    let mut ignore_rail = tl.foreground_contents == 0x62;
                    if ignore_rail && face_index == 3 && has_item && item_t.0 == 0xd1 {
                        ignore_rail = false;
                        if tl.contents == 0x30 {
                            let o = h.interaction_object_at_pos(tile_pos);
                            if o.as_ref().map_or(false, |o| o.object_type() == 0x31) {
                                ignore_rail = true;
                            }
                        }
                        if !ignore_rail {
                            return InteractionType::INTERACTION_DIG_FOREGROUND;
                        }
                    }
                    if !ignore_rail {
                        let ty = item_type_from_tile_is_foregorund(tref, tile_pos, true, &world);
                        let mut can = true;
                        if item_type_is_torch(ty) {
                            can = can_be_removed_by(&h.torch_at_pos(tile_pos), &me);
                        } else if tl.foreground_contents == 0x64 {
                            can = can_be_removed_by(&h.column_at_pos(tile_pos), &me);
                        } else if tl.foreground_contents == 0x63 {
                            can = can_be_removed_by(&h.painting_at_pos(tile_pos), &me) || world.is_mod();
                        } else if tl.foreground_contents == 0x42 {
                            can = can_be_removed_by(&h.ladder_at_pos(tile_pos), &me);
                        }
                        if can && can_pickup(ty, rejected) {
                            return InteractionType::INTERACTION_PICKUP;
                        }
                    }
                }
            }

            // ---- P2 / P3
            let contents = t().contents;
            if contents == 0 {
                if !allow {
                    break 'end;
                }
                // (Quirk) crowbar on a rail: the train-station test is always false here (trap 3)
                if t().foreground_contents == 0x62 && has_item && item_t.0 == 0xd1 {
                    return InteractionType::INTERACTION_DIG_FOREGROUND;
                }
                'p2window: {
                    if !(has_item && y < 0x3f8) {
                        break 'p2window;
                    }
                    let ty = item_t;
                    if count(ty) <= 0 {
                        break 'p2window;
                    }
                    let wm = world.world_width_macro();
                    if ty.0 == 0xa3 || ty.0 == 0x12d {
                        // P2a
                        self.bh.borrow_mut().z_index = world.is_admin() as i32;
                        if t().foreground_contents == 0 {
                            if ty.0 == 0x12d && !world.is_admin() {
                                break 'p2window;
                            }
                            if sign_connection_type_and_offset_for_pos(tile_pos, None, &*world, SignConnectionType(0)).0
                                != 0
                            {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                        }
                        break 'p2window;
                    }
                    let is_col = item_type_is_column(ty);
                    let is_sm = ty.0 == 1087 || ty.0 == 1088;
                    if ty.0 == 53 || ty.0 == 178 || is_col || is_sm {
                        // P2b
                        let tl = t();
                        let blocked = is_sm
                            && (tl.contents != 0
                                || (tl.foreground_contents != 0
                                    && !tile_foreground_contents_can_co_exist_with_blocks_and_elevator_shafts(Some(
                                        tref,
                                    )))
                                || tl.background_contents != 0);
                        let proceed = !blocked
                            && (((ty.0 == 178 || is_sm) && tl.contents == 0)
                                || ((ty.0 == 53 || is_col) && tl.foreground_contents == 0));
                        if !proceed {
                            break 'p2window;
                        }
                        if tl.back_wall_type_index != 2
                            && (tl.type_index != 3 || tl.partial_content_left != 0xff)
                            && tl.background_contents == 0
                            && tl.foreground_contents == 0
                            && !is_col
                        {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                        // vertical neighbours (above, below) use the full rule
                        let vertical_match = |n: Option<TileRef>| -> bool {
                            match tile_copy(&n) {
                                Some(nt) => {
                                    tile_is_solid(n)
                                        || (is_sm && (nt.contents == 0x67 || nt.contents == 0x68))
                                        || (ty.0 == 53 && nt.foreground_contents == 0x42)
                                        || (is_col && nt.foreground_contents == 0x64)
                                        || (ty.0 == 178 && (nt.contents == 0x60 || nt.contents == 0x2e))
                                }
                                None => false,
                            }
                        };
                        if vertical_match(tat(&world, x, y + 1)) {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                        if vertical_match(tat(&world, x, y - 1)) {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                        if ty.0 == 178 || ty.0 == 53 || is_sm {
                            let side_match = |n: Option<TileRef>| -> bool {
                                match tile_copy(&n) {
                                    Some(nt) => {
                                        tile_is_solid(n)
                                            || (ty.0 == 178 && (nt.contents == 0x60 || nt.contents == 0x2e))
                                    }
                                    None => false,
                                }
                            };
                            if side_match(tat(&world, x + 1, y)) {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                            if side_match(tat(&world, x - 1, y)) {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                        }
                        // queued-action scan
                        let queue = self.bh.borrow().action_queue.clone();
                        for qa in queue.iter() {
                            let a = action_of(qa);
                            let g = a.as_ref().map_or(0, |a| a.goal_interaction().0);
                            let goal = a.as_ref().map_or(IntPair { x: 0, y: 0 }, |a| a.goal_tile_pos());
                            let a_item_t = a.as_ref().map_or(ItemType(0), |a| item_type_of(&a.interaction_item()));
                            let dx = wrap_x_distance(goal.x.wrapping_sub(x), wm);
                            if g == 3 && a_item_t.0 != 0x69 {
                                if dx == 0 && goal.y == y - 1 {
                                    return InteractionType::INTERACTION_PLACE_ITEM;
                                }
                                if dx == 0 && goal.y == y + 1 {
                                    return InteractionType::INTERACTION_PLACE_ITEM;
                                }
                                if goal.y == y && (dx == -1 || dx == 1) {
                                    return InteractionType::INTERACTION_PLACE_ITEM;
                                }
                            } else if g == 2 && a_item_t == ty {
                                if ty.0 == 53 || is_col || ty.0 == 1087 {
                                    if dx == 0 && (goal.y == y - 1 || goal.y == y + 1) {
                                        return InteractionType::INTERACTION_PLACE_ITEM;
                                    }
                                } else if dx.abs() < 2 && goal.y >= y - 1 && goal.y <= y + 1 {
                                    return InteractionType::INTERACTION_PLACE_ITEM;
                                }
                            }
                        }
                        break 'p2window;
                    }

                    // P2c: generic item
                    let tl = t();
                    if !(item_type_is_placable_in_water(ty) || !(tl.type_index == 3 && tl.partial_content_left == 0xff))
                    {
                        break 'p2window;
                    }
                    if let Some(bip) = &blockhead_in_pos {
                        if !ptr_eq(bip, &me) && item_type_is_solid(ty) {
                            break 'p2window;
                        }
                    }
                    let mut contents_clear = if item_type_occupies_background_contents(ty) {
                        tl.background_contents == 0
                    } else if item_type_occupies_foreground_contents(ty) {
                        tl.foreground_contents == 0
                    } else if item_type_is_solid(ty) {
                        tl.foreground_contents == 0 && tl.contents == 0
                    } else {
                        tl.contents == 0
                    };
                    let head_ok = |n: Option<TileRef>| -> bool {
                        tile_copy(&n).map_or(false, |nt| {
                            nt.contents == 0
                                && nt.foreground_contents == 0
                                && nt.background_contents == 0
                                && tile_is_air_water_or_snow(n)
                        })
                    };
                    if contents_clear {
                        if is_portal_item(ty) {
                            let a1 = tat(&world, x, y + 1);
                            let ok1 = a1.is_some() && {
                                let a2 = tat(&world, x, y + 2);
                                a2.is_some() && head_ok(a1) && head_ok(a2)
                            };
                            if !ok1 {
                                contents_clear = false;
                            }
                        } else if matches!(ty.0, 221 | 52 | 331 | 164 | 270) {
                            let a1 = tat(&world, x, y + 1);
                            if !(a1.is_some() && head_ok(a1)) {
                                contents_clear = false;
                            }
                        }
                    }
                    if contents_clear {
                        // 1
                        if item_type_is_full_cage(ty) {
                            return InteractionType::INTERACTION_PLACE_ITEM;
                        }
                        // 2
                        if ty.0 == 0x69 {
                            return InteractionType::INTERACTION_PLACE_SOLID_BLOCK;
                        }
                        // 3
                        if (ty.0 == 0x9e || ty.0 == 0x9f)
                            && tile_is_water(Some(tref))
                            && t().partial_content_left > 0x7f
                        {
                            return InteractionType::INTERACTION_PLACE_SOLID_BLOCK;
                        }
                        let solid_t = item_type_is_solid(ty);
                        let attach = item_type_can_be_attached_next_to_to_half_depth_tiles(ty);
                        let sb = |s: bool| {
                            if s {
                                InteractionType::INTERACTION_PLACE_SOLID_BLOCK
                            } else {
                                InteractionType::INTERACTION_PLACE_ITEM
                            }
                        };
                        // 4 hangable
                        if item_type_is_hangable(ty) {
                            let above = tat(&world, x, y + 1);
                            if tile_is_solid(above) {
                                return sb(solid_t);
                            } else if above.is_some()
                                && attach
                                && (tile_is_half_depth(above, &world, IntPair { x, y: y + 1 })
                                    || above.unwrap().get().contents == 0x60)
                            {
                                return InteractionType::INTERACTION_PLACE_ITEM;
                            }
                        }
                        // 5 placable on wall
                        if item_type_is_placable_on_wall(ty) {
                            for dxn in [1i32, -1i32] {
                                let n = tat(&world, x + dxn, y);
                                if tile_is_solid(n)
                                    && (solid_t || !tile_contains_door(n))
                                    && n.unwrap().get().type_index != 0x1f
                                {
                                    return sb(solid_t);
                                } else if n.is_some()
                                    && attach
                                    && (tile_is_half_depth(n, &world, IntPair { x: x + dxn, y })
                                        || n.unwrap().get().contents == 0x60)
                                {
                                    return InteractionType::INTERACTION_PLACE_ITEM;
                                }
                            }
                        }
                        // 6 placable on floor
                        let mut qscan_taken = false;
                        if item_type_is_placable_on_floor(ty) {
                            let below = tat(&world, x, y - 1);
                            if let Some(bl) = below {
                                let blt = bl.get();
                                let supported = tile_is_solid(below)
                                    || (blt.foreground_contents == 0x64 && item_type_can_be_placed_on_columns(ty));
                                if supported && blt.type_index != 0x1f {
                                    qscan_taken = true;
                                    let solid_or_col = |n: Option<TileRef>| {
                                        tile_is_solid(n)
                                            || tile_copy(&n).map_or(false, |nt| nt.foreground_contents == 0x64)
                                    };
                                    let empty5 = |n: Option<TileRef>| -> bool {
                                        match tile_copy(&n) {
                                            Some(nt) => {
                                                !tile_is_solid(n)
                                                    && nt.foreground_contents == 0
                                                    && nt.contents == 0
                                                    && nt.background_contents == 0
                                            }
                                            None => false,
                                        }
                                    };
                                    if ty.0 == 200 {
                                        if solid_or_col(tat(&world, x + 1, y - 1))
                                            && solid_or_col(tat(&world, x - 1, y - 1))
                                            && empty5(tat(&world, x, y + 1))
                                            && empty5(tat(&world, x - 1, y))
                                            && empty5(tat(&world, x - 1, y + 1))
                                            && empty5(tat(&world, x + 1, y))
                                            && empty5(tat(&world, x + 1, y + 1))
                                        {
                                            return InteractionType::INTERACTION_PLACE_ITEM;
                                        }
                                    } else if !item_type_is_two_blocks_wide(ty) {
                                        let portal = is_portal_item(ty);
                                        return sb(solid_t || portal || ty.0 == 0x69);
                                    } else {
                                        let br = tat(&world, x + 1, y - 1);
                                        if solid_or_col(br) {
                                            let r = tat(&world, x + 1, y);
                                            if !tile_is_solid(r) && r.is_some() && r.unwrap().get().contents == 0 {
                                                return sb(solid_t);
                                            }
                                        }
                                        let bl2 = tat(&world, x - 1, y - 1);
                                        if solid_or_col(bl2) {
                                            let l = tat(&world, x - 1, y);
                                            if !tile_is_solid(l) && l.is_some() && l.unwrap().get().contents == 0 {
                                                return sb(solid_t);
                                            }
                                        }
                                    }
                                }
                            }
                            if !qscan_taken {
                                if let Some(bl) = below {
                                    if attach
                                        && (tile_is_half_depth(below, &world, IntPair { x, y: y - 1 })
                                            || bl.get().contents == 0x60)
                                    {
                                        return InteractionType::INTERACTION_PLACE_ITEM;
                                    }
                                }
                            }
                        }
                        // 7 QSCAN
                        let queue = self.bh.borrow().action_queue.clone();
                        for qa in queue.iter() {
                            let Some(a) = action_of(qa) else { continue };
                            if a.goal_interaction().0 != 3 || item_type_of(&a.interaction_item()).0 == 0x69 {
                                continue;
                            }
                            let goal = a.goal_tile_pos();
                            let dx = wrap_x_distance(goal.x.wrapping_sub(x), wm);
                            if (dx == 0 && item_type_is_placable_on_floor(ty) && goal.y == y - 1)
                                || (dx == 0 && item_type_is_hangable(ty) && goal.y == y + 1)
                                || (item_type_is_placable_on_wall(ty) && goal.y == y && (dx == -1 || dx == 1))
                            {
                                return sb(solid_t || ty.0 == 0x69);
                            }
                        }
                        // 8
                        let tl = t();
                        if !item_type_occupies_background_contents(ty)
                            && item_type_is_placable_on_back_wall(ty)
                            && tl.back_wall_type_index != 2
                            && tl.foreground_contents == 0
                        {
                            return sb(solid_t || is_portal_item(ty) || ty.0 == 0x69);
                        }
                    }
                }
                // P2_WINDOW
                if self.window_pickup(&tref, tile_pos, rejected) {
                    return InteractionType::INTERACTION_PICKUP;
                }
                break 'end;
            }

            // ---- P3: tile.contents != 0
            if tile_is_workbench(Some(tref)) {
                let o = h.interaction_object_at_pos(tile_pos);
                let can_use_expert = o.as_ref().map_or(false, |o| {
                    o.as_interaction_like().map_or(false, |i| i.can_be_used_in_expert_mode_when_not_owned())
                });
                let local = h.dynamic_world_local_net_id();
                let owner_is_local = o.as_ref().map_or(false, |o| o.owner_id().map_or(false, |oid| oid == local));
                let cant_use = !world.is_admin() && world.expert_mode() && !can_use_expert && !owner_is_local;
                if !cant_use {
                    return InteractionType::INTERACTION_CRAFT;
                }
            }
            if contents == 0x30 {
                let o = h.interaction_object_at_pos(tile_pos);
                if can_be_used_by(&o, &me) {
                    return InteractionType::INTERACTION_USE_INTERACTIVE_OBJECT;
                }
                break 'end;
            }
            if !allow {
                break 'end;
            }
            if !matches!(contents, 0x60 | 0x67 | 0x68) {
                if contents == 0x65 {
                    if can_be_removed_by(&h.stairs_at_pos(tile_pos), &me) {
                        let ty = item_type_from_tile_is_foregorund(tref, tile_pos, false, &world);
                        if can_pickup(ty, rejected) {
                            return InteractionType::INTERACTION_PICKUP;
                        }
                    }
                    break 'end;
                }
                if contents == 0x92 {
                    if can_be_removed_by(&h.door_at_pos(tile_pos), &me) {
                        let ty = item_type_from_tile_is_foregorund(tref, tile_pos, false, &world);
                        if can_pickup(ty, rejected) {
                            return InteractionType::INTERACTION_PICKUP;
                        }
                    }
                    break 'end;
                }
                if !tile_is_bush_or_scrub(Some(tref)) && !tile_is_tree(Some(tref)) {
                    break 'end;
                }
                // TREE_PLACE
                // (the sign guards of the original, `0xa3` / `0x12d` with an occupied foreground or a non-admin, can
                // never fire: `itemTypeIsPlacableOnTrees` excludes both signs)
                if has_item && item_type_is_placable_on_trees(item_t) && y < 0x3f8 {
                    return InteractionType::INTERACTION_PLACE_ITEM;
                }
                // (a plant here would have been answered `DIG_FOREGROUND` by step H: this point needs `allow`, and
                // the air path above only runs for air / water / snow tiles, which are never solid)
                return InteractionType::INTERACTION_DIG;
            }
            // P3.5: contents is wire, shaft or motor
            if contents == 0x67 && has_item && item_type_is_solid(item_t) {
                let mut place_ok = true;
                if item_t.0 == 0x45 || item_t.0 == 0xa5 {
                    place_ok = tile_is_solid(tat(&world, x, y + 1))
                        || tile_is_solid(tat(&world, x + 1, y))
                        || tile_is_solid(tat(&world, x - 1, y))
                        || tile_is_solid(tat(&world, x, y - 1));
                } else if item_t.0 == 0x34 || item_t.0 == 0xa4 {
                    let d = tat(&world, x, y - 1);
                    let u = tat(&world, x, y + 1);
                    place_ok = tile_is_solid(d)
                        && u.is_some()
                        && {
                            let ut = u.unwrap().get();
                            (ut.contents == 0 || ut.contents == 0x67)
                                && ut.foreground_contents == 0
                                && ut.background_contents == 0
                        }
                        && tile_is_air_water_or_snow(u);
                }
                let tl = t();
                if place_ok
                    && count(item_t) > 0
                    && (item_type_is_placable_in_water(item_t)
                        || !(tl.type_index == 3 && tl.partial_content_left == 0xff))
                {
                    return InteractionType::INTERACTION_PLACE_SOLID_BLOCK;
                }
            }
            let t2 = item_type_from_tile_is_foregorund(tref, tile_pos, false, &world);
            let mut can = true;
            if contents == 0x60 {
                can = can_be_removed_by(&h.wire_at_pos(tile_pos), &me);
            } else if contents == 0x67 {
                can = can_be_removed_by(&h.elevator_shaft_at_pos(tile_pos), &me);
            } else if contents == 0x68 {
                can = can_be_removed_by(&h.elevator_motor_at_pos(tile_pos), &me);
            }
            if can && can_pickup(t2, rejected) {
                return InteractionType::INTERACTION_PICKUP;
            }
        }

        // ---- Step K: END
        if tile_is_solid(Some(tref)) && self.freeblock_scan(&tref, tile_pos, rejected) {
            return InteractionType::INTERACTION_PICKUP_FREEBLOCK;
        }
        if allow
            && self.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(
                Some(&tref),
                tile_pos,
                item,
                include_actions,
            )
        {
            return InteractionType::INTERACTION_DIG_BACK_WALL;
        }
        none
    }

    /// The window pickup of step G / `P2_WINDOW` (bg `'E'` or `'_'`). True means `PICKUP`.
    fn window_pickup(&self, tile: &TileRef, tile_pos: IntPair, rejected: &mut bool) -> bool {
        let bg = tile.get().background_contents;
        if bg != 0x45 && bg != 0x5f {
            return false;
        }
        let wt = if bg == 0x5f { ItemType(0xae) } else { ItemType(0x3a) };
        let r = self.can_pick_up_item_of_type_sub_items(wt, None);
        if r == 1 {
            let win = host().window_at_pos(tile_pos);
            let me = self.me();
            if can_be_removed_by(&win, &me) {
                return true;
            }
        } else if r == -1 {
            *rejected = true;
        }
        false
    }

    /// The ownership test shared by `FREEBLOCK_SCAN` and the PICKUP_FREEBLOCK loop of `queueAction`
    /// : true when the free block belongs to somebody else.
    pub(crate) fn freeblock_ownership_blocks(
        &self,
        f: &crate::physics_objects::free_block::FreeBlock,
        ty: ItemType,
        world: &World,
    ) -> bool {
        let ctl = self.is_client_blockhead_being_controlled_by_server();
        let mut unable = false;
        if item_type_requires_ownership_to_remove(ty) && (!world.is_admin() || ctl) {
            let dict = f.dynamic_object_save_dict();
            let me_id = <Self as crate::objects::base::BlockheadApi>::local_net_id(self);
            let get = |k: &str| -> Option<String> {
                dict.as_ref().and_then(|d| d.map.get(&k.to_string())).and_then(|v| v.as_str().map(|s| s.to_string()))
            };
            let mut owner = get("ownerID");
            if owner.is_none() {
                if ty.0.wrapping_sub(164) < 2 {
                    owner = get("ironPlaceClientID");
                } else if ty.0 == 168 {
                    owner = get("sellerClientID");
                } else if ty.0 == 207 || ty.0 == 1065 {
                    owner = get("safeClientID");
                }
            }
            if let Some(owner) = &owner {
                if me_id.as_deref() != Some(owner.as_str()) {
                    unable = true;
                }
                if unable
                    && !actions_host().dynamic_world_is_client()
                    && (owner == "server" || me_id.as_deref() == Some("server"))
                    && !ctl
                {
                    unable = false;
                }
            }
        }
        unable
    }

    /// `FREEBLOCK_SCAN`: true when the result is `PICKUP_FREEBLOCK`. Writes `zIndex` on every iteration.
    fn freeblock_scan(&self, tile: &TileRef, tile_pos: IntPair, rejected: &mut bool) -> bool {
        let Some(world) = self.world_rc() else { return false };
        let h = host();
        let ah = actions_host();
        let tl = tile.get();
        if !(self.tile_is_lit_for_self_at_pos_impl(&tl, tile_pos) && ah.free_blocks_exist_at_pos(self, tile_pos)) {
            return false;
        }
        let mut list = h.free_blocks_at_pos(tile_pos);
        sort_freeblocks_desc(&mut list);
        let me = self.me();
        for fb in list.iter() {
            let Some(f) = crate::physics_objects::free_block::as_free_block(fb) else { continue };
            let ty = ItemType(f.item_type() as u32);
            self.bh.borrow_mut().z_index = world.is_admin() as i32;
            let unable = self.freeblock_ownership_blocks(f, ty, &world);
            if !unable {
                let p = f.priority_blockhead();
                if p.is_none() || ptr_eq(p.as_ref().unwrap(), &me) {
                    let subs = f.sub_items();
                    let r = self.can_pick_up_item_of_type_sub_items_data_a_data_b(
                        ty,
                        subs.as_deref(),
                        f.data_a(),
                        f.data_b(),
                    );
                    if r == 1 && !f.falling() {
                        return true;
                    } else if r == -1 {
                        *rejected = true;
                    }
                }
            }
        }
        false
    }

    /// `-currentInteractionTypeForTile:atPos:pickupRejectedDueToInventoryFull:includeActions:faceIndex:allowProtectedActions:`
    /// : `interactionTypeForTile` with `[self currentItem]`.
    pub fn current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
        &self,
        tile: Option<&TileRef>,
        tile_pos: IntPair,
        rejected: &mut bool,
        include_actions: bool,
        face_index: i32,
        allow_protected_actions: bool,
    ) -> InteractionType {
        let item = self.current_item();
        self.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            tile, tile_pos, item.as_ref(), rejected, include_actions, face_index, allow_protected_actions,
        )
    }

    /// `-currentInteractionType`: `state.interactionType`.
    pub fn current_interaction_type(&self) -> InteractionType {
        InteractionType(self.bh.borrow().state.interaction_type)
    }

    // ================================================================================================
    // 4.3 / 4.22 back wall
    // ================================================================================================

    /// `-canDigBackWallforTile:atPos:withItem:includeActions:` (sic "Wallfor").
    pub fn can_dig_back_wallfor_tile_at_pos_with_item_include_actions(
        &self,
        tile: Option<&TileRef>,
        tile_pos: IntPair,
        item: Option<&ItemRef>,
        include_actions: bool,
    ) -> bool {
        let (Some(item), Some(tile)) = (item, tile) else { return false };
        let Some(world) = self.world_rc() else { return false };
        if !tile_is_air_water_or_snow(Some(*tile)) {
            return false;
        }
        let tl = tile.get();
        if !(tl.foreground_contents == 0 && tl.background_contents == 0 && tl.contents == 0) {
            return false;
        }
        if tl.back_wall_type_index == 2 || tl.back_wall_type_index == 3 {
            return false;
        }
        if !(back_wall_is_mutable(Some(*tile)) || tl.back_wall_type_index == 1) {
            return false;
        }
        let t = item.borrow().item_type;
        if self.count_of_inventory_items_of_type_include_actions(t, include_actions) <= 0 {
            return false;
        }
        if !self.is_correct_tool_for_back_wall_of_type_for_item(tl.back_wall_type_index as i32, t) {
            return false;
        }
        if tl.back_wall_type_index != 1 {
            return true;
        }
        let mut air_count = 0;
        for (dx, dy) in [(0, 1), (1, 0), (-1, 0), (0, -1)] {
            if let Some(n) = tat(&world, tile_pos.x + dx, tile_pos.y + dy) {
                let bw = n.get().back_wall_type_index;
                if bw == 2 || bw == 3 {
                    air_count += 1;
                }
            }
            if air_count >= 2 {
                return true;
            }
        }
        false
    }

    /// `-isCorrectToolForBackWallOfType:forItem:`.
    pub fn is_correct_tool_for_back_wall_of_type_for_item(&self, tile_type: i32, item_type: ItemType) -> bool {
        let item = item_type.0;
        if item == 0 {
            return false;
        }
        let axes = [6u32, 0x21, 0x46];
        if spade_tile_types().contains(&tile_type) {
            return matches!(item, 0x10 | 0x28 | 0x40 | 0x59);
        }
        let hard = tile_type_is_rock(tile_type) || matches!(tile_type, 0x18 | 0x3b | 4);
        if !hard {
            if tile_type != 9 && tile_type != 0x20 {
                return false;
            }
            return axes.contains(&item);
        }
        if axes.contains(&item) {
            return matches!(tile_type, 0x18 | 0x3b | 4);
        }
        if matches!(item, 8 | 0x22 | 0x2b | 0x42 | 0x5a | 0x11d) || item.wrapping_sub(0x5d) <= 5 {
            return true;
        }
        false
    }
}

// ====================================================================================================
// 4.23 / 4.4 / 4.24
// ====================================================================================================

const MACHETES: [u32; 3] = [0x19, 0x31, 0x43];
const SPADES: [u32; 4] = [0x10, 0x28, 0x40, 0x59];
const AXES: [u32; 3] = [6, 0x21, 0x46];
const PICKAXES: [u32; 7] = [8, 0x22, 0x2b, 0x42, 0x5a, 0x11d, 0];

/// `P(x)` of: "wrong tool" test, `usageIncrementPerUse(t,2,0,0) > 0 && usageIncrementAppliesToIncorrectUse(t)`
/// (literally `2, 0, 0`, trap 8).
fn wrong_tool(t: ItemType) -> bool {
    usage_increment_per_use(t, 2, false, false) > 0 && usage_increment_applies_to_incorrect_use(t)
}

fn is_pickaxe(t: u32) -> bool {
    (PICKAXES[..6].contains(&t)) || t.wrapping_sub(0x5d) <= 5
}

impl Blockhead {
    /// `-goodOrBadInteractionForAction:`: the preview icon and good/bad tint, persisted in the Action save
    /// dict (12 bytes). The jump table covers `g in 1..=28`, anything else returns zeros.
    pub fn good_or_bad_interaction_for_action(&self, action: &ActionRef) -> InteractionTestResult {
        let mut r =
            InteractionTestResult { good_or_bad: 0, render_item_type: 0, render_item_data_a: 0, render_item_data_b: 0 };
        let g = action.goal_interaction().0;
        let item = action.interaction_item();
        let t = item_type_of(&item);
        let h = host();
        let world = self.world_rc();
        match g {
            2 | 3 | 6 | 9 => {
                if let Some(i) = &item {
                    r.render_item_type = t.0;
                    r.render_item_data_b = i.borrow().data_b;
                }
            }
            4 => {
                if let Some(cio) = action.craftable_item_object() {
                    let ci = cio.craftable_item();
                    r.render_item_type = ci.crafted_item_type;
                    r.render_item_data_b = ci.crafted_item_data_b;
                }
            }
            5 => {
                let o = h.interaction_object_with_id(action.interaction_object_id() as i64);
                r.render_item_type = o.map_or(0, |o| actions_host().interaction_render_item_type(&o));
            }
            7 | 8 | 18 | 28 => r.render_item_type = 1,
            12 => r.render_item_type = 0x4a,
            13 => r.render_item_type = 0xf,
            14 | 16 | 19 | 25 | 27 => r.render_item_type = t.0,
            15 => {
                r.render_item_type = t.0;
                if let Some(i) = &item {
                    r.render_item_data_b = i.borrow().data_b;
                }
            }
            17 => r.render_item_type = 0x6f,
            21 => r.render_item_type = 0xcc,
            10 | 20 => {
                if item.is_none() {
                    r.render_item_type = 1;
                } else if matches!(t.0, 7 | 0x19 | 0x2a | 0x31 | 0x32 | 0x43 | 0x44 | 0x101 | 0x11e) {
                    r.render_item_type = t.0;
                    r.good_or_bad = 1;
                } else if t.0 == 0x4a {
                    let npc = h.npc_with_id(action.interaction_object_id());
                    r.render_item_type = 1;
                    if let Some(npc) = npc {
                        let is_donkey = crate::objects::class_info::class_info_by_name("Donkey")
                            .map_or(false, |c| is_kind_of_class(&npc, c));
                        if is_donkey && h.ridable_blockhead_can_ride_using_item(&npc, self, t.0 as i32) {
                            r.render_item_type = t.0;
                            r.good_or_bad = 1;
                        }
                    }
                } else if item_type_is_metal(t) {
                    r.render_item_type = t.0;
                    r.good_or_bad = 1;
                } else {
                    r.render_item_type = 1;
                }
            }
            24 => {
                r.render_item_type = t.0;
                r.good_or_bad = 1;
            }
            23 => {
                r.render_item_type = if item.is_some() { t.0 } else { 1 };
                let goal = action.goal_tile_pos();
                let Some(world) = world else { return r };
                let Some(tl) = tat(&world, goal.x, goal.y) else { return r };
                let tt = tl.get();
                if t.0 == 0xd1 && tt.foreground_contents == 0x62 {
                    r.render_item_type = t.0;
                    r.good_or_bad = 1;
                } else if tt.foreground_contents == 0x30 {
                    r.render_item_type = 1;
                } else if tile_is_plant(Some(tl)) {
                    if MACHETES.contains(&t.0) {
                        r.good_or_bad = 1;
                    } else if t.0 == 0x67 || t.0 == 0x68 {
                        r.render_item_type = 1;
                    } else if wrong_tool(t) {
                        r.good_or_bad = -1;
                    } else {
                        r.render_item_type = 1;
                    }
                    if SPADES.contains(&t.0) {
                        r.good_or_bad = 1;
                    }
                }
            }
            1 => {
                if item.is_some() {
                    r.render_item_type = t.0;
                } else {
                    r.render_item_type = 1;
                }
                let goal = action.goal_tile_pos();
                let Some(world) = world else { return r };
                let Some(tl) = tat(&world, goal.x, goal.y) else { return r };
                let tt = tl.get();
                let ty = tt.type_index as i32;
                let paint = t.0 == 0x67 || t.0 == 0x68;
                if spade_tile_types().contains(&ty) {
                    if ty == 0x3a && t.0 == 0xaf {
                        r.good_or_bad = 1;
                    } else if SPADES.contains(&t.0) {
                        r.good_or_bad = 1;
                    } else if paint {
                        r.render_item_type = 1;
                    } else if wrong_tool(t) {
                        r.good_or_bad = -1;
                    } else {
                        r.render_item_type = 1;
                    }
                } else if tile_is_bush_or_scrub(Some(tl)) {
                    if MACHETES.contains(&t.0) {
                        r.good_or_bad = 1;
                    } else if paint {
                        r.render_item_type = 1;
                    } else if wrong_tool(t) {
                        r.good_or_bad = -1;
                    } else {
                        r.render_item_type = 1;
                    }
                } else if tile_is_rock(Some(tl)) || matches!(ty, 0x18 | 0x3b | 4) {
                    if t.0 == 0xaf && tt.contents == 0x3f {
                        r.good_or_bad = 1;
                    } else if AXES.contains(&t.0) {
                        r.good_or_bad = if matches!(ty, 0x18 | 0x3b | 4) { 1 } else { -1 };
                    } else if is_pickaxe(t.0) {
                        r.good_or_bad = 1;
                    } else if paint {
                        r.render_item_type = 1;
                    } else if wrong_tool(t) {
                        r.good_or_bad = -1;
                    } else {
                        r.render_item_type = 1;
                    }
                } else if tile_is_tree_trunk(Some(tl)) || ty == 9 || ty == 0x20 {
                    if AXES.contains(&t.0) {
                        r.good_or_bad = 1;
                    } else if paint {
                        r.render_item_type = 1;
                    } else if wrong_tool(t) {
                        r.good_or_bad = -1;
                    } else {
                        r.render_item_type = 1;
                    }
                } else if tile_is_interaction_object(Some(tl)) {
                    r.render_item_type = 1;
                }
            }
            _ => {}
        }
        r
    }

    /// `-currentInteractionIsGoodOrBad` (client UI hint; no state effect).
    pub fn current_interaction_is_good_or_bad(&self) -> i32 {
        let f = self.bh.borrow();
        if f.state.interacting == 0 {
            return 0;
        }
        let Some(first) = f.action_queue.first() else { return 0 };
        match action_of(first) {
            Some(a) => a.interaction_test_result().good_or_bad,
            None => 0,
        }
    }

    /// `-updateGatherSpeedAndAnimationForCurrentInterationAndItem` (sic): `state.gatherSpeed` and
    /// `state.subAnimationType`. Mixed f32 / f64 widths as in trap 9; never uses `f32::min/max/clamp`.
    pub fn update_gather_speed_and_animation_for_current_interation_and_item(&self) {
        let Some(world) = self.world_rc() else { return };
        let h = host();
        let (it_, tile, item, isq) = {
            let f = self.bh.borrow();
            (f.state.interaction_type, f.interacting_tile, f.interaction_item.clone(), f.state.interacting_square)
        };
        let ty = if tile.is_some() { item_type_of(&item).0 } else { 0 };
        let mut s: f32 = 0.5;
        let mut sub: Option<u32> = None;
        let plain = !self.is_net() && !self.is_client_blockhead_being_controlled_by_server();
        match it_ {
            4 => sub = Some(2),
            5 => {
                let o = h.interaction_object_at_pos(isq);
                let is_bed = o.as_ref().map_or(false, |o| is_kind_of_class(o, &BED_CLASS));
                if !is_bed {
                    sub = Some(0);
                    if plain {
                        if let Some(o) = &o {
                            let is_gold_chest =
                                o.as_interaction_like().map_or(false, |i| i.interaction_object_type() == 2)
                                    && downcast::<Chest>(o).map_or(false, |c| c.chest_type() == 3);
                            if is_gold_chest {
                                h.report_achievement_with_identifier("grp.treasure");
                            }
                        }
                    }
                } else {
                    sub = Some(5);
                    {
                        let mut f = self.bh.borrow_mut();
                        f.state.sub_animation_type = 5;
                        f.next_to_square = isq;
                        f.to_square = isq;
                        f.from_square = isq;
                        f.prev_from_square = isq;
                        f.from_tile = f.interacting_tile;
                        f.to_tile = f.interacting_tile;
                    }
                    self.update_position(isq);
                    if plain {
                        h.report_achievement_with_identifier("grp.bed");
                        if o.as_ref().and_then(|o| downcast::<Bed>(o)).map_or(false, |b| b.item_type() == 0xaa) {
                            h.report_achievement_with_identifier("grp.golden_bed");
                        }
                    }
                }
            }
            9 | 2 | 3 | 7 | 8 | 6 | 0xe | 0x13 | 0x1b | 0x1c | 0x19 | 0xf | 0x10 => sub = Some(1),
            1 | 0x17 | 0x18 => {
                sub = Some(1);
                if let Some(tref) = tile {
                    let tl = tref.get();
                    let ti = tl.type_index as i32;
                    if it_ == 0x18 {
                        s = 256.0;
                    } else if it_ == 0x17 && tl.foreground_contents == 0x30 {
                        s = 256.0;
                    } else if tile_is_bush_or_scrub(Some(tref)) {
                        s = 2.0;
                        match ty {
                            0x19 => s = 4.0,
                            0x31 => s = 6.0,
                            0x43 => s = 8.0,
                            _ => {}
                        }
                        if tile_is_plant(Some(tref)) && it_ == 0x17 {
                            match ty {
                                0x10 => s = 8.0,
                                0x40 | 0x19 => s = 16.0,
                                0x28 | 0x31 => s = 32.0,
                                0x43 => s = 64.0,
                                0x59 => s = 16.0,
                                _ => {}
                            }
                        }
                    } else if tile_is_workbench(Some(tref)) {
                        s = 256.0;
                    } else if tl.contents == 0x30 {
                        s = 256.0;
                    } else if ti == 0x18 || ti == 0x3b || ti == 4 {
                        s = 4.0;
                        if item_type_is_metal(ItemType(ty)) {
                            s = 256.0;
                        }
                    } else if spade_tile_types().contains(&ti) {
                        s = 2.0;
                        match ty {
                            0x10 => s = 8.0,
                            0x40 => s = 16.0,
                            0x28 => s = 32.0,
                            0x59 => s = 16.0,
                            _ => {}
                        }
                        if ti == 0x3a && ty == 0xaf {
                            s = 32.0;
                        }
                        if tl.contents != 0 && tl.contents != 0x67 {
                            s *= 0.5f32;
                        }
                    } else if !tile_is_rock(Some(tref)) {
                        if tile_is_tree_trunk(Some(tref)) || ti == 9 || ti == 0x20 {
                            s = match ty {
                                6 => 8.0,
                                0x21 => 16.0,
                                0x46 => 32.0,
                                _ => 2.0,
                            };
                        }
                    } else {
                        s = match ty {
                            8 => 4.0,
                            0x22 | 0x5e => 8.0,
                            0x2b | 0x5a | 0x5f => 12.0,
                            0x42 | 0x60 => 16.0,
                            0x5d | 0x61 => 24.0,
                            0x62 => 32.0,
                            0x11d => 48.0,
                            _ => 1.0,
                        };
                        if tl.contents != 0 && tl.contents != 0x67 {
                            if tl.contents == 0x40 {
                                s *= 2.0f32;
                            } else if tl.contents == 0x3f {
                                s *= 0.125f32;
                            } else {
                                s *= 0.25f32;
                            }
                        }
                        match ti {
                            0x35 | 0x36 => s *= 0.125f32,
                            0x37 => s *= 0.0625f32,
                            0x38 => s = ((s as f64) * 0.03125f64) as f32,
                            0x39 => s = ((s as f64) * 0.015625f64) as f32,
                            0x44 | 0x45 => s = ((s as f64) * 0.01171875f64) as f32,
                            _ => {}
                        }
                    }
                    if world.expert_mode() && s < 255.0f32 {
                        s *= 0.5f32;
                    }
                }
            }
            10 => sub = Some(3),
            0x14 => sub = Some(8),
            _ => {}
        }
        let mut f = self.bh.borrow_mut();
        f.state.gather_speed = s;
        if let Some(v) = sub {
            f.state.sub_animation_type = v;
        }
    }

    // ================================================================================================
    // 4.7 updatePosition:
    // ================================================================================================

    /// `-updatePosition:` override (runs on every position change): doors, world loading, achievements,
    /// glow / promise conversion. Order: (1) doors with the raw `newPosition` and the pre-super `pos`, (2) super,
    /// (3) the rest only if `changed` (trap 5).
    pub fn update_position(&self, new_position: IntPair) {
        let Some(world) = self.world_rc() else {
            base_update_position(&self.me(), new_position);
            return;
        };
        let h = host();
        let ah = actions_host();
        let me = self.me();
        let old = self.pos();
        let changed = new_position.x != old.x || new_position.y != old.y;
        let (ride_none, to_kf, from_kf) = {
            let f = self.bh.borrow();
            (f.ride_object.is_none(), f.traverse_to_key_frame, f.traverse_from_key_frame)
        };
        if changed && ride_none && (to_kf != kf::STAND_ELEVATOR as i32 || from_kf != kf::STAND_ELEVATOR as i32) {
            let t0 = tat(&world, old.x, old.y);
            if tile_contains_door(t0) || tile_contains_trap_door(t0) || tile_contains_gate(t0) {
                h.set_door_at_pos_to_open_direction(old, false, 0);
            } else {
                let t1 = tat(&world, old.x, old.y + 1);
                if tile_contains_door(t1) {
                    h.set_door_at_pos_to_open_direction(IntPair { x: old.x, y: old.y + 1 }, false, 0);
                }
            }
            let dir = if old.x < new_position.x { 1 } else { -1 };
            let n0 = tat(&world, new_position.x, new_position.y);
            if tile_contains_door(n0) || tile_contains_trap_door(n0) || tile_contains_gate(n0) {
                h.set_door_at_pos_to_open_direction(new_position, true, dir);
            } else {
                let n1 = tat(&world, new_position.x, new_position.y + 1);
                if tile_contains_door(n1) {
                    h.set_door_at_pos_to_open_direction(
                        IntPair { x: new_position.x, y: new_position.y + 1 },
                        true,
                        dir,
                    );
                }
            }
        }
        base_update_position(&me, new_position);
        if !changed {
            return;
        }
        // world loading
        let is_net = self.is_net();
        let mut light_block_index: i32 = -1;
        let dw_is_server = self.dw().map_or(false, |d| d.is_server());
        if dw_is_server && is_net {
            let cid = self.client_id();
            // `[[world serverClients] objectForKey:clientID]` then `-lightBlockIndex` (seam: ServerClient is a placeholder)
            let sc = h.server_client_light_block_index_for_net_id(cid.as_deref());
            if let Some(i) = sc {
                light_block_index = i;
            }
        }
        if !is_net || light_block_index != -1 {
            h.fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
                self.pos(),
                light_block_index,
                Some(&me),
            );
        }
        // achievements
        if !is_net && !self.is_client_blockhead_being_controlled_by_server() {
            let x = new_position.x;
            let w = world.world_width_macro().wrapping_shl(5);
            if x == 0 {
                h.report_achievement_with_identifier("grp.northpole");
            } else if x == w / 4 {
                h.report_achievement_with_identifier("grp.sunrise");
            } else if x == w / 2 {
                h.report_achievement_with_identifier("grp.southpole");
            } else if x == (w / 4).wrapping_mul(3) {
                h.report_achievement_with_identifier("grp.sunset");
            }
            let y = new_position.y;
            if y < 0x3c && y > 0 {
                h.report_achievement_with_identifier("grp.bottom");
            } else if y < 0x3c1 {
                let hp = world.highest_point();
                if hp.y - 1 <= y && x < hp.x + 4 && x > hp.x - 4 {
                    h.report_achievement_with_identifier("grp.mountaineer");
                }
            } else {
                h.report_achievement_with_identifier("grp.top");
            }
            world.mark_circum_navigate_x(x);
        }
        // glow and promise conversion
        if !ah.dynamic_world_is_client() {
            let pos = self.pos();
            for xo in -4..=4 {
                for yo in -4..=4 {
                    let glow = IntPair { x: pos.x + xo, y: pos.y + yo };
                    let Some(tref) = tat(&world, glow.x, glow.y) else { continue };
                    tref.update(|t| {
                        t.artificial_light_b = if t.artificial_light_b < 2 { 1 } else { t.artificial_light_b }
                    });
                    if tile_requires_glow_block(tref) {
                        ah.load_glow_block_if_needed_at_pos_tile(self, glow, &tref);
                        continue;
                    }
                    let tl = tref.get();
                    let promoted: Option<(u8, i32)> = match tl.foreground_contents {
                        0x34 => Some((0x33, 0x4b)),
                        0x36 => Some((0x35, 0x4c)),
                        0x38 => Some((0x37, 0x56)),
                        0x3a => Some((0x39, 0x57)),
                        0x3c => Some((0x3b, 0x58)),
                        _ => None,
                    };
                    if let Some((fg, item)) = promoted {
                        tref.set_foreground_contents(fg);
                        ah.add_torch_at_pos_of_type(self, glow, item);
                    } else if matches!(tl.contents, 0x5e | 0x91 | 0x90) {
                        let load_troll = tl.contents != 0x91;
                        let load_treasure = tl.contents != 0x90;
                        tref.set_contents(0);
                        if tref.get().type_index == 2 {
                            ah.create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
                                self,
                                &tref,
                                glow,
                                load_troll,
                                load_treasure,
                            );
                        }
                    }
                }
            }
        }
    }

    // ================================================================================================
    // 4.8 - 4.21 accessors
    // ================================================================================================

    /// `-willBeCraftingAtWorkbench:`.
    pub fn will_be_crafting_at_workbench(&self, wb: &ObjRef) -> bool {
        let wp = wb.pos();
        let st = self.bh.borrow().state;
        if st.interaction_type != 4 {
            return false;
        }
        let sq = st.interacting_square;
        if sq.x != wp.x {
            return false;
        }
        if sq.y == wp.y {
            return true;
        }
        if object_is_double_height(wb) && sq.y == wp.y + 1 {
            return true;
        }
        let ty = workbench_type_of(wb);
        if ty != 1 && ty != 0xd {
            return false;
        }
        sq.y == wp.y - 1
    }

    /// `-willBeAddingFuelAtWorkbench:`.
    pub fn will_be_adding_fuel_at_workbench(&self, wb: &ObjRef) -> bool {
        let wp = wb.pos();
        let st = self.bh.borrow().state;
        let sq = st.interacting_square;
        st.interaction_type == 0xd
            && sq.x == wp.x
            && (sq.y == wp.y || (object_is_double_height(wb) && sq.y == wp.y + 1))
    }

    /// `-willBeInteractingWithInteractionObject:`.
    pub fn will_be_interacting_with_interaction_object(&self, o: &ObjRef) -> bool {
        let op = o.pos();
        let st = self.bh.borrow().state;
        if st.interaction_type != 5 {
            return false;
        }
        let sq = st.interacting_square;
        if sq.y != op.y && !(object_is_double_height(o) && sq.y == op.y + 1) {
            if o.object_type() != 0x32 {
                return false;
            }
            if sq.y != op.y - 1 {
                return false;
            }
        }
        if sq.x == op.x {
            return true;
        }
        let two_wide = o.as_interaction_like().map_or(false, |i| i.two_blocks_wide());
        if sq.x == op.x + 1 && two_wide && !o.flipped() {
            return true;
        }
        sq.x == op.x - 1 && two_wide && o.flipped()
    }

    /// `-isCraftingAtAnyWorkbench`.
    pub fn is_crafting_at_any_workbench(&self) -> bool {
        self.bh.borrow().state.interaction_type == 4
    }

    /// `-isAddingFuelToAnyWorkbench`.
    pub fn is_adding_fuel_to_any_workbench(&self) -> bool {
        self.bh.borrow().state.interaction_type == 0xd
    }

    /// `-setInteractionWorkbench:`.
    pub fn set_interaction_workbench(&self, w: Option<ObjRef>) {
        self.bh.borrow_mut().interaction_workbench = w;
    }

    /// `-interactionWorkbench`.
    pub fn interaction_workbench(&self) -> Option<ObjRef> {
        self.bh.borrow().interaction_workbench.clone()
    }

    /// `-getCurrentCraftProgress`: `-1.0f` when not crafting.
    pub fn get_current_craft_progress(&self) -> f32 {
        let f = self.bh.borrow();
        if f.state.interaction_type == 4 {
            if let Some(w) = &f.interaction_workbench {
                return downcast::<Workbench>(w).map_or(0.0, |w| w.fraction_complete() as f64 as f32);
            }
        }
        -1.0f32
    }

    /// `-currentCraftIsOutOfFuel`.
    pub fn current_craft_is_out_of_fuel(&self) -> bool {
        let w = {
            let f = self.bh.borrow();
            if f.state.interaction_type != 4 {
                return false;
            }
            match &f.interaction_workbench {
                Some(w) => w.clone(),
                None => return false,
            }
        };
        let Some(wb) = downcast::<Workbench>(&w) else { return false };
        let (ty, level) = (wb.workbench_type(), wb.level());
        if workbench_requires_fuel(ty, level) && wb.fuel_count() == 0 {
            return true;
        }
        workbench_requires_electricty(ty, level) && (wb.available_electricity() as i16) == 0
    }

    /// `-isInteractingWithAnyInteractionObject`.
    pub fn is_interacting_with_any_interaction_object(&self) -> bool {
        self.bh.borrow().state.interaction_type == 5
    }

    /// `-setInteractionObject:`.
    pub fn set_interaction_object(&self, o: Option<ObjRef>) {
        self.bh.borrow_mut().interaction_object = o;
    }

    /// `-interactionObject`.
    pub fn interaction_object(&self) -> Option<ObjRef> {
        self.bh.borrow().interaction_object.clone()
    }

    /// `-isHeadingForSquare:`.
    pub fn is_heading_for_square(&self, sq: IntPair) -> bool {
        let f = self.bh.borrow();
        if f.final_goal_square == sq {
            return true;
        }
        let Some(path) = &f.path else { return false };
        let Some(last) = path.first() else { return false };
        let idx = last.object_for_key("index").map_or(0, |v| v.int_value());
        let Some(world) = self.world_rc() else { return false };
        let p = crate::world::tiles::get_world_pos_for_world_index(idx, &world);
        p.x == sq.x && p.y == sq.y
    }

    /// `-currentTraverseToKeyFrame`.
    pub fn current_traverse_to_key_frame(&self) -> i32 {
        self.bh.borrow().traverse_to_key_frame
    }

    // ================================================================================================
    // 4.25 / 4.26
    // ================================================================================================

    /// `-stopRiding`.
    pub fn stop_riding(&self) {
        let Some(world) = self.world_rc() else { return };
        {
            let mut f = self.bh.borrow_mut();
            if f.is_in_jet_pack_free_flight_mode {
                f.is_in_jet_pack_free_flight_mode = false;
                return;
            }
        }
        let (ride, anim) = {
            let f = self.bh.borrow();
            (f.ride_object.clone(), f.state.animation_type)
        };
        if !(ride.is_some() || matches!(anim, 0x1b | 0x21 | 0x1c | 0x1d | 0x1e)) {
            return;
        }
        let me = self.me();
        if !self.is_net() {
            self.set_update_needs_to_be_sent(true);
        }
        let mut new_pos = self.pos();
        if let Some(ride) = &ride {
            let rp = ride.as_ridable().map_or([0.0f32; 4], |r| r.rider_pos_for_blockhead(&me));
            let ride_pos = [rp[0] + 0.0f32, rp[1] + 0.5f32, rp[2] + 0.0f32, rp[3]];
            let fp = Vector2 { x: ride_pos[0], y: ride_pos[1] };
            self.base.borrow_mut().float_pos = fp;
            self.bh.borrow_mut().render_pos = fp;
            new_pos = IntPair { x: f32_to_i32(ride_pos[0]), y: f32_to_i32(ride_pos[1]) };
            if let Some(r) = ride.as_ridable() {
                r.remove_rider(ride, &me);
            }
            self.bh.borrow_mut().ride_object = None;
        }
        if tile_is_solid(tat(&world, new_pos.x, new_pos.y)) && !tile_is_solid(tat(&world, new_pos.x, new_pos.y + 1)) {
            new_pos.y += 1;
        }
        if new_pos != self.pos() {
            self.update_position(new_pos);
            let t = tat(&world, new_pos.x, new_pos.y);
            let mut f = self.bh.borrow_mut();
            f.next_to_square = new_pos;
            f.to_square = new_pos;
            f.from_square = new_pos;
            f.prev_from_square = new_pos;
            f.from_tile = t;
            f.to_tile = t;
        }
        {
            let mut f = self.bh.borrow_mut();
            f.state.animation_type = va::STAND;
            f.path_needs_recalculated = true;
            f.path_recalculation_is_fall_path = true;
        }
        self.stop_fishing();
    }

    /// `-pickupDynamicObject:` (server branch; the 17-byte client packet is not ported).
    pub fn pickup_dynamic_object(&self, obj: &ObjRef) {
        if obj.needs_removed() {
            return;
        }
        // `[world client] == nil` on the dedicated server
        actions_host().remove_standard_object(self, obj);
        let ft = obj.freeblock_creation_item_type();
        if ft != 0 {
            let sd = obj.free_block_creation_save_dict(obj).map(crate::plist::value::PlistValue::Dict);
            host().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                obj.pos(),
                ItemType(ft as u32),
                0,
                0,
                None,
                sd.as_ref(),
                false,
                1,
                Some(&self.me()),
            );
        }
    }
}

/// `[o isDoubleHeight]` for a Workbench / InteractionObject (NO otherwise).
fn object_is_double_height(o: &ObjRef) -> bool {
    if let Some(w) = downcast::<Workbench>(o) {
        return w.is_double_height();
    }
    o.as_interaction_like().map_or(false, |i| i.is_double_height())
}

/// `[wb type]` as the raw `WorkbenchType`.
fn workbench_type_of(o: &ObjRef) -> u32 {
    match downcast::<Workbench>(o) {
        Some(w) => w.workbench_type().0,
        None => host().workbench_type(o) as u32,
    }
}

// ====================================================================================================
// tests
// ====================================================================================================

/// Shared fixtures of the four files of this module: one recording object implementing both seams.
#[cfg(test)]
pub(crate) mod testhost {
    use super::*;
    use crate::blockhead::action::ActionBridge;
    use crate::blockhead::blockhead::testutil::*;
    use crate::blockhead::blockhead::BlockheadHost;
    use crate::items::inventory_item::InventoryItem;
    use crate::objects::base::DynamicObject;
    use crate::plist::value::PlistValue;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct AHost {
        pub log: RefCell<Vec<String>>,
        pub plant: RefCell<Option<ObjRef>>,
        pub fbs: RefCell<Vec<ObjRef>>,
        pub fb_exist: Cell<bool>,
        pub dw_client: Cell<bool>,
        pub light_index: Cell<Option<i32>>,
        pub others: RefCell<Vec<ObjRef>>,
        pub bh_at: RefCell<Option<ObjRef>>,
        pub io: RefCell<Option<ObjRef>>,
        pub npc: RefCell<Option<ObjRef>>,
        pub io_type: Cell<i32>,
        pub torch: RefCell<Option<ObjRef>>,
        /// Scripted answers of the other `*_at_pos` lookups (`wire`, `column`, `painting`, `ladder`, `stairs`, `door`,
        /// `window`, `egg`, `shaft`, `motor`).
        pub misc: RefCell<std::collections::HashMap<&'static str, ObjRef>>,
        /// `workbench_type` / `ridable_blockhead_can_ride_using_item` answers for stub objects.
        pub wb_type: Cell<i32>,
        pub can_ride: Cell<bool>,
    }
    impl AHost {
        pub fn misc_obj(&self, k: &str) -> Option<ObjRef> {
            self.misc.borrow().get(k).cloned()
        }
        pub fn install() -> Rc<AHost> {
            let h = Rc::new(AHost::default());
            crate::blockhead::blockhead::set_host(h.clone());
            crate::blockhead::inventory_model::set_inventory_host(h.clone());
            set_actions_host(h.clone());
            h
        }
        pub fn take(&self) -> Vec<String> {
            std::mem::take(&mut *self.log.borrow_mut())
        }
        fn rec(&self, s: String) {
            self.log.borrow_mut().push(s);
        }
    }
    impl crate::blockhead::inventory_model::InventoryHostStub for AHost {
        fn action_goal_interaction(&self, a: &Rc<dyn std::any::Any>) -> i32 {
            ActionBridge.action_goal_interaction(a)
        }
        fn action_craft_count_or_extra_data(&self, a: &Rc<dyn std::any::Any>) -> i32 {
            ActionBridge.action_craft_count_or_extra_data(a)
        }
        fn action_interaction_item(&self, a: &Rc<dyn std::any::Any>) -> Option<ItemRef> {
            ActionBridge.action_interaction_item(a)
        }
        fn action_interaction_item_index(&self, a: &Rc<dyn std::any::Any>) -> i32 {
            ActionBridge.action_interaction_item_index(a)
        }
        fn action_interaction_item_sub_index(&self, a: &Rc<dyn std::any::Any>) -> i32 {
            ActionBridge.action_interaction_item_sub_index(a)
        }
    }
    impl crate::blockhead::blockhead::BlockheadHostStub for AHost {
        fn action_in_progress(&self, a: &Rc<dyn std::any::Any>) -> bool {
            BlockheadHost::action_in_progress(&ActionBridge, a)
        }
        fn action_goal_tile_pos(&self, a: &Rc<dyn std::any::Any>) -> IntPair {
            BlockheadHost::action_goal_tile_pos(&ActionBridge, a)
        }
        fn action_interaction_object_id(&self, a: &Rc<dyn std::any::Any>) -> i64 {
            BlockheadHost::action_interaction_object_id(&ActionBridge, a)
        }
        fn set_door_at_pos_to_open_direction(&self, pos: IntPair, open: bool, dir: i32) {
            self.rec(format!("door:{},{}:{}:{}", pos.x, pos.y, open, dir));
        }
        fn report_achievement_with_identifier(&self, id: &str) {
            self.rec(format!("ach:{}", id));
        }
        fn fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(
            &self,
            pos: IntPair,
            i: i32,
            b: Option<&ObjRef>,
        ) {
            self.rec(format!("load:{},{}:{}:{}", pos.x, pos.y, i, b.is_some()));
        }
        fn server_client_light_block_index_for_net_id(&self, _id: Option<&str>) -> Option<i32> {
            self.light_index.get()
        }
        fn get_plant_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.plant.borrow().clone()
        }
        fn torch_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.torch.borrow().clone()
        }
        fn free_blocks_at_pos(&self, _p: IntPair) -> Vec<ObjRef> {
            self.fbs.borrow().clone()
        }
        fn wire_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("wire")
        }
        fn workbench_type(&self, _wb: &ObjRef) -> i32 {
            self.wb_type.get()
        }
        fn interaction_object_with_id(&self, _id: i64) -> Option<ObjRef> {
            self.io.borrow().clone()
        }
        fn ridable_blockhead_can_ride_using_item(&self, _o: &ObjRef, _b: &Blockhead, _item: i32) -> bool {
            self.can_ride.get()
        }
        fn column_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("column")
        }
        fn painting_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("painting")
        }
        fn ladder_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("ladder")
        }
        fn stairs_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("stairs")
        }
        fn door_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("door")
        }
        fn window_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("window")
        }
        fn egg_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("egg")
        }
        fn elevator_shaft_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("shaft")
        }
        fn elevator_motor_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.misc_obj("motor")
        }
        fn interaction_object_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.io.borrow().clone()
        }
        fn npc_with_id(&self, _id: u64) -> Option<ObjRef> {
            self.npc.borrow().clone()
        }
        fn drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
            &self,
            _b: &Blockhead,
            index: i32,
            sub: i32,
            count: i32,
            ign: bool,
        ) {
            self.rec(format!("drop:{}:{}:{}:{}", index, sub, count, ign));
        }
        fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            &self,
            pos: IntPair,
            t: ItemType,
            a: u16,
            b: u16,
            _s: Option<&[Vec<ItemRef>]>,
            d: Option<&PlistValue>,
            hovers: bool,
            sound: i32,
            p: Option<&ObjRef>,
        ) {
            self.rec(format!(
                "fb:{},{}:{}:{}:{}:{}:{}:{}:{}",
                pos.x,
                pos.y,
                t.0,
                a,
                b,
                d.is_some(),
                hovers,
                sound,
                p.is_some()
            ));
        }
    }
    impl ActionsHostStub for AHost {
        fn free_blocks_exist_at_pos(&self, _b: &Blockhead, _p: IntPair) -> bool {
            self.fb_exist.get()
        }
        fn blockhead_at_pos(&self, _b: &Blockhead, _p: IntPair) -> Option<ObjRef> {
            self.bh_at.borrow().clone()
        }
        fn interaction_object_type_for_object_at_pos(&self, _b: &Blockhead, _p: IntPair) -> i32 {
            self.io_type.get()
        }
        fn load_glow_block_if_needed_at_pos_tile(&self, _b: &Blockhead, p: IntPair, _t: &TileRef) {
            self.rec(format!("glow:{},{}", p.x, p.y));
        }
        fn add_torch_at_pos_of_type(&self, _b: &Blockhead, p: IntPair, t: i32) {
            self.rec(format!("torch:{},{}:{}", p.x, p.y, t));
        }
        fn create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(
            &self,
            _b: &Blockhead,
            _t: &TileRef,
            p: IntPair,
            troll: bool,
            treasure: bool,
        ) {
            self.rec(format!("chest:{},{}:{}:{}", p.x, p.y, troll, treasure));
        }
        fn world_contents_changed_at_pos(&self, _b: &Blockhead, p: IntPair) {
            self.rec(format!("contents:{},{}", p.x, p.y));
        }
        fn remove_standard_object(&self, _b: &Blockhead, _o: &ObjRef) {
            self.rec("removestd".to_string());
        }
        fn all_blockheads_including_net(&self, _b: &Blockhead) -> Vec<ObjRef> {
            self.others.borrow().clone()
        }
        fn dynamic_world_is_client(&self) -> bool {
            self.dw_client.get()
        }
    }

    /// Replaces all inventory slots 1..7 by empty ones and puts one item of `ty` in slot 1.
    pub fn give(b: &Blockhead, ty: u32, data_b: u16) -> ItemRef {
        let f = b.bh.borrow();
        for i in 1..f.inventory_items.len() {
            f.inventory_items[i].borrow_mut().clear();
        }
        let it = InventoryItem::new(ItemType(ty), 0, data_b, None, None);
        f.inventory_items[1].borrow_mut().push(it.clone());
        it
    }

    /// A real `DynamicWorld` on `world` (needed to build real `FreeBlock`s).
    pub fn real_dw(world: &Rc<World>, name: &str) -> Rc<crate::dynworld::state::DynamicWorld> {
        use crate::dynworld::state::{testutil::dbs, testutil::scratch, DynamicWorld, DynamicWorldInit};
        let (main, dwdb) = dbs(name);
        let dir = scratch(&format!("{name}_save"));
        DynamicWorld::new(DynamicWorldInit {
            world: world.clone(),
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(dir),
        })
    }

    /// A hovering `FreeBlock` with a forced unique id.
    pub fn mk_fb(
        world: &Rc<World>,
        dw: &Rc<crate::dynworld::state::DynamicWorld>,
        pos: IntPair,
        ty: i32,
        id: u64,
    ) -> ObjRef {
        let fb =
            crate::physics_objects::free_block::FreeBlock::new(world, dw, pos, ty, 0, 0, None, None, None, true, None)
                .unwrap();
        fb.base().borrow_mut().unique_id = id;
        fb
    }

    /// Lights every tile of the arena block (`light != 0`) so `tileIsLitForSelf` holds.
    pub fn light_all(w: &World) {
        for y in 0..32 {
            for x in 0..32 {
                tile(w, x, y).update(|t| {
                    t.light = 1;
                    t.sun_light = 200;
                });
            }
        }
    }

    /// Puts a jetpack with `data_b` used fuel into `subItems[2]` of the clothing container and equips it.
    pub fn equip_jetpack(b: &Blockhead, data_b: u16) {
        let it = InventoryItem::new(ItemType(278), 0, data_b, None, None);
        let f = b.bh.borrow();
        let cont = f.inventory_items[0].borrow();
        let c0 = cont[0].borrow();
        c0.sub_items.as_ref().unwrap()[2].borrow_mut().clear();
        c0.sub_items.as_ref().unwrap()[2].borrow_mut().push(it);
        drop(c0);
        drop(cont);
        drop(f);
        b.bh.borrow_mut().shirt_item_type = 278;
    }

    /// The standard arena: returns `(world, o)`; tiles are addressed with `testutil::tile`.
    pub fn arena() -> (Rc<World>, Rc<dyn DynamicObject>) {
        let (world, dw, dwref) = setup();
        let o = new_bh(&world, &dwref, 0);
        // the object holds only weak links: keep the mock dynamic world alive for the test
        std::mem::forget((dw, dwref));
        (world, o)
    }
}

#[cfg(test)]
mod tests {
    use super::testhost::*;
    use super::*;
    use crate::blockhead::blockhead::testutil::*;
    use crate::items::inventory_item::InventoryItem;

    fn st(b: &Blockhead) -> crate::types::structs::BlockheadState {
        b.bh.borrow().state
    }

    #[test]
    fn wrap_and_sort() {
        // W = 32 macro tiles * 32 = 1024
        assert_eq!(wrap_x_distance(600, 32), -424);
        assert_eq!(wrap_x_distance(-600, 32), 424);
        assert_eq!(wrap_x_distance(511, 32), 511);
        assert_eq!(wrap_x_distance(512, 32), -512);
        assert_eq!(wrap_x_distance(-512, 32), -512);
        assert_eq!(wrap_x_distance(-513, 32), 511);
        assert_eq!(freeblock_uid_desc(5, 5), Ordering::Greater);
        assert_eq!(freeblock_uid_desc(9, 5), Ordering::Less);
        let mut v: Vec<ObjRef> = [5u64, 9, 7]
            .iter()
            .map(|id| {
                let o = stub(1, true);
                o.base().borrow_mut().unique_id = *id;
                o
            })
            .collect();
        sort_freeblocks_desc(&mut v);
        assert_eq!(v.iter().map(|o| o.unique_id()).collect::<Vec<_>>(), vec![9, 7, 5]);
    }

    #[test]
    fn tool_table() {
        let (_w, o) = arena();
        let b = bh_of(&o);
        let t = |ty: i32, item: u32| b.is_correct_tool_for_back_wall_of_type_for_item(ty, ItemType(item));
        assert!(!t(6, 0));
        assert!(t(6, 0x10) && t(6, 0x28) && t(6, 0x40) && t(6, 0x59));
        assert!(!t(6, 6) && !t(0x46, 8));
        assert!(t(1, 8) && t(1, 0x22) && t(1, 0x2b) && t(1, 0x42) && t(1, 0x5a) && t(1, 0x11d));
        assert!(t(1, 0x5d) && t(1, 0x62) && !t(1, 0x63) && !t(1, 0x5c));
        assert!(!t(1, 6) && !t(1, 0x21) && !t(1, 0x46));
        assert!(t(0x18, 6) && t(0x3b, 0x21) && t(4, 0x46));
        assert!(t(9, 6) && t(0x20, 0x21) && t(9, 0x46) && !t(9, 8) && !t(0x2, 6));
    }

    #[test]
    fn can_dig_back_wall_rules() {
        let (w, o) = arena();
        let b = bh_of(&o);
        let pick = give(b, 8, 0);
        let pos = IntPair { x: 10, y: 12 };
        let tl = tile(&w, 10, 12);
        tl.update(|t| {
            t.type_index = 2;
            t.back_wall_type_index = 1;
        });
        for (x, y) in [(10, 13), (11, 12), (9, 12), (10, 11)] {
            tile(&w, x, y).update(|t| t.back_wall_type_index = 1);
        }
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, Some(&pick), false));
        tile(&w, 10, 13).update(|t| t.back_wall_type_index = 2);
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, Some(&pick), false));
        tile(&w, 11, 12).update(|t| t.back_wall_type_index = 3);
        assert!(b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, Some(&pick), false));
        // nil item / nil tile / wrong tool / occupied tile
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, None, false));
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(None, pos, Some(&pick), false));
        let axe = give(b, 6, 0);
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, Some(&axe), false));
        let pick = give(b, 8, 0);
        tl.update(|t| t.foreground_contents = 0x31);
        assert!(!b.can_dig_back_wallfor_tile_at_pos_with_item_include_actions(Some(&tl), pos, Some(&pick), false));
    }

    /// : gather-speed table rows including the f64 multiplications and expert halving.
    #[test]
    fn gather_speed_rows() {
        let h = AHost::install();
        let _ = h;
        let (w, o) = arena();
        let b = bh_of(&o);
        let tl = tile(&w, 10, 3);
        let run = |it_: u32, ty: u32, ti: u8, contents: u8, fg: u8| -> (f32, u32) {
            tl.update(|t| {
                t.type_index = ti;
                t.contents = contents;
                t.foreground_contents = fg;
            });
            let item = InventoryItem::new(ItemType(ty), 0, 0, None, None);
            {
                let mut f = b.bh.borrow_mut();
                f.interacting_tile = Some(tl);
                f.interaction_item = Some(item);
                f.state.interaction_type = it_;
                f.state.sub_animation_type = 99;
            }
            b.update_gather_speed_and_animation_for_current_interation_and_item();
            let s = st(b);
            (s.gather_speed, s.sub_animation_type)
        };
        // rock with pickaxes
        assert_eq!(run(1, 8, 1, 0, 0), (4.0, 1));
        assert_eq!(run(1, 0x22, 1, 0, 0).0, 8.0);
        assert_eq!(run(1, 0x5e, 1, 0, 0).0, 8.0);
        assert_eq!(run(1, 0x2b, 1, 0, 0).0, 12.0);
        assert_eq!(run(1, 0x42, 1, 0, 0).0, 16.0);
        assert_eq!(run(1, 0x5d, 1, 0, 0).0, 24.0);
        assert_eq!(run(1, 0x62, 1, 0, 0).0, 32.0);
        assert_eq!(run(1, 0x11d, 1, 0, 0).0, 48.0);
        assert_eq!(run(1, 6, 1, 0, 0).0, 1.0);
        // contents multipliers
        assert_eq!(run(1, 0x22, 1, 0x40, 0).0, 16.0);
        assert_eq!(run(1, 0x22, 1, 0x3f, 0).0, 1.0);
        assert_eq!(run(1, 0x22, 1, 0x05, 0).0, 2.0);
        assert_eq!(run(1, 0x22, 1, 0x67, 0).0, 8.0);
        // block types
        assert_eq!(run(1, 8, 0x35, 0, 0).0, 0.5);
        assert_eq!(run(1, 8, 0x37, 0, 0).0, 0.25);
        assert_eq!(run(1, 0x42, 0x38, 0, 0).0, ((16.0f32 as f64) * 0.03125) as f32);
        assert_eq!(run(1, 0x5d, 0x39, 0, 0).0, ((24.0f32 as f64) * 0.015625) as f32);
        assert_eq!(run(1, 0x62, 0x44, 0, 0).0, ((32.0f32 as f64) * 0.01171875) as f32);
        assert_eq!(run(1, 0x62, 0x45, 0, 0).0, 0.375);
        // dirt with spades, magnet on black sand
        assert_eq!(run(1, 0x10, 6, 0, 0).0, 8.0);
        assert_eq!(run(1, 0x40, 6, 0, 0).0, 16.0);
        assert_eq!(run(1, 0x28, 6, 0, 0).0, 32.0);
        assert_eq!(run(1, 0x59, 6, 0, 0).0, 16.0);
        assert_eq!(run(1, 0x3, 6, 0, 0).0, 2.0);
        assert_eq!(run(1, 0xaf, 0x3a, 0, 0).0, 32.0);
        assert_eq!(run(1, 0x10, 6, 0x05, 0).0, 4.0);
        // wood / glass / ice
        assert_eq!(run(1, 6, 9, 0, 0).0, 8.0);
        assert_eq!(run(1, 0x21, 9, 0, 0).0, 16.0);
        assert_eq!(run(1, 0x46, 0x20, 0, 0).0, 32.0);
        assert_eq!(run(1, 8, 9, 0, 0).0, 2.0);
        assert_eq!(run(1, 3, 4, 0, 0).0, 4.0);
        assert_eq!(run(1, 0x42, 4, 0, 0).0, 256.0); // metal tool on glass / ice
                                                    // shortcuts to 256 and sub animations
        assert_eq!(run(0x18, 8, 1, 0, 0), (256.0, 1));
        assert_eq!(run(0x17, 8, 1, 0, 0x30).0, 256.0);
        assert_eq!(run(0x17, 8, 1, 0x30, 0).0, 256.0); // contents == '0' shortcut applies to the whole dig group
        assert_eq!(run(1, 8, 1, 0x30, 0).0, 256.0);
        assert_eq!(run(1, 8, 2, 0x2e, 0).0, 256.0);
        assert_eq!(run(4, 8, 1, 0, 0), (0.5, 2));
        assert_eq!(run(10, 8, 1, 0, 0), (0.5, 3));
        assert_eq!(run(0x14, 8, 1, 0, 0), (0.5, 8));
        assert_eq!(run(7, 8, 1, 0, 0), (0.5, 1));
        assert_eq!(run(0, 8, 1, 0, 0), (0.5, 99));
        // expert mode halves once, only below 255
        w.set_expert_mode(true);
        assert_eq!(run(1, 0x22, 1, 0, 0).0, 4.0);
        assert_eq!(run(0x18, 8, 1, 0, 0).0, 256.0);
        assert_eq!(run(5, 8, 1, 0, 0).0, 0.5);
        // nil tile: S stays 0.5 and no halving
        {
            let mut f = b.bh.borrow_mut();
            f.interacting_tile = None;
            f.state.interaction_type = 1;
        }
        b.update_gather_speed_and_animation_for_current_interation_and_item();
        assert_eq!(st(b).gather_speed, 0.5);
    }

    fn act(b: &Blockhead, g: u32, item: Option<ItemRef>, goal: IntPair) -> ActionRef {
        use crate::types::enums::PathType;
        Action::init_with_goal_pos_goal_interaction_path_type_interaction_item_item_index_item_sub_index_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai_inventory_change(
            goal,
            InteractionType(g),
            PathType(0),
            item,
            -1,
            -1,
            0,
            None,
            0,
            false,
            None,
        )
        .clone_for_test(b)
    }

    use super::super::action::Action;
    trait CloneForTest {
        fn clone_for_test(self, b: &Blockhead) -> ActionRef;
    }
    impl CloneForTest for ActionRef {
        fn clone_for_test(self, _b: &Blockhead) -> ActionRef {
            self
        }
    }

    /// : rows of the good/bad table and the 12-byte layout.
    #[test]
    fn good_or_bad_rows() {
        let h = AHost::install();
        let _ = h;
        let (w, o) = arena();
        let b = bh_of(&o);
        let goal = IntPair { x: 10, y: 3 };
        let tl = tile(&w, 10, 3);
        let it = |t: u32, db: u16| Some(InventoryItem::new(ItemType(t), 0, db, None, None));
        let r = |g: u32, item: Option<ItemRef>| {
            let a = act(b, g, item, goal);
            let x = b.good_or_bad_interaction_for_action(&a);
            (x.good_or_bad, x.render_item_type, x.render_item_data_a, x.render_item_data_b)
        };
        assert_eq!(r(2, it(0x69, 7)), (0, 0x69, 0, 7));
        assert_eq!(r(3, None), (0, 0, 0, 0));
        assert_eq!(r(6, it(0x90, 3)), (0, 0x90, 0, 3));
        assert_eq!(r(9, it(13, 0)), (0, 13, 0, 0));
        assert_eq!(r(7, None), (0, 1, 0, 0));
        assert_eq!(r(8, None), (0, 1, 0, 0));
        assert_eq!(r(18, None), (0, 1, 0, 0));
        assert_eq!(r(28, None), (0, 1, 0, 0));
        assert_eq!(r(12, None), (0, 0x4a, 0, 0));
        assert_eq!(r(13, None), (0, 0xf, 0, 0));
        assert_eq!(r(14, it(0x66, 0)), (0, 0x66, 0, 0));
        assert_eq!(r(15, it(0x67, 9)), (0, 0x67, 0, 9));
        assert_eq!(r(16, it(0x68, 9)), (0, 0x68, 0, 0));
        assert_eq!(r(17, None), (0, 0x6f, 0, 0));
        assert_eq!(r(21, None), (0, 0xcc, 0, 0));
        assert_eq!(r(11, it(5, 0)), (0, 0, 0, 0));
        assert_eq!(r(22, it(5, 0)), (0, 0, 0, 0));
        assert_eq!(r(26, it(5, 0)), (0, 0, 0, 0));
        assert_eq!(r(0, it(5, 0)), (0, 0, 0, 0));
        assert_eq!(r(29, it(5, 0)), (0, 0, 0, 0));
        assert_eq!(r(24, it(8, 0)), (1, 8, 0, 0));
        // stab / ranged
        assert_eq!(r(10, None), (0, 1, 0, 0));
        assert_eq!(r(10, it(7, 0)), (1, 7, 0, 0));
        assert_eq!(r(20, it(0x2a, 0)), (1, 0x2a, 0, 0));
        assert_eq!(r(10, it(0x11e, 0)), (1, 0x11e, 0, 0));
        assert_eq!(r(10, it(0x4a, 0)), (0, 1, 0, 0));
        // dig rows
        tl.update(|t| {
            t.type_index = 6;
            t.contents = 0;
            t.foreground_contents = 0;
        });
        assert_eq!(r(1, it(0x10, 0)), (1, 0x10, 0, 0));
        assert_eq!(r(1, it(0x67, 0)), (0, 1, 0, 0));
        assert_eq!(r(1, None), (0, 1, 0, 0));
        tl.update(|t| t.type_index = 0x3a);
        assert_eq!(r(1, it(0xaf, 0)), (1, 0xaf, 0, 0));
        tl.update(|t| t.type_index = 1);
        assert_eq!(r(1, it(8, 0)), (1, 8, 0, 0));
        assert_eq!(r(1, it(6, 0)), (-1, 6, 0, 0));
        tl.update(|t| t.contents = 0x3f);
        assert_eq!(r(1, it(0xaf, 0)), (1, 0xaf, 0, 0));
        tl.update(|t| {
            t.contents = 0;
            t.type_index = 0x18;
        });
        assert_eq!(r(1, it(6, 0)), (1, 6, 0, 0));
        tl.update(|t| t.type_index = 9);
        assert_eq!(r(1, it(0x21, 0)), (1, 0x21, 0, 0));
        assert_eq!(r(1, it(0x68, 0)), (0, 1, 0, 0));
        // foreground dig
        tl.update(|t| {
            t.type_index = 2;
            t.foreground_contents = 0x62;
        });
        assert_eq!(r(23, it(0xd1, 0)), (1, 0xd1, 0, 0));
        tl.update(|t| t.foreground_contents = 0x30);
        assert_eq!(r(23, it(8, 0)), (0, 1, 0, 0));
        tl.update(|t| t.foreground_contents = 0x43);
        assert_eq!(r(23, it(0x19, 0)), (1, 0x19, 0, 0));
        assert_eq!(r(23, it(0x10, 0)), (1, 0x10, 0, 0));
        assert_eq!(r(23, it(0x67, 0)), (0, 1, 0, 0));
        // byte layout of the persisted result
        let a = act(b, 15, it(0x67, 9), goal);
        a.set_interaction_test_result(b.good_or_bad_interaction_for_action(&a));
        let d = a.get_save_dict();
        let bytes = d
            .map
            .get(&"interactionTestResult".to_string())
            .and_then(|v| if let crate::plist::value::PlistValue::Data(x) = v { Some(x.bytes.clone()) } else { None })
            .expect("data");
        assert_eq!(bytes, vec![0, 0, 0, 0, 0x67, 0, 0, 0, 0, 0, 9, 0]);
    }

    #[test]
    fn current_good_or_bad() {
        let (_w, o) = arena();
        let b = bh_of(&o);
        assert_eq!(b.current_interaction_is_good_or_bad(), 0);
        let a = act(b, 1, None, IntPair { x: 1, y: 1 });
        a.set_interaction_test_result(InteractionTestResult {
            good_or_bad: -1,
            render_item_type: 0,
            render_item_data_a: 0,
            render_item_data_b: 0,
        });
        b.bh.borrow_mut().action_queue.push(a);
        assert_eq!(b.current_interaction_is_good_or_bad(), 0);
        b.bh.borrow_mut().state.interacting = 1;
        assert_eq!(b.current_interaction_is_good_or_bad(), -1);
        assert_eq!(b.current_interaction_type(), InteractionType(0));
    }

    /// Runs the rule engine for the standard arena.
    fn rule(
        b: &Blockhead,
        w: &World,
        x: i32,
        y: i32,
        item: Option<&ItemRef>,
        allow: bool,
        face: i32,
    ) -> (InteractionType, bool) {
        let tl = tile(w, x, y);
        let mut rej = false;
        let r = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            Some(&tl),
            IntPair { x, y },
            item,
            &mut rej,
            false,
            face,
            allow,
        );
        (r, rej)
    }

    /// : selected rows of the rule engine.
    #[test]
    fn rule_engine_rows() {
        let h = AHost::install();
        let (w, o) = arena();
        let b = bh_of(&o);
        // make every tile lit for the self test
        for y in 0..32 {
            for x in 0..32 {
                tile(&w, x, y).update(|t| t.light = 1);
            }
        }
        let none = InteractionType(0);
        // nothing held: plain dirt tile can be dug, air gives NONE
        tile(&w, 10, 5).update(|t| t.type_index = 6);
        assert_eq!(rule(b, &w, 10, 5, None, true, 0).0, InteractionType::INTERACTION_DIG);
        assert_eq!(rule(b, &w, 10, 5, None, false, 0).0, none);
        assert_eq!(rule(b, &w, 10, 12, None, true, 0).0, none);
        // lava, depth limit
        tile(&w, 10, 5).update(|t| t.type_index = 0x1f);
        assert_eq!(rule(b, &w, 10, 5, None, true, 0).0, none);
        tile(&w, 10, 5).update(|t| t.type_index = 1);
        assert_eq!(rule(b, &w, 10, 5, None, true, 0).0, InteractionType::INTERACTION_DIG);
        // water bucket on air
        let wb = give(b, 0x69, 0);
        assert_eq!(rule(b, &w, 10, 12, Some(&wb), true, 0).0, InteractionType::INTERACTION_PLACE_SOLID_BLOCK);
        // an item that is not in the inventory is never placed
        let ghost = InventoryItem::new(ItemType(0x69), 0, 0, None, None);
        b.bh.borrow().inventory_items[1].borrow_mut().clear();
        assert_eq!(rule(b, &w, 10, 12, Some(&ghost), true, 0).0, none);
        // ember on a burnable block
        let ember = give(b, 13, 0);
        tile(&w, 10, 6).update(|t| t.type_index = 9);
        assert_eq!(rule(b, &w, 10, 6, Some(&ember), true, 0).0, InteractionType::INTERACTION_BURN);
        assert_eq!(rule(b, &w, 10, 6, Some(&ember), false, 0).0, none);
        // kelp on a solid tile under water
        let kelp = give(b, 0x90, 0);
        tile(&w, 12, 7).update(|t| t.type_index = 6);
        tile(&w, 12, 8).update(|t| t.type_index = 3);
        assert_eq!(rule(b, &w, 12, 7, Some(&kelp), true, 0).0, InteractionType::INTERACTION_SOW);
        // tin bucket in water
        let tin = give(b, 0x66, 0);
        tile(&w, 14, 12).update(|t| {
            t.type_index = 3;
            t.partial_content_left = 0x21;
        });
        assert_eq!(rule(b, &w, 14, 12, Some(&tin), true, 0).0, InteractionType::INTERACTION_FILL_BUCKET);
        tile(&w, 14, 12).update(|t| t.partial_content_left = 0x20);
        assert_ne!(rule(b, &w, 14, 12, Some(&tin), true, 0).0, InteractionType::INTERACTION_FILL_BUCKET);
        // fish bucket in deep water
        let fish = give(b, 0x9e, 0);
        tile(&w, 14, 14).update(|t| {
            t.type_index = 3;
            t.partial_content_left = 0x80;
        });
        assert_eq!(rule(b, &w, 14, 14, Some(&fish), true, 0).0, InteractionType::INTERACTION_PLACE_SOLID_BLOCK);
        // rail: top face places, other faces do not
        let rail = give(b, 199, 0);
        assert_eq!(rule(b, &w, 16, 12, Some(&rail), true, 3).0, InteractionType::INTERACTION_PLACE_ITEM);
        // crowbar on a rail: only the top face (P1) or the P2 quirk digs it, a station disables P1
        let crow = give(b, 0xd1, 0);
        tile(&w, 18, 12).update(|t| t.foreground_contents = 0x62);
        assert_eq!(rule(b, &w, 18, 12, Some(&crow), true, 3).0, InteractionType::INTERACTION_DIG_FOREGROUND);
        assert_eq!(rule(b, &w, 18, 12, Some(&crow), true, 2).0, InteractionType::INTERACTION_DIG_FOREGROUND);
        // workbench contents: CRAFT, regardless of allowProtected (the server is admin)
        tile(&w, 20, 12).update(|t| t.contents = 0x2e);
        assert_eq!(rule(b, &w, 20, 12, None, false, 0).0, InteractionType::INTERACTION_CRAFT);
        w.set_expert_mode(true);
        w.set_is_admin(false);
        assert_eq!(rule(b, &w, 20, 12, None, false, 0).0, none);
        w.set_is_admin(true);
        // sign placement writes zIndex = 1 (P2a)
        let sign = give(b, 0xa3, 0);
        b.bh.borrow_mut().z_index = 0;
        let _ = rule(b, &w, 22, 12, Some(&sign), true, 0);
        assert_eq!(b.bh.borrow().z_index, 1);
        // ghost plant: foreground plant tile without a plant object is cleared
        tile(&w, 24, 12).update(|t| t.foreground_contents = 0x43);
        h.take();
        let _ = rule(b, &w, 24, 12, None, true, 0);
        assert_eq!(tile(&w, 24, 12).get().foreground_contents, 0);
        assert!(h.take().contains(&"contents:24,12".to_string()));
        // above the depth limit
        let t12 = tile(&w, 10, 12);
        let mut rej = false;
        let r = b.interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(
            Some(&t12),
            IntPair { x: 10, y: 0x3fa },
            None,
            &mut rej,
            false,
            0,
            true,
        );
        assert_eq!(r, none);
    }

    /// : free blocks are scanned 9, 7, 5; zIndex = 1; an unpickable block does not stop it.
    #[test]
    fn freeblock_scan_order_and_rejection() {
        use crate::dynworld::state::{testutil::dbs, testutil::scratch, DynamicWorld, DynamicWorldInit};
        use crate::physics_objects::free_block::FreeBlock;
        let h = AHost::install();
        let (world, o) = arena();
        let b = bh_of(&o);
        let (main, dwdb) = dbs("act_fb");
        let dir = scratch("act_fb_save");
        let rdw = DynamicWorld::new(DynamicWorldInit {
            world: world.clone(),
            world_tile_loader: None,
            server: None,
            server_clients: None,
            cache: None,
            tree_density_noise_function: None,
            season_offset_noise_function: None,
            app_database: None,
            world_database: main,
            dynamic_object_database: dwdb,
            world_save_directory_override: Some(dir),
        });
        let mk = |id: u64| {
            let fb =
                FreeBlock::new(&world, &rdw, IntPair { x: 10, y: 12 }, 3, 0, 0, None, None, None, true, None).unwrap();
            fb.base().borrow_mut().unique_id = id;
            fb
        };
        let (f5, f9, f7) = (mk(5), mk(9), mk(7));
        *h.fbs.borrow_mut() = vec![f5.clone(), f9.clone(), f7.clone()];
        for y in 0..32 {
            for x in 0..32 {
                tile(&world, x, y).update(|t| t.light = 1);
            }
        }
        b.bh.borrow_mut().z_index = 0;
        // no free blocks according to the dynamic world: nothing
        assert_eq!(rule(b, &world, 10, 12, None, false, 0).0, InteractionType(0));
        h.fb_exist.set(true);
        assert_eq!(rule(b, &world, 10, 12, None, false, 0).0, InteractionType::INTERACTION_PICKUP_FREEBLOCK);
        assert_eq!(b.bh.borrow().z_index, 1);
        // blocks reserved for another Blockhead are skipped (the scan continues to the end)
        let other = stub(24, true);
        for f in [&f5, &f9, &f7] {
            crate::physics_objects::free_block::as_free_block(f).unwrap().st.borrow_mut().priority_blockhead =
                Some(other.clone());
        }
        b.bh.borrow_mut().z_index = 0;
        assert_eq!(rule(b, &world, 10, 12, None, false, 0).0, InteractionType(0));
        assert_eq!(b.bh.borrow().z_index, 1);
        // only the smallest id is free: still found
        crate::physics_objects::free_block::as_free_block(&f5).unwrap().st.borrow_mut().priority_blockhead = None;
        assert_eq!(rule(b, &world, 10, 12, None, false, 0).0, InteractionType::INTERACTION_PICKUP_FREEBLOCK);
        // allow_protected = false still scans (use + free blocks)
        let _ = Rc::strong_count(&other);
    }

    /// : doors, world loading, achievements, glow / promise conversion.
    #[test]
    fn update_position_doors_and_loading() {
        let h = AHost::install();
        h.dw_client.set(true); // skip the glow loop here
        let (w, o) = arena();
        let b = bh_of(&o);
        tile(&w, 6, 8).update(|t| t.background_contents = 0x46);
        h.take();
        b.update_position(IntPair { x: 6, y: 8 });
        let log: Vec<String> = h.take().into_iter().filter(|l| !l.starts_with("ach:")).collect();
        assert_eq!(log, vec!["door:6,8:true:1".to_string(), "load:6,8:-1:true".to_string()]);
        assert_eq!(b.pos(), IntPair { x: 6, y: 8 });
        b.update_position(IntPair { x: 5, y: 8 });
        let log: Vec<String> = h.take().into_iter().filter(|l| !l.starts_with("ach:")).collect();
        assert_eq!(log[0], "door:6,8:false:0");
        assert_eq!(log.len(), 2);
        // door above (head height) opens with direction -1 when walking left
        tile(&w, 4, 9).update(|t| t.background_contents = 0x46);
        b.update_position(IntPair { x: 4, y: 8 });
        assert_eq!(h.take()[0], "door:4,9:true:-1");
        // no change: nothing at all
        b.update_position(IntPair { x: 4, y: 8 });
        assert!(h.take().is_empty());
        // riding or both key frames 10 skip the doors but not the loading
        b.bh.borrow_mut().traverse_to_key_frame = 10;
        b.bh.borrow_mut().traverse_from_key_frame = 10;
        b.update_position(IntPair { x: 6, y: 8 });
        assert_eq!(
            h.take().into_iter().filter(|l| !l.starts_with("ach:")).collect::<Vec<_>>(),
            vec!["load:6,8:-1:true".to_string()]
        );
        b.bh.borrow_mut().traverse_to_key_frame = 0;
        // traps and gates close/open the same way
        tile(&w, 7, 8).update(|t| t.background_contents = 0x4b);
        b.update_position(IntPair { x: 7, y: 8 });
        let l = h.take();
        assert_eq!(l[0], "door:6,8:false:0");
        assert_eq!(l[1], "door:7,8:true:1");
        tile(&w, 8, 8).update(|t| t.contents = 0x92);
        b.update_position(IntPair { x: 8, y: 8 });
        let l = h.take();
        assert_eq!(l[0], "door:7,8:false:0");
        assert_eq!(l[1], "door:8,8:true:1");
        // net Blockhead: server client lookup decides the loading
        b.base.borrow_mut().is_net = true;
        h.light_index.set(None);
        b.update_position(IntPair { x: 9, y: 8 });
        assert!(h.take().iter().all(|l| !l.starts_with("load")));
        h.light_index.set(Some(-1));
        b.update_position(IntPair { x: 10, y: 8 });
        assert!(h.take().iter().all(|l| !l.starts_with("load")));
        h.light_index.set(Some(3));
        b.update_position(IntPair { x: 11, y: 8 });
        let l = h.take();
        assert!(l.contains(&"load:11,8:3:true".to_string()), "{l:?}");
    }

    #[test]
    fn update_position_achievements() {
        let h = AHost::install();
        h.dw_client.set(true);
        let (w, o) = arena();
        let b = bh_of(&o);
        w.set_highest_point(IntPair { x: 20, y: 101 });
        let achs = |h: &AHost| h.take().into_iter().filter(|l| l.starts_with("ach:")).collect::<Vec<_>>();
        b.update_position(IntPair { x: 20, y: 100 });
        assert_eq!(achs(&h), vec!["ach:grp.mountaineer".to_string()]);
        b.update_position(IntPair { x: 24, y: 100 });
        assert!(achs(&h).is_empty());
        b.update_position(IntPair { x: 16, y: 100 });
        assert!(achs(&h).is_empty());
        b.update_position(IntPair { x: 17, y: 99 });
        assert!(achs(&h).is_empty());
        b.update_position(IntPair { x: 17, y: 100 });
        assert_eq!(achs(&h), vec!["ach:grp.mountaineer".to_string()]);
        b.update_position(IntPair { x: 10, y: 1000 });
        assert_eq!(achs(&h), vec!["ach:grp.top".to_string()]);
        b.update_position(IntPair { x: 10, y: 0 });
        assert!(achs(&h).is_empty());
        // y in 1..0x3b -> bottom
        b.update_position(IntPair { x: 10, y: 5 });
        assert_eq!(achs(&h), vec!["ach:grp.bottom".to_string()]);
        // x == 0 / W/4 / W/2 / 3W/4 for W = 1024 (positions outside the loaded block are not loaded by the test host)
        b.update_position(IntPair { x: 256, y: 20 });
        assert_eq!(achs(&h)[0], "ach:grp.sunrise");
        b.update_position(IntPair { x: 512, y: 20 });
        assert_eq!(achs(&h)[0], "ach:grp.southpole");
        b.update_position(IntPair { x: 768, y: 20 });
        assert_eq!(achs(&h)[0], "ach:grp.sunset");
        // net Blockheads and controlled ones report nothing
        b.base.borrow_mut().is_net = true;
        b.update_position(IntPair { x: 10, y: 5 });
        assert!(achs(&h).is_empty());
        b.base.borrow_mut().is_net = false;
        b.bh.borrow_mut().is_client_blockhead_being_controlled_by_server = true;
        b.update_position(IntPair { x: 11, y: 5 });
        assert!(achs(&h).is_empty());
    }

    #[test]
    fn update_position_glow_and_promises() {
        let h = AHost::install();
        let (w, o) = arena();
        let b = bh_of(&o);
        // glow loop around (16,16) (all inside the loaded block)
        tile(&w, 15, 16).update(|t| t.foreground_contents = 0x34);
        tile(&w, 16, 18).update(|t| t.foreground_contents = 0x3c);
        tile(&w, 17, 17).update(|t| t.contents = 0x5e);
        tile(&w, 18, 17).update(|t| t.contents = 0x91);
        tile(&w, 14, 17).update(|t| t.contents = 0x90);
        tile(&w, 14, 18).update(|t| {
            t.contents = 0x90;
            t.type_index = 6;
        });
        tile(&w, 13, 13).update(|t| t.type_index = 0x10);
        tile(&w, 19, 19).update(|t| t.artificial_light_b = 5);
        tile(&w, 14, 17).update(|t| t.type_index = 2);
        tile(&w, 14, 18).update(|t| t.type_index = 6);
        h.take();
        b.update_position(IntPair { x: 16, y: 16 });
        let log = h.take();
        let want = [
            "torch:15,16:75",
            "glow:13,13",
            "chest:14,17:true:false",
            "torch:16,18:88",
            "chest:17,17:true:true",
            "chest:18,17:false:true",
        ];
        for l in want {
            assert!(log.contains(&l.to_string()), "{l} missing in {log:?}");
        }
        // order: x outer, y inner
        let pos = |s: &str| log.iter().position(|l| l == s).unwrap();
        assert!(pos("glow:13,13") < pos("torch:15,16:75"));
        assert!(pos("torch:15,16:75") < pos("chest:17,17:true:true"));
        assert_eq!(tile(&w, 15, 16).get().foreground_contents, 0x33);
        assert_eq!(tile(&w, 16, 18).get().foreground_contents, 0x3b);
        assert_eq!(tile(&w, 17, 17).get().contents, 0);
        // chest not created for non-air tiles, but contents cleared
        assert_eq!(tile(&w, 14, 18).get().contents, 0);
        assert!(!log.contains(&"chest:14,18:false:true".to_string()));
        // artificialLightB raised to >= 1 for the 81 tiles, larger values kept
        assert_eq!(tile(&w, 12, 12).get().artificial_light_b, 1);
        assert_eq!(tile(&w, 20, 20).get().artificial_light_b, 1);
        assert_eq!(tile(&w, 19, 19).get().artificial_light_b, 5);
        assert_eq!(tile(&w, 21, 16).get().artificial_light_b, 0);
    }

    #[test]
    fn workbench_and_object_accessors() {
        let (_w, o) = arena();
        let b = bh_of(&o);
        // willBeInteractingWithInteractionObject on a stub: objectType != 0x32 and different y -> NO
        let stubo = stub(15, true);
        stubo.base().borrow_mut().pos = IntPair { x: 5, y: 8 };
        {
            let mut f = b.bh.borrow_mut();
            f.state.interaction_type = 5;
            f.state.interacting_square = IntPair { x: 5, y: 8 };
        }
        assert!(b.will_be_interacting_with_interaction_object(&stubo));
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 5, y: 9 };
        assert!(!b.will_be_interacting_with_interaction_object(&stubo));
        let tp = stub(0x32, true);
        tp.base().borrow_mut().pos = IntPair { x: 5, y: 8 };
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 5, y: 7 };
        assert!(b.will_be_interacting_with_interaction_object(&tp));
        b.bh.borrow_mut().state.interacting_square = IntPair { x: 6, y: 8 };
        assert!(!b.will_be_interacting_with_interaction_object(&tp));
        assert!(b.is_interacting_with_any_interaction_object());
        assert!(!b.is_crafting_at_any_workbench() && !b.is_adding_fuel_to_any_workbench());
        b.bh.borrow_mut().state.interaction_type = 4;
        assert!(b.is_crafting_at_any_workbench());
        assert_eq!(b.get_current_craft_progress(), -1.0);
        assert!(!b.current_craft_is_out_of_fuel());
        b.bh.borrow_mut().state.interaction_type = 0xd;
        assert!(b.is_adding_fuel_to_any_workbench());
        // set/get
        b.set_interaction_object(Some(stubo.clone()));
        assert!(crate::objects::handle::ptr_eq(&b.interaction_object().unwrap(), &stubo));
        b.set_interaction_workbench(Some(tp.clone()));
        assert!(crate::objects::handle::ptr_eq(&b.interaction_workbench().unwrap(), &tp));
        // heading for a square
        b.bh.borrow_mut().final_goal_square = IntPair { x: 3, y: 4 };
        assert!(b.is_heading_for_square(IntPair { x: 3, y: 4 }));
        assert!(!b.is_heading_for_square(IntPair { x: 3, y: 5 }));
        let mut d = crate::plist::value::PlistValue::mutable_dict();
        if let crate::plist::value::PlistValue::Dict(dd) = &mut d {
            dd.map.insert("index".to_string(), crate::plist::value::PlistValue::Int(4 * 1024 + 7));
        }
        b.bh.borrow_mut().path = Some(vec![d]);
        let wi = crate::world::tiles::get_world_pos_for_world_index(4 * 1024 + 7, &b.world_rc().unwrap());
        assert!(b.is_heading_for_square(wi));
        assert_eq!(b.current_traverse_to_key_frame(), b.bh.borrow().traverse_to_key_frame);
    }

    #[test]
    fn stop_riding_and_pickup() {
        let h = AHost::install();
        let (w, o) = arena();
        let b = bh_of(&o);
        // jetpack mode: only the flag is cleared
        b.bh.borrow_mut().is_in_jet_pack_free_flight_mode = true;
        b.bh.borrow_mut().state.animation_type = 0x1b;
        b.stop_riding();
        assert!(!b.bh.borrow().is_in_jet_pack_free_flight_mode);
        assert_eq!(b.bh.borrow().state.animation_type, 0x1b);
        assert!(!o.update_needs_to_be_sent());
        // riding animation without a ride object: standing up, path recalculation, updateNeedsToBeSent (!isNet)
        b.stop_riding();
        assert_eq!(b.bh.borrow().state.animation_type, 0);
        assert!(b.bh.borrow().path_needs_recalculated && b.bh.borrow().path_recalculation_is_fall_path);
        assert!(o.update_needs_to_be_sent());
        // solid tile at the position lifts by one
        tile(&w, 5, 8).update(|t| t.type_index = 6);
        b.bh.borrow_mut().state.animation_type = 0x21;
        b.stop_riding();
        assert_eq!(b.pos(), IntPair { x: 5, y: 9 });
        assert_eq!(b.bh.borrow().to_square, IntPair { x: 5, y: 9 });
        // pickupDynamicObject: server branch
        let obj = stub(17, true);
        obj.base().borrow_mut().pos = IntPair { x: 9, y: 9 };
        h.take();
        b.pickup_dynamic_object(&obj);
        assert_eq!(h.take(), vec!["removestd".to_string()]);
        obj.base().borrow_mut().needs_removed = true;
        b.pickup_dynamic_object(&obj);
        assert!(h.take().is_empty());
    }
}

#[cfg(test)]
mod coverage_tests;
