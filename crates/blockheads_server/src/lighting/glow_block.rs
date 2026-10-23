//! `GlowBlock` (DynamicObject type 18, instance size 0x60): owns the `ArtificialLight` of a light-emitting tile
//! (crystal / gem / gold blocks, luminous plaster, time crystal, portal base stone).
//!

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::lighting::artificial_light::ArtificialLight;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_save_dict_cache, base_remove_from_macro_block, DynamicObject, DynamicObjectBase,
    DynamicWorldRef, WorldRef,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::IntPair;
use crate::world::state::{TileRef, World};
use crate::world::tiles::tile_at_world_position_loaded;

/// `GlowBlock` class descriptor.
pub static GLOW_BLOCK_CLASS: ClassInfo =
    ClassInfo { name: "GlowBlock", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_GLOWBLOCK`
pub const DYNAMIC_OBJECT_TYPE_GLOWBLOCK: i32 = 18;

/// GlowBlock ivars 0x50..0x60.
pub struct GlowBlockState {
    /// `light` (0x50)
    pub light: Option<Rc<ArtificialLight>>,
    /// `tileType` (0x58)
    pub tile_type: i32,
}

/// `GlowBlock: DynamicObject` (type 18).
pub struct GlowBlock {
    base: RefCell<DynamicObjectBase>,
    /// The GlowBlock ivars.
    pub st: RefCell<GlowBlockState>,
    world: Weak<World>,
}

/// `(R, G, B, radius)` for a tile: defaults 76/94/127 r=6.
pub fn glow_color_for_tile(tile_type: i32, contents: u8) -> (i32, i32, i32, i32) {
    let (mut r, mut g, mut b, mut radius) = (0x4c, 0x5e, 0x7f, 6);
    if tile_type != 16 && tile_type != 25 {
        if tile_type == 26 || contents == 0x4d {
            r = 0x3f;
            g = 0x39;
            b = 8;
        } else if tile_type == 71 {
            r = 0x43;
            g = 0x39;
            b = 0x7f;
        } else if tile_type == 72 {
            r = 0;
            g = 0;
            b = 127;
        } else if tile_type == 73 {
            r = 0;
            g = 127;
            b = 0;
        } else if tile_type == 74 {
            r = 127;
            g = 0;
            b = 0;
        } else if tile_type == 75 {
            r = 200;
            g = 255;
            b = 255;
        } else if tile_type == 77 {
            r = 255;
            g = 255;
            b = 255;
            radius = 1;
        }
    }
    (r, g, b, radius)
}

impl GlowBlock {
    fn alloc(world: &Rc<World>) -> Rc<GlowBlock> {
        let w = Rc::downgrade(world);
        handle::alloc_obj(|sw| GlowBlock {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            st: RefCell::new(GlowBlockState { light: None, tile_type: 0 }),
            world: w,
        })
    }

    fn world(&self) -> Rc<World> {
        self.world.upgrade().expect("GlowBlock: world dropped")
    }

    /// `-[GlowBlock initWithWorld:dynamicWorld:atPosition:cache:tile:]`. `tile` is read once.
    pub fn init_with_world_dynamic_world_at_position_cache_tile(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        pos: IntPair,
        cache: CacheRef,
        tile: &TileRef,
    ) -> Option<Rc<GlowBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // `base_init_with_world_dynamic_world_at_position_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_at_position_cache(&me_obj, &wref, dw, pos, cache.clone());
        let tile_type = tile.type_index() as i32;
        me.st.borrow_mut().tile_type = tile_type;
        let (r, g, b, radius) = glow_color_for_tile(tile_type, tile.contents());
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
            0,
            radius,
            0,
        );
        me.st.borrow_mut().light = light;
        // reloadDrawBlockLightGlowQuadsForTile / initSubDerivedItems: empty on the server
        Some(me)
    }

    /// `-[GlowBlock initWithWorld:dynamicWorld:saveDict:cache:]`. No `lightDict` -> no light.
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<GlowBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        // `base_init_with_world_dynamic_world_save_dict_cache` answers true on every path
        let _ = base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, dw, save_dict, cache.clone());
        me.st.borrow_mut().tile_type = save_dict.get(&"tileType".to_string()).map_or(0, |v| v.int_value());
        if let Some(PlistValue::Dict(ld)) = save_dict.get(&"lightDict".to_string()) {
            let light = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
                world,
                dw,
                ld,
                cache,
                Some(&me_obj),
            );
            me.st.borrow_mut().light = light;
        }
        Some(me)
    }

    /// The `light` ivar.
    pub fn light(&self) -> Option<Rc<ArtificialLight>> {
        self.st.borrow().light.clone()
    }
}

impl GlowBlock {
    /// `-[DynamicObject initWithWorld:dynamicWorld:cache:netData:]` inherited unchanged (GlowBlock defines no net init; object_registry A3).
    /// Zero-state species ivars + the DynamicObject net init. `None` when the base init yields nil.
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &DynamicWorldRef,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<GlowBlock>> {
        let me = Self::alloc(world);
        let me_obj: ObjRef = me.clone();
        let wref: WorldRef = world.clone();
        if !crate::objects::base::base_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, dw, cache, net_data)
        {
            return None;
        }
        Some(me)
    }
}

