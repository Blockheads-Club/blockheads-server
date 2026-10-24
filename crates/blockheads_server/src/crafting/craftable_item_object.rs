//! owned by workbench
//!
//! `CraftableItemObject` and `BlockheadCraftableItemObject`: NSObject wrappers around one 0x7c-byte
//! [`CraftableItem`] recipe entry. They are what `Workbench.craftingItemObject` and `Action.craftableItemObject`
//! hold, and what `World.craftItemFinished:...` receives.
//!
//!  Class `CraftableItemObject`" (3.1-3.3);
//!
//! The `PaintingCraftableItemObject` subclass lives in `interaction_objects`; the factory
//! [`craftable_item_object_from_save_dict`] reaches it through [`register_painting_factory`]
//! (now defaulting to the direct constructor).

use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::foundation::exception::{invalid_argument, range_exception_beyond_size};
use crate::items::misc::get_new_options_from_old;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::structs::{BlockheadSkinOptions, CraftableItem, WireStruct};

/// Byte size of `CraftableItem`.
pub const CRAFTABLE_ITEM_SIZE: usize = 0x7c;
/// Byte size of `BlockheadSkinOptions`.
pub const SKIN_OPTIONS_SIZE: usize = 20;

/// A zero-filled `CraftableItem` (what `alloc` leaves in the ivar).
pub fn zero_craftable_item() -> CraftableItem {
    <CraftableItem as WireStruct>::read_le(&[0u8; CRAFTABLE_ITEM_SIZE])
}

/// A zero-filled `BlockheadSkinOptions`.
pub fn zero_skin_options() -> BlockheadSkinOptions {
    <BlockheadSkinOptions as WireStruct>::read_le(&[0u8; SKIN_OPTIONS_SIZE])
}

/// Object-safe view of `CraftableItemObject` and its subclasses (`BlockheadCraftableItemObject`,
/// `PaintingCraftableItemObject`).
pub trait CraftableItemObjectLike: Any {
    /// The `craftableItem` ivar @0x08.
    fn cio_item(&self) -> &RefCell<CraftableItem>;
    /// `&dyn Any` of the concrete object (for `downcast_ref`).
    fn as_any(&self) -> &dyn Any;
    /// `Rc<dyn CraftableItemObjectLike>` -> `Rc<dyn Any>`, for `Rc<Concrete>` downcasts.
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any>;
    /// `-getSaveDict` (virtual). Default: the `CraftableItemObject` implementation.
    fn get_save_dict(&self) -> PlistDict {
        craftable_item_object_get_save_dict(self.cio_item())
    }
    /// `-freeBlockCreationItemSaveDict`: nil in the base class; `PaintingCraftableItemObject` overrides.
    fn free_block_creation_item_save_dict(&self) -> Option<PlistDict> {
        None
    }
    /// `-craftableItem` (atomic struct getter).
    fn craftable_item(&self) -> CraftableItem {
        *self.cio_item().borrow()
    }
    /// `-setCraftableItem:` (atomic struct setter).
    fn set_craftable_item(&self, ci: CraftableItem) {
        *self.cio_item().borrow_mut() = ci;
    }
}

/// `CraftableItemObject*`.
pub type CraftableItemObjectRef = Rc<dyn CraftableItemObjectLike>;

/// `getBytes:length:` of `n` bytes from an NSData: raises `NSRangeException` when the data is shorter.
fn get_bytes_exact(d: &PlistValue, n: usize) -> Option<Vec<u8>> {
    match d {
        PlistValue::Data(b) => {
            if b.bytes.len() < n {
                range_exception_beyond_size("getBytes:length:", 0, n as u64, b.bytes.len() as u64);
            }
            Some(b.bytes[..n].to_vec())
        }
        _ => None,
    }
}

/// `-[CraftableItemObject getSaveDict]`: `{ "craftableItem": Data(124 raw LE bytes) }`. Also the super-chain target
/// of the subclasses.
pub fn craftable_item_object_get_save_dict(item: &RefCell<CraftableItem>) -> PlistDict {
    let mut d = crate::objects::base::new_mutable_dict();
    let bytes = item.borrow().to_vec();
    d.map.insert("craftableItem".to_string(), PlistValue::data(bytes));
    d
}

