//! `InventoryItem`: one inventory entry (type, dataA/dataB, optional sub-slots, optional dynamic-object dict)
//! and its binary blob (`saveData` / `initWithSaveData:`).
//!
//! sections 2-3. The same blob is used on disk and on the wire (inside other plists).

use std::cell::RefCell;
use std::rc::Rc;

use crate::foundation::exception::range_exception_index;
use crate::foundation::gzip::{gzip_deflate, gzip_inflate};
use crate::items::predicates::inventory_sub_items_count_for_item_type;
use crate::plist::helpers::{data_from_property_list, property_list_from_data};
use crate::plist::value::PlistValue;
use crate::types::item_type::ItemType;

/// Shared handle to an item (ObjC pointer identity: use `Rc::ptr_eq`).
pub type ItemRef = Rc<RefCell<InventoryItem>>;
/// One sub-slot: an `NSMutableArray` of `InventoryItem`, shared by identity.
pub type SubSlot = Rc<RefCell<Vec<ItemRef>>>;

/// Class `InventoryItem` (instance size 40).
#[derive(Debug)]
pub struct InventoryItem {
    /// ivar 0x08 (int32, loaded from u16).
    pub item_type: ItemType,
    /// ivar 0x0c.
    pub data_a: u16,
    /// ivar 0x0e.
    pub data_b: u16,
    /// ivar 0x10: `Some` iff `inventory_sub_items_count_for_item_type(item_type) > 0`, with exactly that many slots.
    pub sub_items: Option<Vec<SubSlot>>,
    /// ivar 0x18.
    pub selected_sub_item_index: u8,
    /// ivar 0x20: an `NSDictionary` (`PlistValue::Dict`).
    pub dynamic_object_save_dict: Option<PlistValue>,
}

fn mutable_array_of(items: Vec<PlistValue>) -> PlistValue {
    let mut v = PlistValue::mutable_array();
    if let PlistValue::Array(a) = &mut v {
        a.items = items;
    }
    v
}

impl InventoryItem {
    /// `-initWithType:dataA:dataB:subItems:dynamicObjectSaveDict:`. Slots are NEW arrays (never the
    /// caller's), filled by shallow-copying the item handles of the matching input slots; extra input slots are
    /// dropped, missing ones become empty; `selected_sub_item_index` starts 0.
    pub fn new(
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<&[Vec<ItemRef>]>,
        dynamic_object_save_dict: Option<PlistValue>,
    ) -> ItemRef {
        let n = inventory_sub_items_count_for_item_type(item_type);
        let subs = if n > 0 {
            let input = sub_items.unwrap_or(&[]);
            let mut outer = Vec::with_capacity(n as usize);
            for i in 0..n as usize {
                let slot: Vec<ItemRef> = if i < input.len() { input[i].clone() } else { Vec::new() };
                outer.push(Rc::new(RefCell::new(slot)));
            }
            Some(outer)
        } else {
            None
        };
        Rc::new(RefCell::new(InventoryItem {
            item_type,
            data_a,
            data_b,
            sub_items: subs,
            selected_sub_item_index: 0,
            dynamic_object_save_dict,
        }))
    }

