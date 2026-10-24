//! `Bed: InteractionObject` (DynamicObjectType 23, InteractionObjectType 3).
//!
//!  (object_model, called through its `interaction_object_*` free functions).
//! Client-only members (render cubes, shaders, textures) are not ported.

use std::cell::RefCell;
use std::rc::Rc;

use super::column::{append_plist_tail, read_plist_tail, tail_string, Links};
use crate::blockhead::blockhead::Blockhead;
use crate::dynworld::state::DynamicWorld;
use crate::items::inventory_item::InventoryItem;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase, DynamicWorldRef, WorldRef};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, downcast, CacheRef, ObjRef};
use crate::objects::interaction_object::{
    interaction_object_blockhead_unloaded, interaction_object_blockheads_loaded, interaction_object_dealloc,
    interaction_object_get_save_dict,
    interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name,
    interaction_object_init_with_world_dynamic_world_cache_net_data,
    interaction_object_init_with_world_dynamic_world_save_dict_cache,
    interaction_object_interaction_object_creation_net_data, interaction_object_remote_update,
    interaction_object_should_add_to_macro_block, interaction_object_update_accurate_dt_is_simulation,
    InteractionObjectLike, InteractionObjectState,
};
use crate::objects::protocols::TapChoiceDynamicObject;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::netdata::BedNetData;
use crate::types::structs::{IntPair, WireStruct};
use crate::world::helpers::tile_is_solid;
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `DYNAMIC_OBJECT_TYPE_BED`.
pub const DYNAMIC_OBJECT_TYPE_BED: i32 = 23;
/// `INTERACTION_OBJECT_TYPE_BED`.
pub const INTERACTION_OBJECT_TYPE_BED: u16 = 3;
pub const ITEM_WOOD_BED: i32 = 63;
pub const ITEM_SOFT_BED: i32 = 169;
pub const ITEM_GOLD_BED: i32 = 170;

