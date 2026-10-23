//! `FreightCar` (DynamicObject type 43, item 206): the cargo wagon carrying a `Chest` proxy child.
//!
//! The physics is the shared [`rolling_car_update_after_super`], preceded by the chest
//! update. Client-only parts (DrawCube setup, `draw:`, camera) are not ported.
//!
//! ## Chest seam
//! `Chest` belongs to the later `interaction_objects` module. The car only needs the chest as a generic
//! `ObjRef` (every call is an `InteractionObject` / `DynamicObject` selector), so the three constructors are
//! reached through [`FreightChestFactory`]. [`ChestChildFactory`] (the real `Chest`) is used unless another factory
//! was installed with [`set_chest_factory`]; a factory that returns nothing leaves the car without a chest (every
//! chest call is a message to nil).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::dynworld::state::DynamicWorld;
use crate::fmath::libm::{cosf, sinf};
use crate::foundation::exception::range_exception_beyond_size;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::interaction::chest::Chest;
use crate::items::inventory_item::{InventoryItem, ItemRef};
use crate::objects::base::{new_mutable_dict, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::ClassInfo;
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, Vector2};
use crate::world::state::World;

use super::train_car::*;

/// `FreightCar` class descriptor (`<ProxyObjectOwner>` is not modelled as a protocol name here).
pub static FREIGHT_CAR_CLASS: ClassInfo = ClassInfo {
    name: "FreightCar",
    superclass: Some(&crate::trains::train_car::TRAIN_CAR_CLASS),
    protocols: &["RidableDynamicObject", "TapChoiceDynamicObject"],
};

/// Constructors of the car's `Chest` child (`interaction_objects::Chest`).
/// `initWithWorld:...saveDict:chestSaveDict:cache:`, `initWithWorld:...netData:`; "Chest".
pub trait FreightChestFactory {
    /// `[[Chest alloc] initWithWorld:dynamicWorld:atPosition:cache:item:flipped:NO saveDict:chestDict placedByClient:nil clientName:nil]`
    fn new_chest(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: &ItemRef,
        save_dict: Option<&PlistDict>,
    ) -> Option<ObjRef>;
    /// `[[Chest alloc] initWithWorld:dynamicWorld:saveDict:cache:]` (`save_dict` may be nil)
    fn chest_from_save_dict(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: Option<&PlistDict>,
        cache: CacheRef,
    ) -> Option<ObjRef>;
    /// `[[Chest alloc] initWithWorld:dynamicWorld:cache:netData:]` (`net_data` may be nil)
    fn chest_from_net_data(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: Option<&[u8]>,
    ) -> Option<ObjRef>;
}

thread_local! {
    static CHEST_FACTORY: RefCell<Option<Rc<dyn FreightChestFactory>>> = RefCell::new(None);
}

/// Installs the chest factory (once at startup).
pub fn set_chest_factory(f: Rc<dyn FreightChestFactory>) {
    CHEST_FACTORY.with(|c| *c.borrow_mut() = Some(f));
}

/// The installed factory, else the real `Chest` constructors.
fn chest_factory() -> Rc<dyn FreightChestFactory> {
    CHEST_FACTORY.with(|c| c.borrow().clone()).unwrap_or_else(|| Rc::new(ChestChildFactory))
}

/// The real `Chest` constructors. A nil dictionary or `NSData` reads as an empty one (every lookup in it comes back
/// nil / zero), which is what the empty values stand for.
pub struct ChestChildFactory;

impl FreightChestFactory for ChestChildFactory {
    fn new_chest(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: &ItemRef,
        save_dict: Option<&PlistDict>,
    ) -> Option<ObjRef> {
        let chest = Chest::new(world, dw, pos, cache, Some(&item.borrow()), false, save_dict, None, None)?;
        Some(chest)
    }
    fn chest_from_save_dict(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: Option<&PlistDict>,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let empty = new_mutable_dict();
        let chest = Chest::from_save_dict(world, dw, save_dict.unwrap_or(&empty), cache)?;
        Some(chest)
    }
    fn chest_from_net_data(
        &self,
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: Option<&[u8]>,
    ) -> Option<ObjRef> {
        let chest = Chest::from_net_data(world, dw, cache, net_data.unwrap_or(&[0u8; 0x40]))?;
        Some(chest)
    }
}

