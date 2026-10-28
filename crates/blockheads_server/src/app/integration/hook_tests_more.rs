//! The remaining production hook adapters of the glue, called with live and with missing targets.

#![allow(dropping_copy_types, clippy::unit_arg)]
use super::hook_fixture::fx;
use super::*;

/// Runs one adapter call; an exception raised by the callee (dummy arguments) is noted, not fatal.
fn t(name: &'static str, f: impl FnOnce()) {
    if crate::foundation::exception::catch_ns(f).is_err() {
        eprintln!("raised: {name}");
    }
}

fn isolated(f: impl FnOnce() + Send + 'static) {
    std::thread::spawn(f).join().expect("test body panicked");
}

#[test]
fn remaining_forwarders_reach_live_and_missing_targets() {
    isolated(|| {
        let f = fx("hk_more");
        let srv = f.g.srv().unwrap();
        f.g.clients.borrow_mut().insert(
            "cid".to_string(),
            crate::server::server_client::ServerClient::init_with_client_id_server_light_block_index_light_block_database(
                "cid", &srv, 31, None,
            ),
        );
        f.g.install_post_init(&f.world);
        for g in [&f.g, &f.dead] {
            t("server_match_privacy", || drop(WorldCoreHooks::server_match_privacy(&**g)));
            t("server_database", || drop(WorldCoreHooks::server_database(&**g)));
            t("server_client_paused", || drop(WorldCoreHooks::server_client_paused(&**g, "cid")));
            t("server_client_set_paused", || drop(WorldCoreHooks::server_client_set_paused(&**g, "cid", true)));
            t("loader_set_needs_to_exit", || drop(WorldCoreHooks::loader_set_needs_to_exit(&**g, true)));
            t("loader_update_physical_block_to_latest_version", || {
                drop(WorldCoreHooks::loader_update_physical_block_to_latest_version(&**g, f.pb))
            });
            t("path_creator_init", || drop(WorldCoreHooks::path_creator_init(&**g, &f.world)));
            t("path_creator_in_progress", || drop(WorldCoreHooks::path_creator_in_progress(&**g)));
            t("path_creator_path_user", || drop(WorldCoreHooks::path_creator_path_user(&**g)));
            t("path_creator_abort_path", || drop(WorldCoreHooks::path_creator_abort_path(&**g)));
            t("dynamic_world_portal_level", || drop(WorldCoreHooks::dynamic_world_portal_level(&**g)));
            t("dynamic_world_blockheads", || drop(WorldCoreHooks::dynamic_world_blockheads(&**g)));
            t("dynamic_world_remote_remove", || {
                drop(WorldCoreHooks::dynamic_world_remote_remove(&**g, vec![vec![0u8; 8]], 1, Some("cid")))
            });
            t("dynamic_world_user_mute_changed", || drop(WorldCoreHooks::dynamic_world_user_mute_changed(&**g, "cid")));
            t("dynamic_world_players_changed", || drop(WorldCoreHooks::dynamic_world_players_changed(&**g)));
            t("dynamic_world_append_debug_log", || {
                drop(WorldCoreHooks::dynamic_world_append_debug_log(&**g, &mut String::new()))
            });
            t("dynamic_world_finish_simulating", || drop(WorldCoreHooks::dynamic_world_finish_simulating(&**g)));
            t("dynamic_world_simulate", || drop(WorldCoreHooks::dynamic_world_simulate(&**g, 1.0)));
            t("dynamic_world_set_paused", || drop(WorldCoreHooks::dynamic_world_set_paused(&**g, true)));
            t("dynamic_world_blockhead_previews", || drop(WorldCoreHooks::dynamic_world_blockhead_previews(&**g)));
            t("client_receipts", || drop(WorldgenHost::client_receipts(&**g, &f.world, "cid", f.p)));
            t("initial_dynamic_objects_net_data", || {
                drop(WorldgenHost::initial_dynamic_objects_net_data(&**g, &f.world, 1, "cid"))
            });
            t("create_treasure_chest_or_troll_at_tile", || {
                drop(WorldgenHost::create_treasure_chest_or_troll_at_tile(&**g, &f.world, f.tile, f.p, true, true))
            });
            t("workbench_at_pos", || drop(WorldOpsDeps::workbench_at_pos(&**g, f.p)));
            t("active_blockhead", || drop(WorldOpsDeps::active_blockhead(&**g)));
            t("update_accurate_dt", || drop(WorldOpsDeps::update_accurate_dt(&**g, 1.0, 1.0)));
            t("dynamic_world_set_paused", || drop(WorldOpsDeps::dynamic_world_set_paused(&**g, true)));
            t("water_changed_at_pos_full_block", || {
                drop(WorldOpsDeps::water_changed_at_pos_full_block(&**g, f.p, true))
            });
            t("load_surface_block_at_pos", || drop(WorldOpsDeps::load_surface_block_at_pos(&**g, f.p)));
            t("load_snow_surface_block_at_pos_load_snow", || {
                drop(WorldOpsDeps::load_snow_surface_block_at_pos_load_snow(&**g, f.p, true))
            });
            t("load_glow_block_if_needed_at_pos_tile", || {
                drop(WorldOpsDeps::load_glow_block_if_needed_at_pos_tile(&**g, f.p, &f.tile))
            });
            t("load_gather_block_at_pos", || drop(WorldOpsDeps::load_gather_block_at_pos(&**g, f.p)));
            t("play_time_crystal_received_sound_at_pos", || {
                drop(WorldOpsDeps::play_time_crystal_received_sound_at_pos(&**g, f.p))
            });
            t("load_dynamic_objects_for_macro_tile_include_surface_blocks", || {
                drop(WorldOpsDeps::load_dynamic_objects_for_macro_tile_include_surface_blocks(
                    &**g,
                    &f.world.macro_tiles().get(0).unwrap(),
                    true,
                ))
            });
            t("remove_dynamic_objects_for_macro_tile", || {
                drop(WorldOpsDeps::remove_dynamic_objects_for_macro_tile(&**g, &f.world.macro_tiles().get(0).unwrap()))
            });
            t("load_physical_block_at_x_pos_y_pos_create_if_not_created", || {
                drop(WorldOpsDeps::load_physical_block_at_x_pos_y_pos_create_if_not_created(&**g, f.pb, 1, 1, true))
            });
            t("save_physical_block_macro_tile_send_to_clients_server_send_reliably", || {
                drop(WorldOpsDeps::save_physical_block_macro_tile_send_to_clients_server_send_reliably(
                    &**g,
                    f.pb,
                    &f.world.macro_tiles().get(0).unwrap(),
                    &[],
                    true,
                ))
            });
            t(
                "send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable",
                || {
                    drop(WorldOpsDeps::send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(&**g, f.pb, f.p, "cid", true, true))
                },
            );
            t("unmodified_ground_level_for_x", || drop(WorldOpsDeps::unmodified_ground_level_for_x(&**g, 1)));
            t("sc_requested_block_indices_first_n", || {
                drop(WorldOpsDeps::sc_requested_block_indices_first_n(&**g, "cid", 1))
            });
            t("sc_light_block_index", || drop(WorldOpsDeps::sc_light_block_index(&**g, "cid")));
            t("sc_paused", || drop(WorldOpsDeps::sc_paused(&**g, "cid")));
            t("player_is_admin_with_id", || drop(WorldOpsDeps::player_is_admin_with_id(&**g, Some("cid"))));
            t("update_player", || drop(WorldOpsDeps::update_player(&**g, "cid")));
            t("create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead", || {
                drop(WorldOpsDeps::create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
                    &**g,
                    f.p,
                    true,
                    &f.tile,
                    Some(&f.stub),
                ))
            });
            t("create_background_content_free_block_at_position_for_tile_remove_blockhead", || {
                drop(WorldOpsDeps::create_background_content_free_block_at_position_for_tile_remove_blockhead(
                    &**g,
                    f.p,
                    &f.tile,
                    Some(&f.stub),
                ))
            });
            t("interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name", || {
                drop(WorldOpsDeps::interaction_object_placed_at_position_with_item_flipped_save_dict_placed_by_client_client_name(&**g, f.p, &f.item.borrow(), true, Some(&f.pv), Some("cid"), Some("cid")))
            });
            t("sow_tree_or_plant_at_position_item_type_max_height_growth_rate", || {
                drop(WorldOpsDeps::sow_tree_or_plant_at_position_item_type_max_height_growth_rate(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    1,
                    1,
                ))
            });
            t("add_painting_at_pos_of_type_save_dict_placed_by_client_client_name", || {
                drop(WorldOpsDeps::add_painting_at_pos_of_type_save_dict_placed_by_client_client_name(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                    Some("cid"),
                ))
            });
            t("add_window_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_window_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_ladder_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_ladder_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_wire_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_wire_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_elevator_shaft_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_elevator_motor_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_elevator_motor_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_column_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_column_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("add_stairs_at_pos_of_type_save_dict_placed_by_client", || {
                drop(WorldOpsDeps::add_stairs_at_pos_of_type_save_dict_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::item_type::ItemType(1048),
                    Some(&f.pv),
                    Some("cid"),
                ))
            });
            t("place_fire_at_position", || drop(WorldOpsDeps::place_fire_at_position(&**g, f.p)));
            t("load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client", || {
                drop(WorldOpsDeps::load_npc_at_position_type_save_dict_is_adult_was_placed_placed_by_client(
                    &**g,
                    f.p,
                    crate::types::enums::NPCType(1),
                    Some(&f.pv),
                    true,
                    true,
                    Some("cid"),
                ))
            });
            t("workbench_pos", || drop(WorldOpsDeps::workbench_pos(&**g, &f.stub)));
            t("workbench_type", || drop(WorldOpsDeps::workbench_type(&**g, &f.stub)));
            t("workbench_level", || drop(WorldOpsDeps::workbench_level(&**g, &f.stub)));
            t("workbench_upgrade_to_next_level", || drop(WorldOpsDeps::workbench_upgrade_to_next_level(&**g, &f.stub)));
            t("bh_on_trade_mission", || drop(WorldOpsDeps::bh_on_trade_mission(&**g, &f.stub)));
            t("bh_is_net", || drop(WorldOpsDeps::bh_is_net(&**g, &f.stub)));
            t("bh_add_to_crystal_discrepency", || drop(WorldOpsDeps::bh_add_to_crystal_discrepency(&**g, &f.stub, 1)));
            t("bh_add_expected_craft_item", || {
                drop(WorldOpsDeps::bh_add_expected_craft_item(&**g, &f.stub, crate::types::item_type::ItemType(1048)))
            });
            t("bh_local_net_id", || drop(WorldOpsDeps::bh_local_net_id(&**g, &f.stub)));
            t("bh_stop_interacting", || drop(WorldOpsDeps::bh_stop_interacting(&**g, &f.stub)));
            t("bh_can_use_dynamic_object", || drop(WorldOpsDeps::bh_can_use_dynamic_object(&**g, &f.stub, &f.stub)));
            t("fully_load_if_needed_around_pos", || {
                drop(DwHooks::fully_load_if_needed_around_pos(&**g, f.p, 1, &f.stub))
            });
            t("update_physical_block_to_latest_version", || {
                drop(DwHooks::update_physical_block_to_latest_version(&**g, f.pb))
            });
            t("place_basic_portal_workbench", || drop(DwHooks::place_basic_portal_workbench(&**g, f.p)));
            t("construct_from_save_dict", || {
                drop(DwHooks::construct_from_save_dict(&**g, SaveLoadKind::Workbench, 1, &f.dict, Some(&f.dict)))
            });
            t("construct_from_net_data", || drop(DwHooks::construct_from_net_data(&**g, 1, Some(1), &[0u8; 16])));
            t("workbench_level", || drop(DwHooks::workbench_level(&**g, &f.stub)));
            t("workbench_set_level_silently", || drop(DwHooks::workbench_set_level_silently(&**g, &f.stub, 1)));
            t("chest_type", || drop(DwHooks::chest_type(&**g, &f.stub)));
            t("destroy_item_type", || drop(DwHooks::destroy_item_type(&**g, &f.stub)));
            t("dynamic_world_changed_at_pos_object_type", || {
                drop(DwHooks::dynamic_world_changed_at_pos_object_type(&**g, f.p, 1))
            });
            t("load_npc_at_position", || drop(DwHooks::load_npc_at_position(&**g, f.p, 1, true, true)));
            t("add_torch_at_pos_of_type", || drop(DwHooks::add_torch_at_pos_of_type(&**g, f.p, 1)));
            t("create_treasure_chest_or_troll_at_tile", || {
                drop(DwHooks::create_treasure_chest_or_troll_at_tile(&**g, f.tile, f.p, true, true))
            });
            t("load_glow_block_if_needed_at_pos_tile", || {
                drop(DwHooks::load_glow_block_if_needed_at_pos_tile(&**g, f.p, f.tile))
            });
            t("load_surface_block_at_pos", || drop(DwHooks::load_surface_block_at_pos(&**g, f.p)));
            t("load_snow_surface_block_at_pos_load_snow", || {
                drop(DwHooks::load_snow_surface_block_at_pos_load_snow(&**g, f.p, true))
            });
            t("workbench_at_pos", || drop(DwHooks::workbench_at_pos(&**g, f.p)));
            t("interaction_object_at_pos", || drop(DwHooks::interaction_object_at_pos(&**g, f.p)));
            t("object_at_pos_for_item", || drop(DwHooks::object_at_pos_for_item(&**g, 's', f.p)));
            t("remove_door_at_pos", || drop(DwHooks::remove_door_at_pos(&**g, f.p)));
            t("blockhead_from_save_dict", || {
                drop(DwHooks::blockhead_from_save_dict(
                    &**g,
                    &f.dict,
                    Some(f.pv.clone()),
                    true,
                    Some("cid"),
                    Some(f.pv.clone()),
                ))
            });
            t("freeblock_from_net_data", || drop(DwHooks::freeblock_from_net_data(&**g, &[0u8; 16], None)));
            t("blockhead_set_client_name", || drop(DwHooks::blockhead_set_client_name(&**g, &f.stub, Some("cid"))));
            t("blockhead_client_save_dir", || drop(DwHooks::blockhead_client_save_dir(&**g, &f.stub)));
            t("blockhead_save_dict_including_workbench_or_interaction_object", || {
                drop(DwHooks::blockhead_save_dict_including_workbench_or_interaction_object(&**g, &f.stub, true))
            });
            t("blockhead_save_item_slots_array", || drop(DwHooks::blockhead_save_item_slots_array(&**g, &f.stub)));
            t("blockhead_prepare_inventory_for_saving", || {
                drop(DwHooks::blockhead_prepare_inventory_for_saving(&**g, &f.stub))
            });
            t("blockhead_inventory_needs_saving", || drop(DwHooks::blockhead_inventory_needs_saving(&**g, &f.stub)));
            t("blockhead_set_inventory_needs_saving", || {
                drop(DwHooks::blockhead_set_inventory_needs_saving(&**g, &f.stub, true))
            });
            t("blockhead_inventory_was_changed", || {
                drop(DwHooks::blockhead_inventory_was_changed(&**g, &f.stub, 1, 1, true))
            });
            t("blockhead_stop_interacting_if_no_interaction_object", || {
                drop(DwHooks::blockhead_stop_interacting_if_no_interaction_object(&**g, &f.stub))
            });
            t("blockhead_stop_riding", || drop(DwHooks::blockhead_stop_riding(&**g, &f.stub)));
            t("blockhead_die", || drop(DwHooks::blockhead_die(&**g, &f.stub)));
            t("interaction_object_at_pos", || drop(crate::npc::npc::NpcEnv::interaction_object_at_pos(&**g, f.p)));
            t("set_door_at_pos_to_open_direction", || {
                drop(crate::npc::npc::NpcEnv::set_door_at_pos_to_open_direction(&**g, f.p, true, 1))
            });
            t("portal_is_being_removed_at_pos", || {
                drop(crate::crafting::workbench::WorkbenchHost::portal_is_being_removed_at_pos(&**g, f.p))
            });
            t("find_and_subtract_all_power_up_to_for_user", || {
                drop(crate::crafting::workbench::WorkbenchHost::find_and_subtract_all_power_up_to_for_user(
                    &**g, 1, &f.stub,
                ))
            });
            t("has_lights_to_add", || drop(crate::path::path_creator::PathDynamicWorldHooks::has_lights_to_add(&**g)));
            t("blockhead_occupies_tile_at_pos_ignore_blockhead", || {
                drop(crate::path::path_creator::PathDynamicWorldHooks::blockhead_occupies_tile_at_pos_ignore_blockhead(
                    &**g,
                    f.p,
                    Some(&f.stub),
                ))
            });
            t("workbench_at_pos", || {
                drop(crate::path::path_creator::PathDynamicWorldHooks::workbench_at_pos(&**g, f.p))
            });
            t("save_sunlight_changed_at_pos", || {
                drop(crate::lighting::artificial_light::LightingServices::save_sunlight_changed_at_pos(
                    &**g, &f.world, f.p,
                ))
            });
            t("fill_tile_at_pos_with_type", || {
                drop(crate::lighting::artificial_light::LightingServices::fill_tile_at_pos_with_type(
                    &**g, &f.world, f.tile, f.p, 1,
                ))
            });
            t("peers_interested_in_macro_index_count", || {
                drop(crate::lighting::artificial_light::LightingServices::peers_interested_in_macro_index_count(
                    &**g, &f.world, 1,
                ))
            });
            t("remove_water_tile_at_pos", || {
                drop(crate::lighting::artificial_light::LightingServices::remove_water_tile_at_pos(&**g, &f.world, f.p))
            });
            t("send_net_data_for_electricity_particle_path_if_required_size_ignore_client", || {
                drop(crate::lighting::artificial_light::LightingServices::send_net_data_for_electricity_particle_path_if_required_size_ignore_client(&**g, &f.world, Vec::new(), 1.0, Some("cid")))
            });
            t("snow_changed_at_macro_pos", || {
                drop(crate::lighting::artificial_light::LightingServices::snow_changed_at_macro_pos(&**g, f.p))
            });
            t("water_changed_at_pos_full_block", || {
                drop(crate::lighting::artificial_light::LightingServices::water_changed_at_pos_full_block(
                    &**g, f.p, true,
                ))
            });
            t("blockheads", || drop(crate::lighting::artificial_light::LightingServices::blockheads(&**g)));
            t("add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client", || {
                drop(crate::lighting::artificial_light::LightingServices::add_torch_at_pos_of_type_data_a_data_b_save_dict_placed_by_client(&**g, f.p, 1, 1, 1, Some(&f.dict), Some("cid")))
            });
            t("create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure", || {
                drop(crate::lighting::artificial_light::LightingServices::create_treasure_chest_or_troll_at_tile_at_pos_load_troll_load_treasure(&**g, f.tile, f.p, true, true))
            });
            t("load_glow_block_if_needed_at_pos_tile", || {
                drop(crate::lighting::artificial_light::LightingServices::load_glow_block_if_needed_at_pos_tile(
                    &**g, f.p, f.tile,
                ))
            });
            t("load_surface_block_at_pos", || {
                drop(crate::lighting::artificial_light::LightingServices::load_surface_block_at_pos(&**g, f.p))
            });
            t("remove_standard_object", || {
                drop(crate::lighting::artificial_light::LightingServices::remove_standard_object(&**g, &f.stub))
            });
            t("blockhead_view_radius", || {
                drop(crate::lighting::artificial_light::LightingServices::blockhead_view_radius(&**g, &f.stub))
            });
            t("water_moved_from_from_tile_to_to_tile_amount", || {
                drop(crate::lighting::artificial_light::LightingServices::water_moved_from_from_tile_to_to_tile_amount(
                    &**g, &f.world, f.p, f.tile, f.p, f.tile, 1,
                ))
            });
            t("create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound", || {
                drop(crate::lighting::artificial_light::LightingServices::create_free_block_at_position_of_type_data_a_data_b_dynamic_object_save_dict_hovers_play_sound(&**g, f.p, 1, 1, 1, f.dict.clone(), true, true))
            });
            t("client_receipts_for_test", || drop(g.client_receipts_for_test("cid")));
            t("plant_available_food", || drop(crate::npc::egg::PassiveEnv::plant_available_food(&**g, &f.stub)));
            t("plant_set_available_food", || {
                drop(crate::npc::egg::PassiveEnv::plant_set_available_food(&**g, &f.stub, 1.0))
            });
            t("npc_can_mate", || drop(DwOpsHooks::npc_can_mate(&**g, &f.stub)));
            t("npc_can_be_fed_by_blockhead", || drop(DwOpsHooks::npc_can_be_fed_by_blockhead(&**g, &f.stub, &f.stub)));
            t("freeblock_hovers", || drop(DwOpsHooks::freeblock_hovers(&**g, &f.stub)));
            t("tree_height", || drop(DwOpsHooks::tree_height(&**g, &f.stub)));
            t("tree_max_height_gene_variation", || drop(DwOpsHooks::tree_max_height_gene_variation(&**g, &f.stub)));
            t("tree_growth_rate_gene_variation", || drop(DwOpsHooks::tree_growth_rate_gene_variation(&**g, &f.stub)));
            t("interaction_object_width_radius", || drop(DwOpsHooks::interaction_object_width_radius(&**g, &f.stub)));
            t("interaction_object_height_radius", || drop(DwOpsHooks::interaction_object_height_radius(&**g, &f.stub)));
        }
    });
}