/// `Bed` class descriptor (superclass `InteractionObject`).
pub static BED_CLASS: ClassInfo =
    ClassInfo { name: "Bed", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

/// Bed ivars 0x9c, 0xa0 (render ivars omitted).
pub struct BedState {
    pub item_type: i32,
    pub bedding_color: u16,
}

/// `Bed: InteractionObject` (type 23).
pub struct Bed {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// The Bed ivars.
    pub st: RefCell<BedState>,
    links: Links,
}

impl Bed {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Bed> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|sw| Bed {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(BedState { item_type: 0, bedding_color: 0 }),
            links,
        })
    }

    /// Placement constructor `initWithWorld:...item:flipped:saveDict:placedByClient:clientName:`; `item` may be nil
    /// (nil messages give 0).
    #[allow(clippy::too_many_arguments)]
    pub fn init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // answers true on every path: its only failure is that of the base initialiser, which cannot fail here
        let _ = interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj, &wref, &dref, pos, cache, item, flipped, save_dict, client_id, client_name,
        );
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item.map_or(0, |i| i.item_type.0 as i32);
            st.bedding_color = item.map_or(0, |i| i.data_b);
        }
        Some(me_obj)
    }

    /// Save constructor (legacy `itemType == 0` becomes 63).
    pub fn init_with_world_dynamic_world_save_dict_cache(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        // answers true on every path (see above)
        let _ =
            interaction_object_init_with_world_dynamic_world_save_dict_cache(&me_obj, &wref, &dref, save_dict, cache);
        {
            let g = |k: &str| save_dict.map.get(&k.to_string());
            let mut st = me.st.borrow_mut();
            st.item_type = g("itemType").map_or(0, |v| v.int_value());
            st.bedding_color = g("beddingColor").map_or(0, |v| v.int_value()) as u16;
            if st.item_type == 0 {
                st.item_type = ITEM_WOOD_BED;
            }
        }
        Some(me_obj)
    }

    /// Net constructor (no default to 63 here).
    pub fn init_with_world_dynamic_world_cache_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (wref, dref): (WorldRef, DynamicWorldRef) = (world.clone(), dw.clone());
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me_obj, &wref, &dref, cache, net_data) {
            return None;
        }
        let b = BedNetData::read_le(&padded_bytes(net_data, 0x30));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = b.item_type as i32;
            st.bedding_color = b.bedding_color;
        }
        if net_data.len() > 48 {
            let dict = read_plist_tail(net_data, 48);
            let owner_id = tail_string(&dict, "ownerID");
            let owner_name = tail_string(&dict, "ownerName");
            me.base.borrow_mut().owner_id = owner_id;
            me.io.borrow_mut().owner_name = owner_name;
        }
        // for each blockhead (snapshot): [b netInteractionObjectWasLoaded:self]
        let bl = dw.blockheads();
        for b in bl {
            if let Some(bh) = downcast::<Blockhead>(&b) {
                bh.net_interaction_object_was_loaded(&me_obj);
            }
        }
        Some(me_obj)
    }

    /// `-[Bed itemType]`
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-[Bed beddingColor]`
    pub fn bedding_color(&self) -> u16 {
        self.st.borrow().bedding_color
    }
}

impl DynamicObject for Bed {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &BED_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_BED
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    fn free_block_creation_data_a(&self) -> u16 {
        0
    }
    fn free_block_creation_data_b(&self) -> u16 {
        self.bedding_color()
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        Some(self)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
    fn dealloc(&self, me: &ObjRef) {
        interaction_object_dealloc(me)
    }

    /// `-[Bed getSaveDict]`: super keys plus `itemType` and `beddingColor`.
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        let (it, bc) = {
            let st = self.st.borrow();
            (st.item_type, st.bedding_color)
        };
        d.map.insert("itemType".to_string(), PlistValue::int(it as i64));
        d.map.insert("beddingColor".to_string(), PlistValue::int(bc as i64));
        d
    }

    /// 48 bytes + `{ownerID, ownerName}` tail; does NOT call the InteractionObject version.
    fn update_net_data_for_client(&self, me: &ObjRef, _client_id: Option<&str>) -> Option<Vec<u8>> {
        let (it, bc) = {
            let st = self.st.borrow();
            (st.item_type, st.bedding_color)
        };
        let s = BedNetData {
            interaction_object_creation_net_data: interaction_object_interaction_object_creation_net_data(me),
            item_type: it as u16,
            bedding_color: bc,
            padding: [0; 4],
        };
        let mut out = s.to_vec();
        let owner_id = self.base.borrow().owner_id.clone();
        let owner_name = self.io.borrow().owner_name.clone();
        append_plist_tail(&mut out, &[("ownerID", owner_id.as_deref()), ("ownerName", owner_name.as_deref())]);
        Some(out)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        self.update_net_data_for_client(me, client_id)
    }

    /// `-[Bed remoteUpdate:]`: super first; `itemType`/`beddingColor` are not re-read.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        let no_blockhead = self.io.borrow().current_blockhead.is_none();
        if no_blockhead && net_data.len() > 48 {
            let dict = read_plist_tail(net_data, 48);
            let owner_id = tail_string(&dict, "ownerID");
            let owner_name = tail_string(&dict, "ownerName");
            self.base.borrow_mut().owner_id = owner_id;
            self.io.borrow_mut().owner_name = owner_name;
        }
    }

    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if !self.base.borrow().needs_removed {
            interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        }
    }

    /// `-[Bed worldChanged:]`.
    /// `pos.x-1` only when flipped; `below == None` counts as supported).
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        let flipped = self.io.borrow().flipped;
        for p in positions.iter() {
            let x_hit = p.x == pos.x || p.x == pos.x + 1 || (flipped && p.x == pos.x - 1);
            if x_hit && p.y == pos.y - 1 {
                let below = tile_at_world_position_loaded(pos.x, pos.y - 1, &world);
                let should_remove = if below.is_some() && !tile_is_solid(below) {
                    true
                } else {
                    let dx = if flipped { -1 } else { 1 };
                    let other = tile_at_world_position_loaded(pos.x + dx, pos.y - 1, &world);
                    other.is_some() && !tile_is_solid(other)
                };
                if should_remove {
                    InteractionObjectLike::remove(self, me, None);
                }
            }
        }
    }

    /// DynamicObject's default would forward to `worldChanged:`; Bed overrides it with an empty body.
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
}

impl InteractionObjectLike for Bed {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_BED
    }
    fn two_blocks_wide(&self) -> bool {
        true
    }
    fn destroy_item_type(&self) -> i32 {
        self.item_type()
    }
    fn title(&self) -> String {
        match self.item_type() {
            ITEM_SOFT_BED => "SOFT BED".to_string(),
            ITEM_GOLD_BED => "GOLDEN BED".to_string(),
            _ => "BED".to_string(),
        }
    }
    fn action_title(&self) -> String {
        format!("SLEEP IN {}", self.title())
    }
    fn is_paintable(&self) -> bool {
        true
    }

    /// `-[Bed remove:]`. The superclass `remove:` is not called.
    fn remove(&self, me: &ObjRef, blockhead: Option<&ObjRef>) {
        let (nr, is_net, pos) = {
            let b = self.base.borrow();
            (b.needs_removed, b.is_net, b.pos)
        };
        if nr || is_net {
            return;
        }
        let in_use = self.io.borrow().is_in_use;
        if in_use {
            let cur = self.io.borrow().current_blockhead.clone();
            let remote = self.io.borrow().remote_blockhead_in_use_unique_id;
            if cur.as_ref().is_some_and(|c| c.is_net()) || remote != 0 {
                self.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
                return;
            }
            if let Some(c) = cur {
                if let Some(bh) = c.as_blockhead() {
                    bh.stop_interacting();
                }
            }
        }
        let sd = self.get_save_dict(me);
        let (it, bc) = {
            let st = self.st.borrow();
            (st.item_type, st.bedding_color)
        };
        let dw = self.links.dw();
        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            crate::types::item_type::ItemType(it as u32),
            0,
            bc,
            None,
            Some(PlistValue::Dict(sd)),
            false,
            1,
            blockhead,
        );
        let world = self.links.world();
        world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
            pos.x, pos.y, 0, 0, blockhead,
        );
        let flipped = self.io.borrow().flipped;
        let ox = if flipped { pos.x - 1 } else { pos.x + 1 };
        world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(
            ox, pos.y, 0, 0, blockhead,
        );
        me.set_needs_removed(me, true);
    }

    /// `-[Bed paint:]`: `paintColor` only (`beddingColor` unchanged).
    fn paint(&self, _me: &ObjRef, color_index: u16) {
        self.io.borrow_mut().paint_color = color_index;
        if !self.base.borrow().is_net {
            self.base.borrow_mut().update_needs_to_be_sent = true;
            let pos = self.base.borrow().pos;
            self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_BED);
        }
    }
}

impl TapChoiceDynamicObject for Bed {
    fn action_title(&self) -> String {
        InteractionObjectLike::action_title(self)
    }
    fn is_double_height(&self) -> bool {
        InteractionObjectLike::is_double_height(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::column::testkit::*;
    use super::*;
    use crate::types::item_type::ItemType;

    fn item(t: u32, data_b: u16) -> InventoryItem {
        InventoryItem {
            item_type: ItemType(t),
            data_a: 0,
            data_b,
            sub_items: None,
            selected_sub_item_index: 0,
            dynamic_object_save_dict: None,
        }
    }
    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32, y: i32, it: &InventoryItem, flipped: bool) -> ObjRef {
        Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            w,
            d,
            pos(x, y),
            None,
            Some(it),
            flipped,
            None,
            Some("cid"),
            Some("Name"),
        )
        .unwrap()
    }
    fn bed(o: &ObjRef) -> &Bed {
        handle::downcast::<Bed>(o).unwrap()
    }

    #[test]
    fn placement_titles_and_flags() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(169, 5), false);
        assert_eq!((bed(&o).item_type(), bed(&o).bedding_color()), (169, 5));
        let like = o.as_interaction_like().unwrap();
        assert_eq!(like.title(), "SOFT BED");
        assert_eq!(like.action_title(), "SLEEP IN SOFT BED");
        assert_eq!(like.interaction_object_type(), 3);
        assert!(like.two_blocks_wide() && like.is_paintable());
        assert_eq!(like.destroy_item_type(), 169);
        assert_eq!(o.object_type(), 23);
        assert_eq!(o.free_block_creation_data_b(), 5);
        assert_eq!(o.free_block_creation_data_a(), 0);
        for (t, title) in [(170, "GOLDEN BED"), (63, "BED"), (172, "BED"), (173, "BED")] {
            let b = mk(&w, &d, 50, 20, &item(t, 0), false);
            assert_eq!(b.as_interaction_like().unwrap().title(), title);
        }
        assert_eq!(o.owner_id().as_deref(), Some("cid"));
        assert_eq!(o.owner_name().as_deref(), Some("Name"));
        // nil item gives 0
        let n =
            Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
                &w,
                &d,
                pos(60, 20),
                None,
                None,
                false,
                None,
                None,
                None,
            )
            .unwrap();
        assert_eq!((bed(&n).item_type(), bed(&n).bedding_color()), (0, 0));
    }

    #[test]
    fn save_and_net_round_trip() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(170, 9), true);
        let sd = o.get_save_dict(&o);
        assert_eq!(sd.map.get(&"itemType".to_string()).unwrap().int_value(), 170);
        assert_eq!(sd.map.get(&"beddingColor".to_string()).unwrap().int_value(), 9);
        assert_eq!(sd.map.get(&"interactionObjectType".to_string()).unwrap().int_value(), 3);
        let o2 = Bed::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None).unwrap();
        assert_eq!((bed(&o2).item_type(), bed(&o2).bedding_color()), (170, 9));
        assert!(o2.flipped());
        // legacy: missing itemType -> 63
        let mut legacy = sd.clone();
        legacy.map.remove(&"itemType".to_string());
        let o3 = Bed::init_with_world_dynamic_world_save_dict_cache(&w, &d, &legacy, None).unwrap();
        assert_eq!(bed(&o3).item_type(), 63);
        // net: 48 bytes + tail with sorted keys ownerID, ownerName
        let net = o.creation_net_data_for_client(&o, None).unwrap();
        assert!(net.len() > 48);
        assert_eq!(&net[0x28..0x2a], &170u16.to_le_bytes());
        assert_eq!(&net[0x2a..0x2c], &9u16.to_le_bytes());
        assert!(net[0x2c..0x30].iter().all(|x| *x == 0));
        assert_eq!(o.update_net_data_for_client(&o, None).unwrap(), net);
        let o4 = Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert!(o4.is_net());
        assert_eq!((bed(&o4).item_type(), bed(&o4).bedding_color()), (170, 9));
        assert_eq!(o4.owner_id().as_deref(), Some("cid"));
        assert_eq!(o4.owner_name().as_deref(), Some("Name"));
        // net item type is NOT defaulted
        let mut z = net.clone();
        z[0x28] = 0;
        z[0x29] = 0;
        let o5 = Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &z).unwrap();
        assert_eq!(bed(&o5).item_type(), 0);
        // exactly 48 bytes: owner fields untouched (nil from the net init)
        let o6 = Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net[..48]).unwrap();
        assert!(o6.owner_id().is_none());
        // remoteUpdate re-reads the owner strings only while nobody uses it
        o4.remote_update(&o4, &net);
        assert_eq!(o4.owner_name().as_deref(), Some("Name"));
    }

    /// A bed that arrives over the net tells every real blockhead of the dynamic world that it was loaded.
    #[test]
    fn net_beds_notify_the_blockheads_of_the_world() {
        use crate::blockhead::blockhead::testutil::{new_bh, setup, TestHost};
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(63, 0), false);
        let net = o.creation_net_data_for_client(&o, None).unwrap();
        let _host = TestHost::install();
        let (bw, _bdw, bdref) = setup();
        let bh = new_bh(&bw, &bdref, 0);
        // the bed's own object is in the list too, and is no blockhead
        d.blockheads.borrow_mut().push(o.clone());
        d.blockheads.borrow_mut().push(bh.clone());
        let loaded = Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        assert!(!loaded.needs_removed());
        d.blockheads.borrow_mut().clear();
    }

    #[test]
    fn world_changed_removes_unsupported_bed() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(63, 0), false);
        tile(&w, 40, 20).set_contents(0x30);
        tile(&w, 41, 20).set_contents(0x30);
        // both supports are air: the bed is removed
        o.world_changed(&o, &mut vec![pos(100, 100), pos(41, 19)]);
        assert!(o.needs_removed());
        assert_eq!(tile(&w, 40, 20).contents(), 0);
        assert_eq!(tile(&w, 41, 20).contents(), 0);
        // supported on both: stays; `below == None` counts as supported
        let p = mk(&w, &d, 50, 20, &item(63, 0), false);
        solid(&w, 50, 19, true);
        solid(&w, 51, 19, true);
        p.world_changed(&p, &mut vec![pos(50, 19), pos(51, 19)]);
        assert!(!p.needs_removed());
        // only the far support missing: removed
        solid(&w, 51, 19, false);
        p.world_changed(&p, &mut vec![pos(51, 19)]);
        assert!(p.needs_removed());
        // flipped bed uses pos.x - 1 and its x test includes pos.x - 1
        let f = mk(&w, &d, 60, 20, &item(63, 0), true);
        solid(&w, 60, 19, true);
        solid(&w, 59, 19, true);
        f.world_changed(&f, &mut vec![pos(59, 19)]);
        assert!(!f.needs_removed());
        solid(&w, 59, 19, false);
        f.world_changed(&f, &mut vec![pos(59, 19)]);
        assert!(f.needs_removed());
        // worldContentsChanged is empty
        let g = mk(&w, &d, 70, 20, &item(63, 0), false);
        g.world_contents_changed(&g, &mut vec![pos(70, 19)]);
        assert!(!g.needs_removed());
        // net beds never react
        let net = g.creation_net_data_for_client(&g, None).unwrap();
        let h = Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &net).unwrap();
        h.world_changed(&h, &mut vec![pos(70, 19)]);
        assert!(!h.needs_removed());
    }

    #[test]
    fn remove_is_deferred_while_in_use_by_a_net_player() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(63, 0), false);
        o.as_interaction_object().unwrap().borrow_mut().is_in_use = true;
        o.as_interaction_object().unwrap().borrow_mut().remote_blockhead_in_use_unique_id = 77;
        o.as_interaction_like().unwrap().remove(&o, None);
        assert!(!o.needs_removed());
        assert!(o.as_interaction_object().unwrap().borrow().needs_to_be_removed_when_interaction_ends);
        // after the interaction ends the removal goes through
        o.as_interaction_object().unwrap().borrow_mut().is_in_use = false;
        o.as_interaction_like().unwrap().remove(&o, None);
        assert!(o.needs_removed());
        // already removed: no second effect
        o.as_interaction_like().unwrap().remove(&o, None);
    }

    #[test]
    fn paint_sets_paint_color_only() {
        let (w, d) = env();
        let o = mk(&w, &d, 40, 20, &item(169, 5), false);
        o.set_update_needs_to_be_sent(false);
        o.as_interaction_like().unwrap().paint(&o, 12);
        assert_eq!(o.as_interaction_object().unwrap().borrow().paint_color, 12);
        assert_eq!(bed(&o).bedding_color(), 5);
        assert!(o.update_needs_to_be_sent());
    }
}

#[cfg(test)]
mod more_tests {
    use super::super::column::testkit::*;
    use super::*;
    use crate::objects::base::BlockheadApi;
    use crate::types::item_type::ItemType;

    struct Sleeper {
        base: RefCell<DynamicObjectBase>,
        stops: std::cell::Cell<u32>,
    }
    impl DynamicObject for Sleeper {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static crate::objects::class_info::ClassInfo {
            &crate::objects::class_info::DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
            Some(self)
        }
    }
    impl BlockheadApi for Sleeper {
        fn client_id(&self) -> Option<String> {
            None
        }
        fn local_net_id(&self) -> Option<String> {
            None
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            false
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            false
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {
            self.stops.set(self.stops.get() + 1);
        }
    }

    fn item(t: u32) -> InventoryItem {
        InventoryItem {
            item_type: ItemType(t),
            data_a: 0,
            data_b: 0,
            sub_items: None,
            selected_sub_item_index: 0,
            dynamic_object_save_dict: None,
        }
    }
    fn mk(w: &Rc<World>, d: &Rc<DynamicWorld>, x: i32) -> ObjRef {
        Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            w,
            d,
            pos(x, 20),
            None,
            Some(&item(63)),
            false,
            None,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn trait_answers_and_the_sleeper_leaving() {
        let (w, d) = env();
        let o = mk(&w, &d, 40);
        let _ = o.class_info();
        assert!(o.occupies_normal_contents());
        assert_eq!(o.freeblock_creation_item_type(), 63);
        assert!(o.free_block_creation_save_dict(&o).is_some());
        let tap = o.as_tap_choice().unwrap();
        assert_eq!(tap.action_title(), "SLEEP IN BED");
        let _ = tap.is_double_height();
        o.blockheads_loaded(&o);
        let other = mk(&w, &d, 41);
        o.blockhead_unloaded(&o, &other);
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        assert!(std::ptr::eq(o.as_interaction_like().unwrap().io_state() as *const _, &bed_io(&o).io as *const _));
        // a net bed on a tile that is not loaded cannot be created
        let net = o.creation_net_data_for_client(&o, None).unwrap();
        let mut lost = net.clone();
        lost[12..16].copy_from_slice(&300u32.to_le_bytes());
        assert!(Bed::init_with_world_dynamic_world_cache_net_data(&w, &d, None, &lost).is_none());
        // a removed bed no longer updates
        o.set_needs_removed(&o, true);
        o.update_accurate_dt_is_simulation(&o, 0.2, 0.2, false);
        o.dealloc(&o);
    }

    fn bed_io(o: &ObjRef) -> &Bed {
        handle::downcast::<Bed>(o).unwrap()
    }

    #[test]
    fn a_local_sleeper_is_woken_when_the_bed_goes() {
        let (w, d) = env();
        let o = mk(&w, &d, 40);
        let sleeper: ObjRef = handle::alloc_obj(|s| Sleeper {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(s)),
            stops: std::cell::Cell::new(0),
        });
        {
            let io = o.as_interaction_object().unwrap();
            io.borrow_mut().is_in_use = true;
            io.borrow_mut().current_blockhead = Some(sleeper.clone());
        }
        let b = sleeper.as_blockhead().unwrap();
        let _ = (sleeper.class_info(), b.client_id(), b.local_net_id());
        assert!(!b.is_client_blockhead_being_controlled_by_server() && !b.is_interacting_with_any_interaction_object());
        b.set_interaction_object(None);
        handle::downcast::<Sleeper>(&sleeper).unwrap().stops.set(0);
        o.as_interaction_like().unwrap().remove(&o, None);
        assert_eq!(handle::downcast::<Sleeper>(&sleeper).unwrap().stops.get(), 1);
        assert!(o.needs_removed());
        // a net sleeper defers the removal
        let p = mk(&w, &d, 50);
        sleeper.base().borrow_mut().is_net = true;
        {
            let io = p.as_interaction_object().unwrap();
            io.borrow_mut().is_in_use = true;
            io.borrow_mut().current_blockhead = Some(sleeper.clone());
        }
        p.as_interaction_like().unwrap().remove(&p, None);
        assert!(!p.needs_removed());
        // in use without anyone attached
        let q = mk(&w, &d, 60);
        q.as_interaction_object().unwrap().borrow_mut().is_in_use = true;
        q.as_interaction_like().unwrap().remove(&q, None);
        assert!(q.needs_removed());
    }
}