/// `[o isKindOfClass:[FreightCar class]]` downcast for the `DynamicWorld` seams (`freight_needs_chest_save`,
/// `freight_set_needs_chest_save`, `freight_chest_save_dict`, the virtual `updateNeedsToBeSent` pair).
pub fn freight_car_of(o: &ObjRef) -> Option<&FreightCar> {
    handle::downcast::<FreightCar>(o)
}

/// `FreightCar: TrainCar <ProxyObjectOwner>` (type 43, instance size 0x140).
pub struct FreightCar {
    base: RefCell<DynamicObjectBase>,
    tc: RefCell<TrainCarState>,
    /// 0x130 `chest` (owned child; `Chest: InteractionObject`)
    chest: RefCell<Option<ObjRef>>,
    /// 0x138 `needsChestSave`
    needs_chest_save: Cell<bool>,
    world: Weak<World>,
    dw: Weak<DynamicWorld>,
}

impl FreightCar {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<FreightCar> {
        let (w, d) = (Rc::downgrade(world), Rc::downgrade(dw));
        handle::alloc_obj(|sw| FreightCar {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(sw)),
            tc: RefCell::new(TrainCarState::alloc_zeroed()),
            chest: RefCell::new(None),
            needs_chest_save: Cell::new(false),
            world: w,
            dw: d,
        })
    }

    /// `[chest setProxyObjectOwner:self]; [chest setFloatPosAndUpdatePosition:self.floatPos]` (nil chest: no-op).
    fn attach_chest(&self, me: &ObjRef, chest: Option<ObjRef>) {
        if let Some(c) = &chest {
            if let Some(l) = c.as_interaction_like() {
                l.set_proxy_object_owner(c, Some(me));
            }
            let fp = self.base.borrow().float_pos;
            c.set_float_pos_and_update_position(c, fp);
        }
        *self.chest.borrow_mut() = chest;
    }

    /// `-[FreightCar initWithWorld:dynamicWorld:atPosition:cache:saveDict:placedByClient:]`.
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        if !train_car_init_with_world_dynamic_world_at_position_cache_save_dict_placed_by_client(
            &o,
            world,
            dw,
            pos,
            cache.clone(),
            save_dict,
            client_id,
        ) {
            return None;
        }
        couple_rolling_car_at_placement(&o);
        let chest_dict: Option<PlistDict> = match save_dict.and_then(|d| d.map.get(&"chest".to_string())) {
            Some(PlistValue::Dict(d)) => Some(d.clone()),
            _ => None,
        };
        let item =
            InventoryItem::new(ItemType(ITEM_CHEST as u32), 0, 0, None, chest_dict.clone().map(PlistValue::Dict));
        let cur_pos = o.pos();
        let chest = chest_factory().new_chest(world, dw, cur_pos, cache, &item, chest_dict.as_ref());
        me.attach_chest(&o, chest);
        me.needs_chest_save.set(true);
        Some(o)
    }

    /// `-[FreightCar initWithWorld:dynamicWorld:saveDict:chestSaveDict:cache:]`. No neighbour scan;
    /// `needsChestSave` stays NO.
    pub fn from_save_dict_chest_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        chest_save_dict: Option<&PlistDict>,
        cache: CacheRef,
    ) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        if !train_car_init_with_world_dynamic_world_save_dict_cache(&o, world, dw, save_dict, cache.clone()) {
            return None;
        }
        let chest = chest_factory().chest_from_save_dict(world, dw, chest_save_dict, cache);
        me.attach_chest(&o, chest);
        Some(o)
    }

    /// `-[FreightCar initWithWorld:dynamicWorld:cache:netData:]` (`isNet` replica). The tail
    /// `subdataWithRange:{0x68, length-0x68}` has no length check.
    pub fn from_net_data(world: &Rc<World>, dw: &Rc<DynamicWorld>, cache: CacheRef, net_data: &[u8]) -> Option<ObjRef> {
        let me = Self::alloc(world, dw);
        let o: ObjRef = me.clone();
        if !train_car_init_with_world_dynamic_world_cache_net_data(&o, world, dw, cache.clone(), net_data) {
            return None;
        }
        if net_data.len() < 0x68 {
            range_exception_beyond_size(
                "subdataWithRange:",
                0x68,
                (net_data.len() as u64).wrapping_sub(0x68),
                net_data.len() as u64,
            );
        }
        let plist = gzip_inflate(&net_data[0x68..]).as_deref().and_then(property_list_from_data);
        o.set_owner_id(
            plist.as_ref().and_then(|p| p.object_for_key("ownerID")).and_then(|v| v.as_str()).map(|s| s.to_string()),
        );
        let chest_data: Option<Vec<u8>> = match plist.as_ref().and_then(|p| p.object_for_key("chestData")) {
            Some(PlistValue::Data(d)) => Some(d.bytes.clone()),
            _ => None,
        };
        let chest = chest_factory().chest_from_net_data(world, dw, cache, chest_data.as_deref());
        me.attach_chest(&o, chest);
        Some(o)
    }

    /// `chest` (nil on a car built without a chest factory).
    pub fn chest(&self) -> Option<ObjRef> {
        self.chest.borrow().clone()
    }

    /// `-[FreightCar getChestSaveDict]`: `[chest getSaveDict]` (an empty dict when there is no chest).
    pub fn get_chest_save_dict(&self) -> PlistDict {
        match self.chest() {
            Some(c) => c.get_save_dict(&c),
            None => new_mutable_dict(),
        }
    }
    /// `-[FreightCar needsChestSave]`.
    pub fn needs_chest_save(&self) -> bool {
        self.needs_chest_save.get()
    }
    /// `-[FreightCar setNeedsChestSave:]`.
    pub fn set_needs_chest_save(&self, v: bool) {
        self.needs_chest_save.set(v);
    }
    /// `-[FreightCar updateNeedsToBeSent]`: `[super updateNeedsToBeSent] || [chest updateNeedsToBeSent]`
    /// (R7 / 3.9: `DynamicWorld` net-send code must call this instead of the base accessor).
    pub fn update_needs_to_be_sent_virtual(&self) -> bool {
        self.base.borrow().update_needs_to_be_sent || self.chest().map_or(false, |c| c.update_needs_to_be_sent())
    }
    /// `-[FreightCar setUpdateNeedsToBeSent:]`: `[super set...]`; if NO also clears the chest's flag.
    pub fn set_update_needs_to_be_sent_virtual(&self, v: bool) {
        self.base.borrow_mut().update_needs_to_be_sent = v;
        if !v {
            if let Some(c) = self.chest() {
                c.set_update_needs_to_be_sent(false);
            }
        }
    }
    /// `-[FreightCar childNeedsSaving:]` (; also reachable through the
    /// `DynamicObject::child_needs_saving` override).
    pub fn child_needs_saving(&self, me: &ObjRef, _child: &ObjRef) {
        self.child_needs_saving_impl(me)
    }

    fn child_needs_saving_impl(&self, me: &ObjRef) {
        self.needs_chest_save.set(true);
        car_changed_notify(me);
        self.base.borrow_mut().update_needs_to_be_sent = true;
    }

    fn acts_as_interaction_object_impl(&self) -> Option<ObjRef> {
        self.chest()
    }

    /// Builds `[0x68 struct][gzip(XML plist {ownerID?, chestData?})]`.
    fn net_data_with_dict(&self, me: &ObjRef, chest_data: Option<Vec<u8>>) -> Vec<u8> {
        let mut out = train_car_creation_net_data_for_client(me);
        let mut dict = new_mutable_dict();
        if let Some(o) = me.owner_id() {
            dict.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        if let Some(cd) = chest_data {
            dict.map.insert("chestData".to_string(), PlistValue::data(cd));
        }
        if let Some(p) = data_from_property_list(&PlistValue::Dict(dict)) {
            out.extend_from_slice(&gzip_deflate(&p));
        }
        out
    }
}

