//! `Torch` (DynamicObject type 17, instance size 0x80): every light-emitting placeable (torches, lanterns, gem
//! lights, chandeliers, steel up/down lights, shark jaw).
//!
//! `lightGlowQuadCount`, `staticGeometryDrawQuadCount...`, `draw:` are not ported),
//! sections 3.3 and 7 item 3. `torchConnectionTypeForPos` lives in `items::placement`.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::items::placement::torch_connection_type_for_pos;
use crate::lighting::artificial_light::{
    services, ArtificialLight, LIGHT_DIRECTION_ALL, LIGHT_DIRECTION_DOWN, LIGHT_DIRECTION_UP,
};
use crate::objects::base::{
    base_dynamic_object_net_data, base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, base_remove_from_macro_block, padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef,
    WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::TorchCreationNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::{tile_contains_door, tile_is_half_depth, tile_is_solid, tile_is_water};
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

/// `Torch` class descriptor.
pub static TORCH_CLASS: ClassInfo =
    ClassInfo { name: "Torch", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_TORCH`
pub const DYNAMIC_OBJECT_TYPE_TORCH: i32 = 17;
/// `ITEM_SHARK_JAW`: the only torch type without a light.
pub const ITEM_SHARK_JAW: i32 = 0x9d;

/// Torch ivars 0x50..0x80 (client-only animation/draw ivars omitted).
pub struct TorchState {
    /// `light` (0x50), nil for the shark jaw.
    pub light: Option<Rc<ArtificialLight>>,
    /// `connectionType` (0x58)
    pub connection_type: i32,
    /// `itemType` (0x5c)
    pub item_type: i32,
    /// `animates` (0x68)
    pub animates: bool,
    /// `flatOnSideAndBottom` (0x69)
    pub flat_on_side_and_bottom: bool,
    /// `chandelier` (0x6a)
    pub chandelier: bool,
    /// `dataA` (0x6c)
    pub data_a: u16,
    /// `dataB` (0x6e)
    pub data_b: u16,
}

/// `Torch: DynamicObject` (type 17).
pub struct Torch {
    base: RefCell<DynamicObjectBase>,
    /// The Torch ivars.
    pub st: RefCell<TorchState>,
    world: Weak<World>,
}

/// `[torch getLightRGB]` table, first match wins.
pub fn light_rgb_for_item(item_type: i32) -> [i32; 3] {
    match item_type {
        0x2f => [255, 255, 127],
        0xb7 => [12, 100, 220],
        0x96 | 0xfe | 0x102 => [300, 300, 170],
        0x94 => [510, 130, 130],
        0x93 => [130, 510, 130],
        0x92 => [130, 130, 510],
        0x91 => [270, 230, 510],
        0x95 => [400, 510, 510],
        0x4b => [220, 0, 0],
        0x4c => [0, 220, 0],
        0x56 => [0, 0, 220],
        0x57 => [135, 115, 220],
        0x58 => [200, 255, 255],
        _ => [253, 150, 55],
    }
}

/// `(radius, heat)` table of `initWithWorld:...placedByClient:` step 4. Defaults 15 / 1.
pub fn radius_heat_for_item(item_type: i32) -> (i32, i32) {
    match item_type {
        0x2f => (20, 1),
        0xb7 => (10, -4),
        0x96 | 0xfe | 0x102 => (25, 1),
        0x94 | 0x93 | 0x92 | 0x91 | 0x95 => (25, 0),
        0x57 | 0x56 | 0x4c | 0x4b | 0x58 => (4, 0),
        _ => (15, 1),
    }
}

/// `(chandelier, animates, flatOnSideAndBottom)` of `initSubDerivedItems`.
pub fn derived_flags_for_item(item_type: i32) -> (bool, bool, bool) {
    let chandelier = matches!(item_type, 0x94 | 0x93 | 0x92 | 0x91 | 0x95);
    let animates = !chandelier && !matches!(item_type, 0x4b | 0x4c | 0x56 | 0x57 | 0x58 | 0x9d | 0xfe | 0x102);
    let flat = !animates && item_type != 0xfe && item_type != 0x102;
    (chandelier, animates, flat)
}

impl Torch {
    fn alloc(world: &Rc<World>) -> Rc<Torch> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| Torch {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(TorchState {
                light: None,
                connection_type: 0,
                item_type: 0,
                animates: false,
                flat_on_side_and_bottom: false,
                chandelier: false,
                data_a: 0,
                data_b: 0,
            }),
            world: w,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("Torch: world dropped")
    }

    /// `-[Torch initWithWorld:dynamicWorld:atPosition:cache:type:dataA:dataB:saveDict:placedByClient:]`.
    /// The `dynamic_world` parameter is the abstract `DynamicWorldRef` the object base stores.
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_type_data_a_data_b_save_dict_placed_by_client(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        data_a: u16,
        data_b: u16,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<Rc<Torch>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dw, pos, cache.clone());
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.data_a = data_a;
            st.data_b = data_b;
        }
        // ownerID: clientID if given, else saveDict["ownerID"] if present.
        let owner = match client_id {
            Some(c) => Some(c.to_string()),
            None => {
                save_dict.and_then(|d| d.get(&"ownerID".to_string())).and_then(|v| v.as_str()).map(|s| s.to_string())
            }
        };
        if owner.is_some() {
            me.base.borrow_mut().owner_id = owner;
        }
        let [r, g, b] = light_rgb_for_item(item_type);
        let (radius, heat) = radius_heat_for_item(item_type);
        if item_type != ITEM_SHARK_JAW {
            let direction = if item_type == 0xfe {
                LIGHT_DIRECTION_DOWN
            } else if item_type == 0x102 {
                LIGHT_DIRECTION_UP
            } else {
                LIGHT_DIRECTION_ALL
            };
            let p = me.base.borrow().pos;
            let light = ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
                world,
                dw,
                p,
                cache,
                Some(&me_obj),
                r,
                g,
                b,
                heat,
                radius,
                direction,
            );
            me.st.borrow_mut().light = light;
        }
        let p = me.base.borrow().pos;
        let conn = torch_connection_type_for_pos(p, &**world, ItemType(item_type as u32));
        me.st.borrow_mut().connection_type = conn;
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Torch initWithWorld:dynamicWorld:saveDict:cache:]`. `connectionType` comes from the save.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Torch>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dw, save_dict, cache.clone());
        let get = |k: &str| save_dict.get(&k.to_string());
        let geti = |k: &str| get(k).map_or(0, |v| v.int_value());
        {
            let mut st = me.st.borrow_mut();
            st.connection_type = geti("connectionType");
            st.item_type = geti("itemType");
            st.data_a = geti("dataA") as u16;
            st.data_b = geti("dataB") as u16;
        }
        me.base.borrow_mut().owner_id = get("ownerID").and_then(|v| v.as_str()).map(|s| s.to_string());
        let item_type = me.st.borrow().item_type;
        if item_type != ITEM_SHARK_JAW {
            let empty;
            let light_dict: &PlistDict = match get("lightDict") {
                Some(PlistValue::Dict(d)) => d,
                _ => {
                    empty = crate::objects::base::new_mutable_dict();
                    &empty
                }
            };
            let light = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
                world,
                dw,
                light_dict,
                cache,
                Some(&me_obj),
            );
            me.st.borrow_mut().light = light;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Torch initWithWorld:dynamicWorld:cache:netData:]`. Net-created: no light.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Torch>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, dw, cache, net_data) {
            return None;
        }
        let s = TorchCreationNetData::read_le(&padded_bytes(net_data, 32));
        {
            let mut st = me.st.borrow_mut();
            st.connection_type = s.connection_type as i32;
            st.item_type = s.item_type as i32;
            st.data_a = s.data_a;
            st.data_b = s.data_b;
        }
        if net_data.len() > 32 {
            if let Some(inflated) = gzip_inflate(&net_data[32..]) {
                if let Some(plist) = property_list_from_data(&inflated) {
                    me.base.borrow_mut().owner_id =
                        plist.object_for_key("ownerID").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
            }
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[Torch initSubDerivedItems]`: flags only (draw-cache refresh calls dropped).
    pub fn init_sub_derived_items(&self) {
        let mut st = self.st.borrow_mut();
        let (c, a, f) = derived_flags_for_item(st.item_type);
        st.chandelier = c;
        st.animates = a;
        st.flat_on_side_and_bottom = f;
    }

    /// `-[Torch getLightRGB]` as floats.
    pub fn get_light_rgb(&self) -> [f32; 3] {
        let [r, g, b] = light_rgb_for_item(self.st.borrow().item_type);
        [r as f32, g as f32, b as f32]
    }

    /// `-[Torch dataA]`
    pub fn data_a(&self) -> u16 {
        self.st.borrow().data_a
    }
    /// `-[Torch setDataA:]`
    pub fn set_data_a(&self, v: u16) {
        self.st.borrow_mut().data_a = v;
    }
    /// `-[Torch dataB]`
    pub fn data_b(&self) -> u16 {
        self.st.borrow().data_b
    }
    /// `-[Torch setDataB:]`
    pub fn set_data_b(&self, v: u16) {
        self.st.borrow_mut().data_b = v;
    }

    /// The `light` ivar.
    pub fn light(&self) -> Option<Rc<ArtificialLight>> {
        self.st.borrow().light.clone()
    }

    fn drop_as_free_block(&self, me: &ObjRef) {
        // "drop": !isClient (server) && !needsRemoved
        if self.base.borrow().needs_removed {
            return;
        }
        let (pos, item_type, a, b) = {
            let st = self.st.borrow();
            (self.base.borrow().pos, st.item_type, st.data_a, st.data_b)
        };
        let dict = self.get_save_dict(me);
        let sv = services();
        sv.create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound(
            pos, item_type, a, b, dict, false, true,
        );
        sv.remove_standard_object(me);
    }
}

