//! `Painting` (DynamicObjectType 52, NOT an InteractionObject) and `PaintingCraftableItemObject`.
//!

use std::cell::RefCell;
use std::io::Write;
use std::process::{Command, Stdio};
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::foundation::exception::invalid_argument;
use crate::foundation::paths::{append_path_component, appsup, home_dir};
use crate::interaction::chest::{dict_str, gunzip_plist, gz_plist_bytes, subdata_from, Links};
use crate::nslog;
use crate::objects::base::{
    base_get_save_dict, base_init_with_world_dynamic_world_at_position_cache,
    base_init_with_world_dynamic_world_cache_net_data, base_init_with_world_dynamic_world_save_dict_cache,
    base_remote_update, new_mutable_dict, padded_bytes, DynamicObject, DynamicObjectBase,
};
use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::protocols::TapChoiceDynamicObject;
use crate::plist::value::{PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::netdata::PaintingCreationNetData;
use crate::types::structs::{CraftableItem, IntPair, WireStruct};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `Painting: DynamicObject <TapChoiceDynamicObject>` (instance size 0x80).
pub static PAINTING_CLASS: ClassInfo =
    ClassInfo { name: "Painting", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &["TapChoiceDynamicObject"] };

/// `DYNAMIC_OBJECT_TYPE_PAINTING`
pub const DYNAMIC_OBJECT_TYPE_PAINTING: i32 = 52;

/// Painting ivars (client-only `shader`, `texture`, `textureLoadedIsHD`, `hidden` omitted).
pub struct PaintingState {
    /// 0x50
    pub item_type: i32,
    /// 0x58 (PNG bytes), nil when unverified/hidden
    pub image_data: Option<Vec<u8>>,
    /// 0x60 (`ownerID` is `DynamicObjectBase.owner_id`)
    pub owner_name: Option<String>,
    /// 0x79
    pub hidden_due_to_server_ban: bool,
    /// 0x7b
    pub has_verified_image_data: bool,
}

/// `Painting`.
pub struct Painting {
    base: RefCell<DynamicObjectBase>,
    /// The Painting ivars.
    pub st: RefCell<PaintingState>,
    links: Links,
}

fn data_value(d: &PlistDict, key: &str) -> Option<Vec<u8>> {
    match d.map.get(&key.to_string()) {
        Some(PlistValue::Data(b)) => Some(b.bytes.clone()),
        _ => None,
    }
}

/// Runs `sh -c "pnginfo <path>"` (stdout captured, stderr inherited) and reports whether the output contains
/// `not a valid PNG file`. The spawn failure prints `Failed to run command`
/// (stdout) and counts as success.
pub fn run_pnginfo_check(command_line: &str) -> bool {
    let out = Command::new("sh")
        .arg("-c")
        .arg(command_line)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output();
    match out {
        Err(_) => {
            let mut so = std::io::stdout();
            let _ = so.write_all(b"Failed to run command\n");
            let _ = so.flush();
            true
        }
        Ok(o) => {
            // `appendFormat:@"%s"`: the bytes up to a NUL per fgets chunk; lossy UTF-8 is enough for the substring test
            let result = String::from_utf8_lossy(&o.stdout).into_owned();
            if result.contains("not a valid PNG file") {
                nslog!("painting image data verification failed:\n{}", result);
                false
            } else {
                true
            }
        }
    }
}

/// `atomically:YES` write: temp file in the same directory, then rename. Failures are silent.
fn write_atomically(path: &str, bytes: &[u8]) {
    let tmp = format!("{}.tmp{}", path, std::process::id());
    if std::fs::write(&tmp, bytes).is_ok() {
        if std::fs::rename(&tmp, path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

impl Painting {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<Painting> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| Painting {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            st: RefCell::new(PaintingState {
                item_type: 0,
                image_data: None,
                owner_name: None,
                hidden_due_to_server_ban: false,
                has_verified_image_data: false,
            }),
            links,
        })
    }

    /// `-[Painting initWithWorld:dynamicWorld:atPosition:cache:type:saveDict:placedByClient:clientName:]`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item_type: i32,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<Painting>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !base_init_with_world_dynamic_world_at_position_cache(&me_obj, &w, &d, pos, cache) {
            return None;
        }
        let image = save_dict.and_then(|d| data_value(d, "outputImageData"));
        {
            let mut st = me.st.borrow_mut();
            st.item_type = item_type;
            st.image_data = image.clone();
        }
        if !me.verify_image_server_side(image.as_deref()) {
            let mut st = me.st.borrow_mut();
            st.image_data = None;
            st.hidden_due_to_server_ban = true;
        }
        match client_id {
            None => {
                if let Some(v) = save_dict.and_then(|d| d.map.get(&"ownerID".to_string())) {
                    me_obj.set_owner_id(v.as_str().map(|s| s.to_string()));
                }
            }
            Some(c) => me_obj.set_owner_id(Some(c.to_string())),
        }
        me.st.borrow_mut().owner_name = match client_name {
            None => save_dict
                .and_then(|d| d.map.get(&"ownerName".to_string()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            Some(n) => Some(n.to_string()),
        };
        Some(me)
    }

    /// `-[Painting initWithWorld:dynamicWorld:saveDict:cache:]`. Order matters (trap 13): the verified flag
    /// is read BEFORE the verification, the ban check overwrites `hiddenDueToServerBan` AFTER it.
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<Painting>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !base_init_with_world_dynamic_world_save_dict_cache(&me_obj, &w, &d, save_dict, cache) {
            return None;
        }
        let image = data_value(save_dict, "outputImageData");
        {
            let mut st = me.st.borrow_mut();
            st.item_type = save_dict.map.get(&"itemType".to_string()).map_or(0, |v| v.int_value());
            st.image_data = image.clone();
            st.has_verified_image_data =
                save_dict.map.get(&"hasVerifiedImageData".to_string()).map_or(false, |v| v.bool_value());
        }
        let dwc = |m: &Rc<Painting>| {
            m.links.dw().dynamic_world_changed_at_pos_object_type(m.base.borrow().pos, DYNAMIC_OBJECT_TYPE_PAINTING)
        };
        if !me.verify_image_server_side(image.as_deref()) {
            {
                let mut st = me.st.borrow_mut();
                st.image_data = None;
                st.hidden_due_to_server_ban = true;
            }
            dwc(&me);
        } else if !me.st.borrow().has_verified_image_data {
            me.st.borrow_mut().has_verified_image_data = true;
            dwc(&me);
        }
        me_obj.set_owner_id(save_dict.map.get(&"ownerID".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string()));
        me.st.borrow_mut().owner_name =
            save_dict.map.get(&"ownerName".to_string()).and_then(|v| v.as_str()).map(|s| s.to_string());
        if dw.is_server() {
            let owner = me_obj.owner_id();
            if owner.is_some() && me.st.borrow().owner_name.is_none() {
                let n = dw.get_owner_name_for_object_owner_id(owner.as_deref().unwrap());
                me.st.borrow_mut().owner_name = n;
            }
            // Note: `playerIsBannedWithID:nil` for an ownerless painting is taken as NO (nil never matches).
            let banned = owner.as_deref().map_or(false, |o| dw.player_is_banned_with_id(o));
            me.st.borrow_mut().hidden_due_to_server_ban = banned;
        }
        Some(me)
    }

    /// `-[Painting initWithWorld:dynamicWorld:cache:netData:]` (client-uploaded creation, `isNet = YES`).
    /// Reads `<HOME>/GNUstep/Library/Caches/TheBlockheads/paintings/<uniqueID>.png` (P5).
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<Painting>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !base_init_with_world_dynamic_world_cache_net_data(&me_obj, &w, &d, cache, net_data) {
            return None;
        }
        let hdr = PaintingCreationNetData::read_le(&padded_bytes(net_data, 0x20));
        let dict = gunzip_plist(subdata_from(net_data, 0x20));
        me_obj.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
        let image = std::fs::read(Self::cache_file_for(me_obj.unique_id())).ok();
        {
            let mut st = me.st.borrow_mut();
            st.item_type = hdr.item_type as i32;
            st.owner_name = dict_str(dict.as_ref(), "ownerName");
            st.hidden_due_to_server_ban =
                dict.as_ref().and_then(|d| d.object_for_key("hiddenDueToServerBan")).map_or(false, |v| v.bool_value());
            st.image_data = image;
        }
        let need = me.st.borrow().image_data.as_ref().map_or(true, |i| i.is_empty());
        if need {
            dw.request_painting_data_for_painting(&me_obj);
        }
        Some(me)
    }

    fn cache_file_for(unique_id: u64) -> String {
        let dir = format!("{}/GNUstep/Library/Caches/TheBlockheads/paintings/", home_dir());
        append_path_component(&dir, &format!("{}.png", unique_id))
    }

    /// `-itemType`.
    pub fn item_type(&self) -> i32 {
        self.st.borrow().item_type
    }
    /// `-setItemType:`.
    pub fn set_item_type(&self, v: i32) {
        self.st.borrow_mut().item_type = v;
    }
    /// `-imageData`.
    pub fn image_data(&self) -> Option<Vec<u8>> {
        self.st.borrow().image_data.clone()
    }
    /// `-ownerName`.
    pub fn owner_name(&self) -> Option<String> {
        self.st.borrow().owner_name.clone()
    }
    /// `hiddenDueToServerBan`
    pub fn hidden_due_to_server_ban(&self) -> bool {
        self.st.borrow().hidden_due_to_server_ban
    }

    /// `-verifyImageServerSide:` (P3). Returns true without any I/O when the image was verified before.
    pub fn verify_image_server_side(&self, image: Option<&[u8]>) -> bool {
        if self.st.borrow().has_verified_image_data {
            return true;
        }
        let save_id = self.links.world().save_id().unwrap_or_else(|| "(null)".to_string());
        let path = append_path_component(&appsup(), &format!("saves/{}/testImage.png", save_id));
        if let Some(img) = image {
            write_atomically(&path, img);
        }
        run_pnginfo_check(&format!("pnginfo {}", path))
    }

    /// `-userMuteChanged:`: `updateTexture` is empty, no effect.
    pub fn user_mute_changed(&self, _player_id: &str) {}

    /// `-userBanChanged:isBanned:`.
    pub fn user_ban_changed_is_banned(&self, me: &ObjRef, player_id: &str, is_banned: bool) {
        if !self.base.borrow().is_net && me.owner_id().as_deref() == Some(player_id) {
            self.st.borrow_mut().hidden_due_to_server_ban = is_banned;
            me.set_update_needs_to_be_sent(true);
            self.links
                .dw()
                .dynamic_world_changed_at_pos_object_type(self.base.borrow().pos, DYNAMIC_OBJECT_TYPE_PAINTING);
        }
    }
}

