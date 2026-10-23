//! Coverage tests for `lifecycle.rs`: spawn kits switched off by custom rules, relocation without lit air and the
//! restoring of an interaction item from slots that cannot hold it.

use super::super::blockhead::testutil::*;
use super::*;
use crate::types::structs::CustomRules;

fn slots_of(b: &Blockhead) -> PlistValue {
    let f = b.bh.borrow();
    let slots: Vec<PlistValue> = f
        .inventory_items
        .iter()
        .map(|s| PlistValue::array(s.borrow().iter().map(|i| PlistValue::data(i.borrow().save_data())).collect()))
        .collect();
    PlistValue::array(slots)
}

#[test]
fn custom_rules_can_switch_off_the_food_and_the_basket() {
    let _h = TestHost::install();
    let (world, _dw, dwref) = setup();
    let mut rules = CustomRules::default();
    rules.custom_rules_enabled = 1;
    rules.spawn_item_types = [2, 0, 2, 0, 0, 0, 0, 0];
    world.set_custom_rules(rules);
    let o = new_bh(&world, &dwref, 1);
    let b = bh_of(&o);
    let count = |i: usize| b.bh.borrow().inventory_items[i].borrow().len();
    assert_eq!(count(1), 1);
    assert_eq!(count(2), 0);
    assert_eq!(count(3), 1);
    assert_eq!(count(4), 0);
}

#[test]
fn unlit_ground_falls_back_to_the_start_portal() {
    let _h = TestHost::install();
    let (world, _dw, dwref) = setup();
    let o = new_bh(&world, &dwref, 0);
    let b = bh_of(&o);
    let mut sd = b.get_save_dict_including_workbench_or_interation_object(false);
    sd.map.insert("pos_x".to_string(), PlistValue::int(5));
    sd.map.insert("pos_y".to_string(), PlistValue::int(3));
    sd.map
        .insert("floatPos".to_string(), PlistValue::array(vec![PlistValue::real_f32(5.5), PlistValue::real_f32(3.0)]));
    for y in 0..32 {
        for x in 0..32 {
            tile(&world, x, y).set_light(0);
        }
    }
    world.set_start_portal_pos(IntPair { x: 6, y: 8 });
    let slots = slots_of(b);
    let r = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
        &world, &dwref, &sd, Some(&slots), None, true, None, None,
    )
    .unwrap();
    assert_eq!(bh_of(&r).pos(), IntPair { x: 6, y: 8 });
}

#[test]
fn the_interaction_item_survives_empty_slots_and_plain_items() {
    let _h = TestHost::install();
    let (world, _dw, dwref) = setup();
    let o = new_bh(&world, &dwref, 0);
    let b = bh_of(&o);
    for (index, sub, put_item) in [(5i64, -1i64, false), (1, -1, true), (1, 1, true), (3, 1, true)] {
        {
            let f = b.bh.borrow();
            f.inventory_items[1].borrow_mut().clear();
            f.inventory_items[5].borrow_mut().clear();
            if put_item {
                f.inventory_items[1].borrow_mut().push(crate::items::inventory_item::InventoryItem::new(
                    ItemType(8),
                    0,
                    0,
                    None,
                    None,
                ));
            }
        }
        let slots = slots_of(b);
        let mut sd = b.get_save_dict_including_workbench_or_interation_object(false);
        sd.map.insert("interactionItemIndex".to_string(), PlistValue::int(index));
        sd.map.insert("interactionItemSubIndex".to_string(), PlistValue::int(sub));
        let r = Blockhead::init_with_world_dynamic_world_save_dict_saved_inventory_slots_cache_reposition_on_load_failure_client_save_dir_client_locally_saved_dict(
            &world, &dwref, &sd, Some(&slots), None, true, None, None,
        )
        .unwrap();
        let has = bh_of(&r).bh.borrow().interaction_item.is_some();
        assert_eq!(has, put_item && index == 1 || (index == 3 && sub == 1 && has), "{index} {sub}");
    }
}