impl TrainCarLike for FreightCar {
    fn tc(&self) -> &RefCell<TrainCarState> {
        &self.tc
    }
    fn tc_world(&self) -> Rc<World> {
        self.world.upgrade().expect("FreightCar: world dropped")
    }
    fn tc_dw(&self) -> Rc<DynamicWorld> {
        self.dw.upgrade().expect("FreightCar: dynamic world dropped")
    }
    /// : `[super loadDerivedStuff]`, then wheels at `floatPos +- (1, 0)`.
    fn load_derived_stuff(&self, me: &ObjRef) {
        train_car_load_derived_stuff(me);
        let fp = self.base.borrow().float_pos;
        let mut s = self.tc.borrow_mut();
        s.left_wheel_pos = Vector2 { x: fp.x + (-1.0f32), y: fp.y + 0.0f32 };
        s.right_wheel_pos = Vector2 { x: fp.x + 1.0f32, y: fp.y + 0.0f32 };
    }
    fn item_type(&self) -> i32 {
        ITEM_FREIGHT_CAR
    }
    fn action_title(&self) -> String {
        "FREIGHT CAR".to_string()
    }
    /// : never ridden.
    fn blockhead_can_ride_using_item(&self, _me: &ObjRef, _b: &ObjRef, _item: i32) -> bool {
        false
    }
    ///  (R2): rotated body, windows 1.5 / 2.0.
    fn tap_is_within_body_radius(&self, tap: Vector2) -> bool {
        let s = self.tc.borrow();
        let m = self.tc_world().world_width_macro();
        rotated_body_tap_test(self.base.borrow().float_pos, s.left_wheel_pos, s.right_wheel_pos, tap, m, 2.0f32)
    }
}

