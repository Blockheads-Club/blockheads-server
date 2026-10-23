//! `world_ops` rules API: the found-items list. (The custom-rule mutators and `verifyClientCustomRulesData:`
//! are owned by world_state `custom_rules.rs`.)
//!
//!
//! `foundItemsList` is an `NSMutableIndexSet` of ItemType values; dodo-egg breeds live in the same set at
//! `0x2000 + breed`. A nil set behaves like ObjC nil messaging (`containsIndex:` is NO, `addIndex:` is dropped).

use super::state::World;
use crate::foundation::indexset::IndexSet;
use crate::items::inventory_item::InventoryItem;
use crate::types::item_type::ItemType;

/// `(NSUInteger)(breed + 0x2000)`: int addition, then sign extension.
fn egg_index(breed: i32) -> u64 {
    breed.wrapping_add(0x2000) as i64 as u64
}

impl World {
    /// `-[World addItemsFromServerFoundItemsList:]`: `[foundItemsList addIndexes:incomingList]`.
    pub fn add_items_from_server_found_items_list(&self, incoming: &IndexSet) {
        if let Some(s) = self.found_items_list.borrow_mut().as_mut() {
            s.add_indexes(incoming);
        }
    }

    /// `-[World sendUpdatedFoundItemsListToServer]`: `client == nil` => no-op on the server.
    pub fn send_updated_found_items_list_to_server(&self) {}

    fn found_contains(&self, i: u64) -> bool {
        self.found_items_list.borrow().as_ref().map_or(false, |s| s.contains_index(i))
    }

    fn found_add(&self, i: u64) {
        if let Some(s) = self.found_items_list.borrow_mut().as_mut() {
            s.add_index(i);
        }
    }

    /// `-[World privateAddItemToFoundList:addBasketContents:]`. Returns whether the list changed. The recursion
    /// into basket sub-items is always evaluated (no short circuit).
    pub fn private_add_item_to_found_list_add_basket_contents(&self, item: &InventoryItem, add_basket: bool) -> bool {
        let mut changed = false;
        let t = item.item_type;
        if !self.found_contains(t.0 as u64) {
            self.found_add(t.0 as u64);
            changed = true;
        }
        if t == ItemType::ITEM_DODO_EGG {
            let breed = item.data_b as i32;
            if !self.found_list_contains_egg_with_dodo_breed(breed) {
                self.add_dodo_egg_to_found_list_with_breed(breed);
                changed = true;
            }
        }
        if add_basket && t == ItemType::ITEM_BASKET {
            if let Some(subs) = item.sub_items.as_ref() {
                for slot in subs.iter() {
                    let items = slot.borrow().clone();
                    for sub in items.iter() {
                        let r = self.private_add_item_to_found_list_add_basket_contents(&sub.borrow(), false);
                        changed = r || changed;
                    }
                }
            }
        }
        changed
    }

    /// `-[World addItemToFoundList:]`. `client == nil`: only the private add.
    pub fn add_item_to_found_list(&self, item: &InventoryItem) {
        let _changed = self.private_add_item_to_found_list_add_basket_contents(item, true);
    }

    /// `-[World addChestItemsToFoundList:]`: `slots` is an array of arrays of items.
    pub fn add_chest_items_to_found_list(&self, slots: &[Vec<InventoryItem>]) {
        let mut _changed = false;
        for slot in slots {
            for sub in slot {
                let r = self.private_add_item_to_found_list_add_basket_contents(sub, false);
                _changed = r || _changed;
            }
        }
    }

    /// `-[World addDodoEggToFoundListWithBreed:]`: `addIndex:(breed + 0x2000)`.
    pub fn add_dodo_egg_to_found_list_with_breed(&self, breed: i32) {
        self.found_add(egg_index(breed));
    }