#[test]
fn the_query_face_the_server_face_and_the_loader_adapter() {
    isolated(|| {
        let f = fx("hk_faces");
        f.g.install_post_init(&f.world);
        let _ = WorldCoreHooks::loader_init(&*f.g, &f.world, 1, true, "faces", 1100, None);
        let live = Queries { glue: Rc::downgrade(&f.g) };
        let dead = Queries { glue: Rc::downgrade(&f.dead) };
        for q in [&live, &dead] {
            for x in 0..25 {
                let at = IntPair { x, y: 9 };
                let _ = (q.ladder_at_pos(at), q.column_at_pos(at), q.stairs_at_pos(at), q.elevator_shaft_at_pos(at));
                let _ = (q.painting_at_pos(at), q.elevator_motor_for_shaft_at_pos(at));
                let _ = (q.door_can_be_used_by_path_user(Some(&f.stub), at), q.door_can_be_used_by_path_user(None, at));
            }
        }
        let face = DwFace(f.dw.clone());
        let _ = face.get_next_dynamic_object_id();
        face.client_pickup_request_count_client_id_blockhead_requester_unique_id(&[1], "cid", 1);
        face.create_client_freeblocks_with_data(&[0u8; 8]);
        let _ = face.load_client_blockheads_data_for_player_id("cid");
        face.client_blockheads_recieved_for_player_id_data("cid", &[0u8; 8]);
        face.client_blockhead_inventory_recieved_for_player_id_blockhead_id_data("cid", 1, &[0u8; 8]);
        face.send_painting_data_for_painting_with_id_to_client(1, "cid");
        face.new_found_list_recieved_from_client_list("cid", &crate::foundation::indexset::IndexSet::new());
        face.client_blockhead_with_id_from_client_requests_dyamic_object_removal_with_id(1, "cid", 1);
        face.user_ban_changed_is_banned("cid", true);
        let loader = f.g.loader().expect("loader installed by install_post_init");
        let adapter = LoaderAdapter::new(loader);
        let _ = (adapter.tree_positions(), adapter.npc_positions(), adapter.plant_positions());
        let _ = (adapter.max_of_rock_and_dirt_height_for_x(5), adapter.lake_height_for_x(5));
    });
}