impl DynamicObject for Painting {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &PAINTING_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_tap_choice(&self) -> Option<&dyn TapChoiceDynamicObject> {
        Some(self)
    }
    /// `-objectType`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_PAINTING
    }
    /// `-occupiesForegroundContents`.
    fn occupies_foreground_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType`.
    fn freeblock_creation_item_type(&self) -> i32 {
        self.item_type()
    }
    /// `-freeBlockCreationSaveDict`.
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    /// `-getSaveDict` (`hiddenDueToServerBan` is not saved).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = base_get_save_dict(me);
        let st = self.st.borrow();
        d.map.insert("itemType".to_string(), PlistValue::int(st.item_type as i64));
        if let Some(img) = &st.image_data {
            if !img.is_empty() {
                d.map.insert("outputImageData".to_string(), PlistValue::data(img.clone()));
            }
        }
        if let Some(o) = me.owner_id() {
            d.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        if let Some(n) = &st.owner_name {
            d.map.insert("ownerName".to_string(), PlistValue::string(n.clone()));
        }
        d.map.insert("hasVerifiedImageData".to_string(), PlistValue::boolean(st.has_verified_image_data));
        d
    }
    /// `-updateNetDataForClient:` (= creation).
    fn update_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        self.creation_net_data_for_client(me, client)
    }
    /// `-creationNetDataForClient:` (padding written as 0; `hiddenDueToServerBan` key only when hidden).
    fn creation_net_data_for_client(&self, me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
        let mut hdr = PaintingCreationNetData::default();
        hdr.dynamic_object_net_data = me.dynamic_object_net_data(me);
        let st = self.st.borrow();
        hdr.item_type = st.item_type as u16;
        let mut out = hdr.to_vec();
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            if let Some(o) = me.owner_id() {
                dd.map.insert("ownerID".to_string(), PlistValue::string(o));
            }
            if let Some(n) = &st.owner_name {
                dd.map.insert("ownerName".to_string(), PlistValue::string(n.clone()));
            }
            let visible = !st.hidden_due_to_server_ban && st.image_data.as_ref().map_or(false, |i| !i.is_empty());
            if !visible {
                dd.map.insert("hiddenDueToServerBan".to_string(), PlistValue::boolean(true));
            }
        }
        out.extend_from_slice(&gz_plist_bytes(&dict));
        Some(out)
    }
    /// `-remoteUpdate:`.
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        base_remote_update(me, net_data);
        let dict = gunzip_plist(subdata_from(net_data, 0x20));
        let new =
            dict.as_ref().and_then(|d| d.object_for_key("hiddenDueToServerBan")).map_or(false, |v| v.bool_value());
        if new != self.st.borrow().hidden_due_to_server_ban {
            self.st.borrow_mut().hidden_due_to_server_ban = new;
        }
    }
    /// `-worldChanged:`: only the FIRST matching position acts (P6).
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        let pos = self.base.borrow().pos;
        for p in positions.clone() {
            if p.x == pos.x && p.y >= pos.y && !self.base.borrow().needs_removed {
                let world = self.links.world();
                let Some(t) = tile_at_world_position_loaded(pos.x, pos.y, &world) else { return };
                if t.back_wall_type_index() != 2 {
                    return;
                }
                // `[dynamicWorld isClient]` is NO on the server
                if self.base.borrow().needs_removed {
                    return;
                }
                let dw = self.links.dw();
                let dict = self.get_save_dict(me);
                dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    pos,
                    ItemType(self.item_type() as u32),
                    0,
                    0,
                    None,
                    Some(PlistValue::Dict(dict)),
                    false,
                    1,
                    None,
                );
                dw.remove_standard_object(Some(me));
                return;
            }
        }
    }
    /// `-dealloc`.
    fn dealloc(&self, me: &ObjRef) {
        {
            let mut st = self.st.borrow_mut();
            st.image_data = None;
            st.owner_name = None;
        }
        me.set_owner_id(None);
    }
}

impl TapChoiceDynamicObject for Painting {
    /// `-actionTitle` returns nil (modelled as the empty string, the trait returns `String`).
    fn action_title(&self) -> String {
        String::new()
    }
    /// `-isDoubleHeight`.
    fn is_double_height(&self) -> bool {
        false
    }
}

// =====================================================================================================
// PaintingCraftableItemObject
// =====================================================================================================

/// `PaintingCraftableItemObject: CraftableItemObject` (instance size 0x98).
///
/// Note: `CraftableItemObject` and its trait
/// `CraftableItemObjectLike` belong to the workbench module (`crafting/craftable_item_object.rs`), which is built
/// in parallel. This struct therefore embeds the 0x7c `CraftableItem` itself and exposes exactly the virtual
/// selectors the trait asks for (`cio_item`, `get_save_dict`, `free_block_creation_item_save_dict`); the workbench
/// module adds the one-line `impl CraftableItemObjectLike for PaintingCraftableItemObject` forwarding to them.
pub struct PaintingCraftableItemObject {
    /// `craftableItem` (base class ivar @0x08)
    pub item: RefCell<CraftableItem>,
    /// `imageData` (0x88)
    pub image_data: RefCell<Option<Vec<u8>>>,
    /// `outputImageData` (0x90)
    pub output_image_data: RefCell<Option<Vec<u8>>>,
}

/// `craftableObjectType` stored by this subclass.
pub const CRAFTABLE_OBJECT_TYPE_PAINTING: i32 = 2;

impl PaintingCraftableItemObject {
    /// `-initWithCraftableItem:imageData:outputImageData:`.
    pub fn new_with_craftable_item_image_data_output_image_data(
        ci: CraftableItem,
        img: Option<Vec<u8>>,
        out: Option<Vec<u8>>,
    ) -> Rc<Self> {
        Rc::new(PaintingCraftableItemObject {
            item: RefCell::new(ci),
            image_data: RefCell::new(img),
            output_image_data: RefCell::new(out),
        })
    }

    /// `-initWithSaveDict:` (base: `craftableItem` NSData `getBytes:` 0x7c; a missing key = zeroed struct).
    pub fn from_save_dict(d: &PlistDict) -> Rc<Self> {
        let raw = data_value(d, "craftableItem").unwrap_or_default();
        let ci = CraftableItem::read_le(&padded_bytes(&raw, CraftableItem::SIZE));
        Self::new_with_craftable_item_image_data_output_image_data(
            ci,
            data_value(d, "imageData"),
            data_value(d, "outputImageData"),
        )
    }

    /// Alias used by the workbench factory (`init_with_save_dict`).
    pub fn init_with_save_dict(d: &PlistDict) -> Rc<Self> {
        Self::from_save_dict(d)
    }

    /// `cio_item()` of `CraftableItemObjectLike`.
    pub fn cio_item(&self) -> &RefCell<CraftableItem> {
        &self.item
    }

    /// `-getSaveDict`: `craftableItem`, `imageData`, `outputImageData` (if any), `craftableObjectType = 2`.
    /// A nil `imageData` raises `NSInvalidArgumentException` (`setObject:nil forKey:`).
    pub fn get_save_dict(&self) -> PlistDict {
        let mut d = new_mutable_dict();
        d.map.insert("craftableItem".to_string(), PlistValue::data(self.item.borrow().to_vec()));
        match self.image_data.borrow().as_ref() {
            Some(i) => {
                d.map.insert("imageData".to_string(), PlistValue::data(i.clone()));
            }
            None => invalid_argument("Tried to add nil value for key 'imageData' to dictionary"),
        }
        if let Some(o) = self.output_image_data.borrow().as_ref() {
            d.map.insert("outputImageData".to_string(), PlistValue::data(o.clone()));
        }
        d.map.insert("craftableObjectType".to_string(), PlistValue::int(CRAFTABLE_OBJECT_TYPE_PAINTING as i64));
        d
    }

    /// `-freeBlockCreationItemSaveDict`.
    pub fn free_block_creation_item_save_dict(&self) -> Option<PlistValue> {
        let out = self.output_image_data.borrow().clone()?;
        let mut d = new_mutable_dict();
        d.map.insert("outputImageData".to_string(), PlistValue::data(out));
        Some(PlistValue::Dict(d))
    }

    /// `-imageData` / `-setImageData:`
    pub fn image_data(&self) -> Option<Vec<u8>> {
        self.image_data.borrow().clone()
    }
    pub fn set_image_data(&self, v: Option<Vec<u8>>) {
        *self.image_data.borrow_mut() = v;
    }
    /// `-outputImageData` / `-setOutputImageData:`
    pub fn output_image_data(&self) -> Option<Vec<u8>> {
        self.output_image_data.borrow().clone()
    }
    pub fn set_output_image_data(&self, v: Option<Vec<u8>>) {
        *self.output_image_data.borrow_mut() = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::sign::testkit::*;

    fn painting(w: &Rc<World>, d: &Rc<DynamicWorld>, owner: Option<&str>) -> Rc<Painting> {
        let mut sd = new_mutable_dict();
        sd.map.insert("outputImageData".to_string(), PlistValue::data(vec![1u8, 2, 3]));
        Painting::new(w, d, IntPair { x: 10, y: 10 }, None, 77, Some(&sd), owner, Some("Bob")).unwrap()
    }

    #[test]
    fn placement_save_dict_and_net_data() {
        let (w, d, _pb) = world_with_block("paint_a");
        let p = painting(&w, &d, Some("cid"));
        let me: ObjRef = p.clone();
        assert_eq!(p.object_type(), 52);
        assert_eq!(p.item_type(), 77);
        assert_eq!(p.freeblock_creation_item_type(), 77);
        assert_eq!(p.image_data(), Some(vec![1, 2, 3]), "pnginfo is absent or accepts the image");
        let sd = me.get_save_dict(&me);
        for k in ["itemType", "outputImageData", "ownerID", "ownerName", "hasVerifiedImageData"] {
            assert!(sd.map.get(&k.to_string()).is_some(), "{k}");
        }
        assert!(!sd.map.get(&"hasVerifiedImageData".to_string()).unwrap().bool_value());
        assert!(sd.map.get(&"hiddenDueToServerBan".to_string()).is_none());
        // creation blob: 0x20 header + gzip plist, no hidden key
        let b = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(u16::from_le_bytes([b[0x18], b[0x19]]), 77);
        assert!(b[0x1a..0x20].iter().all(|x| *x == 0));
        let dict = gunzip_plist(&b[0x20..]).unwrap();
        assert_eq!(dict_str(Some(&dict), "ownerID").as_deref(), Some("cid"));
        assert!(dict.object_for_key("hiddenDueToServerBan").is_none());
        assert_eq!(b, me.update_net_data_for_client(&me, None).unwrap());
        // hidden: key present
        p.st.borrow_mut().hidden_due_to_server_ban = true;
        let b2 = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b2[0x20..]).unwrap().object_for_key("hiddenDueToServerBan").unwrap().bool_value());
        // empty image also hides
        p.st.borrow_mut().hidden_due_to_server_ban = false;
        p.st.borrow_mut().image_data = None;
        let b3 = me.creation_net_data_for_client(&me, None).unwrap();
        assert!(gunzip_plist(&b3[0x20..]).unwrap().object_for_key("hiddenDueToServerBan").is_some());
    }

    #[test]
    fn load_sets_verified_flag_and_roundtrips() {
        let (w, d, _pb) = world_with_block("paint_b");
        let p = painting(&w, &d, Some("cid"));
        let me: ObjRef = p.clone();
        let sd = me.get_save_dict(&me);
        let p2 = Painting::from_save_dict(&w, &d, &sd, None).unwrap();
        assert!(p2.st.borrow().has_verified_image_data);
        assert_eq!(p2.item_type(), 77);
        assert_eq!(p2.owner_name().as_deref(), Some("Bob"));
        let me2: ObjRef = p2.clone();
        assert_eq!(me2.owner_id().as_deref(), Some("cid"));
        // the reloaded dict now carries the verified flag
        assert!(me2.get_save_dict(&me2).map.get(&"hasVerifiedImageData".to_string()).unwrap().bool_value());
    }

    #[test]
    fn ban_fanout_and_remote_update() {
        let (w, d, _pb) = world_with_block("paint_c");
        let p = painting(&w, &d, Some("cid"));
        let me: ObjRef = p.clone();
        p.user_ban_changed_is_banned(&me, "other", true);
        assert!(!p.hidden_due_to_server_ban());
        p.user_ban_changed_is_banned(&me, "cid", true);
        assert!(p.hidden_due_to_server_ban());
        assert!(me.update_needs_to_be_sent());
        p.user_mute_changed("cid");
        // remote update with the key absent resets the flag
        let mut upd = crate::types::structs::WireStruct::to_vec(&PaintingCreationNetData::default());
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("ownerID".to_string(), PlistValue::string("cid"));
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        me.remote_update(&me, &upd);
        assert!(!p.hidden_due_to_server_ban());
    }

    #[test]
    fn pnginfo_substring_rule() {
        assert!(run_pnginfo_check("echo fine"));
        assert!(!run_pnginfo_check("echo 'x: not a valid PNG file'"));
        // a missing binary prints to stderr only: success
        assert!(run_pnginfo_check("definitely-not-a-command-xyz 2>/dev/null"));
    }

    #[test]
    fn world_changed_drops_on_first_match_only() {
        let (w, d, pb) = world_with_block("paint_d");
        let p = painting(&w, &d, Some("cid"));
        let me: ObjRef = p.clone();
        // back wall present -> nothing happens
        tile(pb, 10, 10).set_back_wall_type_index(9);
        me.world_changed(&me, &mut vec![IntPair { x: 10, y: 11 }]);
        assert!(!me.needs_removed());
        // wrong column / below the painting -> ignored
        tile(pb, 10, 10).set_back_wall_type_index(2);
        me.world_changed(&me, &mut vec![IntPair { x: 11, y: 11 }, IntPair { x: 10, y: 9 }]);
        assert!(!me.needs_removed());
    }

    #[test]
    fn craftable_object_save_dict() {
        let mut ci = CraftableItem::read_le(&[0u8; 0x7c]);
        ci.crafted_item_type = 5;
        let o = PaintingCraftableItemObject::new_with_craftable_item_image_data_output_image_data(
            ci,
            Some(vec![9]),
            Some(vec![7, 7]),
        );
        let d = o.get_save_dict();
        assert_eq!(d.map.get(&"craftableObjectType".to_string()).unwrap().int_value(), 2);
        match d.map.get(&"craftableItem".to_string()).unwrap() {
            PlistValue::Data(b) => assert_eq!(b.bytes.len(), 124),
            _ => panic!(),
        }
        let back = PaintingCraftableItemObject::from_save_dict(&d);
        assert_eq!(back.item.borrow().crafted_item_type, 5);
        assert_eq!(back.image_data(), Some(vec![9]));
        assert_eq!(back.output_image_data(), Some(vec![7, 7]));
        let fb = o.free_block_creation_item_save_dict().unwrap();
        assert!(fb.object_for_key("outputImageData").is_some());
        o.set_output_image_data(None);
        assert!(o.free_block_creation_item_save_dict().is_none());
        o.set_image_data(None);
        let r = crate::foundation::exception::catch_ns(|| o.get_save_dict());
        assert_eq!(r.err().unwrap().name, "NSInvalidArgumentException");
    }
}
