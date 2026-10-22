//! Per-player data persistence: client blockhead files, found items, inventory uploads, and the painting / chest
//! inventory net messages.
//!
//! Covers `clientBlockheadsRecievedForPlayerID:data:`, `clientBlockheadInventoryRecievedForPlayerID:...`,
//! `loadClientBlockheadsDataForPlayerID:` and the painting / chest inventory messages.

use super::lookups::ops;
use super::state::DynamicWorld;
use crate::foundation::exception::{range_exception_beyond_size, range_exception_index, unrecognized_selector};
use crate::foundation::gsdict::GsDict;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::foundation::indexset::IndexSet;
use crate::foundation::paths::{append_path_component, last_path_component};
use crate::items::inventory_item::InventoryItem;
use crate::nslog;
use crate::objects::base::padded_bytes;
use crate::objects::handle::ObjRef;
use crate::plist::helpers::{data_from_property_list, property_list_from_data, property_list_with_data_raw};
use crate::plist::keyed_archiver::{archive_index_set, unarchive_index_set};
use crate::plist::value::PlistValue;

/// `objectAtIndex:` on a property-list value that must be an array (NSRangeException / unrecognized selector
/// otherwise).
fn object_at_index<'a>(v: &'a PlistValue, i: usize) -> &'a PlistValue {
    match v {
        PlistValue::Array(a) => match a.items.get(i) {
            Some(x) => x,
            None => range_exception_index(i as u64, a.items.len() as u64, "objectAtIndex:"),
        },
        other => unrecognized_selector(other.class_name(), "objectAtIndex:"),
    }
}

fn data_bytes(v: &PlistValue) -> Option<&[u8]> {
    match v {
        PlistValue::Data(d) => Some(&d.bytes),
        _ => None,
    }
}

impl DynamicWorld {
    /// `-newFoundListRecievedFromClient:list:`: remove all indexes, add the incoming set, then store the
    /// keyed archive under `"<pid>_foundItems"`. No `ServerClient` record -> nothing stored.
    pub fn new_found_list_recieved_from_client_list(&self, player_id: &str, incoming: &IndexSet) {
        let hooks = ops();
        hooks.found_items_list_remove_all_indexes(player_id);
        hooks.found_items_list_add_indexes(player_id, incoming);
        let save_key = format!("{}_foundItems", player_id);
        if let Some(fl) = hooks.found_items_list_snapshot(player_id) {
            let data = archive_index_set(&fl, "foundItems");
            self.world_database.set_data(Some(&data), Some(&save_key));
        }
    }

    /// `-clientBlockheadsRecievedForPlayerID:data:`: validate (inflate + parse), then store the ORIGINAL
    /// gzip bytes under `"<pid>_blockheads"`.
    pub fn client_blockheads_recieved_for_player_id_data(&self, player_id: &str, data: &[u8]) {
        let plist = gzip_inflate(data).and_then(|b| property_list_from_data(&b));
        if plist.is_some() {
            self.world_database.set_data(Some(data), Some(&format!("{}_blockheads", player_id)));
        }
    }