#[test]
fn mods_hooks_give_items_and_edit_tile_records() {
    isolated(|| {
        use crate::world::lifecycle::WorldCoreHooks as W;
        let f = fx("hk_mods_records");
        // Nobody owns a blockhead yet, and a glue without a dynamic world gives nothing.
        assert!(!W::mods_give_item(&*f.g, "p1", 1107, 1));
        assert!(!W::mods_give_item(&*f.dead, "p1", 1107, 1));
        f.bh().set_client_id("p1");
        f.dw.net_blockheads.borrow_mut().push(f.bh_obj.clone());
        assert!(!W::mods_give_item(&*f.g, "p2", 1107, 1));
        assert!(W::mods_give_item(&*f.g, "p1", 1107, 3));

        // The tile record: wrapped x, block type and the 16 spare bytes of a loaded tile.
        let (wx, ty, pad) = W::mods_tile_record(&*f.g, 5, 9).unwrap();
        assert_eq!((wx, pad), (5, [0; 16]));
        assert_eq!(ty, f.tile.get().type_index);
        assert_eq!(W::mods_tile_record(&*f.g, 5, -1), None);
        assert_eq!(W::mods_tile_record(&*f.dead, 5, 9), None);
        let width = f.world.world_width_macro * 32;
        assert_eq!(W::mods_tile_record(&*f.g, 5 - width, 9).map(|r| r.0), Some(5));
        assert_eq!(W::mods_world_width(&*f.g), Some(width));
        assert_eq!(W::mods_world_width(&*f.dead), None);

        // Setting it changes the block, keeps the bytes and tells the clients; the record gets the wrapped x.
        let seen = std::cell::Cell::new((0, 0));
        let record = |x: i32, y: i32| {
            seen.set((x, y));
            [x as u8; 16]
        };
        assert!(W::mods_set_tile_record(&*f.g, 5 - width, 9, 11, &record));
        assert_eq!(seen.get(), (5, 9));
        let t = f.tile.get();
        assert_eq!((t.type_index, t.padding), (11, [5; 16]));
        assert!(f.dw.world_changed_positions.borrow().contains(&IntPair { x: 5 - width, y: 9 }));
        // A change of the block type from outside ends the record.
        f.tile.set_type_index(2);
        assert_eq!(f.tile.get().padding, [0; 16]);
        assert!(!W::mods_set_tile_record(&*f.g, 5, -1, 11, &record));
        assert!(!W::mods_set_tile_record(&*f.dead, 5, 9, 11, &record));
        // A glue with a world but no dynamic world still edits the tile.
        *f.g.dw.borrow_mut() = None;
        assert!(W::mods_set_tile_record(&*f.g, 5, 9, 10, &record));
        assert_eq!(f.tile.get().type_index, 10);
        assert!(!W::mods_give_item(&*f.g, "p1", 1107, 1));
    });
}