    /// `-initWithSaveData:`. `len < 8` zero-fills the missing header bytes (the original reads
    /// stack garbage; no producer exists). May `ns_raise` NSRangeException when "s" is shorter than the slot
    /// count.
    pub fn from_save_data(save_data: &[u8]) -> ItemRef {
        let mut hdr = [0u8; 8];
        let copy_len = save_data.len().min(8);
        hdr[..copy_len].copy_from_slice(&save_data[..copy_len]);
        let item_type = ItemType(u16::from_le_bytes([hdr[0], hdr[1]]) as u32);
        let data_a = u16::from_le_bytes([hdr[2], hdr[3]]);
        let data_b = u16::from_le_bytes([hdr[4], hdr[5]]);
        let selected = hdr[6];

        let mut dict: Option<PlistValue> = None;
        if save_data.len() > 8 {
            // propertyListFromData(nil) is nil; failures are logged by the helper itself.
            if let Some(inflated) = gzip_inflate(&save_data[8..]) {
                dict = property_list_from_data(&inflated);
            }
        }
        let d = dict.as_ref().and_then(|p| p.object_for_key("d")).cloned();
        let saved: Option<&Vec<PlistValue>> = match dict.as_ref().and_then(|p| p.object_for_key("s")) {
            Some(PlistValue::Array(a)) => Some(&a.items),
            _ => None,
        };

        let n = inventory_sub_items_count_for_item_type(item_type);
        let subs = if n > 0 {
            let mut outer = Vec::with_capacity(n as usize);
            for i in 0..n as usize {
                let mut slot: Vec<ItemRef> = Vec::new();
                match saved {
                    // savedSubItems nil -> objectAtIndex: on nil gives nil -> empty slot
                    None => {}
                    Some(arr) => {
                        // NO bounds check in the original: a short array raises NSRangeException.
                        if i >= arr.len() {
                            range_exception_index(i as u64, arr.len() as u64, "objectAtIndex:");
                        }
                        if let PlistValue::Array(sa) = &arr[i] {
                            for e in &sa.items {
                                if let PlistValue::Data(bytes) = e {
                                    slot.push(InventoryItem::from_save_data(&bytes.bytes));
                                }
                            }
                        }
                    }
                }
                outer.push(Rc::new(RefCell::new(slot)));
            }
            Some(outer)
        } else {
            None
        };
        Rc::new(RefCell::new(InventoryItem {
            item_type,
            data_a,
            data_b,
            sub_items: subs,
            selected_sub_item_index: selected,
            dynamic_object_save_dict: d,
        }))
    }

    /// `-saveData`: 8-byte LE header `{u16 type, u16 dataA, u16 dataB, u8 selected, u8 0}`,
    /// then `gzip(plist{"s":..., "d":...})` only if that extras dict is non-empty. "s" holds all slots
    /// (empty ones as empty arrays) and is present only when at least one sub item exists.
    pub fn save_data(&self) -> Vec<u8> {
        let mut ret = Vec::with_capacity(8);
        ret.extend_from_slice(&self.item_type.to_u16().to_le_bytes());
        ret.extend_from_slice(&self.data_a.to_le_bytes());
        ret.extend_from_slice(&self.data_b.to_le_bytes());
        ret.push(self.selected_sub_item_index);
        ret.push(0);

        let mut dd = crate::objects::base::new_mutable_dict();
        if let Some(subs) = &self.sub_items {
            if !subs.is_empty() {
                let mut found = false;
                let mut saved = Vec::new();
                for slot in subs {
                    let mut save_slot = Vec::new();
                    for item in slot.borrow().iter() {
                        found = true;
                        save_slot.push(PlistValue::data(item.borrow().save_data()));
                    }
                    saved.push(mutable_array_of(save_slot));
                }
                if found {
                    dd.insert("s".to_string(), mutable_array_of(saved));
                }
            }
        }
        if let Some(d) = &self.dynamic_object_save_dict {
            dd.insert("d".to_string(), d.clone());
        }
        if dd.len() != 0 {
            if let Some(xml) = data_from_property_list(&PlistValue::Dict(dd)) {
                ret.extend_from_slice(&gzip_deflate(&xml));
            }
        }
        ret
    }