    /// `-clientBlockheadInventoryRecievedForPlayerID:blockheadID:data:`. `incoming` with fewer than 8
    /// entries raises NSRangeException (mutations done so far stay done).
    pub fn client_blockhead_inventory_recieved_for_player_id_blockhead_id_data(
        &self,
        player_id: &str,
        blockhead_id: u64,
        data: &[u8],
    ) {
        // `%lld` of a uint64
        let file_name = format!("{}_blockhead_{}_inventory", player_id, blockhead_id as i64);
        // `[NSPropertyListSerialization propertyListWithData:options:0...]` is called directly here (no
        // `propertyListFromData` helper, hence no @try): parser exceptions propagate.
        //  ("raw variant without @try (blockhead inventory loads)"). Note: (the synthesis
        // maps the selector to the @try helper; the body and are followed here).
        let Some(incoming) = gzip_inflate(data).and_then(|b| property_list_with_data_raw(&b, 0)) else { return };
        let mut live = self.live_server_client_blockhead_inventories.borrow_mut();
        let pid_key = player_id.to_string();
        if !live.contains_key(&pid_key) {
            live.insert(pid_key.clone(), PlistValue::mutable_dict());
        }
        let Some(PlistValue::Dict(client_dict)) = live.get_mut(&pid_key) else { return };
        if !client_dict.map.contains_key(&file_name) {
            let mut arr = PlistValue::mutable_array();
            if let PlistValue::Array(a) = &mut arr {
                for _ in 0..8 {
                    a.items.push(PlistValue::mutable_array());
                }
            }
            client_dict.map.insert(file_name.clone(), arr);
        }
        let Some(PlistValue::Array(current)) = client_dict.map.get_mut(&file_name) else { return };
        for i in 0..8usize {
            let save_slot = object_at_index(&incoming, i);
            let Some(PlistValue::Array(mem)) = current.items.get_mut(i) else { continue };
            match save_slot {
                PlistValue::Array(sa) => {
                    // full slot replacement
                    mem.items = sa.items.clone();
                }
                PlistValue::Dict(sd) => {
                    // sub-item change of the container item in the slot
                    let container = mem.items.first().and_then(|v| data_bytes(v)).map(|b| b.to_vec());
                    match container {
                        None => nslog!("expected container for sub item change"),
                        Some(bytes) => {
                            let item = InventoryItem::from_save_data(&bytes);
                            for (k, v) in sd.map.iter() {
                                let slot_data: Vec<Vec<u8>> = match v {
                                    PlistValue::Array(a) => {
                                        a.items.iter().filter_map(|x| data_bytes(x).map(|b| b.to_vec())).collect()
                                    }
                                    _ => Vec::new(),
                                };
                                let idx = crate::foundation::nsstring::int_value(k);
                                item.borrow().update_sub_item_slot(&slot_data, idx);
                            }
                            let new_save = item.borrow().save_data();
                            if mem.items.is_empty() {
                                mem.items.push(PlistValue::data(new_save));
                            } else {
                                mem.items[0] = PlistValue::data(new_save);
                            }
                        }
                    }
                }
                _ => {} // anything else (e.g. NSNull): slot untouched
            }
        }
        let current_slots = client_dict.map.get(&file_name).cloned();
        drop(live);
        if let Some(cs) = current_slots {
            if let Some(xml) = data_from_property_list(&cs) {
                let gz = gzip_deflate(&xml);
                self.client_blockhead_inventories_to_save.borrow_mut().insert(file_name, gz);
            }
        }
    }

    /// `-loadClientBlockheadsDataForPlayerID:`: the join blob (XML plist, NOT gzipped) with keys
    /// `blockheadFiles`, `welcomeBackEvents`, `foundItems`, `foundItems_v2`. The default found-items grant applies
    /// only when `<pid>_foundItems` is missing AND `blockheadFiles` is non-empty. The legacy path runs only before
    /// `hasFinishedDatabaseMigrationTo17`.
    pub fn load_client_blockheads_data_for_player_id(&self, player_id: &str) -> Option<Vec<u8>> {
        let hooks = ops();
        let world = self.world();
        let mut files: GsDict<String, Vec<u8>> = GsDict::new();
        let mut return_extra: Vec<(&str, PlistValue)> = Vec::new();

        let found_items_data = self.world_database.data_for_key(Some(&format!("{}_foundItems", player_id)));
        let blockheads_data = self.world_database.data_for_key(Some(&format!("{}_blockheads", player_id)));
        if let Some(bd) = &blockheads_data {
            files.insert(format!("{}_blockheads", player_id), bd.clone());
            let plist = gzip_inflate(bd).and_then(|b| property_list_from_data(&b));
            if let Some(PlistValue::Array(objs)) = plist.as_ref().and_then(|p| p.object_for_key("dynamicObjects")) {
                for object_dict in objs.items.iter() {
                    let unique_id = object_dict.object_for_key("uniqueID").map_or(0, |v| v.unsigned_long_value());
                    let name = format!("blockhead_{}_inventory", unique_id);
                    let full_key = format!("{}_{}", player_id, name);
                    if let Some(inv) = self.world_database.data_for_key(Some(&full_key)) {
                        files.insert(full_key, inv);
                    }
                }
            }
        }

        // legacy (pre-LMDB) file migration path
        if !world.has_finished_database_migration_to17() {
            let chars: Vec<char> = player_id.chars().collect();
            let n = chars.len();
            let sub = |loc: usize, len: usize| -> String {
                if loc + len > n {
                    range_exception_beyond_size("substringWithRange:", loc as u64, len as u64, n as u64);
                }
                chars[loc..loc + len].iter().collect()
            };
            let player_dir = format!("players/{}/{}/{}/", sub(0, 1), sub(1, 1), sub(2, n.saturating_sub(2)));
            let dir_path = append_path_component(&self.world_save_directory, &player_dir);
            if let Ok(rd) = std::fs::read_dir(&dir_path) {
                for entry in rd.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.contains("blockhead") {
                        let file_path = append_path_component(&dir_path, &file_name);
                        let db_key = format!("{}_{}", player_id, file_name);
                        if !self.world_database.has_key(Some(&db_key)) {
                            if let Ok(d) = std::fs::read(&file_path) {
                                files.insert(format!("{}_{}", player_id, last_path_component(&file_path)), d);
                            }
                        }
                    }
                }
            }
        }

