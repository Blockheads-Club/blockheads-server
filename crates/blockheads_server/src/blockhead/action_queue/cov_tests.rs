//! Coverage tests for `action_queue.rs`: the corners of the queueing rules and the jetpack container lookups.

use super::super::interaction_rules::testhost::*;
use super::*;
use crate::blockhead::blockhead::testutil::*;
use crate::items::inventory_item::InventoryItem;
use crate::types::enums::PathType;
use std::cell::RefCell;

fn setup_q() -> (Rc<World>, ObjRef, Rc<AHost>) {
    let h = AHost::install();
    let (world, dw, dwref) = setup();
    let o = new_bh(&world, &dwref, 0);
    std::mem::forget((dw, dwref));
    light_all(&world);
    (world, o, h)
}

fn queue(b: &Blockhead, pos: IntPair, g: InteractionType) -> bool {
    b.queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_disable_cancel_check_is_ai(
        pos, g, PathType(0), 0, None, 0, true, false,
    )
}

fn last_change(b: &Blockhead) -> Vec<(String, i32)> {
    let q = b.bh.borrow().action_queue.clone();
    let a = action_of(q.last().unwrap()).unwrap();
    let ic = a.inventory_change().unwrap();
    ic.keys_in_order().into_iter().map(|k| (k.clone(), ic.get(&k).unwrap().int_value())).collect()
}

#[test]
fn digging_the_column_above_needs_no_open_neighbour() {
    let (w, o, _h) = setup_q();
    let b = bh_of(&o);
    // the tile at the blockhead's own column, one up: dx == 0 and inside the band
    tile(&w, 5, 9).update(|t| t.type_index = 1);
    assert!(queue(b, IntPair { x: 5, y: 9 }, InteractionType::INTERACTION_DIG));
}

#[test]
fn time_crystals_are_not_counted_and_unknown_objects_destroy_nothing() {
    let (w, o, h) = setup_q();
    let b = bh_of(&o);
    // tile type 16 gives item 11 but the server never wants a crystal block
    tile(&w, 6, 8).update(|t| {
        t.type_index = 16;
        t.contents = 0;
    });
    assert!(queue(b, IntPair { x: 6, y: 8 }, InteractionType::INTERACTION_DIG));
    assert!(last_change(b).is_empty());
    // an object that destroys nothing leaves the prediction empty
    tile(&w, 7, 8).update(|t| {
        t.type_index = 2;
        t.contents = 0x30;
    });
    *h.io.borrow_mut() = Some(stub(1, true));
    assert!(queue(b, IntPair { x: 7, y: 8 }, InteractionType::INTERACTION_DIG));
    assert!(last_change(b).is_empty());
}

#[test]
fn free_blocks_of_other_players_are_not_predicted_and_stabs_keep_their_goal() {
    use crate::physics_objects::free_block::FreeBlock;
    let (w, o, h) = setup_q();
    let b = bh_of(&o);
    let dw = real_dw(&w, "aq_cov_fb");
    w.set_is_admin(false);
    let mut d = crate::objects::base::new_mutable_dict();
    d.map.insert("ownerID".to_string(), crate::plist::value::PlistValue::string("someone"));
    let fb = FreeBlock::new(&w, &dw, IntPair { x: 6, y: 9 }, 164, 0, 0, None, Some(d), None, true, None).unwrap();
    *h.fbs.borrow_mut() = vec![fb];
    assert!(queue(b, IntPair { x: 6, y: 9 }, InteractionType::INTERACTION_PICKUP_FREEBLOCK));
    assert!(last_change(b).is_empty());
    // a stab with an ordinary item or no item
    give(b, 8, 0);
    assert!(queue(b, IntPair { x: 6, y: 9 }, InteractionType::INTERACTION_STAB));
    b.bh.borrow().inventory_items[1].borrow_mut().clear();
    assert!(queue(b, IntPair { x: 6, y: 9 }, InteractionType::INTERACTION_STAB));
}

#[test]
fn riders_need_no_human_and_dead_ones_cannot_be_tapped() {
    let (_w, o, _h) = setup_q();
    let b = bh_of(&o);
    b.bh.borrow_mut().ride_object = Some(stub(1, true));
    assert!(b.current_interaction_requires_human_input());
    b.bh.borrow_mut().ride_object = None;
    b.bh.borrow_mut().state.regenerating = 1;
    assert!(!b.tap_is_within_body_radius(Vector2 { x: 5.0, y: 8.0 }));
    b.bh.borrow_mut().state.regenerating = 0;
    b.bh.borrow_mut().state.on_trade_mission = 1;
    assert!(!b.tap_is_within_body_radius(Vector2 { x: 5.0, y: 8.0 }));
}

#[test]
fn jetpack_containers_with_too_few_slots_or_another_item() {
    let (_w, o, _h) = setup_q();
    let b = bh_of(&o);
    let container = InventoryItem::new(ItemType(1), 0, 0, None, None);
    container.borrow_mut().sub_items = Some(vec![]);
    {
        let f = b.bh.borrow();
        f.inventory_items[0].borrow_mut().clear();
        f.inventory_items[0].borrow_mut().push(container.clone());
    }
    assert_eq!(b.jetpack_fuel_count(), 0);
    // three slots, but the fuel slot holds something else
    let subs: Vec<_> = (0..3).map(|_| Rc::new(RefCell::new(Vec::new()))).collect();
    subs[2].borrow_mut().push(InventoryItem::new(ItemType(8), 0, 0, None, None));
    container.borrow_mut().sub_items = Some(subs);
    assert_eq!(b.jetpack_fuel_count(), 0);
}