impl DynamicObject for FreightCar {
    train_car_dynamic_object_common!();
    fn class_info(&self) -> &'static ClassInfo {
        &FREIGHT_CAR_CLASS
    }
    fn object_type(&self) -> i32 {
        DOT_TRAIN_CAR_FREIGHT
    }
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_FREIGHT_CAR
    }
    /// : `d = [self getSaveDict]; d["chest"] = [chest getSaveDict]`.
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        let mut d = me.get_save_dict(me);
        if let Some(c) = self.chest() {
            let cd = c.get_save_dict(&c);
            d.map.insert("chest".to_string(), PlistValue::Dict(cd));
        }
        Some(d)
    }
    ///  (= TrainCar; the chest is saved separately)
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        train_car_get_save_dict(me)
    }
    fn creation_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        let cd = self.chest().and_then(|c| c.creation_net_data_for_client(&c, client_id));
        Some(self.net_data_with_dict(me, cd))
    }
    /// : the chest data is only included when the chest has pending changes.
    fn update_net_data_for_client(&self, me: &ObjRef, client_id: Option<&str>) -> Option<Vec<u8>> {
        if self.chest().map_or(false, |c| c.update_needs_to_be_sent()) {
            return self.creation_net_data_for_client(me, client_id);
        }
        Some(self.net_data_with_dict(me, None))
    }
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        train_car_remote_update(me, net_data);
        if net_data.len() > 0x68 {
            let plist = gzip_inflate(&net_data[0x68..]).as_deref().and_then(property_list_from_data);
            if let Some(PlistValue::Data(d)) = plist.as_ref().and_then(|p| p.object_for_key("chestData")) {
                if let Some(c) = self.chest() {
                    c.remote_update(&c, &d.bytes);
                }
            }
        }
        if !me.is_net() {
            self.needs_chest_save.set(true);
            car_changed_notify(me);
            self.base.borrow_mut().update_needs_to_be_sent = true;
        }
    }
    /// : `[super update:]` (unclamped), the 0.05 clamp, the chest update with the clamped dt in both
    /// slots, then steps 1-9.
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        train_car_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        let mut dtc = dt;
        if (dt as f64) > 0.05 {
            dtc = 0.05f32;
        }
        if let Some(c) = self.chest() {
            c.update_accurate_dt_is_simulation(&c, dtc, dtc, is_simulation);
        }
        rolling_car_update_after_super(me, dtc);
    }
    fn update_position(&self, me: &ObjRef, p: IntPair) {
        train_car_update_position(me, p);
        if let Some(c) = self.chest() {
            let fp = self.base.borrow().float_pos;
            c.set_float_pos_and_update_position(&c, fp);
        }
    }
    fn set_needs_removed(&self, me: &ObjRef, v: bool) {
        if v && !self.base.borrow().needs_removed {
            if let Some(c) = self.chest() {
                c.set_needs_removed(&c, true);
            }
        }
        train_car_set_needs_removed(me, v);
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        train_car_blockheads_loaded(me);
        if let Some(c) = self.chest() {
            c.blockheads_loaded(&c);
        }
    }
    fn child_needs_saving(&self, me: &ObjRef, _child: &ObjRef) {
        self.child_needs_saving_impl(me)
    }
    /// : `[chest setProxyObjectOwner:nil]`.
    fn dealloc(&self, _me: &ObjRef) {
        if let Some(c) = self.chest.borrow_mut().take() {
            if let Some(l) = c.as_interaction_like() {
                l.set_proxy_object_owner(&c, None);
            }
        }
    }
    /// : `floatPos + R(theta) * (0, 0.75)`.
    fn render_pos(&self) -> Vector2 {
        let s = self.tc.borrow();
        let fp = self.base.borrow().float_pos;
        let theta = wheel_axis_angle(s.left_wheel_pos, s.right_wheel_pos);
        let (x, y) = glk_xy(cosf(theta), sinf(theta), 0.0, 0.75, fp.x, fp.y);
        Vector2 { x, y }
    }
}