        // load inventories into memory (live cache)
        if !files.is_empty() {
            for key in files.keys_in_order() {
                if key.contains("_inventory") {
                    // `live[playerID]` is created (if absent) BEFORE the parse, as in the original.
                    {
                        let mut live = self.live_server_client_blockhead_inventories.borrow_mut();
                        let pid_key = player_id.to_string();
                        if !live.contains_key(&pid_key) {
                            live.insert(pid_key, PlistValue::mutable_dict());
                        }
                    }
                    let bytes = files.get(&key).cloned().unwrap_or_default();
                    let plist = gzip_inflate(&bytes).and_then(|b| property_list_with_data_raw(&b, 0));
                    match plist {
                        None => nslog!("Error loading blockhead inventory to memory"),
                        Some(plist) => {
                            let mut slots = PlistValue::mutable_array();
                            if let PlistValue::Array(sa) = &mut slots {
                                for i in 0..8usize {
                                    let mut s = PlistValue::mutable_array();
                                    let src = object_at_index(&plist, i);
                                    if let (PlistValue::Array(sv), PlistValue::Array(src)) = (&mut s, src) {
                                        sv.items.extend(src.items.iter().cloned());
                                    }
                                    sa.items.push(s);
                                }
                            }
                            let mut live = self.live_server_client_blockhead_inventories.borrow_mut();
                            let pid_key = player_id.to_string();
                            if !live.contains_key(&pid_key) {
                                live.insert(pid_key.clone(), PlistValue::mutable_dict());
                            }
                            if let Some(PlistValue::Dict(cd)) = live.get_mut(&pid_key) {
                                cd.map.insert(key.clone(), slots);
                            }
                        }
                    }
                }
            }
        }

        if let Some(w) = hooks.world_welcome_back_events_message_for_client_id(player_id) {
            return_extra.push(("welcomeBackEvents", w));
        }

        match &found_items_data {
            None => {
                if !files.is_empty() {
                    // existing player with blockheads but no found-items record: default grant {0..301, 1024..1095}
                    let mut set = IndexSet::new();
                    set.add_indexes_in_range(0, 0x12e);
                    set.add_indexes_in_range(0x400, 0x48);
                    hooks.found_items_list_add_indexes(player_id, &set);
                    return_extra.push(("foundItems", PlistValue::data(archive_index_set(&set, "foundItems"))));
                    let ser = set.serialized_data();
                    return_extra.push(("foundItems_v2", PlistValue::data(gzip_deflate(&ser))));
                }
            }
            Some(fd) => {
                return_extra.push(("foundItems", PlistValue::data(fd.clone())));
                if let Some(set) = unarchive_index_set(fd, "foundItems") {
                    hooks.found_items_list_add_indexes(player_id, &set);
                    let ser = set.serialized_data();
                    return_extra.push(("foundItems_v2", PlistValue::data(gzip_deflate(&ser))));
                }
            }
        }