impl DynamicObject for GlowBlock {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &GLOW_BLOCK_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_GLOWBLOCK
    }
    /// R4: zeros when there is no light.
    fn get_light_rgb(&self) -> [f32; 3] {
        match self.light() {
            Some(l) => {
                let c = l.light_color();
                [c[0], c[1], c[2]]
            }
            None => [0.0; 3],
        }
    }
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        d.map.insert("tileType".to_string(), PlistValue::int(self.st.borrow().tile_type as i64));
        if let Some(l) = self.light() {
            let lo: ObjRef = l;
            let ld = lo.get_save_dict(&lo);
            d.map.insert("lightDict".to_string(), PlistValue::Dict(ld));
        }
        d
    }
    fn remove_from_macro_block(&self, me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.remove_from_macro_block_light();
            l.dealloc_light();
        }
        base_remove_from_macro_block(me);
    }
    fn dealloc(&self, _me: &ObjRef) {
        let light = self.st.borrow_mut().light.take();
        if let Some(l) = light {
            l.dealloc_light();
        }
    }
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if let Some(l) = self.light() {
            l.world_changed_positions(positions);
        }
        let pos = self.base.borrow().pos;
        if !positions.iter().any(|p| *p == pos) {
            return;
        }
        let world = self.world();
        let tile = tile_at_world_position_loaded(pos.x, pos.y, &world);
        // Note: the original does not null-check; an unloaded tile cannot be dereferenced, treat as "unchanged".
        let Some(tile) = tile else { return };
        if tile.type_index() as i32 == self.st.borrow().tile_type {
            return;
        }
        self.set_needs_removed(me, true);
    }
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            if let Some(l) = self.light() {
                l.remove_from_tiles();
            }
            // reloadDrawBlockLightGlowQuadsForTile: empty on the server
        }
    }
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        if let Some(l) = self.light() {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(macro_x, macro_y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::*;

    #[test]
    fn colours() {
        assert_eq!(glow_color_for_tile(16, 0), (76, 94, 127, 6));
        assert_eq!(glow_color_for_tile(25, 0x4d), (76, 94, 127, 6));
        assert_eq!(glow_color_for_tile(26, 0), (63, 57, 8, 6));
        assert_eq!(glow_color_for_tile(1, 0x4d), (63, 57, 8, 6));
        assert_eq!(glow_color_for_tile(71, 0), (67, 57, 127, 6));
        assert_eq!(glow_color_for_tile(72, 0), (0, 0, 127, 6));
        assert_eq!(glow_color_for_tile(73, 0), (0, 127, 0, 6));
        assert_eq!(glow_color_for_tile(74, 0), (127, 0, 0, 6));
        assert_eq!(glow_color_for_tile(75, 0), (200, 255, 255, 6));
        assert_eq!(glow_color_for_tile(77, 0), (255, 255, 255, 1));
        assert_eq!(glow_color_for_tile(9, 0), (76, 94, 127, 6));
    }

    #[test]
    fn lifecycle() {
        install_rec();
        let w = world_with_blocks(4, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        tile(&w, 40, 40).update(|t| t.type_index = 77);
        let (_d, dwr) = test_dw();
        let t = tile(&w, 40, 40);
        let g = GlowBlock::init_with_world_dynamic_world_at_position_cache_tile(
            &w,
            &dwr,
            IntPair { x: 40, y: 40 },
            None,
            &t,
        )
        .unwrap();
        let me: ObjRef = g.clone();
        assert_eq!(me.object_type(), 18);
        assert_eq!(me.get_light_rgb(), [255.0, 255.0, 255.0]);
        // radius 1 -> diameter 2; the centre cell is at grid (1,1) -> only the source tile is lit
        assert_eq!(tile(&w, 40, 40).get().artificial_light_r, 255);
        let d = me.get_save_dict(&me);
        assert_eq!(d.get(&"tileType".to_string()).unwrap().int_value(), 77);
        assert!(d.get(&"lightDict".to_string()).is_some());
        // unchanged tile type: stays; changed: removed and light taken off the tiles
        me.world_changed(&me, &mut vec![IntPair { x: 40, y: 40 }]);
        assert!(!me.needs_removed());
        tile(&w, 40, 40).update(|t| t.type_index = 2);
        me.world_changed(&me, &mut vec![IntPair { x: 41, y: 40 }]);
        assert!(!me.needs_removed());
        me.world_changed(&me, &mut vec![IntPair { x: 40, y: 40 }]);
        assert!(me.needs_removed());
        assert_eq!(tile(&w, 40, 40).get().artificial_light_r, 0);
        // save-dict reload without lightDict gives no light
        let mut d2 = d.clone();
        d2.map.remove(&"lightDict".to_string());
        let g2 = GlowBlock::init_with_world_dynamic_world_save_dict_cache(&w, &dwr, &d2, None).unwrap();
        assert!(g2.light().is_none());
        let m2: ObjRef = g2.clone();
        assert_eq!(m2.get_light_rgb(), [0.0; 3]);
    }
}