impl DynamicObject for Torch {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &TORCH_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_TORCH
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    fn free_block_creation_data_a(&self) -> u16 {
        self.data_a()
    }
    fn free_block_creation_data_b(&self) -> u16 {
        self.data_b()
    }
    fn get_light_rgb(&self) -> [f32; 3] {
        Torch::get_light_rgb(self)
    }
    fn is_downlight(&self) -> bool {
        self.st.borrow().item_type == 0xfe
    }
    fn is_uplight(&self) -> bool {
        self.st.borrow().item_type == 0x102
    }
    fn occupies_foreground_contents(&self) -> bool {
        true
    }

    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let (item_type, a, b, conn, light) = {
            let st = self.st.borrow();
            (st.item_type, st.data_a, st.data_b, st.connection_type, st.light.clone())
        };
        d.map.insert("itemType".to_string(), PlistValue::int(item_type as i64));
        d.map.insert("dataA".to_string(), PlistValue::int(a as i64));
        d.map.insert("dataB".to_string(), PlistValue::int(b as i64));
        d.map.insert("connectionType".to_string(), PlistValue::int(conn as i64));
        if let Some(l) = light {
            let lo: ObjRef = l;
            let ld = lo.get_save_dict(&lo);
            d.map.insert("lightDict".to_string(), PlistValue::Dict(ld));
        }
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        d
    }

    /// : 32-byte struct (padding 0) + gzip(XML plist `{ownerID}`), tail always appended.
    fn creation_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let st = self.st.borrow();
        let s = TorchCreationNetData {
            dynamic_object_net_data: base_dynamic_object_net_data(me),
            item_type: st.item_type as u16,
            data_a: st.data_a,
            data_b: st.data_b,
            connection_type: st.connection_type as i8,
            padding: [0],
        };
        let mut out = vec![0u8; 32];
        s.write_le(&mut out);
        let mut d = crate::objects::base::new_mutable_dict();
        if let Some(o) = self.base.borrow().owner_id.clone() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        let plist = data_from_property_list(&PlistValue::Dict(d)).unwrap_or_default();
        out.extend_from_slice(&gzip_deflate(&plist));
        Some(out)
    }
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client_id)
    }

    fn remove_from_macro_block(&self, me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
        // reloadDrawBlock* calls: empty in the server binary
        base_remove_from_macro_block(me);
    }

    fn dealloc(&self, _me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.dealloc_light();
        }
        self.base.borrow_mut().owner_id = None;
    }

    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        let s = TorchCreationNetData::read_le(&padded_bytes(net_data, 32));
        self.st.borrow_mut().connection_type = s.connection_type as i32;
        base_remote_update(me, net_data);
    }

    fn water_content_changed(&self, me: &ObjRef, _positions: &mut Vec<IntPair>) {
        let (flat, item) = {
            let st = self.st.borrow();
            (st.flat_on_side_and_bottom, st.item_type)
        };
        if !flat && !matches!(item, 0x96 | 0xfe | 0x102) {
            let world = self.world();
            let pos = self.base.borrow().pos;
            let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
            // !isClient (server) && tileIsWater(tile) && !needsRemoved
            if tile_is_water(tile) && !self.base.borrow().needs_removed {
                self.drop_as_free_block(me);
            }
        }
    }

    fn world_contents_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let world = self.world();
        let (pos, old, flat, backing) = {
            let st = self.st.borrow();
            (self.base.borrow().pos, st.connection_type, st.flat_on_side_and_bottom, st.connection_type)
        };
        let _ = backing;
        let t = |x: i32, y: i32| tile_at_world_position_loaded(x, y, &world);
        for p in positions.iter() {
            let conn = self.st.borrow().connection_type;
            let in_x = p.x > pos.x - 2 && p.x < pos.x + 2;
            let in_y = (p.y > pos.y - 2 && p.y < pos.y + 2) || (p.y > pos.y && conn == -2);
            if !(in_x && in_y) {
                continue;
            }
            let (x, y) = (pos.x, pos.y);
            let mut new_conn: Option<i32> = None; // None = drop
            let solid_non_door =
                |tile: Option<TileRef>| tile.is_some() && tile_is_solid(tile) && !tile_contains_door(tile);
            if !flat {
                // Case A
                let below = t(x, y - 1);
                if below.is_some() && tile_is_solid(below) {
                    new_conn = Some(0);
                } else if below.map_or(false, |b| b.foreground_contents() == 100) {
                    new_conn = Some(3);
                } else {
                    let this = t(x, y);
                    if this.is_some() && tile_is_half_depth(this, &world, IntPair { x, y }) {
                        new_conn = Some(2);
                    } else if solid_non_door(t(x + 1, y)) {
                        new_conn = Some(1);
                    } else if solid_non_door(t(x - 1, y)) {
                        new_conn = Some(-1);
                    } else if this.map_or(false, |tt| tt.back_wall_type_index() != 2 && tt.contents() == 0) {
                        new_conn = Some(-2);
                    }
                }
            } else {
                // Case B
                let this = t(x, y);
                if this.is_none() || this.map_or(false, |tt| tt.back_wall_type_index() == 2 || tt.contents() != 0) {
                    if this.is_some() && tile_is_half_depth(this, &world, IntPair { x, y }) {
                        new_conn = Some(2);
                    } else {
                        let below = t(x, y - 1);
                        if below.is_some() && tile_is_solid(below) {
                            new_conn = Some(0);
                        } else if below.map_or(false, |b| b.foreground_contents() == 100) {
                            new_conn = Some(3);
                        } else if solid_non_door(t(x + 1, y)) {
                            new_conn = Some(1);
                        } else if solid_non_door(t(x - 1, y)) {
                            new_conn = Some(-1);
                        }
                    }
                } else {
                    new_conn = Some(-2);
                }
            }
            match new_conn {
                Some(c) => self.st.borrow_mut().connection_type = c,
                None => self.drop_as_free_block(me),
            }
            break;
        }
        let now = self.st.borrow().connection_type;
        if old != now {
            if let Some(dw) = self.dynamic_world_ref() {
                dw.dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_TORCH);
            }
            // reloadDrawBlock* calls: empty on the server
            self.base.borrow_mut().update_needs_to_be_sent = true;
        }
    }

    /// : `[light worldChanged:positions]; [self worldContentsChanged:positions]`.
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let light = self.st.borrow().light.clone();
        if let Some(l) = light {
            l.world_changed_positions(positions);
        }
        self.world_contents_changed(me, positions);
    }

    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            let light = self.st.borrow().light.clone();
            if let Some(l) = light {
                l.remove_from_tiles();
            }
        }
    }

    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        let light = self.st.borrow().light.clone();
        if let Some(l) = light {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(macro_x, macro_y);
        }
    }
}