    /// `-updateSubItemSlot:atIndex:`. The test is `(idx as i64 as u64) <= count` (off by one on
    /// purpose): `idx == count` raises NSRangeException; negative idx is skipped; `sub_items == None` is a
    /// no-op. The slot object is mutated in place.
    pub fn update_sub_item_slot(&self, sub_item_save_data: &[Vec<u8>], sub_index: i32) {
        let count = self.sub_items.as_ref().map_or(0u64, |s| s.len() as u64);
        if (sub_index as i64 as u64) <= count {
            if let Some(subs) = &self.sub_items {
                if sub_index as i64 as u64 == count {
                    range_exception_index(sub_index as i64 as u64, count, "objectAtIndex:");
                }
                let slot = subs[sub_index as usize].clone();
                slot.borrow_mut().clear();
                for d in sub_item_save_data {
                    let it = InventoryItem::from_save_data(d);
                    slot.borrow_mut().push(it);
                }
            }
        }
    }

    /// `-subItemSlotDataAtIndex:`. `count < idx as u64` gives `None` (negative idx and `idx > count`);
    /// `idx == count` with sub items raises NSRangeException; with no sub items and `idx == 0` the nil slot
    /// enumerates as empty (`Some(vec![])`).
    pub fn sub_item_slot_data_at_index(&self, sub_index: i32) -> Option<Vec<Vec<u8>>> {
        let count = self.sub_items.as_ref().map_or(0u64, |s| s.len() as u64);
        if count < sub_index as i64 as u64 {
            return None;
        }
        match &self.sub_items {
            None => Some(Vec::new()),
            Some(subs) => {
                if sub_index as i64 as u64 == count {
                    range_exception_index(sub_index as i64 as u64, count, "objectAtIndex:");
                }
                let slot = subs[sub_index as usize].clone();
                let out: Vec<Vec<u8>> = slot.borrow().iter().map(|i| i.borrow().save_data()).collect();
                Some(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;
    use crate::plist::helpers::property_list_from_data as plfd;

    fn plain(t: ItemType, a: u16, b: u16) -> ItemRef {
        InventoryItem::new(t, a, b, None, None)
    }

    fn extras(blob: &[u8]) -> PlistValue {
        let xml = gzip_inflate(&blob[8..]).unwrap();
        plfd(&xml).unwrap()
    }

    fn basket_with(slots: &[(usize, ItemRef)]) -> ItemRef {
        let mut input: Vec<Vec<ItemRef>> = vec![Vec::new(); 4];
        for (i, it) in slots {
            input[*i].push(it.clone());
        }
        InventoryItem::new(ItemType::ITEM_BASKET, 1, 2, Some(&input), None)
    }

    #[test]
    fn plain_blob_is_8_bytes() {
        let it = plain(ItemType::ITEM_STICK, 0x1234, 0x5678);
        it.borrow_mut().selected_sub_item_index = 9;
        let b = it.borrow().save_data();
        assert_eq!(b, vec![4, 0, 0x34, 0x12, 0x78, 0x56, 9, 0]);
        let r = InventoryItem::from_save_data(&b);
        let r = r.borrow();
        assert_eq!(
            (r.item_type, r.data_a, r.data_b, r.selected_sub_item_index),
            (ItemType::ITEM_STICK, 0x1234, 0x5678, 9)
        );
        assert!(r.sub_items.is_none() && r.dynamic_object_save_dict.is_none());
    }

    #[test]
    fn basket_round_trip() {
        let b = basket_with(&[(1, plain(ItemType::ITEM_APPLE, 3, 4)), (3, plain(ItemType::ITEM_STICK, 0, 0))]);
        let blob = b.borrow().save_data();
        assert!(blob.len() > 8);
        let ex = extras(&blob);
        assert!(ex.object_for_key("d").is_none());
        let Some(PlistValue::Array(a)) = ex.object_for_key("s") else { unreachable!("no s") };
        assert_eq!(a.items.len(), 4);
        let lens: Vec<usize> =
            a.items.iter().map(|x| if let PlistValue::Array(s) = x { s.items.len() } else { 99 }).collect();
        assert_eq!(lens, vec![0, 1, 0, 1]);
        let r = InventoryItem::from_save_data(&blob);
        assert_eq!(r.borrow().save_data(), blob);
        let r = r.borrow();
        let subs = r.sub_items.as_ref().unwrap();
        assert_eq!(subs.len(), 4);
        assert_eq!(subs[1].borrow()[0].borrow().item_type, ItemType::ITEM_APPLE);
        assert_eq!(subs[1].borrow()[0].borrow().data_a, 3);
    }

    #[test]
    fn nested_basket_in_chest() {
        let inner = basket_with(&[(0, plain(ItemType::ITEM_FLINT, 1, 1))]);
        let mut input: Vec<Vec<ItemRef>> = vec![Vec::new(); 16];
        input[15].push(inner);
        let chest = InventoryItem::new(ItemType::ITEM_CHEST, 0, 0, Some(&input), None);
        let blob = chest.borrow().save_data();
        let r = InventoryItem::from_save_data(&blob);
        assert_eq!(r.borrow().sub_items.as_ref().unwrap().len(), 16);
        assert_eq!(r.borrow().save_data(), blob);
    }

    #[test]
    fn dict_only_and_empty_basket() {
        let mut d = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut d {
            dd.insert("k".to_string(), PlistValue::int(5));
        }
        let it = InventoryItem::new(ItemType::ITEM_STICK, 0, 0, None, Some(d.clone()));
        let blob = it.borrow().save_data();
        let ex = extras(&blob);
        assert!(ex.object_for_key("s").is_none());
        assert!(ex.object_for_key("d").is_some());
        let r = InventoryItem::from_save_data(&blob);
        assert!(r.borrow().dynamic_object_save_dict.is_some());
        // basket with no sub items present and a dict: only d
        let bk = InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, None, Some(d));
        let ex = extras(&bk.borrow().save_data());
        assert!(ex.object_for_key("s").is_none() && ex.object_for_key("d").is_some());
        // basket with nothing: 8 bytes
        let bk = InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, None, None);
        assert_eq!(bk.borrow().save_data().len(), 8);
    }

    #[test]
    fn non_container_ignores_s() {
        let b = basket_with(&[(0, plain(ItemType::ITEM_FLINT, 1, 1))]);
        let mut blob = b.borrow().save_data();
        blob[0..2].copy_from_slice(&(ItemType::ITEM_STICK.0 as u16).to_le_bytes());
        let r = InventoryItem::from_save_data(&blob);
        assert!(r.borrow().sub_items.is_none());
    }

    #[test]
    fn short_s_raises_and_absent_s_is_empty() {
        let b = basket_with(&[(0, plain(ItemType::ITEM_FLINT, 1, 1))]);
        let blob = b.borrow().save_data();
        // craft a short "s" (2 entries)
        let mut ex = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut ex {
            dd.insert("s".to_string(), mutable_array_of(vec![mutable_array_of(vec![]), mutable_array_of(vec![])]));
        }
        let mut crafted = blob[..8].to_vec();
        crafted.extend(gzip_deflate(&data_from_property_list(&ex).unwrap()));
        let e = catch_ns(|| InventoryItem::from_save_data(&crafted)).err().unwrap();
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(e.reason.as_deref(), Some("Index 2 is out of range 2 (in 'objectAtIndex:')"));
        // absent s
        let mut ex = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut ex {
            dd.insert("d".to_string(), PlistValue::mutable_dict());
        }
        let mut c2 = blob[..8].to_vec();
        c2.extend(gzip_deflate(&data_from_property_list(&ex).unwrap()));
        let r = InventoryItem::from_save_data(&c2);
        assert_eq!(r.borrow().sub_items.as_ref().unwrap().iter().filter(|s| s.borrow().is_empty()).count(), 4);
        // garbage gzip
        let mut c3 = blob[..8].to_vec();
        c3.extend_from_slice(&[1, 2, 3, 4, 5]);
        let r = InventoryItem::from_save_data(&c3);
        assert_eq!(r.borrow().sub_items.as_ref().unwrap().len(), 4);
        // short header
        let r = InventoryItem::from_save_data(&[4, 0, 1]);
        assert_eq!((r.borrow().item_type, r.borrow().data_a), (ItemType::ITEM_STICK, 1));
    }

    #[test]
    fn slot_update_and_read_bounds() {
        let b = basket_with(&[(0, plain(ItemType::ITEM_FLINT, 1, 1))]);
        let data = vec![plain(ItemType::ITEM_APPLE, 7, 8).borrow().save_data()];
        let slot_before = b.borrow().sub_items.as_ref().unwrap()[3].clone();
        b.borrow().update_sub_item_slot(&data, 3);
        assert_eq!(slot_before.borrow().len(), 1); // same SubSlot object mutated in place
        assert_eq!(b.borrow().sub_item_slot_data_at_index(3).unwrap(), data);
        b.borrow().update_sub_item_slot(&data, -1); // no-op
        assert!(catch_ns(|| b.borrow().update_sub_item_slot(&data, 4)).is_err());
        assert!(catch_ns(|| b.borrow().sub_item_slot_data_at_index(4)).is_err());
        assert!(b.borrow().sub_item_slot_data_at_index(5).is_none());
        assert!(b.borrow().sub_item_slot_data_at_index(-1).is_none());
        // no sub items
        let p = plain(ItemType::ITEM_STICK, 0, 0);
        assert_eq!(p.borrow().sub_item_slot_data_at_index(0), Some(vec![]));
        assert!(p.borrow().sub_item_slot_data_at_index(1).is_none());
        p.borrow().update_sub_item_slot(&data, 0);
    }

    #[test]
    fn new_copies_slots_shares_items() {
        let it = plain(ItemType::ITEM_FLINT, 1, 1);
        let input = vec![vec![it.clone()], vec![], vec![], vec![], vec![plain(ItemType::ITEM_STICK, 0, 0)]];
        let b = InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, Some(&input), None);
        let b = b.borrow();
        let subs = b.sub_items.as_ref().unwrap();
        assert_eq!(subs.len(), 4);
        assert!(Rc::ptr_eq(&subs[0].borrow()[0], &it));
        subs[0].borrow_mut().clear();
        assert_eq!(input[0].len(), 1);
        let non = InventoryItem::new(ItemType::ITEM_STICK, 0, 0, Some(&input), None);
        assert!(non.borrow().sub_items.is_none());
    }

    #[test]
    fn save_data_skips_empty_slots_and_foreign_entries() {
        // a basket whose slots are all empty has no extras at all
        let bk = basket_with(&[]);
        assert_eq!(bk.borrow().save_data().len(), 8);
        // so does one without any slot list entries
        bk.borrow_mut().sub_items = Some(Vec::new());
        assert_eq!(bk.borrow().save_data().len(), 8);
        // entries of a saved slot that are not data blobs are ignored on load
        let mut dd = crate::objects::base::new_mutable_dict();
        let slot = mutable_array_of(vec![
            PlistValue::int(3),
            PlistValue::data(plain(ItemType::ITEM_STICK, 1, 2).borrow().save_data()),
        ]);
        let empty = || mutable_array_of(vec![]);
        // a slot that is not an array at all loads as an empty slot
        dd.insert("s".to_string(), mutable_array_of(vec![slot, empty(), empty(), PlistValue::int(1)]));
        let ex = PlistValue::Dict(dd);
        let mut crafted = bk.borrow().save_data()[..8].to_vec();
        crafted.extend(gzip_deflate(&data_from_property_list(&ex).unwrap()));
        let r = InventoryItem::from_save_data(&crafted);
        let r = r.borrow();
        let slot0 = r.sub_items.as_ref().unwrap()[0].borrow();
        assert_eq!(slot0.len(), 1);
        assert_eq!(slot0[0].borrow().item_type, ItemType::ITEM_STICK);
    }
}