    /// `-[World foundListContainsEggWithDodoBreed:]`.
    pub fn found_list_contains_egg_with_dodo_breed(&self, breed: i32) -> bool {
        self.found_contains(egg_index(breed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::inventory_item::InventoryItem;

    fn world() -> std::rc::Rc<World> {
        let w = World::alloc(32);
        *w.found_items_list.borrow_mut() = Some(IndexSet::new());
        w
    }

    #[test]
    fn plain_items_and_eggs() {
        let w = world();
        let sword = InventoryItem::new(ItemType::ITEM_FLINT_AXE, 0, 0, None, None);
        w.add_item_to_found_list(&sword.borrow());
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(ItemType::ITEM_FLINT_AXE.0 as u64));
        assert!(!w.private_add_item_to_found_list_add_basket_contents(&sword.borrow(), true));
        let egg = InventoryItem::new(ItemType::ITEM_DODO_EGG, 0, 7, None, None);
        assert!(w.private_add_item_to_found_list_add_basket_contents(&egg.borrow(), true));
        assert!(w.found_list_contains_egg_with_dodo_breed(7));
        assert!(!w.found_list_contains_egg_with_dodo_breed(8));
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(0x2007));
        // a second egg of the same breed changes nothing
        assert!(!w.private_add_item_to_found_list_add_basket_contents(&egg.borrow(), true));
        // a second breed does
        let egg2 = InventoryItem::new(ItemType::ITEM_DODO_EGG, 0, 9, None, None);
        assert!(w.private_add_item_to_found_list_add_basket_contents(&egg2.borrow(), true));
    }

    #[test]
    fn baskets_add_contents_one_level() {
        let w = world();
        let inner = InventoryItem::new(ItemType::ITEM_FLINT, 0, 0, None, None);
        let basket = InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, Some(&[vec![inner.clone()]]), None);
        assert!(w.private_add_item_to_found_list_add_basket_contents(&basket.borrow(), true));
        let s = w.found_items_list.borrow();
        let s = s.as_ref().unwrap();
        assert!(s.contains_index(ItemType::ITEM_BASKET.0 as u64) && s.contains_index(ItemType::ITEM_FLINT.0 as u64));
        drop(s);
        // add_basket == false does not descend
        let w2 = world();
        assert!(w2.private_add_item_to_found_list_add_basket_contents(&basket.borrow(), false));
        assert!(!w2.found_items_list.borrow().as_ref().unwrap().contains_index(ItemType::ITEM_FLINT.0 as u64));
    }

    #[test]
    fn chests_and_baskets_without_slots() {
        let w = world();
        w.send_updated_found_items_list_to_server();
        let a = InventoryItem::new(ItemType::ITEM_FLINT, 0, 0, None, None);
        let b = InventoryItem::new(ItemType::ITEM_FLINT_AXE, 0, 0, None, None);
        let own = |i: crate::items::inventory_item::ItemRef| std::rc::Rc::try_unwrap(i).ok().unwrap().into_inner();
        w.add_chest_items_to_found_list(&[vec![own(a)], vec![own(b)]]);
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(ItemType::ITEM_FLINT_AXE.0 as u64));
        // a basket whose slots are gone adds only itself
        let basket = InventoryItem::new(ItemType::ITEM_BASKET, 0, 0, None, None);
        basket.borrow_mut().sub_items = None;
        assert!(w.private_add_item_to_found_list_add_basket_contents(&basket.borrow(), true));
    }

    #[test]
    fn union_and_nil_set() {
        let w = world();
        let mut inc = IndexSet::new();
        inc.add_index(5);
        inc.add_index(0x2001);
        w.add_items_from_server_found_items_list(&inc);
        assert!(w.found_items_list.borrow().as_ref().unwrap().contains_index(5));
        assert!(w.found_list_contains_egg_with_dodo_breed(1));
        // nil set: nil messaging
        let n = World::alloc(32);
        n.add_dodo_egg_to_found_list_with_breed(3);
        assert!(!n.found_list_contains_egg_with_dodo_breed(3));
        assert!(n.found_items_list.borrow().is_none());
    }
}