impl Torch {
    fn dynamic_world_ref(&self) -> Option<DynamicWorldRef> {
        let me: ObjRef = self.me();
        me.dynamic_world()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;

    fn place(world: &Rc<World>, x: i32, y: i32, item: i32) -> Rc<Torch> {
        let (_d, dwr) = test_dw();
        Torch::init_with_world_dynamic_world_at_position_cache_type_data_a_data_b_save_dict_placed_by_client(
            world,
            &dwr,
            IntPair { x, y },
            None,
            item,
            1,
            2,
            None,
            Some("owner-1"),
        )
        .unwrap()
    }

    fn world() -> Rc<World> {
        world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)])
    }

    #[test]
    fn tables() {
        assert_eq!(light_rgb_for_item(0x11), [253, 150, 55]);
        assert_eq!(light_rgb_for_item(0x2f), [255, 255, 127]);
        assert_eq!(light_rgb_for_item(0xb7), [12, 100, 220]);
        assert_eq!(light_rgb_for_item(0xfe), [300, 300, 170]);
        assert_eq!(light_rgb_for_item(0x95), [400, 510, 510]);
        assert_eq!(light_rgb_for_item(0x58), [200, 255, 255]);
        assert_eq!(light_rgb_for_item(9999), [253, 150, 55]);
        assert_eq!(radius_heat_for_item(0x11), (15, 1));
        assert_eq!(radius_heat_for_item(0xb7), (10, -4));
        assert_eq!(radius_heat_for_item(0x96), (25, 1));
        assert_eq!(radius_heat_for_item(0x91), (25, 0));
        assert_eq!(radius_heat_for_item(0x4b), (4, 0));
        // flags
        assert_eq!(derived_flags_for_item(0x11), (false, true, false));
        assert_eq!(derived_flags_for_item(0x91), (true, false, true));
        assert_eq!(derived_flags_for_item(0x4b), (false, false, true));
        assert_eq!(derived_flags_for_item(0x9d), (false, false, true));
        assert_eq!(derived_flags_for_item(0xfe), (false, false, false));
        assert_eq!(derived_flags_for_item(0x102), (false, false, false));
        assert_eq!(derived_flags_for_item(0x96), (false, true, false));
    }

    #[test]
    fn init_makes_light_and_connection() {
        install_rec();
        let w = world();
        tile(&w, 40, 39).update(|t| t.type_index = 1); // floor
        let t = place(&w, 40, 40, 0x11);
        assert_eq!(t.st.borrow().connection_type, 0);
        assert!(t.light().is_some());
        assert_eq!(tile(&w, 40, 40).get().artificial_light_r, 253);
        let shark = place(&w, 50, 40, 0x9d);
        assert!(shark.light().is_none());
        // owner from client id
        let me: ObjRef = t.clone();
        assert_eq!(me.owner_id().as_deref(), Some("owner-1"));
        // set_needs_removed(true) removes the light from tiles
        me.set_needs_removed(&me, true);
        assert_eq!(tile(&w, 40, 40).get().artificial_light_r, 0);
    }

    #[test]
    fn save_dict_and_reload() {
        install_rec();
        let w = world();
        let t = place(&w, 40, 40, 0xb7);
        let me: ObjRef = t.clone();
        let d = me.get_save_dict(&me);
        assert_eq!(d.get(&"itemType".to_string()).unwrap().int_value(), 0xb7);
        assert_eq!(d.get(&"dataA".to_string()).unwrap().int_value(), 1);
        assert_eq!(d.get(&"dataB".to_string()).unwrap().int_value(), 2);
        assert!(d.get(&"lightDict".to_string()).is_some());
        assert_eq!(d.get(&"ownerID".to_string()).unwrap().as_str(), Some("owner-1"));
        // reload from the dict (connectionType taken from the save)
        t.remove_from_macro_block(&me);
        assert_eq!(tile(&w, 40, 40).get().artificial_light_r, 0);
        let (_dw, dwr) = test_dw();
        let t2 = Torch::init_with_world_dynamic_world_save_dict_cache(&w, &dwr, &d, None).unwrap();
        assert_eq!(t2.st.borrow().connection_type, t.st.borrow().connection_type);
        assert_eq!(t2.st.borrow().item_type, 0xb7);
        assert_eq!(tile(&w, 40, 40).get().artificial_heat, -4);
    }

    #[test]
    fn net_bytes() {
        install_rec();
        let w = world();
        let t = place(&w, 40, 40, 0x11);
        let me: ObjRef = t.clone();
        let bytes = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(bytes.len() > 32);
        let s = TorchCreationNetData::read_le(&bytes[..32]);
        assert_eq!((s.item_type, s.data_a, s.data_b), (0x11, 1, 2));
        assert_eq!(s.padding, [0]);
        assert_eq!(s.dynamic_object_net_data.pos_x, 40);
        let plist = property_list_from_data(&gzip_inflate(&bytes[32..]).unwrap()).unwrap();
        assert_eq!(plist.object_for_key("ownerID").unwrap().as_str(), Some("owner-1"));
        assert_eq!(me.update_net_data_for_client(&me, None).unwrap(), bytes);
        // round trip through the net constructor
        let (_dw, dwr) = test_dw();
        let t2 = Torch::init_with_world_dynamic_world_cache_net_data(&w, &dwr, None, &bytes).unwrap();
        assert!(t2.light().is_none());
        assert_eq!(t2.st.borrow().item_type, 0x11);
        let m2: ObjRef = t2.clone();
        assert!(m2.is_net());
        assert_eq!(m2.owner_id().as_deref(), Some("owner-1"));
    }

    #[test]
    fn contents_changed_case_a_drops_and_reconnects() {
        let rec = install_rec();
        let w = world();
        tile(&w, 40, 39).update(|t| t.type_index = 1);
        let t = place(&w, 40, 40, 0x11);
        let me: ObjRef = t.clone();
        // floor removed, no walls, back wall = air (2): drop
        tile(&w, 40, 39).update(|t| t.type_index = 2);
        rec.take();
        me.world_contents_changed(&me, &mut vec![IntPair { x: 40, y: 39 }]);
        let log = rec.take();
        assert!(log.iter().any(|s| s.starts_with("freeBlock(40,40,0x11,1,2,false,true)")), "{log:?}");
        assert!(log.contains(&"removeStandard".to_string()));
        // wall on the right instead: connection 1, update flagged
        tile(&w, 41, 40).update(|t| t.type_index = 1);
        let t2 = place(&w, 40, 40, 0x11);
        let m2: ObjRef = t2.clone();
        assert_eq!(t2.st.borrow().connection_type, 1);
        tile(&w, 41, 40).update(|t| t.type_index = 2);
        tile(&w, 39, 40).update(|t| t.type_index = 1);
        rec.take();
        m2.world_contents_changed(&m2, &mut vec![IntPair { x: 41, y: 40 }]);
        assert_eq!(t2.st.borrow().connection_type, -1);
        assert!(m2.update_needs_to_be_sent());
        assert!(rec.take().iter().all(|s| !s.starts_with("freeBlock")));
        // out-of-window change is ignored
        tile(&w, 39, 40).update(|t| t.type_index = 2);
        m2.world_contents_changed(&m2, &mut vec![IntPair { x: 45, y: 40 }]);
        assert_eq!(t2.st.borrow().connection_type, -1);
        // back wall connection: p.y > pos.y && conn == -2 widens the window
        m2.world_contents_changed(&m2, &mut vec![IntPair { x: 40, y: 40 }]);
        // nothing around, back wall type 2, contents 0 -> drop
        assert!(rec.take().iter().any(|s| s.starts_with("freeBlock")));
    }

    #[test]
    fn chandelier_uses_case_b_and_water_drops_torch_only() {
        let rec = install_rec();
        let w = world();
        let c = place(&w, 40, 40, 0x91);
        let me: ObjRef = c.clone();
        // nothing supports: Case B -> drop
        rec.take();
        me.world_contents_changed(&me, &mut vec![IntPair { x: 40, y: 40 }]);
        assert!(rec.take().iter().any(|s| s.starts_with("freeBlock(40,40,0x91")));
        // water floods torch / not chandelier
        let tt = place(&w, 50, 40, 0x11);
        let mt: ObjRef = tt.clone();
        tile(&w, 50, 40).update(|t| t.type_index = 3);
        mt.water_content_changed(&mt, &mut vec![]);
        assert!(rec.take().iter().any(|s| s.starts_with("freeBlock(50,40,0x11")));
        let c2 = place(&w, 60, 40, 0x91);
        let m3: ObjRef = c2.clone();
        tile(&w, 60, 40).update(|t| t.type_index = 3);
        rec.take();
        m3.water_content_changed(&m3, &mut vec![]);
        assert!(rec.take().is_empty());
        // needs_removed suppresses
        mt.set_needs_removed(&mt, true);
        rec.take();
        mt.water_content_changed(&mt, &mut vec![]);
        assert!(rec.take().is_empty());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;

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

    const TORCHES: [i32; 18] =
        [0x11, 0x2f, 0xb7, 0x96, 0xfe, 0x102, 0x94, 0x93, 0x92, 0x91, 0x95, 0x4b, 0x4c, 0x56, 0x57, 0x58, 0x9d, 0x31];

    fn place(world: &Rc<World>, dwr: &DynamicWorldRef, x: i32, y: i32, item: i32, client: Option<&str>) -> Rc<Torch> {
        Torch::init_with_world_dynamic_world_at_position_cache_type_data_a_data_b_save_dict_placed_by_client(
            world,
            dwr,
            IntPair { x, y },
            None,
            item,
            1,
            2,
            None,
            client,
        )
        .unwrap()
    }

    #[test]
    fn every_table_row_and_every_kind_of_torch() {
        let _rec = install_rec();
        for i in 0..0x120 {
            let _ = (light_rgb_for_item(i), radius_heat_for_item(i), derived_flags_for_item(i));
        }
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (_d, dwr) = test_dw();
        for item in TORCHES {
            let t = place(&world, &dwr, 40, 40, item, if item % 2 == 0 { Some("o") } else { None });
            let o: ObjRef = t.clone();
            let _ = o.class_info();
            assert_eq!(o.freeblock_creation_item_type(), item);
            assert!(o.free_block_creation_save_dict(&o).is_some());
            assert_eq!((o.free_block_creation_data_a(), o.free_block_creation_data_b()), (1, 2));
            assert_eq!(t.get_light_rgb().len(), 3);
            let _ = (o.is_downlight(), o.is_uplight(), o.occupies_foreground_contents());
            t.set_data_a(5);
            t.set_data_b(6);
            assert_eq!((t.data_a(), t.data_b()), (5, 6));
            // save and net round trips
            let sd = o.get_save_dict(&o);
            let back = Torch::init_with_world_dynamic_world_save_dict_cache(&world, &dwr, &sd, None).unwrap();
            assert_eq!(back.st.borrow().item_type, item);
            let mut bare = sd.clone();
            bare.map.remove(&"lightDict".to_string());
            let _ = Torch::init_with_world_dynamic_world_save_dict_cache(&world, &dwr, &bare, None).unwrap();
            let net = o.creation_net_data_for_client(&o, None).unwrap();
            let replica = Torch::init_with_world_dynamic_world_cache_net_data(&world, &dwr, None, &net).unwrap();
            let r: ObjRef = replica.clone();
            r.remote_update(&r, &net);
            let mut lost = net.clone();
            lost[12..16].copy_from_slice(&300u32.to_le_bytes());
            assert!(Torch::init_with_world_dynamic_world_cache_net_data(&world, &dwr, None, &lost).is_none());
            let _ = Torch::init_with_world_dynamic_world_cache_net_data(&world, &dwr, None, &net[..32]).unwrap();
            // save dicts can name the owner
            let mut with_owner = sd.clone();
            with_owner.map.insert("ownerID".to_string(), PlistValue::string("x"));
            let _ =
                Torch::init_with_world_dynamic_world_at_position_cache_type_data_a_data_b_save_dict_placed_by_client(
                    &world,
                    &dwr,
                    IntPair { x: 45, y: 40 },
                    None,
                    item,
                    0,
                    0,
                    Some(&with_owner),
                    None,
                )
                .unwrap();
            o.world_changed(&o, &mut vec![IntPair { x: 40, y: 41 }]);
            o.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&o, 1, 1);
            o.set_needs_removed(&o, true);
            o.set_needs_removed(&o, true);
            o.remove_from_macro_block(&o);
            o.dealloc(&o);
        }
    }

    #[test]
    fn connections_follow_the_surroundings() {
        let _rec = install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (d, dwr) = test_dw();
        let mut r = Lcg(77);
        for item in TORCHES {
            for y in [0, 40, 63] {
                for _ in 0..30 {
                    for dy in -2..=2 {
                        for dx in -2..=2 {
                            if y + dy < 0 || y + dy >= 64 {
                                continue;
                            }
                            let (ty, bw, c, fg, pl) = (
                                r.pick(&[2u8, 2, 6, 3, 9]),
                                r.pick(&[2u8, 6]),
                                r.pick(&[0u8, 0, 0x30, 0x2e, 3]),
                                r.pick(&[0u8, 0, 100, 0x31]),
                                r.pick(&[0u8, 255]),
                            );
                            tile(&world, 40 + dx, y + dy).update(|t| {
                                t.type_index = ty;
                                t.back_wall_type_index = bw;
                                t.contents = c;
                                t.foreground_contents = fg;
                                t.partial_content_left = pl;
                            });
                        }
                    }
                    let t = place(&world, &dwr, 40, y, item, None);
                    let o: ObjRef = t.clone();
                    t.st.borrow_mut().connection_type = r.pick(&[-2, -1, 0, 1, 2, 3]);
                    if r.next() % 5 == 0 {
                        o.set_needs_removed(&o, true);
                    }
                    let p = IntPair { x: 40 + (r.next() % 3) as i32 - 1, y: y + (r.next() % 4) as i32 - 1 };
                    o.world_contents_changed(&o, &mut vec![p]);
                    o.water_content_changed(&o, &mut vec![p]);
                }
            }
        }
        assert!(!d.changed.borrow().is_empty());
    }

    #[test]
    fn odd_net_tails_and_a_vanished_dynamic_world() {
        let _rec = install_rec();
        let world = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        let (d, dwr) = test_dw();
        let t = place(&world, &dwr, 40, 40, 0x11, Some("o"));
        let o: ObjRef = t.clone();
        let net = o.creation_net_data_for_client(&o, None).unwrap();
        // a tail that is no gzip, and one that is gzip of no plist
        let mut junk = net[..32].to_vec();
        junk.extend_from_slice(b"not gzip at all");
        assert!(Torch::init_with_world_dynamic_world_cache_net_data(&world, &dwr, None, &junk).is_some());
        let mut junk = net[..32].to_vec();
        junk.extend_from_slice(&gzip_deflate(b"((("));
        assert!(Torch::init_with_world_dynamic_world_cache_net_data(&world, &dwr, None, &junk).is_some());
        // trait answers
        assert!(o.as_any().is::<Torch>());
        o.set_needs_removed(&o, false);
        assert_eq!(o.get_light_rgb().len(), 3);
        // a torch that is dropped with its light still attached
        let t2 = place(&world, &dwr, 50, 40, 0x11, None);
        let o2: ObjRef = t2.clone();
        o2.dealloc(&o2);
        // the world's dynamic world is gone when the connection changes
        for dy in -1..=1 {
            tile(&world, 40, 40 + dy).update(|x| {
                x.type_index = 2;
                x.back_wall_type_index = 2;
            });
        }
        t.st.borrow_mut().connection_type = 3;
        drop(d);
        drop(dwr);
        o.world_contents_changed(&o, &mut vec![IntPair { x: 40, y: 40 }]);
    }
}