impl_train_car_protocols!(FreightCar);

#[cfg(test)]
mod world_tests {
    use super::*;
    use crate::trains::train_car::world_tests::{find_car, lay_rails, setup};

    fn place_freight(dw: &Rc<DynamicWorld>, x: i32) -> ObjRef {
        dw.place_train_car_at_pos_of_type_save_dict_placed_by_client(IntPair { x, y: 10 }, 206, None, None);
        let c = find_car(dw, 43);
        dw.do_set(43, c.unique_id(), c.clone());
        c
    }

    fn freight_of(o: &ObjRef) -> &FreightCar {
        freight_car_of(o).unwrap()
    }

    /// The freight car carries a real chest child: it is created, saved and sent with the car.
    #[test]
    fn the_car_carries_a_chest() {
        let (world, dw) = setup("freight_chest");
        lay_rails(&world, &dw, 3, 40);
        let car = place_freight(&dw, 20);
        let f = freight_of(&car);
        let chest = f.chest().expect("a freight car has a chest");
        assert_eq!(chest.object_type(), crate::interaction::chest::DYNAMIC_OBJECT_TYPE_CHEST);
        assert!(f.needs_chest_save());
        f.set_needs_chest_save(false);
        assert!(!f.needs_chest_save());
        // the chest rides with the car and its saves reach the car
        assert!(f.get_chest_save_dict().map.contains_key(&"chestType".to_string()));
        let sd = car.free_block_creation_save_dict(&car).unwrap();
        assert!(matches!(sd.map.get(&"chest".to_string()), Some(PlistValue::Dict(_))));
        f.child_needs_saving(&car, &chest);
        assert!(f.needs_chest_save() && car.update_needs_to_be_sent());
        car.child_needs_saving(&car, &chest);
        // the virtual "needs to be sent" pair
        f.set_update_needs_to_be_sent_virtual(true);
        assert!(f.update_needs_to_be_sent_virtual());
        f.set_update_needs_to_be_sent_virtual(false);
        assert!(!f.update_needs_to_be_sent_virtual());
        chest.set_update_needs_to_be_sent(true);
        assert!(f.update_needs_to_be_sent_virtual());
        f.set_update_needs_to_be_sent_virtual(false);
        assert!(!chest.update_needs_to_be_sent());
        // the car stands in for its chest as the interaction object
        assert!(crate::objects::protocols::RidableDynamicObject::acts_as_interaction_object(f).is_some());
        assert_eq!(f.action_title(), "FREIGHT CAR");
        assert!(!TrainCarLike::blockhead_can_ride_using_item(f, &car, &car, 0));
        assert_eq!(car.freeblock_creation_item_type(), ITEM_FREIGHT_CAR);
        assert_eq!(car.class_info().name, "FreightCar");
        let _ = car.render_pos();
        assert!(f.tap_is_within_body_radius(Vector2 { x: 20.5, y: 11.0 }));
        assert!(!f.tap_is_within_body_radius(Vector2 { x: 30.5, y: 11.0 }));
    }
}
