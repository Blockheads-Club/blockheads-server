//! Every production hook adapter, called with live objects of the right and the wrong class and without a
//! World / DynamicWorld.

use super::hook_fixture::fx;

fn isolated(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f).join().expect("test body panicked");
}

#[test]
fn fixture_builds() {
    isolated(|| {
        let f = fx("hk_fixture");
        assert!(f.objs.len() > 20);
    });
}

#[test]
fn dwopshooks_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_dwopshooks");
        let _ = crate::dynworld::lookups::DwOpsHooks::new_torch(&*f.g, &f.dw, f.p, 1, 1, 1, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_torch(&*f.g, &f.dw, f.p, 1, 1, 1, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_torch(&*f.g, &f.dw, f.p, 1, 1, 1, Some(&f.dict), Some("cid"));
        let _ =
            crate::dynworld::lookups::DwOpsHooks::new_torch(&*f.dead, &f.dw, f.p, 1, 1, 1, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_egg(&*f.g, &f.dw, f.p, Some(&f.dict));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_egg(&*f.g, &f.dw, f.p, Some(&f.dict));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_egg(&*f.g, &f.dw, f.p, Some(&f.dict));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_egg(&*f.dead, &f.dw, f.p, Some(&f.dict));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.g,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_painting(
            &*f.dead,
            &f.dw,
            f.p,
            1,
            Some(&f.dict),
            Some("cid"),
            Some("cid"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.g, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.g, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.g, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.g, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.g, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_rail(&*f.dead, &f.dw, f.p, 1, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_boat(&*f.g, &f.dw, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_boat(&*f.g, &f.dw, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_boat(&*f.g, &f.dw, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_boat(&*f.dead, &f.dw, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.g, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ =
            crate::dynworld::lookups::DwOpsHooks::new_train_car(&*f.dead, &f.dw, 1, f.p, Some(&f.dict), Some("cid"));
        let _ = crate::dynworld::lookups::DwOpsHooks::new_glow_block(&*f.g, &f.dw, f.p, f.tile);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_glow_block(&*f.g, &f.dw, f.p, f.tile);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_glow_block(&*f.g, &f.dw, f.p, f.tile);
        let _ = crate::dynworld::lookups::DwOpsHooks::new_glow_block(&*f.dead, &f.dw, f.p, f.tile);
        let _ =
            crate::dynworld::lookups::DwOpsHooks::new_blockhead(&*f.g, &f.dw, f.p, 1, &f.stub, f.bh_obj.unique_id());
        let _ =
            crate::dynworld::lookups::DwOpsHooks::new_blockhead(&*f.g, &f.dw, f.p, 1, &f.bh_obj, f.bh_obj.unique_id());
        let _ =
            crate::dynworld::lookups::DwOpsHooks::new_blockhead(&*f.dead, &f.dw, f.p, 1, &f.stub, f.bh_obj.unique_id());
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_type(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_type(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_type(&*f.g, &f.obj("tree"));
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_type(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_is_required_soil_type(&*f.g, &f.stub, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_is_required_soil_type(&*f.g, &f.bh_obj, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_is_required_soil_type(&*f.g, &f.obj("tree"), 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::tree_is_required_soil_type(&*f.dead, &f.stub, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_type(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_type(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_type(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_type(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_max_age_gene_variation(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_max_age_gene_variation(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_max_age_gene_variation(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_max_age_gene_variation(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_growth_rate_gene_variation(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_growth_rate_gene_variation(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_growth_rate_gene_variation(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_growth_rate_gene_variation(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_is_required_soil_type(&*f.g, &f.stub, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_is_required_soil_type(&*f.g, &f.bh_obj, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_is_required_soil_type(&*f.g, &f.obj("plant"), 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_is_required_soil_type(&*f.dead, &f.stub, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.g, &f.obj("dodo"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.g, &f.obj("egg"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_can_breed(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_above(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_above(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_above(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_above(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_below(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_below(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_below(&*f.g, &f.obj("plant"));
        let _ = crate::dynworld::lookups::DwOpsHooks::plant_number_of_occupied_tiles_below(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::tap_is_within_body_radius(&*f.g, &f.stub, f.v);
        let _ = crate::dynworld::lookups::DwOpsHooks::tap_is_within_body_radius(&*f.g, &f.bh_obj, f.v);
        let _ = crate::dynworld::lookups::DwOpsHooks::tap_is_within_body_radius(&*f.g, &f.obj("boat"), f.v);
        let _ = crate::dynworld::lookups::DwOpsHooks::tap_is_within_body_radius(&*f.g, &f.obj("hand_car"), f.v);
        let _ = crate::dynworld::lookups::DwOpsHooks::tap_is_within_body_radius(&*f.dead, &f.stub, f.v);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("gate"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("painting"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("column"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("stairs"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("ladder"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("wire"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("rail"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("window"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("free_block"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.g, &f.obj("workbench"));
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_current_item_type(&*f.dead, &f.stub);
        let _ =
            crate::dynworld::lookups::DwOpsHooks::blockhead_goal_interaction_for_npc_chase(&*f.g, &f.stub, &f.stub, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_goal_interaction_for_npc_chase(
            &*f.g, &f.bh_obj, &f.bh_obj, 1,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_goal_interaction_for_npc_chase(
            &*f.g,
            &f.obj("dodo"),
            &f.obj("dodo"),
            1,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_goal_interaction_for_npc_chase(
            &*f.g,
            &f.obj("clownfish"),
            &f.obj("clownfish"),
            1,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_goal_interaction_for_npc_chase(
            &*f.dead, &f.stub, &f.stub, 1,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_crouching(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_crouching(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_crouching(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_set_paused(&*f.g, &f.stub, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_set_paused(&*f.g, &f.bh_obj, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_set_paused(&*f.dead, &f.stub, true);
        let _ =
            crate::dynworld::lookups::DwOpsHooks::blockhead_remote_pickup_request_response(&*f.g, &f.stub, true, &[1]);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_remote_pickup_request_response(
            &*f.g,
            &f.bh_obj,
            true,
            &[1],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_remote_pickup_request_response(
            &*f.dead,
            &f.stub,
            true,
            &[1],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_update_name_text_view(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_update_name_text_view(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_update_name_text_view(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_open(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_open(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_open(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_open(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_last_known_motor_pos(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_last_known_motor_pos(&*f.g, &f.bh_obj);
        let _ =
            crate::dynworld::lookups::DwOpsHooks::elevator_shaft_last_known_motor_pos(&*f.g, &f.obj("elevator_shaft"));
        let _ =
            crate::dynworld::lookups::DwOpsHooks::elevator_shaft_last_known_motor_pos(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_shaft_last_known_motor_pos(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_min_y(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_min_y(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_min_y(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_min_y(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_max_y(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_max_y(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_max_y(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::dynworld::lookups::DwOpsHooks::elevator_motor_max_y(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.obj("rail"));
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.obj("hand_car"));
        let _ =
            crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.obj("steam_train"));
        let _ =
            crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.obj("freight_car"));
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(
            &*f.g,
            &f.obj("passenger_car"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.g, &f.obj("torch"));
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_rail_or_station_name_changed(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.stub, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.bh_obj, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.obj("hand_car"), true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.obj("steam_train"), true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.obj("freight_car"), true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.obj("passenger_car"), true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.g, &f.obj("torch"), true);
        let _ = crate::dynworld::lookups::DwOpsHooks::train_car_set_paused(&*f.dead, &f.stub, true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("painting"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("column"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("ladder"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("stairs"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.g, &f.obj("chest"));
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.stub, &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.bh_obj, &[1, 2, 3]);
        let _ =
            crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.obj("painting"), &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.obj("column"), &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.obj("ladder"), &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.obj("stairs"), &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(
            &*f.g,
            &f.obj("elevator_shaft"),
            &[1, 2, 3],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.g, &f.obj("chest"), &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_image_data_recieved(&*f.dead, &f.stub, &[1, 2, 3]);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.stub, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.bh_obj, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("painting"), "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("column"), "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("ladder"), "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("stairs"), "cid");
        let _ =
            crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("elevator_shaft"), "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.g, &f.obj("chest"), "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_mute_changed(&*f.dead, &f.stub, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.stub, "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.bh_obj, "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.obj("painting"), "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.obj("column"), "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.obj("ladder"), "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.obj("stairs"), "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(
            &*f.g,
            &f.obj("elevator_shaft"),
            "cid",
            true,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.g, &f.obj("chest"), "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::painting_user_ban_changed(&*f.dead, &f.stub, "cid", true);
        let _ = crate::dynworld::lookups::DwOpsHooks::trade_portal_randomize_local_trade_offsets(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::trade_portal_randomize_local_trade_offsets(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::trade_portal_randomize_local_trade_offsets(
            &*f.g,
            &f.obj("trade_portal"),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::trade_portal_randomize_local_trade_offsets(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.g, &f.obj("chest"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.g, &f.obj("trade_portal"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.g, &f.obj("workbench"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_land_owner_id(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.g, &f.obj("chest"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.g, &f.obj("trade_portal"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.g, &f.obj("workbench"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_width_radius(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.g, &f.obj("chest"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.g, &f.obj("trade_portal"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.g, &f.obj("workbench"));
        let _ = crate::dynworld::lookups::DwOpsHooks::interaction_object_height_radius(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::world_welcome_back_events_message_for_client_id(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::world_welcome_back_events_message_for_client_id(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::world_welcome_back_events_message_for_client_id(&*f.dead, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.g, "cid",
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.g, "cid",
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.dead, "cid",
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_is_banned_with_id(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_is_banned_with_id(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::server_player_is_banned_with_id(&*f.dead, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_snapshot(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_snapshot(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_snapshot(&*f.dead, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_add_indexes(
            &*f.g,
            "cid",
            &crate::foundation::indexset::IndexSet::new(),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_add_indexes(
            &*f.g,
            "cid",
            &crate::foundation::indexset::IndexSet::new(),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_add_indexes(
            &*f.dead,
            "cid",
            &crate::foundation::indexset::IndexSet::new(),
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::particle_emitter_add_particle(
            &*f.g, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5, [0.0; 4],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::particle_emitter_add_particle(
            &*f.g, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5, [0.0; 4],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::particle_emitter_add_particle(
            &*f.dead, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5, [0.0; 4],
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_teleport_to_pos(&*f.g, &f.stub, f.p);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_teleport_to_pos(&*f.g, &f.bh_obj, f.p);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_teleport_to_pos(&*f.dead, &f.stub, f.p);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_stop_all_actions(&*f.g, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_stop_all_actions(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::lookups::DwOpsHooks::blockhead_stop_all_actions(&*f.dead, &f.stub);
        let _ = crate::dynworld::lookups::DwOpsHooks::world_remove_tile_at_world_x_world_y(&*f.g, 1, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::world_remove_tile_at_world_x_world_y(&*f.g, 1, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::world_remove_tile_at_world_x_world_y(&*f.dead, 1, 1);
        let _ = crate::dynworld::lookups::DwOpsHooks::world_ownership_sign_was_placed_or_changed_at_pos(
            &*f.g,
            f.p,
            Some("cid"),
            1,
            1,
            true,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::world_ownership_sign_was_placed_or_changed_at_pos(
            &*f.g,
            f.p,
            Some("cid"),
            1,
            1,
            true,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::world_ownership_sign_was_placed_or_changed_at_pos(
            &*f.g,
            f.p,
            Some("cid"),
            1,
            1,
            true,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::world_ownership_sign_was_placed_or_changed_at_pos(
            &*f.dead,
            f.p,
            Some("cid"),
            1,
            1,
            true,
        );
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_remove_all_indexes(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_remove_all_indexes(&*f.g, "cid");
        let _ = crate::dynworld::lookups::DwOpsHooks::found_items_list_remove_all_indexes(&*f.dead, "cid");
    });
}

#[test]
fn dwhooks_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_dwhooks");
        let _ = crate::dynworld::state::DwHooks::tree_update_all_owned_tiles_to_new_id_size(&*f.g, &f.stub);
        let _ = crate::dynworld::state::DwHooks::tree_update_all_owned_tiles_to_new_id_size(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::state::DwHooks::tree_update_all_owned_tiles_to_new_id_size(&*f.g, &f.obj("tree"));
        let _ = crate::dynworld::state::DwHooks::tree_update_all_owned_tiles_to_new_id_size(&*f.dead, &f.stub);
        let _ = crate::dynworld::state::DwHooks::update_rain_dt(&*f.g, &f.stub, 0.5, 0.5);
        let _ = crate::dynworld::state::DwHooks::update_rain_dt(&*f.g, &f.bh_obj, 0.5, 0.5);
        let _ = crate::dynworld::state::DwHooks::update_rain_dt(&*f.g, &f.obj("torch"), 0.5, 0.5);
        let _ = crate::dynworld::state::DwHooks::update_rain_dt(&*f.dead, &f.stub, 0.5, 0.5);
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.stub);
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("gate"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("painting"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("column"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("stairs"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("ladder"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("wire"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("rail"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("window"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("free_block"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.g, &f.obj("workbench"));
        let _ = crate::dynworld::state::DwHooks::item_type(&*f.dead, &f.stub);
        let _ = crate::dynworld::state::DwHooks::remove_window_at_pos(&*f.g, f.p);
        let _ = crate::dynworld::state::DwHooks::remove_window_at_pos(&*f.g, f.p);
        let _ = crate::dynworld::state::DwHooks::remove_window_at_pos(&*f.dead, f.p);
        let _ = crate::dynworld::state::DwHooks::cleanup(&*f.g, &f.stub);
        let _ = crate::dynworld::state::DwHooks::cleanup(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::state::DwHooks::cleanup(&*f.g, &f.obj("hand_car"));
        let _ = crate::dynworld::state::DwHooks::cleanup(&*f.g, &f.obj("steam_train"));
        let _ = crate::dynworld::state::DwHooks::cleanup(&*f.dead, &f.stub);
        let _ = crate::dynworld::state::DwHooks::blockhead_stop_riding(&*f.g, &f.stub);
        let _ = crate::dynworld::state::DwHooks::blockhead_stop_riding(&*f.g, &f.bh_obj);
        let _ = crate::dynworld::state::DwHooks::blockhead_stop_riding(&*f.dead, &f.stub);
    });
}

#[test]
fn worldcorehooks_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_worldcorehooks");
        let _ = crate::world::lifecycle::WorldCoreHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.g,
            Some("cid"),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.g,
            Some("cid"),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::server_player_name_for_player_with_id_including_old_players(
            &*f.dead,
            Some("cid"),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::projectile_manager_fire(
            &*f.g,
            f.v,
            f.v,
            Some(&f.stub),
            1,
            Some(&f.stub),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::projectile_manager_fire(
            &*f.g,
            f.v,
            f.v,
            Some(&f.bh_obj),
            1,
            Some(&f.bh_obj),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::projectile_manager_fire(
            &*f.dead,
            f.v,
            f.v,
            Some(&f.stub),
            1,
            Some(&f.stub),
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_set_server_server_clients(&*f.g, true);
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_set_server_server_clients(&*f.g, true);
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_set_server_server_clients(&*f.dead, true);
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_blockhead_previews(&*f.g);
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_blockhead_previews(&*f.g);
        let _ = crate::world::lifecycle::WorldCoreHooks::dynamic_world_blockhead_previews(&*f.dead);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_name(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_name(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_name(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_is_client_blockhead_being_controlled_by_server(
            &*f.g, &f.stub,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_is_client_blockhead_being_controlled_by_server(
            &*f.g, &f.bh_obj,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_is_client_blockhead_being_controlled_by_server(
            &*f.dead, &f.stub,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_should_continue_simulating(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_should_continue_simulating(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_should_continue_simulating(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_suffer_damage_is_simulation_recoil(
            &*f.g, &f.stub, 0.5, true, true,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_suffer_damage_is_simulation_recoil(
            &*f.g, &f.bh_obj, 0.5, true, true,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_suffer_damage_is_simulation_recoil(
            &*f.dead, &f.stub, 0.5, true, true,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_asleep(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_asleep(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_asleep(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_regenerating(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_regenerating(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_regenerating(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_meditating(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_meditating(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_meditating(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_on_trade_mission(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_on_trade_mission(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_on_trade_mission(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_has_actions(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_has_actions(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_has_actions(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_action_count(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_action_count(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_action_count(&*f.dead, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_cancel_simulate_due_to_collapse(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_cancel_simulate_due_to_collapse(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_cancel_simulate_due_to_collapse(&*f.dead, &f.stub);
        let _ =
            crate::world::lifecycle::WorldCoreHooks::blockhead_current_interaction_requires_human_input(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_current_interaction_requires_human_input(
            &*f.g, &f.bh_obj,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_current_interaction_requires_human_input(
            &*f.dead, &f.stub,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::world_load_light_block_for_client_light_block_index_into_physical_block(&*f.g, &f.world, 1, f.pb);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_load_light_block_for_client_light_block_index_into_physical_block(&*f.g, &f.world, 1, f.pb);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_load_light_block_for_client_light_block_index_into_physical_block(&*f.dead, &f.world, 1, f.pb);
        let _ =
            crate::world::lifecycle::WorldCoreHooks::world_welcome_back_message_for_simulation_events(&*f.g, &f.world);
        let _ =
            crate::world::lifecycle::WorldCoreHooks::world_welcome_back_message_for_simulation_events(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_welcome_back_message_for_simulation_events(
            &*f.dead, &f.world,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::world_pause_button_tapped(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_pause_button_tapped(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_pause_button_tapped(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_pause_button_tapped(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_pause_button_tapped(&*f.dead, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_path_request_pump(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_path_request_pump(&*f.g, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::world_path_request_pump(&*f.dead, &f.world);
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_set_world_width_macro(&*f.g, 1);
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_set_world_width_macro(&*f.g, 1);
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_set_world_width_macro(&*f.dead, 1);
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_do_add_electricity_particle_with_path_size(
            &*f.g,
            &[],
            0.5,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_do_add_electricity_particle_with_path_size(
            &*f.g,
            &[],
            0.5,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::particle_emitter_do_add_electricity_particle_with_path_size(
            &*f.dead,
            &[],
            0.5,
        );
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_display_tip(&*f.g, "cid");
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_display_tip(&*f.g, "cid");
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_display_tip(&*f.dead, "cid");
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_current_tip(&*f.g);
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_current_tip(&*f.g);
        let _ = crate::world::lifecycle::WorldCoreHooks::tip_manager_current_tip(&*f.dead);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_die_for_good(&*f.g, &f.stub);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_die_for_good(&*f.g, &f.bh_obj);
        let _ = crate::world::lifecycle::WorldCoreHooks::blockhead_die_for_good(&*f.dead, &f.stub);
    });
}

#[test]
fn worldopsdeps_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_worldopsdeps");
        let _ = crate::world::misc::WorldOpsDeps::load_new_blockhead_at_pos_craftable_item_object_unique_id(
            &*f.g,
            f.p,
            &f.stub,
            f.bh_obj.unique_id(),
        );
        let _ = crate::world::misc::WorldOpsDeps::load_new_blockhead_at_pos_craftable_item_object_unique_id(
            &*f.g,
            f.p,
            &f.bh_obj,
            f.bh_obj.unique_id(),
        );
        let _ = crate::world::misc::WorldOpsDeps::load_new_blockhead_at_pos_craftable_item_object_unique_id(
            &*f.g,
            f.p,
            &f.obj("torch"),
            f.bh_obj.unique_id(),
        );
        let _ = crate::world::misc::WorldOpsDeps::load_new_blockhead_at_pos_craftable_item_object_unique_id(
            &*f.dead,
            f.p,
            &f.stub,
            f.bh_obj.unique_id(),
        );
        let _ = crate::world::misc::WorldOpsDeps::continue_simulate(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::continue_simulate(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::continue_simulate(&*f.dead);
        let _ = crate::world::misc::WorldOpsDeps::delegate_set_paused(&*f.g, true);
        let _ = crate::world::misc::WorldOpsDeps::delegate_set_paused(&*f.g, true);
        let _ = crate::world::misc::WorldOpsDeps::delegate_set_paused(&*f.dead, true);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_set_count_watcher_none(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_set_count_watcher_none(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_set_count_watcher_none(&*f.dead);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_amount(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_amount(&*f.g);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_amount(&*f.dead);
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_modify_modify_string(&*f.g, 1, "cid");
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_modify_modify_string(&*f.g, 1, "cid");
        let _ = crate::world::misc::WorldOpsDeps::crystal_manager_modify_modify_string(&*f.dead, 1, "cid");
        let _ = crate::world::misc::WorldOpsDeps::particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(&*f.g, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5);
        let _ = crate::world::misc::WorldOpsDeps::particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(&*f.g, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5);
        let _ = crate::world::misc::WorldOpsDeps::particle_emitter_add_particle_at_pos_velocity_color_gravity_type_life_scale(&*f.dead, [0.0; 3], [0.0; 3], [0.0; 4], 1, 0.5, 0.5);
        let _ = crate::world::misc::WorldOpsDeps::bh_is_idle(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::bh_is_idle(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::bh_is_idle(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(
            &*f.g,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(
            &*f.g,
            &f.bh_obj,
            crate::types::item_type::ItemType(1048),
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::bh_check_if_can_warp_in_second_blockhead_after_item_added_data_b(
            &*f.dead,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&*f.g, &f.stub, &f.tile, f.p, &mut false, true, 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&*f.g, &f.bh_obj, &f.tile, f.p, &mut false, true, 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&*f.dead, &f.stub, &f.tile, f.p, &mut false, true, 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.stub, f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.stub), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.bh_obj, f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.bh_obj), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.obj("chest"), f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.obj("chest")), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.obj("trade_portal"), f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.obj("trade_portal")), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.obj("workbench"), f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.obj("workbench")), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.g, &f.obj("torch"), f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.obj("torch")), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_queue_action_with_goal_pos_goal_interaction_path_type_interaction_object_id_craftable_item_object_craft_count_or_extra_data_is_ai(&*f.dead, &f.stub, f.p, crate::types::enums::InteractionType(1), 1, f.bh_obj.unique_id(), Some(&f.stub), 1, true);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.g, &f.stub, f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.g, &f.bh_obj, f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.g, &f.obj("chest"), f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.g, &f.obj("trade_portal"), f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.g, &f.obj("workbench"), f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::bh_cancel_any_action_at_goal_pos_or_with_interaction_object_id_goal_interaction_craft_count_or_extra_data(&*f.dead, &f.stub, f.p, f.bh_obj.unique_id(), crate::types::enums::InteractionType(1), 1);
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(&*f.g, &f.stub, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(&*f.g, &f.bh_obj, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(&*f.g, &f.obj("chest"), &f.obj("chest"));
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(
            &*f.g,
            &f.obj("trade_portal"),
            &f.obj("trade_portal"),
        );
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(
            &*f.g,
            &f.obj("workbench"),
            &f.obj("workbench"),
        );
        let _ = crate::world::misc::WorldOpsDeps::io_can_be_used_by_blockhead(&*f.dead, &f.stub, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.g, &f.obj("chest"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.g, &f.obj("trade_portal"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.g, &f.obj("workbench"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_in_use(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(&*f.g, &f.stub, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(&*f.g, &f.bh_obj, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(
            &*f.g,
            &f.obj("chest"),
            &f.obj("chest"),
        );
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(
            &*f.g,
            &f.obj("trade_portal"),
            &f.obj("trade_portal"),
        );
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(
            &*f.g,
            &f.obj("workbench"),
            &f.obj("workbench"),
        );
        let _ = crate::world::misc::WorldOpsDeps::io_start_interaction_with_blockhead(&*f.dead, &f.stub, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("chest"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("trade_portal"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("workbench"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("column"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("ladder"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("stairs"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::world::misc::WorldOpsDeps::io_is_paintable(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.stub, 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.bh_obj, 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.obj("column"), 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.obj("ladder"), 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.obj("stairs"), 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.obj("elevator_shaft"), 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.g, &f.obj("chest"), 1);
        let _ = crate::world::misc::WorldOpsDeps::obj_paint(&*f.dead, &f.stub, 1);
        let _ = crate::world::misc::WorldOpsDeps::craftable_item(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::craftable_item(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::craftable_item(&*f.g, &f.obj("torch"));
        let _ = crate::world::misc::WorldOpsDeps::craftable_item(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.g, &f.obj("free_block"));
        let _ = crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.g,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.g,
            &f.bh_obj,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.dead,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::world::misc::WorldOpsDeps::obj_needs_removed(&*f.g, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::obj_needs_removed(&*f.g, &f.bh_obj);
        let _ = crate::world::misc::WorldOpsDeps::obj_needs_removed(&*f.g, &f.obj("torch"));
        let _ = crate::world::misc::WorldOpsDeps::obj_needs_removed(&*f.dead, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::obj_can_be_removed_by_blockhead(&*f.g, &f.stub, &f.stub);
        let _ = crate::world::misc::WorldOpsDeps::obj_can_be_removed_by_blockhead(&*f.g, &f.bh_obj, &f.bh_obj);
        let _ =
            crate::world::misc::WorldOpsDeps::obj_can_be_removed_by_blockhead(&*f.g, &f.obj("torch"), &f.obj("torch"));
        let _ = crate::world::misc::WorldOpsDeps::obj_can_be_removed_by_blockhead(&*f.dead, &f.stub, &f.stub);
    });
}

#[test]
fn npcenv_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_npcenv");
        let _ = crate::npc::npc::NpcEnv::mark_circum_navigate_x(&*f.g, 1);
        let _ = crate::npc::npc::NpcEnv::mark_circum_navigate_x(&*f.g, 1);
        let _ = crate::npc::npc::NpcEnv::mark_circum_navigate_x(&*f.dead, 1);
        let _ = crate::npc::npc::NpcEnv::is_client(&*f.g);
        let _ = crate::npc::npc::NpcEnv::is_client(&*f.g);
        let _ = crate::npc::npc::NpcEnv::is_client(&*f.dead);
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.stub);
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.bh_obj);
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("free_block"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("gate"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("painting"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("column"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("stairs"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("ladder"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("wire"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("elevator_shaft"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("elevator_motor"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("rail"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("window"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.g, &f.obj("workbench"));
        let _ = crate::npc::npc::NpcEnv::free_block_item_type(&*f.dead, &f.stub);
        let _ = crate::npc::npc::NpcEnv::blockhead_paused(&*f.g, &f.stub);
        let _ = crate::npc::npc::NpcEnv::blockhead_paused(&*f.g, &f.bh_obj);
        let _ = crate::npc::npc::NpcEnv::blockhead_paused(&*f.dead, &f.stub);
        let _ = crate::npc::npc::NpcEnv::blockhead_set_riding_object(&*f.g, &f.stub, Some(&f.stub));
        let _ = crate::npc::npc::NpcEnv::blockhead_set_riding_object(&*f.g, &f.bh_obj, Some(&f.bh_obj));
        let _ = crate::npc::npc::NpcEnv::blockhead_set_riding_object(&*f.dead, &f.stub, Some(&f.stub));
        let _ = crate::npc::npc::NpcEnv::feeder_chest_remove_item_if_available(&*f.g, &f.stub, 1);
        let _ = crate::npc::npc::NpcEnv::feeder_chest_remove_item_if_available(&*f.g, &f.bh_obj, 1);
        let _ = crate::npc::npc::NpcEnv::feeder_chest_remove_item_if_available(&*f.g, &f.obj("chest"), 1);
        let _ = crate::npc::npc::NpcEnv::feeder_chest_remove_item_if_available(&*f.dead, &f.stub, 1);
        let _ = crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.g, f.p, 1, &f.dict, &f.stub);
        let _ = crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.g, f.p, 1, &f.dict, &f.bh_obj);
        let _ = crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.g, f.p, 1, &f.dict, &f.obj("dodo"));
        let _ =
            crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.g, f.p, 1, &f.dict, &f.obj("clownfish"));
        let _ =
            crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.g, f.p, 1, &f.dict, &f.obj("free_block"));
        let _ = crate::npc::npc::NpcEnv::create_free_block_for_captured_npc(&*f.dead, f.p, 1, &f.dict, &f.stub);
        let _ = crate::npc::npc::NpcEnv::remove_curse_words_from_blockhead_name(&*f.g, "cid");
        let _ = crate::npc::npc::NpcEnv::remove_curse_words_from_blockhead_name(&*f.g, "cid");
        let _ = crate::npc::npc::NpcEnv::remove_curse_words_from_blockhead_name(&*f.dead, "cid");
        let _ = crate::npc::npc::NpcEnv::blockhead_stop_riding(&*f.g, &f.stub);
        let _ = crate::npc::npc::NpcEnv::blockhead_stop_riding(&*f.g, &f.bh_obj);
        let _ = crate::npc::npc::NpcEnv::blockhead_stop_riding(&*f.dead, &f.stub);
    });
}

#[test]
fn passiveenv_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_passiveenv");
        let _ = crate::npc::egg::PassiveEnv::plant_type(&*f.g, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::plant_type(&*f.g, &f.bh_obj);
        let _ = crate::npc::egg::PassiveEnv::plant_type(&*f.g, &f.obj("plant"));
        let _ = crate::npc::egg::PassiveEnv::plant_type(&*f.dead, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::bh_paused(&*f.g, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::bh_paused(&*f.g, &f.bh_obj);
        let _ = crate::npc::egg::PassiveEnv::bh_paused(&*f.dead, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::bh_should_continue_simulating(&*f.g, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::bh_should_continue_simulating(&*f.g, &f.bh_obj);
        let _ = crate::npc::egg::PassiveEnv::bh_should_continue_simulating(&*f.dead, &f.stub);
        let _ = crate::npc::egg::PassiveEnv::rod_valid(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_valid(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_valid(&*f.dead, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hook_pos(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hook_pos(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hook_pos(&*f.dead, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hooked_fish(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hooked_fish(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hooked_fish(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_hooked_fish(&*f.dead, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_set_hooked_fish(&*f.g, &f.rod, Some(&f.stub));
        let _ = crate::npc::egg::PassiveEnv::rod_set_hooked_fish(&*f.g, &f.rod, Some(&f.bh_obj));
        let _ = crate::npc::egg::PassiveEnv::rod_set_hooked_fish(&*f.g, &f.rod, Some(&f.obj("clownfish")));
        let _ = crate::npc::egg::PassiveEnv::rod_set_hooked_fish(&*f.dead, &f.rod, Some(&f.stub));
        let _ = crate::npc::egg::PassiveEnv::rod_blockhead(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_blockhead(&*f.g, &f.rod);
        let _ = crate::npc::egg::PassiveEnv::rod_blockhead(&*f.dead, &f.rod);
    });
}

#[test]
fn workbenchhost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_workbenchhost");
        let _ = crate::crafting::workbench::WorkbenchHost::bh_paused(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_paused(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_paused(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_has_coffee_energy(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_has_coffee_energy(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_has_coffee_energy(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_unable_to_work_reason(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_unable_to_work_reason(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_unable_to_work_reason(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_should_continue_simulating(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_should_continue_simulating(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_should_continue_simulating(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_crafting_at_any_workbench(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_crafting_at_any_workbench(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_crafting_at_any_workbench(&*f.g, &f.obj("workbench"));
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_crafting_at_any_workbench(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_adding_fuel_to_any_workbench(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_adding_fuel_to_any_workbench(&*f.g, &f.bh_obj);
        let _ =
            crate::crafting::workbench::WorkbenchHost::bh_is_adding_fuel_to_any_workbench(&*f.g, &f.obj("workbench"));
        let _ = crate::crafting::workbench::WorkbenchHost::bh_is_adding_fuel_to_any_workbench(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_interaction_workbench(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_interaction_workbench(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_interaction_workbench(&*f.g, &f.obj("workbench"));
        let _ = crate::crafting::workbench::WorkbenchHost::bh_interaction_workbench(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_set_interaction_workbench(&*f.g, &f.stub, Some(&f.stub));
        let _ =
            crate::crafting::workbench::WorkbenchHost::bh_set_interaction_workbench(&*f.g, &f.bh_obj, Some(&f.bh_obj));
        let _ = crate::crafting::workbench::WorkbenchHost::bh_set_interaction_workbench(
            &*f.g,
            &f.obj("workbench"),
            Some(&f.obj("workbench")),
        );
        let _ =
            crate::crafting::workbench::WorkbenchHost::bh_set_interaction_workbench(&*f.dead, &f.stub, Some(&f.stub));
        let _ = crate::crafting::workbench::WorkbenchHost::bh_double_time_unlocked(&*f.g, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_double_time_unlocked(&*f.g, &f.bh_obj);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_double_time_unlocked(&*f.dead, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.g,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.g,
            &f.bh_obj,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_subtract_items_from_inventory_of_type_count_data_b(
            &*f.dead,
            &f.stub,
            crate::types::item_type::ItemType(1048),
            1,
            1,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_add_item_to_inventory(&*f.g, &f.stub, &f.item);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_add_item_to_inventory(&*f.g, &f.bh_obj, &f.item);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_add_item_to_inventory(&*f.dead, &f.stub, &f.item);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_craft_item_finished_at_workbench(
            &*f.g, &f.stub, true, &f.stub,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_craft_item_finished_at_workbench(
            &*f.g, &f.bh_obj, true, &f.bh_obj,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_craft_item_finished_at_workbench(
            &*f.g,
            &f.obj("workbench"),
            true,
            &f.obj("workbench"),
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_craft_item_finished_at_workbench(
            &*f.dead, &f.stub, true, &f.stub,
        );
        let _ =
            crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(&*f.g, &f.stub, &f.stub);
        let _ = crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(
            &*f.g, &f.bh_obj, &f.bh_obj,
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(
            &*f.g,
            &f.obj("chest"),
            &f.obj("chest"),
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(
            &*f.g,
            &f.obj("trade_portal"),
            &f.obj("trade_portal"),
        );
        let _ = crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(
            &*f.g,
            &f.obj("workbench"),
            &f.obj("workbench"),
        );
        let _ =
            crate::crafting::workbench::WorkbenchHost::bh_net_interaction_object_was_loaded(&*f.dead, &f.stub, &f.stub);
    });
}

#[test]
fn blockheadhost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_blockheadhost");
        let _ = crate::blockhead::blockhead::BlockheadHost::update_position(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_position(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_position(&f.dead_bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_position(&f.gone, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_dynamic_object(&f.bg, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_dynamic_object(&f.bg, f.bh(), &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_dynamic_object(&f.dead_bg, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_dynamic_object(&f.gone, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.bg, f.bh(), Some(f.tile), f.p, Some(&f.item), &mut false, true, 1, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.bg, f.bh(), Some(f.tile), f.p, Some(&f.item), &mut false, true, 1, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.dead_bg, f.bh(), Some(f.tile), f.p, Some(&f.item), &mut false, true, 1, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_type_for_tile_at_pos_item_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.gone, f.bh(), Some(f.tile), f.p, Some(&f.item), &mut false, true, 1, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::good_or_bad_interaction_for_action(
            &f.bg,
            f.bh(),
            Some(&f.action),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::good_or_bad_interaction_for_action(
            &f.bg,
            f.bh(),
            Some(&f.action),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::good_or_bad_interaction_for_action(
            &f.dead_bg,
            f.bh(),
            Some(&f.action),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::good_or_bad_interaction_for_action(
            &f.gone,
            f.bh(),
            Some(&f.action),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::update_gather_speed_and_animation_for_current_interation_and_item(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::update_gather_speed_and_animation_for_current_interation_and_item(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::update_gather_speed_and_animation_for_current_interation_and_item(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::update_gather_speed_and_animation_for_current_interation_and_item(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_adding_fuel_to_any_workbench(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_adding_fuel_to_any_workbench(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_adding_fuel_to_any_workbench(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_adding_fuel_to_any_workbench(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_adding_fuel_to_any_workbench(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_traverse_to_key_frame(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_traverse_to_key_frame(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_traverse_to_key_frame(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_traverse_to_key_frame(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::next_pos(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::next_pos(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::next_pos(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::next_pos(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::info_for_path_recalculation(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::info_for_path_recalculation(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::info_for_path_recalculation(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::info_for_path_recalculation(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::set_path_type_goal_interaction_extra_data(
            &f.bg,
            f.bh(),
            Some(vec![]),
            1,
            1,
            Some(crate::blockhead::blockhead::PathExtraData::new()),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_path_type_goal_interaction_extra_data(
            &f.bg,
            f.bh(),
            Some(vec![]),
            1,
            1,
            Some(crate::blockhead::blockhead::PathExtraData::new()),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_path_type_goal_interaction_extra_data(
            &f.dead_bg,
            f.bh(),
            Some(vec![]),
            1,
            1,
            Some(crate::blockhead::blockhead::PathExtraData::new()),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_path_type_goal_interaction_extra_data(
            &f.gone,
            f.bh(),
            Some(vec![]),
            1,
            1,
            Some(crate::blockhead::blockhead::PathExtraData::new()),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_waiting_for_path_to_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_waiting_for_path_to_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_waiting_for_path_to_pos(&f.dead_bg, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_waiting_for_path_to_pos(&f.gone, f.bh(), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_no_longer_waiting_for_path(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::set_no_longer_waiting_for_path(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::set_no_longer_waiting_for_path(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::set_no_longer_waiting_for_path(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_crawl(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_crawl(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_crawl(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_crawl(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_fly(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_fly(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_fly(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_fly(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.dead_bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::tap_is_within_body_radius(&f.gone, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::hit_with_force_blockhead(&f.bg, f.bh(), 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::hit_with_force_blockhead(&f.bg, f.bh(), 1, Some(&f.bh_obj));
        let _ =
            crate::blockhead::blockhead::BlockheadHost::hit_with_force_blockhead(&f.dead_bg, f.bh(), 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::hit_with_force_blockhead(&f.gone, f.bh(), 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::center(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::center(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::center(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::center(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.bg, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::is_interacting_with_any_interaction_object(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(&f.bg, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(&f.bg, f.bh(), Some(&f.bh_obj));
        let _ =
            crate::blockhead::blockhead::BlockheadHost::set_interaction_object(&f.bg, f.bh(), Some(&f.obj("chest")));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(
            &f.bg,
            f.bh(),
            Some(&f.obj("trade_portal")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(
            &f.bg,
            f.bh(),
            Some(&f.obj("workbench")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(&f.dead_bg, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_object(&f.gone, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_workbench(&f.bg, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_workbench(&f.bg, f.bh(), Some(&f.bh_obj));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_workbench(
            &f.bg,
            f.bh(),
            Some(&f.obj("workbench")),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::set_interaction_workbench(&f.dead_bg, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::set_interaction_workbench(&f.gone, f.bh(), Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items(
            &f.bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items(
            &f.bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items(
            &f.dead_bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items(
            &f.gone,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items_data_a_data_b(
            &f.bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
            1,
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items_data_a_data_b(
            &f.bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
            1,
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items_data_a_data_b(
            &f.dead_bg,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
            1,
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::can_pick_up_item_of_type_sub_items_data_a_data_b(
            &f.gone,
            f.bh(),
            crate::types::item_type::ItemType(1048),
            Some(&[vec![f.item.clone()]]),
            1,
            1,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash_disable_warp_check_force_slot_index(
                &f.bg,
                f.bh(),
                &f.item,
                true,
                true,
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash_disable_warp_check_force_slot_index(
                &f.bg,
                f.bh(),
                &f.item,
                true,
                true,
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash_disable_warp_check_force_slot_index(
                &f.dead_bg,
                f.bh(),
                &f.item,
                true,
                true,
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash_disable_warp_check_force_slot_index(
                &f.gone,
                f.bh(),
                &f.item,
                true,
                true,
                1,
            );
        let _ = crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash(&f.bg, f.bh(), &f.item, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash(&f.bg, f.bh(), &f.item, true);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash(&f.dead_bg, f.bh(), &f.item, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::add_item_to_inventory_flash(&f.gone, f.bh(), &f.item, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_freeblock_if_possible_in_tile_intentional(
            &f.bg,
            f.bh(),
            &f.stub,
            Some(f.tile),
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_freeblock_if_possible_in_tile_intentional(
            &f.bg,
            f.bh(),
            &f.bh_obj,
            Some(f.tile),
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_freeblock_if_possible_in_tile_intentional(
            &f.bg,
            f.bh(),
            &f.obj("free_block"),
            Some(f.tile),
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_freeblock_if_possible_in_tile_intentional(
            &f.dead_bg,
            f.bh(),
            &f.stub,
            Some(f.tile),
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::pickup_freeblock_if_possible_in_tile_intentional(
            &f.gone,
            f.bh(),
            &f.stub,
            Some(f.tile),
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_interaction_item(&f.bg, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_interaction_item(&f.bg, f.bh(), true);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::increment_usage_of_interaction_item(&f.dead_bg, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_interaction_item(&f.gone, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_clothing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_clothing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_clothing(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_clothing(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_ice_clothing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_ice_clothing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_ice_clothing(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_usage_of_ice_clothing(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_fuel_usage(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_fuel_usage(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_fuel_usage(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_fuel_usage(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_passive_item_usage(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_passive_item_usage(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_passive_item_usage(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_passive_item_usage(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_damage_of_armor_clothing(&f.bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_damage_of_armor_clothing(&f.bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_damage_of_armor_clothing(&f.dead_bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::increment_damage_of_armor_clothing(&f.gone, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::inventory_was_changed_sub_index_was_usage(
            &f.bg,
            f.bh(),
            1,
            1,
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::inventory_was_changed_sub_index_was_usage(
            &f.bg,
            f.bh(),
            1,
            1,
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::inventory_was_changed_sub_index_was_usage(
            &f.dead_bg,
            f.bh(),
            1,
            1,
            true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::inventory_was_changed_sub_index_was_usage(
            &f.gone,
            f.bh(),
            1,
            1,
            true,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
                &f.bg,
                f.bh(),
                1,
                1,
                1,
                true,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
                &f.bg,
                f.bh(),
                1,
                1,
                1,
                true,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
                &f.bg,
                f.bh(),
                1,
                1,
                1,
                true,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
                &f.dead_bg,
                f.bh(),
                1,
                1,
                1,
                true,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::drop_inventory_items_at_index_sub_index_count_ignore_freeblocks(
                &f.gone,
                f.bh(),
                1,
                1,
                1,
                true,
            );
        let _ = crate::blockhead::blockhead::BlockheadHost::has_interaction_inventory_item_available(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::has_interaction_inventory_item_available(&f.bg, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::has_interaction_inventory_item_available(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::has_interaction_inventory_item_available(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::can_use_dynamic_object(&f.bg, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::can_use_dynamic_object(&f.bg, f.bh(), &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::can_use_dynamic_object(&f.dead_bg, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::can_use_dynamic_object(&f.gone, f.bh(), &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::unable_to_work_reason(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::unable_to_work_reason(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::unable_to_work_reason(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::unable_to_work_reason(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::jetpack_fuel_count(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::jetpack_fuel_count(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::jetpack_fuel_count(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::jetpack_fuel_count(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::update_jet_motion(&f.bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_jet_motion(&f.bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_jet_motion(&f.dead_bg, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::update_jet_motion(&f.gone, f.bh(), 0.5);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_block_pickup_radius(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::free_block_pickup_radius(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::free_block_pickup_radius(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::free_block_pickup_radius(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::free_block_pickup_radius(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::dpad_should_be_displayed(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::dpad_should_be_displayed(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::dpad_should_be_displayed(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::dpad_should_be_displayed(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_item(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_item(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_item(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_item(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_interaction_requires_human_input(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_interaction_requires_human_input(&f.bg, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::current_interaction_requires_human_input(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::current_interaction_requires_human_input(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::action_in_progress(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_in_progress(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_in_progress(&f.dead_bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_in_progress(&f.gone, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_in_progress(&f.bg, &f.action, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_in_progress(&f.bg, &f.action, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_in_progress(&f.dead_bg, &f.action, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_in_progress(&f.gone, &f.action, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_goal_tile_pos(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_goal_tile_pos(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_goal_tile_pos(&f.dead_bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_goal_tile_pos(&f.gone, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_get_save_dict(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_get_save_dict(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_get_save_dict(&f.dead_bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_get_save_dict(&f.gone, &f.action);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_init_with_save_dict_inventory_items(&f.bg, &f.pv, &[]);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_init_with_save_dict_inventory_items(&f.bg, &f.pv, &[]);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_init_with_save_dict_inventory_items(
            &f.dead_bg,
            &f.pv,
            &[],
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_init_with_save_dict_inventory_items(&f.gone, &f.pv, &[]);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_test_result(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_test_result(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_test_result(&f.dead_bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_test_result(&f.gone, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.dead_bg, &f.action);
        let _ = crate::blockhead::blockhead::BlockheadHost::action_interaction_object_id(&f.gone, &f.action);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_inventory_change_int_for_key(&f.bg, &f.action, "cid");
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_inventory_change_int_for_key(&f.bg, &f.action, "cid");
        let _ = crate::blockhead::blockhead::BlockheadHost::action_inventory_change_int_for_key(
            &f.dead_bg, &f.action, "cid",
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::action_inventory_change_int_for_key(&f.gone, &f.action, "cid");
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_inventory_change_int_for_key(
            &f.bg, &f.action, "cid", 1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_inventory_change_int_for_key(
            &f.bg, &f.action, "cid", 1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_inventory_change_int_for_key(
            &f.dead_bg, &f.action, "cid", 1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::action_set_inventory_change_int_for_key(
            &f.gone, &f.action, "cid", 1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::add_simulation_event_of_type_for_blockhead_extra_data(
            &f.bg,
            1,
            &f.stub,
            crate::blockhead::blockhead::SimulationEventExtra::ItemType(1),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::add_simulation_event_of_type_for_blockhead_extra_data(
            &f.bg,
            1,
            &f.bh_obj,
            crate::blockhead::blockhead::SimulationEventExtra::ItemType(1),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::add_simulation_event_of_type_for_blockhead_extra_data(
            &f.dead_bg,
            1,
            &f.stub,
            crate::blockhead::blockhead::SimulationEventExtra::ItemType(1),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::add_simulation_event_of_type_for_blockhead_extra_data(
            &f.gone,
            1,
            &f.stub,
            crate::blockhead::blockhead::SimulationEventExtra::ItemType(1),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::report_achievement_with_identifier(&f.bg, "cid");
        let _ = crate::blockhead::blockhead::BlockheadHost::report_achievement_with_identifier(&f.bg, "cid");
        let _ = crate::blockhead::blockhead::BlockheadHost::report_achievement_with_identifier(&f.dead_bg, "cid");
        let _ = crate::blockhead::blockhead::BlockheadHost::report_achievement_with_identifier(&f.gone, "cid");
        let _ =
            crate::blockhead::blockhead::BlockheadHost::send_damage_for_net_blockhead_recoil(&f.bg, 0.5, &f.stub, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::send_damage_for_net_blockhead_recoil(
            &f.bg, 0.5, &f.bh_obj, true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_damage_for_net_blockhead_recoil(
            &f.dead_bg, 0.5, &f.stub, true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_damage_for_net_blockhead_recoil(
            &f.gone, 0.5, &f.stub, true,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.stub),
            1,
            Some(&f.stub),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.bh_obj),
            1,
            Some(&f.bh_obj),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("gate")),
            1,
            Some(&f.obj("gate")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("painting")),
            1,
            Some(&f.obj("painting")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("column")),
            1,
            Some(&f.obj("column")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("stairs")),
            1,
            Some(&f.obj("stairs")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("ladder")),
            1,
            Some(&f.obj("ladder")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("wire")),
            1,
            Some(&f.obj("wire")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("elevator_shaft")),
            1,
            Some(&f.obj("elevator_shaft")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("elevator_motor")),
            1,
            Some(&f.obj("elevator_motor")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("rail")),
            1,
            Some(&f.obj("rail")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("window")),
            1,
            Some(&f.obj("window")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("free_block")),
            1,
            Some(&f.obj("free_block")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.bg,
            f.v,
            f.v,
            Some(&f.obj("workbench")),
            1,
            Some(&f.obj("workbench")),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.dead_bg,
            f.v,
            f.v,
            Some(&f.stub),
            1,
            Some(&f.stub),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fire_projectile_from_to_at_fire_item_type_firer(
            &f.gone,
            f.v,
            f.v,
            Some(&f.stub),
            1,
            Some(&f.stub),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::tile_is_lit_for_client_at_pos_tile(
            &f.bg,
            Some("cid"),
            f.p,
            Some(f.tile),
            None,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::tile_is_lit_for_client_at_pos_tile(
            &f.bg,
            Some("cid"),
            f.p,
            Some(f.tile),
            None,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::tile_is_lit_for_client_at_pos_tile(
            &f.dead_bg,
            Some("cid"),
            f.p,
            Some(f.tile),
            None,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::tile_is_lit_for_client_at_pos_tile(
            &f.gone,
            Some("cid"),
            f.p,
            Some(f.tile),
            None,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(&f.bg, f.p, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(&f.bg, f.p, 1, Some(&f.bh_obj));
        let _ = crate::blockhead::blockhead::BlockheadHost::fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(&f.dead_bg, f.p, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::fully_load_if_needed_around_pos_client_light_block_index_for_blockhead(&f.gone, f.p, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, 1, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, 1, f.p, Some(&f.pv), Some("cid"), &f.bh_obj, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, 1, f.p, Some(&f.pv), Some("cid"), &f.obj("workbench"), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.dead_bg, 1, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_workbench_of_type_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.gone, 1, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.bh_obj, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.obj("chest"), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.obj("trade_portal"), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.obj("workbench"), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.dead_bg, &f.item, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::place_interaction_object_with_item_at_pos_save_dict_placed_by_client_placed_by_blockhead_placed_by_client_name(&f.gone, &f.item, f.p, Some(&f.pv), Some("cid"), &f.stub, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type(&f.bg, Some(f.tile), f.p, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type(&f.bg, Some(f.tile), f.p, 1);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type(&f.dead_bg, Some(f.tile), f.p, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type(&f.gone, Some(f.tile), f.p, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(&f.bg, Some(f.tile), f.p, 1, 1, 1, Some("cid"), Some(&f.pv), Some(&f.stub), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(&f.bg, Some(f.tile), f.p, 1, 1, 1, Some("cid"), Some(&f.pv), Some(&f.bh_obj), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(&f.dead_bg, Some(f.tile), f.p, 1, 1, 1, Some("cid"), Some(&f.pv), Some(&f.stub), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::fill_tile_at_pos_with_type_data_a_data_b_placed_by_client_save_dict_placed_by_blockhead_placed_by_client_name(&f.gone, Some(f.tile), f.p, 1, 1, 1, Some("cid"), Some(&f.pv), Some(&f.stub), Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.bh_obj,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.obj("column"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.obj("ladder"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.obj("stairs"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.obj("elevator_shaft"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.obj("chest"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.dead_bg,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::paint_tile_at_pos_color_index_face_index_paint_blockhead(
            &f.gone,
            Some(f.tile),
            f.p,
            1,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_gather_notification_for_tile_at_pos(
            &f.bg,
            Some(f.tile),
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_gather_notification_for_tile_at_pos(
            &f.bg,
            Some(f.tile),
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_gather_notification_for_tile_at_pos(
            &f.dead_bg,
            Some(f.tile),
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::send_gather_notification_for_tile_at_pos(
            &f.gone,
            Some(f.tile),
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.bg, &f.stub, Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.bg, &f.bh_obj, Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.bg, &f.obj("chest"), Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.bg, &f.obj("trade_portal"), Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.bg, &f.obj("workbench"), Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.dead_bg, &f.stub, Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_reached_interaction_object_destination_path_extra_data(&f.gone, &f.stub, Some(&crate::blockhead::blockhead::PathExtraData::new()));
        let _ = crate::blockhead::blockhead::BlockheadHost::player_is_admin_with_id(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::player_is_admin_with_id(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::player_is_admin_with_id(&f.dead_bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::player_is_admin_with_id(&f.gone, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::update_player(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::update_player(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::update_player(&f.dead_bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::update_player(&f.gone, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::player_name_for_player_with_id_including_old_players(
            &f.bg,
            Some("cid"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::player_name_for_player_with_id_including_old_players(
            &f.bg,
            Some("cid"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::player_name_for_player_with_id_including_old_players(
            &f.dead_bg,
            Some("cid"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::player_name_for_player_with_id_including_old_players(
            &f.gone,
            Some("cid"),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::server_client_light_block_index_for_net_id(&f.bg, Some("cid"));
        let _ =
            crate::blockhead::blockhead::BlockheadHost::server_client_light_block_index_for_net_id(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::server_client_light_block_index_for_net_id(
            &f.dead_bg,
            Some("cid"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::server_client_light_block_index_for_net_id(
            &f.gone,
            Some("cid"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::dynamic_world_local_net_id(&f.bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::dynamic_world_local_net_id(&f.bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::dynamic_world_local_net_id(&f.dead_bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::dynamic_world_local_net_id(&f.gone);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.dead_bg, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_with_id(&f.gone, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_object_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_object_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_object_with_id(&f.dead_bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_object_with_id(&f.gone, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.dead_bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_with_id(&f.gone, f.bh_obj.unique_id());
        let _ = crate::blockhead::blockhead::BlockheadHost::get_plant_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::get_plant_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::get_plant_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::get_plant_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::get_plant_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_blocks_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_blocks_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_blocks_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_blocks_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::free_blocks_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::door_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::door_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::door_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::door_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::door_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::window_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::window_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::window_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::window_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::painting_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::column_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::column_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::column_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::column_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::ladder_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::ladder_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::ladder_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::ladder_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::wire_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::wire_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::wire_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::wire_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::stairs_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::stairs_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::stairs_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::stairs_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_shaft_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_shaft_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_shaft_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_shaft_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_shaft_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::load_gather_block_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::load_gather_block_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::load_gather_block_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::load_gather_block_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pos_of_doors_other_block_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pos_of_doors_other_block_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pos_of_doors_other_block_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pos_of_doors_other_block_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::pos_of_doors_other_block_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_door_at_pos_to_open_direction(&f.bg, f.p, true, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_door_at_pos_to_open_direction(&f.bg, f.p, true, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_door_at_pos_to_open_direction(&f.bg, f.p, true, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_door_at_pos_to_open_direction(&f.dead_bg, f.p, true, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_door_at_pos_to_open_direction(&f.gone, f.p, true, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::open_elevator_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::open_elevator_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::open_elevator_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::open_elevator_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.bg, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.bg, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.bg, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.bg, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.dead_bg, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::check_for_harmable_dynamic_object_under_tap_ignore_local_blockheads_ignore_all_blockheads(&f.gone, f.v, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&f.bg, f.p, crate::types::item_type::ItemType(1048), 1, 1, Some(&[vec![f.item.clone()]]), Some(&f.pv), true, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&f.bg, f.p, crate::types::item_type::ItemType(1048), 1, 1, Some(&[vec![f.item.clone()]]), Some(&f.pv), true, 1, Some(&f.bh_obj));
        let _ = crate::blockhead::blockhead::BlockheadHost::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&f.bg, f.p, crate::types::item_type::ItemType(1048), 1, 1, Some(&[vec![f.item.clone()]]), Some(&f.pv), true, 1, Some(&f.obj("free_block")));
        let _ = crate::blockhead::blockhead::BlockheadHost::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&f.dead_bg, f.p, crate::types::item_type::ItemType(1048), 1, 1, Some(&[vec![f.item.clone()]]), Some(&f.pv), true, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(&f.gone, f.p, crate::types::item_type::ItemType(1048), 1, 1, Some(&[vec![f.item.clone()]]), Some(&f.pv), true, 1, Some(&f.stub));
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.bg, &f.obj("clownfish"));
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_npc(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_donkey(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_donkey(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_donkey(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::is_donkey(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_donkey(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_bed(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_bed(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_bed(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::is_bed(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("clownfish"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("gate"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("painting"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("column"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("stairs"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("ladder"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("wire"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("elevator_shaft"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("elevator_motor"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("rail"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("window"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("free_block"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_food_item_type(&f.gone, &f.stub);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(&f.bg, &f.stub, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(
            &f.bg,
            &f.obj("dodo"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(
            &f.dead_bg,
            &f.stub,
            f.bh(),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_belongs_to_player_with_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.bg, &f.obj("clownfish"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_fed_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(&f.bg, &f.bh_obj, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_begin_blockhead_inspection(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(&f.bg, &f.bh_obj, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_remote_blockhead_hooked_you(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.bg, &f.obj("egg"));
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::donkey_breed(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.bg, &f.obj("egg"));
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::egg_breed(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_data_a_b(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_data_a_b(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_data_a_b(&f.bg, &f.obj("torch"));
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_data_a_b(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::torch_data_a_b(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("gate"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("painting"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("column"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("stairs"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("ladder"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("wire"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("elevator_shaft"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("elevator_motor"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("rail"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("window"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("free_block"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::object_item_type(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::sign_text_length(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::sign_text_length(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::sign_text_length(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::sign_text_length(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.bh_obj,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.obj("boat"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.obj("hand_car"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.obj("steam_train"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.bg,
            &f.obj("dodo"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.dead_bg,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ridable_blockhead_can_ride_using_item(
            &f.gone,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.obj("boat"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.obj("hand_car"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.obj("steam_train"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_animation_type(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.bg, &f.bh_obj);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.bg, &f.obj("boat"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(
            &f.bg,
            &f.obj("hand_car"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(
            &f.bg,
            &f.obj("steam_train"),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::ride_object_rider_rides_with_arms_down(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_type(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_type(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_type(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_level(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_level(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_level(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_level(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_level(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_has_required_fuel(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_has_required_fuel(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_has_required_fuel(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_has_required_fuel(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_has_required_fuel(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_uses_stores_conducts_or_produces_electricity(
            &f.bg, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_uses_stores_conducts_or_produces_electricity(
            &f.bg, &f.bh_obj,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_uses_stores_conducts_or_produces_electricity(
            &f.bg,
            &f.obj("workbench"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_uses_stores_conducts_or_produces_electricity(
            &f.dead_bg, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_uses_stores_conducts_or_produces_electricity(
            &f.gone, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_is_in_use(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_is_in_use(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_is_in_use(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_is_in_use(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_is_in_use(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_current_blockhead(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_current_blockhead(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_current_blockhead(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_current_blockhead(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_current_blockhead(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_craft_item_with_blockhead_count(
            &f.bg,
            &f.stub,
            &f.action,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_craft_item_with_blockhead_count(
            &f.bg,
            &f.bh_obj,
            &f.action,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_craft_item_with_blockhead_count(
            &f.bg,
            &f.obj("workbench"),
            &f.action,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_craft_item_with_blockhead_count(
            &f.dead_bg,
            &f.stub,
            &f.action,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_craft_item_with_blockhead_count(
            &f.gone,
            &f.stub,
            &f.action,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_start_managing_fuel_with_blockhead(
            &f.bg,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_start_managing_fuel_with_blockhead(
            &f.bg,
            &f.bh_obj,
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_start_managing_fuel_with_blockhead(
            &f.bg,
            &f.obj("workbench"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_start_managing_fuel_with_blockhead(
            &f.dead_bg,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_start_managing_fuel_with_blockhead(
            &f.gone,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.bg, &f.stub, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.bg, &f.bh_obj, 1);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.bg, &f.obj("workbench"), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(
            &f.bg,
            &f.obj("trade_portal"),
            1,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.bg, &f.obj("steam_train"), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.dead_bg, &f.stub, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::set_workbench_choice_ui_option(&f.gone, &f.stub, 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.bg, &f.obj("chest"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.bg, &f.obj("trade_portal"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_type(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.bg, &f.obj("chest"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.bg, &f.obj("trade_portal"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_is_in_use(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_gather_progress_for_tile(&f.bg, &f.stub, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_gather_progress_for_tile(&f.bg, &f.bh_obj, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_gather_progress_for_tile(&f.bg, &f.obj("plant"), f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_gather_progress_for_tile(&f.dead_bg, &f.stub, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_gather_progress_for_tile(&f.gone, &f.stub, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_set_gather_progress_for_tile(&f.bg, &f.stub, 1, f.p);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_set_gather_progress_for_tile(&f.bg, &f.bh_obj, 1, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::plant_set_gather_progress_for_tile(
            &f.bg,
            &f.obj("plant"),
            1,
            f.p,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_set_gather_progress_for_tile(&f.dead_bg, &f.stub, 1, f.p);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_set_gather_progress_for_tile(&f.gone, &f.stub, 1, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_valid(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_valid(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_valid(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_valid(&f.dead_bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_valid(&f.gone, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_hooked_fish_unique_id(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_hooked_fish_unique_id(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_hooked_fish_unique_id(&f.bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_hooked_fish_unique_id(&f.dead_bg, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_hooked_fish_unique_id(&f.gone, &f.rod);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_new_with_swipe(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_new_with_swipe(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_new_with_swipe(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_new_with_swipe(&f.dead_bg, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_new_with_swipe(&f.gone, f.bh(), f.v);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_update(&f.bg, &f.rod, 0.5, 0.5, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_update(&f.bg, &f.rod, 0.5, 0.5, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_update(&f.bg, &f.rod, 0.5, 0.5, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_update(&f.dead_bg, &f.rod, 0.5, 0.5, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::fishing_rod_update(&f.gone, &f.rod, 0.5, 0.5, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_riding(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_riding(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_riding(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_riding(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::will_die_if_hit_by_force(&f.bg, f.bh(), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::will_die_if_hit_by_force(&f.bg, f.bh(), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::will_die_if_hit_by_force(&f.dead_bg, f.bh(), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::will_die_if_hit_by_force(&f.gone, f.bh(), 1);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_item(&f.bg, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_item(&f.bg, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_item(&f.dead_bg, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_item(&f.gone, f.bh(), true);
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_all_actions(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_all_actions(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_all_actions(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_all_actions(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_fishing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_fishing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_fishing(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_fishing(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::stop_fishing(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::die_for_good(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::die_for_good(&f.bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::die_for_good(&f.dead_bg, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::die_for_good(&f.gone, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::abort_in_progress_path_if_for_blockhead(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::abort_in_progress_path_if_for_blockhead(&f.bg, &f.bh_obj);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::abort_in_progress_path_if_for_blockhead(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::abort_in_progress_path_if_for_blockhead(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&f.bg, 1, 1, 1, 1, &f.stub, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&f.bg, 1, 1, 1, 1, &f.bh_obj, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&f.bg, 1, 1, 1, 1, &f.obj("free_block"), true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&f.dead_bg, 1, 1, 1, 1, &f.stub, true, true);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead_only_remove_c_ontents_only_remove_foreground_contents(&f.gone, 1, 1, 1, 1, &f.stub, true, true);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_back_wall_at_pos_remove_blockhead(&f.bg, f.p, &f.stub);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_back_wall_at_pos_remove_blockhead(&f.bg, f.p, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_back_wall_at_pos_remove_blockhead(
            &f.dead_bg, f.p, &f.stub,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_back_wall_at_pos_remove_blockhead(&f.gone, f.p, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_water_tile_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_water_tile_at_pos(&f.bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_water_tile_at_pos(&f.dead_bg, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_water_tile_at_pos(&f.gone, f.p);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.bh_obj,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.obj("column"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.obj("ladder"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.obj("stairs"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.obj("elevator_shaft"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.bg,
            Some(f.tile),
            f.p,
            1,
            &f.obj("chest"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.dead_bg,
            Some(f.tile),
            f.p,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_paint_at_tile_at_pos_face_index_paint_blockhead(
            &f.gone,
            Some(f.tile),
            f.p,
            1,
            &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::display_ownership_areas(&f.bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::display_ownership_areas(&f.bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::display_ownership_areas(&f.bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::display_ownership_areas(&f.dead_bg);
        let _ = crate::blockhead::blockhead::BlockheadHost::display_ownership_areas(&f.gone);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_curse_words_from_blockhead_name(&f.bg, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_curse_words_from_blockhead_name(&f.bg, Some("cid"));
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_curse_words_from_blockhead_name(&f.dead_bg, Some("cid"));
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_curse_words_from_blockhead_name(&f.gone, Some("cid"));
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_object_at_pos(
            &f.bg,
            crate::blockhead::blockhead::Remover::Torch,
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_object_at_pos(
            &f.bg,
            crate::blockhead::blockhead::Remover::Torch,
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_object_at_pos(
            &f.dead_bg,
            crate::blockhead::blockhead::Remover::Torch,
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_object_at_pos(
            &f.gone,
            crate::blockhead::blockhead::Remover::Torch,
            f.p,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.bg, f.p, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.bg, f.p, &f.bh_obj,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.bg,
            f.p,
            &f.obj("chest"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.bg,
            f.p,
            &f.obj("trade_portal"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.bg,
            f.p,
            &f.obj("workbench"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.dead_bg, f.p, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_interaction_object_at_pos_remove_blockhead(
            &f.gone, f.p, &f.stub,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_workbench_at_pos_remove_blockhead(&f.bg, f.p, &f.stub);
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_workbench_at_pos_remove_blockhead(&f.bg, f.p, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_workbench_at_pos_remove_blockhead(
            &f.bg,
            f.p,
            &f.obj("workbench"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::remove_workbench_at_pos_remove_blockhead(
            &f.dead_bg, f.p, &f.stub,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::remove_workbench_at_pos_remove_blockhead(&f.gone, f.p, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.bg,
            &f.stub,
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.bg,
            &f.bh_obj,
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.bg,
            &f.obj("dodo"),
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.dead_bg,
            &f.stub,
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_captured_by_blockhead(
            &f.gone,
            &f.stub,
            f.bh(),
            crate::types::item_type::ItemType(1048),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_milked_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_can_be_shaved_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.bg, &f.obj("clownfish"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_feed_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ =
            crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.bg, &f.obj("clownfish"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_set_free_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.bh_obj,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("dodo"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("gate"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("painting"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("column"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("stairs"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("ladder"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("wire"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("elevator_shaft"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("elevator_motor"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("rail"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("window"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("free_block"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.bg,
            &f.obj("workbench"),
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.dead_bg,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_capture_by_blockhead_with_item_type(
            &f.gone,
            &f.stub,
            f.bh(),
            1,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.bg, &f.obj("clownfish"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_shave_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.bg, &f.obj("dodo"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.bg, &f.obj("clownfish"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_milk_by_blockhead(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.bg, &f.obj("dodo"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.bg, &f.obj("clownfish"));
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::npc_inspection_stopped(&f.gone, &f.stub);
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_abort_immediately_and_restore_blockhead_items(
            &f.bg, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_abort_immediately_and_restore_blockhead_items(
            &f.bg, &f.bh_obj,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_abort_immediately_and_restore_blockhead_items(
            &f.bg,
            &f.obj("workbench"),
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_abort_immediately_and_restore_blockhead_items(
            &f.dead_bg, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::workbench_abort_immediately_and_restore_blockhead_items(
            &f.gone, &f.stub,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.bg, &f.bh_obj, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.bg, &f.obj("chest"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(
            &f.bg,
            &f.obj("trade_portal"),
            f.bh(),
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.bg, &f.obj("workbench"), f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.dead_bg, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::interaction_object_remove(&f.gone, &f.stub, f.bh());
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.bg,
            &f.stub,
            f.bh(),
            &f.pv,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.bg,
            &f.bh_obj,
            f.bh(),
            &f.pv,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.bg,
            &f.obj("chest"),
            f.bh(),
            &f.pv,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.dead_bg,
            &f.stub,
            f.bh(),
            &f.pv,
        );
        let _ = crate::blockhead::blockhead::BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.gone,
            &f.stub,
            f.bh(),
            &f.pv,
        );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
                &f.bg,
                &f.stub,
                f.p,
                f.bh(),
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
                &f.bg,
                &f.bh_obj,
                f.p,
                f.bh(),
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
                &f.bg,
                &f.obj("plant"),
                f.p,
                f.bh(),
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
                &f.dead_bg,
                &f.stub,
                f.p,
                f.bh(),
                1,
            );
        let _ =
            crate::blockhead::blockhead::BlockheadHost::plant_tile_harvested_remove_blockhead_correct_tool_multiplier(
                &f.gone,
                &f.stub,
                f.p,
                f.bh(),
                1,
            );
    });
}

#[test]
fn actionshost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_actionshost");
        let _ = crate::blockhead::interaction_rules::ActionsHost::free_blocks_exist_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::free_blocks_exist_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::free_blocks_exist_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::free_blocks_exist_at_pos(&f.dead_bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::free_blocks_exist_at_pos(&f.gone, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::blockhead_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::blockhead_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::blockhead_at_pos(&f.dead_bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::blockhead_at_pos(&f.gone, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.bg, f.bh(), f.v);
        let _ =
            crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.dead_bg, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::check_for_train_car_under_tap(&f.gone, f.bh(), f.v);
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.dead_bg,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_object_type_for_object_at_pos(
            &f.gone,
            f.bh(),
            f.p,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::load_glow_block_if_needed_at_pos_tile(
            &f.bg,
            f.bh(),
            f.p,
            &f.tile,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::load_glow_block_if_needed_at_pos_tile(
            &f.bg,
            f.bh(),
            f.p,
            &f.tile,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::load_glow_block_if_needed_at_pos_tile(
            &f.bg,
            f.bh(),
            f.p,
            &f.tile,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::load_glow_block_if_needed_at_pos_tile(
            &f.dead_bg,
            f.bh(),
            f.p,
            &f.tile,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::load_glow_block_if_needed_at_pos_tile(
            &f.gone,
            f.bh(),
            f.p,
            &f.tile,
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::add_torch_at_pos_of_type(&f.bg, f.bh(), f.p, 1);
        let _ = crate::blockhead::interaction_rules::ActionsHost::add_torch_at_pos_of_type(&f.bg, f.bh(), f.p, 1);
        let _ = crate::blockhead::interaction_rules::ActionsHost::add_torch_at_pos_of_type(&f.bg, f.bh(), f.p, 1);
        let _ = crate::blockhead::interaction_rules::ActionsHost::add_torch_at_pos_of_type(&f.dead_bg, f.bh(), f.p, 1);
        let _ = crate::blockhead::interaction_rules::ActionsHost::add_torch_at_pos_of_type(&f.gone, f.bh(), f.p, 1);
        let _ = crate::blockhead::interaction_rules::ActionsHost::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&f.bg, f.bh(), &f.tile, f.p, true, true);
        let _ = crate::blockhead::interaction_rules::ActionsHost::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&f.bg, f.bh(), &f.tile, f.p, true, true);
        let _ = crate::blockhead::interaction_rules::ActionsHost::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&f.bg, f.bh(), &f.tile, f.p, true, true);
        let _ = crate::blockhead::interaction_rules::ActionsHost::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&f.dead_bg, f.bh(), &f.tile, f.p, true, true);
        let _ = crate::blockhead::interaction_rules::ActionsHost::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&f.gone, f.bh(), &f.tile, f.p, true, true);
        let _ = crate::blockhead::interaction_rules::ActionsHost::world_contents_changed_at_pos(&f.bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::world_contents_changed_at_pos(&f.bg, f.bh(), f.p);
        let _ =
            crate::blockhead::interaction_rules::ActionsHost::world_contents_changed_at_pos(&f.dead_bg, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::world_contents_changed_at_pos(&f.gone, f.bh(), f.p);
        let _ = crate::blockhead::interaction_rules::ActionsHost::all_blockheads_including_net(&f.bg, f.bh());
        let _ = crate::blockhead::interaction_rules::ActionsHost::all_blockheads_including_net(&f.bg, f.bh());
        let _ = crate::blockhead::interaction_rules::ActionsHost::all_blockheads_including_net(&f.dead_bg, f.bh());
        let _ = crate::blockhead::interaction_rules::ActionsHost::all_blockheads_including_net(&f.gone, f.bh());
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.stub);
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("gate"));
        let _ =
            crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("painting"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("column"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("stairs"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("ladder"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("wire"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(
            &f.bg,
            &f.obj("elevator_shaft"),
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(
            &f.bg,
            &f.obj("elevator_motor"),
        );
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("rail"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("window"));
        let _ =
            crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("free_block"));
        let _ =
            crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.bg, &f.obj("workbench"));
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::interaction_rules::ActionsHost::interaction_render_item_type(&f.gone, &f.stub);
        let _ = crate::blockhead::interaction_rules::ActionsHost::dynamic_world_is_client(&f.bg);
        let _ = crate::blockhead::interaction_rules::ActionsHost::dynamic_world_is_client(&f.bg);
        let _ = crate::blockhead::interaction_rules::ActionsHost::dynamic_world_is_client(&f.dead_bg);
        let _ = crate::blockhead::interaction_rules::ActionsHost::dynamic_world_is_client(&f.gone);
        let _ = crate::blockhead::interaction_rules::ActionsHost::remove_standard_object(&f.bg, f.bh(), &f.stub);
        let _ = crate::blockhead::interaction_rules::ActionsHost::remove_standard_object(&f.bg, f.bh(), &f.bh_obj);
        let _ = crate::blockhead::interaction_rules::ActionsHost::remove_standard_object(&f.dead_bg, f.bh(), &f.stub);
        let _ = crate::blockhead::interaction_rules::ActionsHost::remove_standard_object(&f.gone, f.bh(), &f.stub);
    });
}

#[test]
fn inventoryhost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_inventoryhost");
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_data(&f.bg, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_data(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_data(&f.bg, &f.obj("free_block"));
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_data(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_data(&f.gone, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_priority_blockhead_cannot_pickup(
            &f.bg, &f.stub,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_priority_blockhead_cannot_pickup(
            &f.bg, &f.bh_obj,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_priority_blockhead_cannot_pickup(
            &f.bg,
            &f.obj("free_block"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_priority_blockhead_cannot_pickup(
            &f.dead_bg, &f.stub,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::free_block_priority_blockhead_cannot_pickup(
            &f.gone, &f.stub,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_is_client(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_is_client(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_is_client(&f.dead_bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_is_client(&f.gone);
        let _ = crate::blockhead::inventory_model::InventoryHost::add_simulation_event_got(&f.bg, &f.stub, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::add_simulation_event_got(&f.bg, &f.bh_obj, &f.bh_obj);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::add_simulation_event_got(&f.dead_bg, &f.stub, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::add_simulation_event_got(&f.gone, &f.stub, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::world_start_using_camera(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::world_start_using_camera(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::world_start_using_camera(&f.dead_bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::world_start_using_camera(&f.gone);
        let _ = crate::blockhead::inventory_model::InventoryHost::harmable_dynamic_object_with_id(
            &f.bg,
            f.bh_obj.unique_id(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::harmable_dynamic_object_with_id(
            &f.bg,
            f.bh_obj.unique_id(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::harmable_dynamic_object_with_id(
            &f.dead_bg,
            f.bh_obj.unique_id(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::harmable_dynamic_object_with_id(
            &f.gone,
            f.bh_obj.unique_id(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::boat_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::boat_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::boat_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::boat_with_id(&f.dead_bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::boat_with_id(&f.gone, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.dead_bg, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::train_car_with_id(&f.gone, f.bh_obj.unique_id());
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_active_blockhead(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_active_blockhead(&f.bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_active_blockhead(&f.dead_bg);
        let _ = crate::blockhead::inventory_model::InventoryHost::dynamic_world_active_blockhead(&f.gone);
        let _ = crate::blockhead::inventory_model::InventoryHost::current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.bg, f.bh(), Some(f.tile), f.p, &mut false, true, 1, true);
        let _ = crate::blockhead::inventory_model::InventoryHost::current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.bg, f.bh(), Some(f.tile), f.p, &mut false, true, 1, true);
        let _ = crate::blockhead::inventory_model::InventoryHost::current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.dead_bg, f.bh(), Some(f.tile), f.p, &mut false, true, 1, true);
        let _ = crate::blockhead::inventory_model::InventoryHost::current_interaction_type_for_tile_at_pos_pickup_rejected_due_to_inventory_full_include_actions_face_index_allow_protected_actions(&f.gone, f.bh(), Some(f.tile), f.p, &mut false, true, 1, true);
        let _ = crate::blockhead::inventory_model::InventoryHost::can_fish(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::can_fish(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::can_fish(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::can_fish(&f.dead_bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::can_fish(&f.gone, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::fishing_rod_new(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::fishing_rod_new(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::fishing_rod_new(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::fishing_rod_new(&f.dead_bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::fishing_rod_new(&f.gone, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(&f.bg, &f.stub);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(
            &f.bg,
            &f.obj("boat"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(
            &f.bg,
            &f.obj("hand_car"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(
            &f.bg,
            &f.obj("steam_train"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(
            &f.bg,
            &f.obj("dodo"),
        );
        let _ =
            crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(&f.dead_bg, &f.stub);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::ride_object_set_target_velocity_zero(&f.gone, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::has_blockhead_ai(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::has_blockhead_ai(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::has_blockhead_ai(&f.dead_bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::has_blockhead_ai(&f.gone, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::blockhead_ai_no_path_to_action(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::blockhead_ai_no_path_to_action(&f.bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::blockhead_ai_no_path_to_action(&f.dead_bg, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::blockhead_ai_no_path_to_action(&f.gone, f.bh());
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.bg,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.bg,
            &f.bh_obj,
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.bg,
            &f.obj("dodo"),
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.bg,
            &f.obj("clownfish"),
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.dead_bg,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::npc_blockhead_is_coming_to_inspect(
            &f.gone,
            &f.stub,
            f.bh(),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_goal_interaction(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_goal_interaction(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_goal_interaction(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_goal_interaction(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_index(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_index(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_index(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_index(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_sub_index(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_sub_index(&f.bg, &f.action);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::action_interaction_item_sub_index(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_interaction_item_sub_index(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item(
            &f.bg,
            &f.action,
            Some(&f.item),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item(
            &f.bg,
            &f.action,
            Some(&f.item),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item(
            &f.dead_bg,
            &f.action,
            Some(&f.item),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item(
            &f.gone,
            &f.action,
            Some(&f.item),
        );
        let _ =
            crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_index(&f.bg, &f.action, 1);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_index(&f.bg, &f.action, 1);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_index(
            &f.dead_bg, &f.action, 1,
        );
        let _ =
            crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_index(&f.gone, &f.action, 1);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_sub_index(
            &f.bg, &f.action, 1,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_sub_index(
            &f.bg, &f.action, 1,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_sub_index(
            &f.dead_bg, &f.action, 1,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_set_interaction_item_sub_index(
            &f.gone, &f.action, 1,
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craft_count_or_extra_data(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craft_count_or_extra_data(&f.bg, &f.action);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::action_craft_count_or_extra_data(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craft_count_or_extra_data(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_path_type(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_path_type(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_path_type(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_path_type(&f.gone, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craftable_item_object(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craftable_item_object(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craftable_item_object(&f.bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craftable_item_object(&f.dead_bg, &f.action);
        let _ = crate::blockhead::inventory_model::InventoryHost::action_craftable_item_object(&f.gone, &f.action);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(&f.bg, &f.stub);
        let _ =
            crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("workbench"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("gate"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("painting"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("column"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("stairs"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("ladder"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("wire"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("elevator_shaft"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("elevator_motor"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("rail"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("window"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.bg,
            &f.obj("free_block"),
        );
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(
            &f.dead_bg, &f.stub,
        );
        let _ =
            crate::blockhead::inventory_model::InventoryHost::workbench_currently_crafting_item_type(&f.gone, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_total_items_left_to_craft(&f.bg, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_total_items_left_to_craft(&f.bg, &f.bh_obj);
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_total_items_left_to_craft(
            &f.bg,
            &f.obj("workbench"),
        );
        let _ =
            crate::blockhead::inventory_model::InventoryHost::workbench_total_items_left_to_craft(&f.dead_bg, &f.stub);
        let _ = crate::blockhead::inventory_model::InventoryHost::workbench_total_items_left_to_craft(&f.gone, &f.stub);
    });
}

#[test]
fn fishinghost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_fishinghost");
        let _ = crate::physics_objects::fishing_rod::FishingHost::blockhead_fishing_rod_cast(&f.bg, &f.stub, f.v);
        let _ = crate::physics_objects::fishing_rod::FishingHost::blockhead_fishing_rod_cast(&f.bg, &f.bh_obj, f.v);
        let _ = crate::physics_objects::fishing_rod::FishingHost::blockhead_fishing_rod_cast(
            &f.bg,
            &f.obj("clownfish"),
            f.v,
        );
        let _ = crate::physics_objects::fishing_rod::FishingHost::blockhead_fishing_rod_cast(&f.dead_bg, &f.stub, f.v);
        let _ = crate::physics_objects::fishing_rod::FishingHost::blockhead_fishing_rod_cast(&f.gone, &f.stub, f.v);
        let _ = crate::physics_objects::fishing_rod::FishingHost::clown_fish_die(&f.bg, &f.stub, Some(&f.stub));
        let _ = crate::physics_objects::fishing_rod::FishingHost::clown_fish_die(&f.bg, &f.bh_obj, Some(&f.bh_obj));
        let _ = crate::physics_objects::fishing_rod::FishingHost::clown_fish_die(
            &f.bg,
            &f.obj("clownfish"),
            Some(&f.obj("clownfish")),
        );
        let _ = crate::physics_objects::fishing_rod::FishingHost::clown_fish_die(&f.dead_bg, &f.stub, Some(&f.stub));
        let _ = crate::physics_objects::fishing_rod::FishingHost::clown_fish_die(&f.gone, &f.stub, Some(&f.stub));
    });
}

#[test]
fn firehost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_firehost");
        let _ = crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(&f.bg, &f.stub);
        let _ =
            crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(&f.bg, &f.bh_obj);
        let _ = crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(
            &f.bg,
            &f.obj("plant"),
        );
        let _ = crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(
            &f.bg,
            &f.obj("free_block"),
        );
        let _ = crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(
            &f.dead_bg, &f.stub,
        );
        let _ =
            crate::physics_objects::fire_object::FireHost::remove_plant_without_creating_freeblocks(&f.gone, &f.stub);
    });
}

#[test]
fn hostilehost_adapters_forward_and_are_inert_without_targets() {
    isolated(|| {
        let f = fx("hk_hostilehost");
        let _ = crate::npc::cave_troll::HostileHost::world(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::world(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::world(&f.dead_bg);
        let _ = crate::npc::cave_troll::HostileHost::world(&f.gone);
        let _ = crate::npc::cave_troll::HostileHost::custom_rules(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::custom_rules(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::custom_rules(&f.dead_bg);
        let _ = crate::npc::cave_troll::HostileHost::custom_rules(&f.gone);
        let _ = crate::npc::cave_troll::HostileHost::expert_mode(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::expert_mode(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::expert_mode(&f.dead_bg);
        let _ = crate::npc::cave_troll::HostileHost::expert_mode(&f.gone);
        let _ = crate::npc::cave_troll::HostileHost::tutorial_active(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::tutorial_active(&f.bg);
        let _ = crate::npc::cave_troll::HostileHost::tutorial_active(&f.dead_bg);
        let _ = crate::npc::cave_troll::HostileHost::tutorial_active(&f.gone);
        let _ = crate::npc::cave_troll::HostileHost::day_night_fraction(&f.bg, 0.5, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::day_night_fraction(&f.bg, 0.5, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::day_night_fraction(&f.dead_bg, 0.5, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::day_night_fraction(&f.gone, 0.5, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::fire_projectile(&f.bg, f.v, f.v, Some(&f.stub), 1, Some(&f.stub));
        let _ =
            crate::npc::cave_troll::HostileHost::fire_projectile(&f.bg, f.v, f.v, Some(&f.bh_obj), 1, Some(&f.bh_obj));
        let _ =
            crate::npc::cave_troll::HostileHost::fire_projectile(&f.dead_bg, f.v, f.v, Some(&f.stub), 1, Some(&f.stub));
        let _ =
            crate::npc::cave_troll::HostileHost::fire_projectile(&f.gone, f.v, f.v, Some(&f.stub), 1, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::add_damage_simulation_event(&f.bg, &f.stub, "cid");
        let _ = crate::npc::cave_troll::HostileHost::add_damage_simulation_event(&f.bg, &f.bh_obj, "cid");
        let _ = crate::npc::cave_troll::HostileHost::add_damage_simulation_event(&f.dead_bg, &f.stub, "cid");
        let _ = crate::npc::cave_troll::HostileHost::add_damage_simulation_event(&f.gone, &f.stub, "cid");
        let _ = crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.bg, f.p, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.bg, f.p, Some(&f.bh_obj));
        let _ = crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.bg, f.p, Some(&f.obj("dodo")));
        let _ =
            crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.bg, f.p, Some(&f.obj("clownfish")));
        let _ = crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.dead_bg, f.p, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::npc_exists_at_pos_ignore_npc(&f.gone, f.p, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::blockhead_occupies_tile_at_pos(&f.bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::blockhead_occupies_tile_at_pos(&f.bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::blockhead_occupies_tile_at_pos(&f.dead_bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::blockhead_occupies_tile_at_pos(&f.gone, f.p);
        let _ = crate::npc::cave_troll::HostileHost::create_free_block(&f.bg, f.p, 1, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::create_free_block(&f.bg, f.p, 1, Some(&f.bh_obj));
        let _ = crate::npc::cave_troll::HostileHost::create_free_block(&f.bg, f.p, 1, Some(&f.obj("free_block")));
        let _ = crate::npc::cave_troll::HostileHost::create_free_block(&f.dead_bg, f.p, 1, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::create_free_block(&f.gone, f.p, 1, Some(&f.stub));
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.bg, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.bg, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.bg, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.bg, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.dead_bg, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::load_baby_npc_at_position(&f.gone, f.p, 1);
        let _ = crate::npc::cave_troll::HostileHost::cactus_available_food_at(&f.bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::cactus_available_food_at(&f.bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::cactus_available_food_at(&f.bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::cactus_available_food_at(&f.dead_bg, f.p);
        let _ = crate::npc::cave_troll::HostileHost::cactus_available_food_at(&f.gone, f.p);
        let _ = crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(&f.bg, f.p, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(&f.bg, f.p, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(&f.bg, f.p, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(&f.dead_bg, f.p, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::set_cactus_available_food_at(&f.gone, f.p, 0.5);
        let _ = crate::npc::cave_troll::HostileHost::bh_paused(&f.bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_paused(&f.bg, &f.bh_obj);
        let _ = crate::npc::cave_troll::HostileHost::bh_paused(&f.dead_bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_paused(&f.gone, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_regenerating(&f.bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_regenerating(&f.bg, &f.bh_obj);
        let _ = crate::npc::cave_troll::HostileHost::bh_regenerating(&f.dead_bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_regenerating(&f.gone, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_on_trade_mission(&f.bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_on_trade_mission(&f.bg, &f.bh_obj);
        let _ = crate::npc::cave_troll::HostileHost::bh_on_trade_mission(&f.dead_bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_on_trade_mission(&f.gone, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_should_continue_simulating(&f.bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_should_continue_simulating(&f.bg, &f.bh_obj);
        let _ = crate::npc::cave_troll::HostileHost::bh_should_continue_simulating(&f.dead_bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_should_continue_simulating(&f.gone, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_suffer_damage(&f.bg, &f.stub, 0.5, true, true);
        let _ = crate::npc::cave_troll::HostileHost::bh_suffer_damage(&f.bg, &f.bh_obj, 0.5, true, true);
        let _ = crate::npc::cave_troll::HostileHost::bh_suffer_damage(&f.dead_bg, &f.stub, 0.5, true, true);
        let _ = crate::npc::cave_troll::HostileHost::bh_suffer_damage(&f.gone, &f.stub, 0.5, true, true);
        let _ = crate::npc::cave_troll::HostileHost::bh_center(&f.bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_center(&f.bg, &f.bh_obj);
        let _ = crate::npc::cave_troll::HostileHost::bh_center(&f.dead_bg, &f.stub);
        let _ = crate::npc::cave_troll::HostileHost::bh_center(&f.gone, &f.stub);
    });
}

// ------------------------------------------------------------------------------------------------ targeted cases

use std::any::Any;
use std::rc::Rc;

use crate::blockhead::blockhead::{BlockheadHost, ExtraValue, PathExtraData, Remover, SimulationEventExtra};
use crate::crafting::craftable_item_object::{
    zero_craftable_item, zero_skin_options, BlockheadCraftableItemObject, CraftableItemObject,
};
use crate::plist::value::PlistValue;
use crate::types::structs::IntPair;

/// Craftable item objects of each class reach the workbench (as `Rc<dyn Any>` from an action / path extra data).
#[test]
fn craftable_item_objects_of_each_class_reach_the_workbench() {
    isolated(|| {
        let f = fx("hk_cio");
        let wb = f.obj("workbench").clone();
        let any: Vec<Rc<dyn Any>> = vec![
            BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(
                zero_craftable_item(),
                Some("Bob".into()),
                zero_skin_options(),
            ),
            CraftableItemObject::init_with_craftable_item(zero_craftable_item()),
            crate::interaction::painting::PaintingCraftableItemObject::new_with_craftable_item_image_data_output_image_data(
                zero_craftable_item(),
                None,
                None,
            ),
            Rc::new(5u8),
        ];
        for a in &any {
            let _ = BlockheadHost::workbench_craft_item_with_blockhead_count(&f.bg, &wb, a, f.bh(), 1);
        }
        // a blockhead craftable item object through the World seam (crafted blockhead / portal)
        let shim = crate::crafting::workbench_craft::craftable_item_shim(
            BlockheadCraftableItemObject::init_with_craftable_item_name_skin_options(
                zero_craftable_item(),
                Some("Bob".into()),
                zero_skin_options(),
            ),
        );
        let before = f.dw.blockheads().len();
        crate::world::misc::WorldOpsDeps::load_new_blockhead_at_pos_craftable_item_object_unique_id(
            &*f.g,
            IntPair { x: 6, y: 9 },
            &shim,
            u64::MAX,
        );
        assert_eq!(f.dw.blockheads().len(), before + 1, "crafted blockhead not loaded");
        let ci = crate::world::misc::WorldOpsDeps::craftable_item(&*f.g, &shim);
        assert_eq!(ci.crafted_item_type, 0);
        assert!(crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.g, &shim).is_none());
        let paint = crate::crafting::workbench_craft::craftable_item_shim(
            crate::interaction::painting::PaintingCraftableItemObject::new_with_craftable_item_image_data_output_image_data(
                zero_craftable_item(),
                Some(vec![1]),
                Some(vec![2]),
            ),
        );
        assert!(crate::world::misc::WorldOpsDeps::free_block_creation_item_save_dict(&*f.g, &paint).is_some());
    });
}

/// Path extra data: the numbers go through the path request, the objects come back on `setPath:`.
#[test]
fn path_extra_data_objects_survive_the_path_request() {
    isolated(|| {
        let f = fx("hk_path");
        let b = f.bh();
        assert!(BlockheadHost::info_for_path_recalculation(&f.bg, b).is_none());
        // a fall: no extra data
        b.bh.borrow_mut().path_needs_recalculated = true;
        let d = BlockheadHost::info_for_path_recalculation(&f.bg, b).expect("fall info");
        assert!(d.map.get(&"extraData".to_string()).is_none());
        // chasing an NPC: extra data
        crate::blockhead::action::action_of(&f.action).unwrap().set_in_progress(true);
        b.bh.borrow_mut().action_queue.push(f.action.clone());
        b.bh.borrow_mut().chasing_npc = true;
        let d = BlockheadHost::info_for_path_recalculation(&f.bg, b).expect("chase info");
        assert!(matches!(d.map.get(&"extraData".to_string()), Some(PlistValue::Dict(_))));
        let ty = d.map.get(&"pathType".to_string()).unwrap().int_value() as i32;
        // the request comes back without the objects; the pending objects are merged in
        let mut sent = PathExtraData::new();
        sent.set("count", ExtraValue::Int(2));
        sent.set("workbench", ExtraValue::Obj(f.obj("workbench").clone()));
        let key = (b.me().unique_id(), ty);
        super::blockhead_host::PENDING_PATH_OBJECTS.with(|m| m.borrow_mut().insert(key, sent));
        BlockheadHost::set_path_type_goal_interaction_extra_data(&f.bg, b, None, ty, 0, Some(PathExtraData::new()));
        assert!(b.bh.borrow().path_extra_data.as_ref().unwrap().object_for_key("workbench").is_some());
        // nothing pending: unchanged
        BlockheadHost::set_path_type_goal_interaction_extra_data(&f.bg, b, None, ty, 0, None);
        assert!(b.bh.borrow().path_extra_data.is_none());
    });
}

#[test]
fn path_extra_data_plist_keeps_numbers_only() {
    let mut a = PathExtraData::new();
    a.set("count", ExtraValue::Int(3));
    a.set("cio", ExtraValue::Any(Rc::new(1u8)));
    match a.to_plist() {
        PlistValue::Dict(d) => {
            assert_eq!(d.map.get(&"count".to_string()).unwrap().int_value(), 3);
            assert!(d.map.get(&"cio".to_string()).is_none());
        }
        _ => panic!("dict"),
    }
    let mut b = PathExtraData::new();
    b.set("cio", ExtraValue::Int(9));
    let merged = b.with_objects_from(&a);
    assert_eq!(merged.int_for_key("cio"), 9, "an existing key is kept");
    assert_eq!(PathExtraData::new().with_objects_from(&a).any_for_key("cio").is_some(), true);
}

/// Every removable kind through `removeXAtPos:`, the simulation-event extras, an elevator motor's power.
#[test]
fn blockhead_host_removers_events_and_elevator_power() {
    isolated(|| {
        let f = fx("hk_remove");
        for (ex, ty) in [
            (SimulationEventExtra::None, 0),
            (SimulationEventExtra::Text("hit".into()), 4),
            (SimulationEventExtra::ItemType(5), 3),
        ] {
            BlockheadHost::add_simulation_event_of_type_for_blockhead_extra_data(&f.bg, ty, &f.bh_obj, ex);
        }
        BlockheadHost::blockhead_would_like_to_take_ownership_with_save_dict(
            &f.bg,
            f.obj("chest"),
            f.bh(),
            &PlistValue::int(1),
        );
        let shaft = f.obj("elevator_shaft").pos();
        BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, shaft);
        let motor = f.obj("elevator_motor").clone();
        BlockheadHost::elevator_motor_for_shaft_at_pos_use_power(&f.bg, motor.pos());
        for (which, name) in [
            (Remover::Torch, "torch"),
            (Remover::Egg, "egg"),
            (Remover::Painting, "painting"),
            (Remover::Ladder, "ladder"),
            (Remover::Column, "column"),
            (Remover::Wire, "wire"),
            (Remover::Stairs, "stairs"),
            (Remover::ElevatorShaft, "elevator_shaft"),
            (Remover::ElevatorMotor, "elevator_motor"),
            (Remover::Rail, "rail"),
            (Remover::Window, "window"),
            (Remover::Door, "gate"),
        ] {
            let _ = BlockheadHost::remove_object_at_pos(&f.bg, which, f.obj(name).pos());
        }
        assert!(BlockheadHost::remove_object_at_pos(&f.dead_bg, Remover::Torch, f.p).is_none());
    });
}

/// The remaining World / DynamicWorld adapters with their live targets.
#[test]
fn glue_adapters_with_live_targets() {
    use crate::dynworld::lookups::DwOpsHooks;
    use crate::dynworld::state::DwHooks;
    use crate::world::lifecycle::WorldCoreHooks;
    isolated(|| {
        let f = fx("hk_live");
        // projectiles once the manager exists
        WorldCoreHooks::projectile_manager_init(&*f.g, &f.world);
        WorldCoreHooks::projectile_manager_fire(&*f.g, f.v, f.v, None, 1, Some(&f.bh_obj));
        // rain on a snow surface block
        let snow = crate::env::snow_surface_block::SnowSurfaceBlock::init_with_world_dynamic_world_at_position_cache(
            &f.world,
            &f.dwr(),
            IntPair { x: 3, y: 12 },
            None,
        )
        .unwrap();
        let snow: crate::objects::handle::ObjRef = snow;
        DwHooks::update_rain_dt(&*f.g, &snow, 1.0, 0.5);
        // a feeder chest (chest type 6) gives up an item
        let feeder =
            DwOpsHooks::new_chest(&*f.g, &f.dw, IntPair { x: 2, y: 13 }, 0x450, false, None, Some("cid"), None)
                .unwrap();
        assert_eq!(crate::npc::npc::NpcEnv::feeder_chest_remove_item_if_available(&*f.g, &feeder, 1048), Some(false));
        // found items once the client record exists
        WorldCoreHooks::server_client_create(&*f.g, "cid", 0, None);
        let mut set = crate::foundation::indexset::IndexSet::new();
        set.add_indexes_in_range(5, 3);
        DwOpsHooks::found_items_list_add_indexes(&*f.g, "cid", &set);
        assert!(DwOpsHooks::found_items_list_snapshot(&*f.g, "cid").unwrap().contains_index(6));
        DwOpsHooks::found_items_list_remove_all_indexes(&*f.g, "cid");
        assert!(DwOpsHooks::found_items_list_snapshot(&*f.g, "cid").unwrap().is_empty());
        // the offline path pump keeps pumping while a path is in progress
        WorldCoreHooks::path_creator_init(&*f.g, &f.world);
        crate::world::misc::install_deps(f.g.clone());
        let pc = crate::world::misc::WorldOpsDeps::path_creator(&*f.g).unwrap();
        let mut req = crate::path::path_creator::PathRequest::from_plist_dict_with_user(
            &crate::objects::base::new_mutable_dict(),
            Some(f.bh_obj.clone()),
        );
        crate::blockhead::interaction_rules::testhost::light_all(&f.world);
        req.goal_x = 30;
        req.goal_y = 31;
        pc.create_path_with_dict(&req);
        WorldCoreHooks::world_path_request_pump(&*f.g, &f.world);
        WorldCoreHooks::world_path_request_pump(&*f.g, &f.world);
    });
}

/// Beds answer `itemType`; a glue without a server keeps names; the GameController delegate gets `setPaused:`.
#[test]
fn beds_orphan_glue_and_delegate() {
    use crate::dynworld::state::DwHooks;
    isolated(|| {
        let f = fx("hk_bed");
        let bed = f.obj("bed");
        assert_eq!(DwHooks::item_type(&*f.g, bed), 63);
        assert_eq!(BlockheadHost::object_item_type(&f.bg, bed), 63);
        let orphan = {
            let h = crate::server::bhserver::test_support::Harness::new("hk_orphan");
            super::Glue::new(&h.srv)
        };
        assert_eq!(crate::npc::npc::NpcEnv::remove_curse_words_from_blockhead_name(&*orphan, "Bob"), "Bob");
        let d: Rc<dyn crate::world::lifecycle::WorldDelegate> =
            Rc::new(crate::world::lifecycle::testkit::TestDelegate {
                default_save: crate::objects::base::new_mutable_dict(),
            });
        crate::world::state::World::set_core_delegate_for_tests(Some(Rc::downgrade(&d)));
        crate::world::misc::WorldOpsDeps::delegate_set_paused(&*f.g, true);
        crate::world::state::World::set_core_delegate_for_tests(None);
    });
}

/// `teleportBlockhead:toWorkbench:` (what a portal / mod teleport uses) moves a blockhead through the production
/// hook (it was inert: the teleport always failed).
#[test]
fn teleport_moves_the_blockhead_through_the_production_hook() {
    isolated(|| {
        let f = fx("hk_teleport");
        crate::blockhead::interaction_rules::testhost::light_all(&f.world);
        let wb = f.obj("workbench").clone();
        assert!(f.dw.teleport_blockhead_to_workbench(&f.bh_obj, Some(&wb)), "teleport refused");
        assert_eq!(f.bh_obj.pos(), wb.pos());
        assert!(!crate::dynworld::lookups::DwOpsHooks::blockhead_teleport_to_pos(&*f.g, &f.stub, wb.pos()));
    });
}
