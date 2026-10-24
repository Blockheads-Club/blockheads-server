//! Coverage tests for `action.rs`.

use super::*;
use crate::plist::value::PlistValue;
use std::cell::RefCell;

#[test]
fn a_saved_action_whose_inventory_slot_is_empty_has_no_item() {
    let mut d = crate::objects::base::new_mutable_dict();
    d.map.insert("interactionItemIndex".to_string(), PlistValue::int(2));
    d.map.insert("interactionItemSubIndex".to_string(), PlistValue::int(-1));
    d.map.insert("interactionItemType".to_string(), PlistValue::int(8));
    let slots: Vec<SubSlot> = (0..4).map(|_| Rc::new(RefCell::new(Vec::new()))).collect();
    let a = Action::init_with_save_dict_inventory_items(&d, &slots);
    assert!(a.interaction_item().is_none());
}