        let mut return_dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(rd) = &mut return_dict {
            let mut bf = PlistValue::mutable_dict();
            if let PlistValue::Dict(b) = &mut bf {
                for k in files.keys_in_order() {
                    if let Some(v) = files.get(&k) {
                        b.map.insert(k, PlistValue::data(v.clone()));
                    }
                }
            }
            rd.map.insert("blockheadFiles".to_string(), bf);
            for (k, v) in return_extra {
                rd.map.insert(k.to_string(), v);
            }
        }
        data_from_property_list(&return_dict)
    }

    /// `-requestPaintingDataForPainting:` (client path): `client` is nil on the server, a silent no-op.
    pub fn request_painting_data_for_painting(&self, _painting: &ObjRef) {}

    /// `-sendPaintingDataForPaintingWithID:toClient:`: nothing is sent when the painting is missing or
    /// has empty image data. Layout `[0x31][u64 id][image]`, reliable.
    pub fn send_painting_data_for_painting_with_id_to_client(&self, painting_id: u64, client_id: &str) {
        let Some(p) = self.painting_with_id(painting_id) else { return };
        let hooks = ops();
        let Some(image) = hooks.painting_image_data(&p) else { return };
        if image.is_empty() {
            return;
        }
        let mut net = vec![0x31u8];
        net.extend_from_slice(&p.unique_id().to_le_bytes());
        net.extend_from_slice(&image);
        if self.has_server() {
            hooks.server_send_network_data_to_peers_reliable(net, &[client_id.to_string()], true);
        }
    }

    /// `-paintingDataRecievedFromServer:` (client path; the type byte has been stripped).
    pub fn painting_data_recieved_from_server(&self, painting_data: &[u8]) {
        let id = u64::from_le_bytes(padded_bytes(painting_data, 8)[..8].try_into().unwrap());
        if let Some(p) = self.painting_with_id(id) {
            if painting_data.len() > 8 {
                ops().painting_image_data_recieved(&p, &painting_data[8..]);
            }
        }
    }

    /// `-sendChestInventoryForChest:toClientOwningBlockheadWithID:`: only `netBlockheads` is searched;
    /// SAFE (1) and DISPLAY_CABINET (5) require the owner or an admin. Layout `[0x32][u64 chest id][inventory?]`.
    pub fn send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(
        &self,
        chest: &ObjRef,
        remote_blockhead_id: u64,
    ) {
        let hooks = ops();
        let mut client_id: Option<String> = None;
        let nets: Vec<ObjRef> = self.net_blockheads.borrow().clone();
        for bh in nets {
            if bh.unique_id() == remote_blockhead_id {
                client_id = self.hooks().blockhead_client_id(&bh);
                break;
            }
        }
        let Some(cid) = client_id else { return };
        let t = self.hooks().chest_type(chest);
        if t == 5 || t == 1 {
            let owner_ok = chest.owner_id().as_deref() == Some(cid.as_str());
            let admin = self.server.borrow().as_ref().map_or(false, |s| s.player_is_admin_with_id(&cid));
            if !owner_ok && !admin {
                return;
            }
        }
        let mut net = vec![0x32u8];
        net.extend_from_slice(&chest.unique_id().to_le_bytes());
        hooks.chest_load_inventory_items_from_disk_if_needed(chest);
        if let Some(inv) = hooks.chest_inventory_data(chest, false) {
            net.extend_from_slice(&inv);
        }
        if self.has_server() {
            hooks.server_send_network_data_to_peers_reliable(net, &[cid], true);
        }
    }

    /// `-chestInventoryDataRecievedFromServer:` (client path; the type byte has been stripped).
    pub fn chest_inventory_data_recieved_from_server(&self, chest_data: &[u8]) {
        let id = u64::from_le_bytes(padded_bytes(chest_data, 8)[..8].try_into().unwrap());
        let hooks = ops();
        if let Some(obj) = self.interaction_object_with_id(id) {
            if hooks.is_chest(&obj) {
                let inv = if chest_data.len() > 8 { Some(&chest_data[8..]) } else { None };
                hooks.chest_remote_inventory_data_recieved(&obj, inv);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::lookups::{set_ops_hooks, DwOpsHooks};
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::{DwHooks, DwServer};
    use crate::foundation::exception::catch_ns;
    use crate::plist::value::PlistValue;
    use crate::types::structs::IntPair;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn gz_plist(v: &PlistValue) -> Vec<u8> {
        gzip_deflate(&data_from_property_list(v).unwrap())
    }
    fn arr(items: Vec<PlistValue>) -> PlistValue {
        PlistValue::array(items)
    }
    fn slot_array_of_8(first: Vec<PlistValue>) -> PlistValue {
        let mut v = vec![arr(first)];
        for _ in 1..8 {
            v.push(arr(vec![]));
        }
        arr(v)
    }

    /// array replace; <8 entries raises NSRangeException with the exact reason.
    #[test]
    fn inventory_upload_array_replace_and_short() {
        let d = dw("ps_inv", 1);
        let full = slot_array_of_8(vec![PlistValue::data(vec![1u8, 2, 3])]);
        d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data("p1", 7, &gz_plist(&full));
        {
            let live = d.live_server_client_blockhead_inventories.borrow();
            let cd = live.get(&"p1".to_string()).unwrap();
            let slots = cd.object_for_key("p1_blockhead_7_inventory").unwrap();
            if let PlistValue::Array(a) = slots {
                assert_eq!(a.items.len(), 8);
                if let PlistValue::Array(s0) = &a.items[0] {
                    assert_eq!(s0.items.len(), 1);
                } else {
                    panic!()
                }
            } else {
                panic!()
            }
        }
        // saved gzip(xml plist of the 8-array)
        let saved = d
            .client_blockhead_inventories_to_save
            .borrow()
            .get(&"p1_blockhead_7_inventory".to_string())
            .cloned()
            .unwrap();
        let back = property_list_from_data(&gzip_inflate(&saved).unwrap()).unwrap();
        assert!(matches!(back, PlistValue::Array(ref a) if a.items.len() == 8));
        // garbage -> silently ignored
        d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data("p1", 8, &[1, 2, 3]);
        assert!(d.client_blockhead_inventories_to_save.borrow().get(&"p1_blockhead_8_inventory".to_string()).is_none());
        // short upload raises
        let short = arr(vec![arr(vec![]); 3]);
        let r = catch_ns(|| {
            d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data("p1", 9, &gz_plist(&short))
        });
        let e = r.err().expect("raises");
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(e.reason.as_deref(), Some("Index 3 is out of range 3 (in 'objectAtIndex:')"));
    }

    /// A dict in a slot without a container item logs and leaves the slot untouched; NSNull-like slot untouched.
    #[test]
    fn inventory_upload_dict_without_container() {
        let d = dw("ps_dict", 1);
        let mut v = vec![PlistValue::mutable_dict(), PlistValue::Null];
        for _ in 2..8 {
            v.push(arr(vec![]));
        }
        d.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data("p2", 1, &gz_plist(&arr(v)));
        let live = d.live_server_client_blockhead_inventories.borrow();
        let slots = live.get(&"p2".to_string()).unwrap().object_for_key("p2_blockhead_1_inventory").unwrap();
        if let PlistValue::Array(a) = slots {
            for s in a.items.iter() {
                assert!(matches!(s, PlistValue::Array(x) if x.items.is_empty()));
            }
        }
    }

    /// stored bytes equal the uploaded gzip; garbage not stored.
    #[test]
    fn blockheads_upload_stored_verbatim() {
        let d = dw("ps_bh", 1);
        let payload = PlistValue::mutable_dict();
        let gz = gz_plist(&payload);
        d.client_blockheads_recieved_for_player_id_data("p3", &gz);
        assert_eq!(d.world_database.data_for_key(Some("p3_blockheads")), Some(gz));
        d.client_blockheads_recieved_for_player_id_data("p4", &[0, 1, 2]);
        assert!(d.world_database.data_for_key(Some("p4_blockheads")).is_none());
    }

    struct FoundOps {
        list: RefCell<IndexSet>,
        exists: bool,
    }
    impl crate::dynworld::lookups::DwOpsHooksStub for FoundOps {
        fn found_items_list_snapshot(&self, _c: &str) -> Option<IndexSet> {
            if self.exists {
                Some(self.list.borrow().clone())
            } else {
                None
            }
        }
        fn found_items_list_remove_all_indexes(&self, _c: &str) {
            self.list.borrow_mut().remove_all_indexes();
        }
        fn found_items_list_add_indexes(&self, _c: &str, s: &IndexSet) {
            self.list.borrow_mut().add_indexes(s);
        }
    }

    /// found list archive is stored under `<pid>_foundItems` (bplist00).
    #[test]
    fn found_list_stored_as_bplist() {
        let d = dw("ps_found", 1);
        set_ops_hooks(Rc::new(FoundOps { list: RefCell::new(IndexSet::new()), exists: true }));
        let mut s = IndexSet::new();
        s.add_indexes_in_range(5, 3);
        d.new_found_list_recieved_from_client_list("p5", &s);
        let blob = d.world_database.data_for_key(Some("p5_foundItems")).unwrap();
        assert_eq!(&blob[..8], b"bplist00");
        assert_eq!(unarchive_index_set(&blob, "foundItems").unwrap().count(), 3);
        // no ServerClient record: nothing stored
        let d2 = dw("ps_found2", 1);
        set_ops_hooks(Rc::new(FoundOps { list: RefCell::new(IndexSet::new()), exists: false }));
        d2.new_found_list_recieved_from_client_list("p6", &s);
        assert!(d2.world_database.data_for_key(Some("p6_foundItems")).is_none());
    }

    /// a new player gets blockheadFiles = {} and no foundItems keys.
    #[test]
    fn join_blob_for_new_player() {
        let d = dw("ps_join", 1);
        set_ops_hooks(Rc::new(FoundOps { list: RefCell::new(IndexSet::new()), exists: true }));
        d.world().set_has_finished_database_migration_to17(true);
        let blob = d.load_client_blockheads_data_for_player_id("abcdef").unwrap();
        let v = property_list_from_data(&blob).unwrap();
        let bf = v.object_for_key("blockheadFiles").unwrap();
        assert!(matches!(bf, PlistValue::Dict(x) if x.map.is_empty()));
        assert!(v.object_for_key("foundItems").is_none());
        assert!(v.object_for_key("foundItems_v2").is_none());
        assert!(v.object_for_key("welcomeBackEvents").is_none());
    }

    /// Reconnect: blockheadFiles holds `<pid>_blockheads` and one inventory per blockhead; live cache is filled.
    /// The found-items part is covered only with an empty set (a non-empty `serializedData` aborts the reference).
    #[test]
    fn join_blob_with_blockheads() {
        let d = dw("ps_join2", 1);
        set_ops_hooks(Rc::new(FoundOps { list: RefCell::new(IndexSet::new()), exists: true }));
        d.world().set_has_finished_database_migration_to17(true);
        let mut bh_dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(x) = &mut bh_dict {
            x.map.insert("uniqueID".into(), PlistValue::int(77));
        }
        let mut root = PlistValue::mutable_dict();
        if let PlistValue::Dict(x) = &mut root {
            x.map.insert("dynamicObjects".into(), arr(vec![bh_dict]));
        }
        d.client_blockheads_recieved_for_player_id_data("pp", &gz_plist(&root));
        d.world_database.set_data(
            Some(&gz_plist(&slot_array_of_8(vec![PlistValue::data(vec![9u8])]))),
            Some("pp_blockhead_77_inventory"),
        );
        // an existing, EMPTY found-items archive keeps the default grant away and avoids the block-API abort
        d.world_database.set_data(Some(&archive_index_set(&IndexSet::new(), "foundItems")), Some("pp_foundItems"));
        let blob = d.load_client_blockheads_data_for_player_id("pp").unwrap();
        let v = property_list_from_data(&blob).unwrap();
        let bf = v.object_for_key("blockheadFiles").unwrap();
        assert!(bf.object_for_key("pp_blockheads").is_some());
        assert!(bf.object_for_key("pp_blockhead_77_inventory").is_some());
        assert!(v.object_for_key("foundItems").is_some());
        let live = d.live_server_client_blockhead_inventories.borrow();
        assert!(live.get(&"pp".to_string()).unwrap().object_for_key("pp_blockhead_77_inventory").is_some());
    }

    /// The default grant words (see tests): the algorithm's output, independent of the faithful abort.
    #[test]
    fn default_grant_words() {
        let mut set = IndexSet::new();
        set.add_indexes_in_range(0, 0x12e);
        set.add_indexes_in_range(0x400, 0x48);
        let raw = set.serialized_data_raw();
        let words: Vec<u32> = raw.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(words, vec![0x171e8f, 0, 302, 1024, 72]);
    }

    struct Srv {
        admin: bool,
    }
    impl DwServer for Srv {
        fn player_info_for_peer_id(&self, _p: &str) -> Option<PlistValue> {
            None
        }
        fn player_is_admin_with_id(&self, _c: &str) -> bool {
            self.admin
        }
        fn player_update(&self) -> Option<String> {
            None
        }
        fn notify_players_changed(&self) {}
    }
    struct Hk;
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn blockhead_client_id(&self, b: &ObjRef) -> Option<String> {
            Some(format!("c{}", b.unique_id()))
        }
        fn chest_type(&self, c: &ObjRef) -> i32 {
            c.unique_id() as i32
        }
    }
    struct SendOps {
        sent: RefCell<Vec<(Vec<u8>, Vec<String>)>>,
    }
    impl crate::dynworld::lookups::DwOpsHooksStub for SendOps {
        fn server_send_network_data_to_peers_reliable(&self, d: Vec<u8>, p: &[String], _r: bool) {
            self.sent.borrow_mut().push((d, p.to_vec()));
        }
        fn painting_image_data(&self, _p: &ObjRef) -> Option<Vec<u8>> {
            Some(vec![9, 8, 7])
        }
        fn chest_inventory_data(&self, _c: &ObjRef, allow_empty: bool) -> Option<Vec<u8>> {
            assert!(!allow_empty);
            Some(vec![5, 5])
        }
    }

    fn obj(ty: i32, id: u64) -> ObjRef {
        let o: ObjRef = Obj::new(ty, id, Rc::new(RefCell::new(vec![])));
        o.base().borrow_mut().pos = IntPair { x: 1, y: 1 };
        o
    }

    /// painting and chest messages (layout, access rule).
    #[test]
    fn painting_and_chest_messages() {
        let d = dw("ps_net", 1);
        d.set_hooks(Rc::new(Hk));
        d.set_server_server_clients(Some(Rc::new(Srv { admin: false })), None);
        let so = Rc::new(SendOps { sent: RefCell::new(vec![]) });
        set_ops_hooks(so.clone());
        let painting = obj(52, 33);
        d.do_set(52, 33, painting);
        d.send_painting_data_for_painting_with_id_to_client(33, "cl");
        d.send_painting_data_for_painting_with_id_to_client(34, "cl"); // missing
        {
            let s = so.sent.borrow();
            assert_eq!(s.len(), 1);
            let mut exp = vec![0x31u8];
            exp.extend_from_slice(&33u64.to_le_bytes());
            exp.extend_from_slice(&[9, 8, 7]);
            assert_eq!(s[0].0, exp);
            assert_eq!(s[0].1, vec!["cl".to_string()]);
        }
        so.sent.borrow_mut().clear();
        // blockhead 7 belongs to client "c7"; chest id 1 = SAFE owned by someone else
        d.net_blockheads.borrow_mut().push(obj(24, 7));
        let safe = obj(46, 1);
        d.send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(&safe, 7);
        assert!(so.sent.borrow().is_empty());
        safe.set_owner_id(Some("c7".into()));
        d.send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(&safe, 7);
        assert_eq!(so.sent.borrow().len(), 1);
        let mut exp = vec![0x32u8];
        exp.extend_from_slice(&1u64.to_le_bytes());
        exp.extend_from_slice(&[5, 5]);
        assert_eq!(so.sent.borrow()[0].0, exp);
        // unknown blockhead: nothing
        d.send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(&safe, 99);
        assert_eq!(so.sent.borrow().len(), 1);
        // admin may open someone else's safe
        let d2 = dw("ps_net2", 1);
        d2.set_hooks(Rc::new(Hk));
        d2.set_server_server_clients(Some(Rc::new(Srv { admin: true })), None);
        d2.net_blockheads.borrow_mut().push(obj(24, 8));
        let safe2 = obj(46, 1);
        d2.send_chest_inventory_for_chest_to_client_owning_blockhead_with_id(&safe2, 8);
        assert_eq!(so.sent.borrow().len(), 2);
    }
}