/// Reads `d["craftableItem"]` into a struct; a missing key leaves the zero-filled struct from `alloc`.
fn item_from_save_dict(d: &PlistDict) -> CraftableItem {
    match d.map.get(&"craftableItem".to_string()) {
        Some(v) => match get_bytes_exact(v, CRAFTABLE_ITEM_SIZE) {
            Some(b) => <CraftableItem as WireStruct>::read_le(&b),
            None => zero_craftable_item(),
        },
        None => zero_craftable_item(),
    }
}

/// `CraftableItemObject: NSObject`, instance size 0x88.
pub struct CraftableItemObject {
    item: RefCell<CraftableItem>,
}

impl CraftableItemObject {
    /// `-initWithCraftableItem:` (memcpy 0x7c).
    pub fn init_with_craftable_item(ci: CraftableItem) -> Rc<CraftableItemObject> {
        Rc::new(CraftableItemObject { item: RefCell::new(ci) })
    }
    /// `-initWithSaveDict:`: `[saveDict[@"craftableItem"] getBytes:&craftableItem length:0x7c]`.
    pub fn init_with_save_dict(d: &PlistDict) -> Rc<CraftableItemObject> {
        Rc::new(CraftableItemObject { item: RefCell::new(item_from_save_dict(d)) })
    }
}

impl CraftableItemObjectLike for CraftableItemObject {
    fn cio_item(&self) -> &RefCell<CraftableItem> {
        &self.item
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

/// Mutable fields of `BlockheadCraftableItemObject`: `name` @0x88, `skinOptions` @0x90 (20 bytes).
#[derive(Clone, Debug)]
pub struct BlockheadCioState {
    /// `name` (retained; nil allowed on load, fatal on save)
    pub name: Option<String>,
    /// `skinOptions`
    pub skin_options: BlockheadSkinOptions,
}

/// `BlockheadCraftableItemObject: CraftableItemObject`, instance size 0xa8.
pub struct BlockheadCraftableItemObject {
    base: CraftableItemObject,
    st: RefCell<BlockheadCioState>,
}

impl BlockheadCraftableItemObject {
    /// `-initWithCraftableItem:name:skinOptions:`.
    pub fn init_with_craftable_item_name_skin_options(
        ci: CraftableItem,
        name: Option<String>,
        so: BlockheadSkinOptions,
    ) -> Rc<Self> {
        Rc::new(BlockheadCraftableItemObject {
            base: CraftableItemObject { item: RefCell::new(ci) },
            st: RefCell::new(BlockheadCioState { name, skin_options: so }),
        })
    }

    /// `-initWithSaveDict:`: base key `craftableItem`, then `name`, then `skinOptions` (raw 20 bytes) or the legacy
    /// `isMale`/`headIndex`/`skinIndex`/`hairStyleIndex` through `getNewOptionsFromOld`.
    pub fn init_with_save_dict(d: &PlistDict) -> Rc<Self> {
        let item = item_from_save_dict(d);
        let get = |k: &str| d.map.get(&k.to_string());
        let name = get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
        let skin_options = match get("skinOptions") {
            None => {
                let is_male = get("isMale").map_or(false, |v| v.bool_value());
                let head = get("headIndex").map_or(0, |v| v.int_value());
                let skin = get("skinIndex").map_or(0, |v| v.int_value());
                let hair = get("hairStyleIndex").map_or(0, |v| v.int_value());
                get_new_options_from_old(is_male, head, skin, hair)
            }
            Some(so) => match get_bytes_exact(so, SKIN_OPTIONS_SIZE) {
                Some(b) => <BlockheadSkinOptions as WireStruct>::read_le(&b),
                None => zero_skin_options(),
            },
        };
        Rc::new(BlockheadCraftableItemObject {
            base: CraftableItemObject { item: RefCell::new(item) },
            st: RefCell::new(BlockheadCioState { name, skin_options }),
        })
    }

    /// `-name`
    pub fn name(&self) -> Option<String> {
        self.st.borrow().name.clone()
    }
    /// `-setName:`
    pub fn set_name(&self, n: Option<String>) {
        self.st.borrow_mut().name = n;
    }
    /// `-skinOptions`
    pub fn skin_options(&self) -> BlockheadSkinOptions {
        self.st.borrow().skin_options
    }
    /// `-setSkinOptions:`
    pub fn set_skin_options(&self, so: BlockheadSkinOptions) {
        self.st.borrow_mut().skin_options = so;
    }
}

impl CraftableItemObjectLike for BlockheadCraftableItemObject {
    fn cio_item(&self) -> &RefCell<CraftableItem> {
        &self.base.item
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    /// `-[BlockheadCraftableItemObject getSaveDict]`: base dict + `name` (a nil name raises
    /// `NSInvalidArgumentException` from `setObject:forKey:`), `skinOptions` (20 raw bytes), `craftableObjectType = 1`.
    fn get_save_dict(&self) -> PlistDict {
        let mut d = craftable_item_object_get_save_dict(&self.base.item);
        let (name, so) = {
            let s = self.st.borrow();
            (s.name.clone(), s.skin_options)
        };
        match name {
            Some(n) => {
                d.map.insert("name".to_string(), PlistValue::string(n));
            }
            None => invalid_argument("NSDictionary setObject:forKey: attempt to insert nil value (key: name)"),
        }
        d.map.insert("skinOptions".to_string(), PlistValue::data(so.to_vec()));
        d.map.insert("craftableObjectType".to_string(), PlistValue::int(1));
        d
    }
}

/// The blockhead module's view of this class (`name`, `skinOptions`).
impl crate::blockhead::lifecycle::BlockheadCraftableItemObjectLike for BlockheadCraftableItemObject {
    fn name(&self) -> Option<String> {
        BlockheadCraftableItemObject::name(self)
    }
    fn skin_options(&self) -> BlockheadSkinOptions {
        BlockheadCraftableItemObject::skin_options(self)
    }
}

/// Constructor of `PaintingCraftableItemObject initWithSaveDict:` (owned by `interaction_objects`).
pub type PaintingFactory = fn(&PlistDict) -> CraftableItemObjectRef;

thread_local! {
    static PAINTING_FACTORY: RefCell<Option<PaintingFactory>> = const { RefCell::new(None) };
}

/// Registers `PaintingCraftableItemObject::init_with_save_dict` for [`craftable_item_object_from_save_dict`].
pub fn register_painting_factory(f: PaintingFactory) {
    PAINTING_FACTORY.with(|c| *c.borrow_mut() = Some(f));
}

/// The `switch ([v2 craftableObjectType])` shared by Workbench (load + take-ownership) and Action:
/// 1 -> `BlockheadCraftableItemObject`, 2 -> `PaintingCraftableItemObject`, anything else (incl. missing) ->
/// `CraftableItemObject`.
// Type 2 constructs `PaintingCraftableItemObject` directly (Q-W1).
pub fn craftable_item_object_from_save_dict(d: &PlistDict) -> CraftableItemObjectRef {
    let t = d.map.get(&"craftableObjectType".to_string()).map_or(0, |v| v.int_value());
    match t {
        1 => BlockheadCraftableItemObject::init_with_save_dict(d),
        2 => {
            let f = PAINTING_FACTORY.with(|c| *c.borrow());
            match f {
                Some(f) => f(d),
                None => crate::interaction::painting::PaintingCraftableItemObject::init_with_save_dict(d)
                    as CraftableItemObjectRef,
            }
        }
        _ => CraftableItemObject::init_with_save_dict(d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;

    fn sample() -> CraftableItem {
        let mut ci = zero_craftable_item();
        ci.crafted_item_type = 19;
        ci.crafted_item_count = 1;
        ci.required_items[0] = 11;
        ci.number_of_required_items = 1;
        ci.seconds_required = 7;
        ci
    }

    #[test]
    fn plain_save_dict_has_only_craftable_item() {
        let o = CraftableItemObject::init_with_craftable_item(sample());
        let d = o.get_save_dict();
        assert_eq!(d.map.len(), 1);
        let Some(PlistValue::Data(b)) = d.map.get(&"craftableItem".to_string()) else { unreachable!("no data") };
        assert_eq!(b.bytes.len(), 124);
        let back = CraftableItemObject::init_with_save_dict(&d);
        assert_eq!(back.craftable_item(), sample());
    }

    #[test]
    fn missing_key_is_zero_and_short_data_raises() {
        let d = crate::objects::base::new_mutable_dict();
        assert_eq!(CraftableItemObject::init_with_save_dict(&d).craftable_item(), zero_craftable_item());
        let mut d2 = crate::objects::base::new_mutable_dict();
        d2.map.insert("craftableItem".to_string(), PlistValue::data(vec![0u8; 10]));
        let r = catch_ns(|| CraftableItemObject::init_with_save_dict(&d2).craftable_item());
        assert_eq!(r.unwrap_err().name, "NSRangeException");
    }

    #[test]
    fn blockhead_variant_round_trip() {
        let so = zero_skin_options();
        let o =
            BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(sample(), Some("Bob".into()), so);
        let d = o.get_save_dict();
        assert_eq!(d.map.len(), 4);
        assert_eq!(d.map.get(&"craftableObjectType".to_string()).unwrap().int_value(), 1);
        let Some(PlistValue::Data(b)) = d.map.get(&"skinOptions".to_string()) else { unreachable!("no skin options") };
        assert_eq!(b.bytes.len(), 20);
        let back = BlockheadCraftableItemObject::init_with_save_dict(&d);
        assert_eq!(back.name().as_deref(), Some("Bob"));
        assert_eq!(back.craftable_item(), sample());
    }

    #[test]
    fn nil_name_raises() {
        let o = BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(
            sample(),
            None,
            zero_skin_options(),
        );
        let r = catch_ns(|| o.get_save_dict());
        assert_eq!(r.unwrap_err().name, "NSInvalidArgumentException");
    }

    #[test]
    fn legacy_dict_uses_get_new_options_from_old() {
        let mut d = CraftableItemObject::init_with_craftable_item(sample()).get_save_dict();
        d.map.insert("name".to_string(), PlistValue::string("Al"));
        d.map.insert("isMale".to_string(), PlistValue::boolean(true));
        d.map.insert("headIndex".to_string(), PlistValue::int(1));
        d.map.insert("skinIndex".to_string(), PlistValue::int(2));
        d.map.insert("hairStyleIndex".to_string(), PlistValue::int(3));
        let o = BlockheadCraftableItemObject::init_with_save_dict(&d);
        assert_eq!(o.skin_options(), get_new_options_from_old(true, 1, 2, 3));
    }

    #[test]
    fn non_data_values_load_as_zero() {
        let mut d = crate::objects::base::new_mutable_dict();
        d.map.insert("craftableItem".to_string(), PlistValue::string("not data"));
        d.map.insert("skinOptions".to_string(), PlistValue::string("not data"));
        d.map.insert("craftableObjectType".to_string(), PlistValue::int(1));
        let o = craftable_item_object_from_save_dict(&d);
        assert_eq!(o.craftable_item(), zero_craftable_item());
        let b = o.as_any().downcast_ref::<BlockheadCraftableItemObject>().unwrap();
        assert_eq!(b.skin_options(), zero_skin_options());
        assert_eq!(b.name(), None);
    }

    #[test]
    fn blockhead_setters_and_plain_downcast() {
        let o = BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(
            sample(),
            None,
            zero_skin_options(),
        );
        o.set_name(Some("Cy".into()));
        assert_eq!(o.name().as_deref(), Some("Cy"));
        let mut so = zero_skin_options();
        so.hair_style_index = 4;
        o.set_skin_options(so);
        assert_eq!(o.skin_options(), so);
        let plain = CraftableItemObject::init_with_craftable_item(sample());
        let any = plain.clone().into_any_rc();
        assert!(any.downcast::<CraftableItemObject>().is_ok());
        assert_eq!(plain.free_block_creation_item_save_dict().map(|d| d.map.len()), None);
        let mut ci = sample();
        ci.seconds_required = 99;
        plain.set_craftable_item(ci);
        assert_eq!(plain.craftable_item().seconds_required, 99);
    }

    #[test]
    fn registered_painting_factory_is_used() {
        fn fake(d: &PlistDict) -> CraftableItemObjectRef {
            CraftableItemObject::init_with_save_dict(d)
        }
        let mut d = crate::objects::base::new_mutable_dict();
        d.map.insert("craftableObjectType".to_string(), PlistValue::int(2));
        register_painting_factory(fake);
        let o = craftable_item_object_from_save_dict(&d);
        assert!(o.as_any().downcast_ref::<CraftableItemObject>().is_some());
    }

    #[test]
    fn factory_switch() {
        let b = BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(
            sample(),
            Some("x".into()),
            zero_skin_options(),
        );
        let d = b.get_save_dict();
        let o = craftable_item_object_from_save_dict(&d);
        assert!(o.as_any().downcast_ref::<BlockheadCraftableItemObject>().is_some());
        let plain = CraftableItemObject::init_with_craftable_item(sample()).get_save_dict();
        let o = craftable_item_object_from_save_dict(&plain);
        assert!(o.as_any().downcast_ref::<CraftableItemObject>().is_some());
        let mut d3 = plain.clone();
        d3.map.insert("craftableObjectType".to_string(), PlistValue::int(7));
        assert!(craftable_item_object_from_save_dict(&d3).as_any().downcast_ref::<CraftableItemObject>().is_some());
        // into_any_rc downcast (world_ops::craft_item_finished path)
        let rc: CraftableItemObjectRef = b;
        assert!(rc.into_any_rc().downcast::<BlockheadCraftableItemObject>().is_ok());
    }
}

impl CraftableItemObjectLike for crate::interaction::painting::PaintingCraftableItemObject {
    fn cio_item(&self) -> &RefCell<CraftableItem> {
        &self.item
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn into_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
    fn get_save_dict(&self) -> PlistDict {
        crate::interaction::painting::PaintingCraftableItemObject::get_save_dict(self)
    }
    fn free_block_creation_item_save_dict(&self) -> Option<PlistDict> {
        match crate::interaction::painting::PaintingCraftableItemObject::free_block_creation_item_save_dict(self) {
            Some(PlistValue::Dict(d)) => Some(d),
            _ => None,
        }
    }
}

#[cfg(test)]
mod painting_tests {
    use super::*;
    use crate::interaction::painting::PaintingCraftableItemObject;

    #[test]
    fn painting_factory_type2_roundtrip() {
        let p = PaintingCraftableItemObject::new_with_craftable_item_image_data_output_image_data(
            zero_craftable_item(),
            Some(vec![9]),
            Some(vec![7]),
        );
        let d = CraftableItemObjectLike::get_save_dict(&*p);
        let back = craftable_item_object_from_save_dict(&d);
        assert!(back.as_any().downcast_ref::<PaintingCraftableItemObject>().is_some());
        assert!(back.free_block_creation_item_save_dict().is_some());
        // trait view of the painting object
        let rc: CraftableItemObjectRef = p;
        assert_eq!(rc.cio_item().borrow().crafted_item_type, 0);
        assert!(rc.into_any_rc().downcast::<PaintingCraftableItemObject>().is_ok());
    }

    #[test]
    fn painting_without_free_block_dict() {
        let p = PaintingCraftableItemObject::new_with_craftable_item_image_data_output_image_data(
            zero_craftable_item(),
            None,
            None,
        );
        assert!(CraftableItemObjectLike::free_block_creation_item_save_dict(&*p).is_none());
    }
}
