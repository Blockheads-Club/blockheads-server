//! Adapters of the glue that need an object of the right class: called with every fixture object.

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
fn adapters_with_objects_of_every_class() {
    isolated(|| {
        let f = fx("hk_objects");
        f.g.install_post_init(&f.world);
        let mut objs: Vec<ObjRef> = vec![f.stub.clone(), f.bh_obj.clone()];
        objs.extend(f.objs.iter().map(|(_, o)| o.clone()));
        let _ = WorldCoreHooks::loader_init(&*f.g, &f.world, 1, true, "objs", 1100, None);
        // A blockhead the dynamic world knows, so the preview list has an entry.
        f.dw.blockheads.borrow_mut().push(f.bh_obj.clone());
        // One object of every class that can be built from a (nearly empty) save dictionary.
        for ty in 0..130 {
            for kind in [SaveLoadKind::Generic, SaveLoadKind::TreeOrPlant, SaveLoadKind::InteractionObject(ty)] {
                let made =
                    crate::foundation::exception::catch_ns(|| f.g.construct_from_save_dict(kind, ty, &f.dict, None));
                objs.extend(made.ok().flatten());
            }
        }
        // Every tree and plant species, so the per-class adapters find their class.
        let ops: &dyn crate::dynworld::lookups::DwOpsHooks = &*f.g;
        for ty in 1..16 {
            let tree = ops.new_tree(&f.dw, ty, IntPair { x: 60 + ty, y: 9 }, 5, 5, true, 100.0);
            let plant = ops.new_plant(&f.dw, ty, IntPair { x: 90 + ty, y: 9 }, 0, 0, true);
            objs.extend(tree.into_iter().chain(plant));
        }
        for g in [&f.g, &f.dead] {
            t("loader_update_physical_block_to_latest_version", || {
                drop(WorldCoreHooks::loader_update_physical_block_to_latest_version(&**g, f.pb))
            });
            t("dynamic_world_blockhead_previews", || drop(WorldCoreHooks::dynamic_world_blockhead_previews(&**g)));
            t("client_receipts", || drop(WorldgenHost::client_receipts(&**g, &f.world, "cid", f.p)));
            t("load_physical_block_at_x_pos_y_pos_create_if_not_created", || {
                drop(WorldOpsDeps::load_physical_block_at_x_pos_y_pos_create_if_not_created(&**g, f.pb, 1, 1, true))
            });
            t(
                "send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable",
                || {
                    drop(WorldOpsDeps::send_block_to_client_without_saving_for_block_pos_send_to_client_server_send_dynamic_objects_reliable(&**g, f.pb, f.p, "cid", true, true))
                },
            );
            t("unmodified_ground_level_for_x", || drop(WorldOpsDeps::unmodified_ground_level_for_x(&**g, 1)));
            t("workbench_level", || {
                for o in &objs {
                    drop(WorldOpsDeps::workbench_level(&**g, o))
                }
            });
            t("workbench_upgrade_to_next_level", || {
                for o in &objs {
                    drop(WorldOpsDeps::workbench_upgrade_to_next_level(&**g, o))
                }
            });
            t("bh_is_net", || {
                for o in &objs {
                    drop(WorldOpsDeps::bh_is_net(&**g, o))
                }
            });
            t("bh_add_expected_craft_item", || {
                for o in &objs {
                    drop(WorldOpsDeps::bh_add_expected_craft_item(&**g, o, crate::types::item_type::ItemType(1048)))
                }
            });
            t("bh_local_net_id", || {
                for o in &objs {
                    drop(WorldOpsDeps::bh_local_net_id(&**g, o))
                }
            });
            t("bh_can_use_dynamic_object", || {
                for o in &objs {
                    drop(WorldOpsDeps::bh_can_use_dynamic_object(&**g, o, &f.stub))
                }
            });
            t("update_physical_block_to_latest_version", || {
                drop(DwHooks::update_physical_block_to_latest_version(&**g, f.pb))
            });
            t("tile_is_lit_for_client", || drop(DwHooks::tile_is_lit_for_client(&**g, Some("cid"), f.p, f.tile)));
            t("construct_from_save_dict", || {
                drop(DwHooks::construct_from_save_dict(&**g, SaveLoadKind::Workbench, 1, &f.dict, Some(&f.dict)))
            });
            t("construct_from_net_data", || drop(DwHooks::construct_from_net_data(&**g, 1, Some(1), &[0u8; 16])));
            t("workbench_set_level_silently", || {
                for o in &objs {
                    drop(DwHooks::workbench_set_level_silently(&**g, o, 1))
                }
            });
            t("destroy_item_type", || {
                for o in &objs {
                    drop(DwHooks::destroy_item_type(&**g, o))
                }
            });
            t("object_at_pos_for_item", || drop(DwHooks::object_at_pos_for_item(&**g, 's', f.p)));
            t("freeblock_from_net_data", || drop(DwHooks::freeblock_from_net_data(&**g, &[0u8; 16], None)));
            t("blockhead_client_save_dir", || {
                for o in &objs {
                    drop(DwHooks::blockhead_client_save_dir(&**g, o))
                }
            });
            t("blockhead_save_dict_including_workbench_or_interaction_object", || {
                for o in &objs {
                    drop(DwHooks::blockhead_save_dict_including_workbench_or_interaction_object(&**g, o, true))
                }
            });
            t("blockhead_save_item_slots_array", || {
                for o in &objs {
                    drop(DwHooks::blockhead_save_item_slots_array(&**g, o))
                }
            });
            t("blockhead_prepare_inventory_for_saving", || {
                for o in &objs {
                    drop(DwHooks::blockhead_prepare_inventory_for_saving(&**g, o))
                }
            });
            t("blockhead_inventory_needs_saving", || {
                for o in &objs {
                    drop(DwHooks::blockhead_inventory_needs_saving(&**g, o))
                }
            });
            t("blockhead_set_inventory_needs_saving", || {
                for o in &objs {
                    drop(DwHooks::blockhead_set_inventory_needs_saving(&**g, o, true))
                }
            });
            t("blockhead_inventory_was_changed", || {
                for o in &objs {
                    drop(DwHooks::blockhead_inventory_was_changed(&**g, o, 1, 1, true))
                }
            });
            t("blockhead_stop_interacting_if_no_interaction_object", || {
                for o in &objs {
                    drop(DwHooks::blockhead_stop_interacting_if_no_interaction_object(&**g, o))
                }
            });
            t("blockhead_die", || {
                for o in &objs {
                    drop(DwHooks::blockhead_die(&**g, o))
                }
            });
            t("plant_available_food", || {
                for o in &objs {
                    drop(crate::npc::egg::PassiveEnv::plant_available_food(&**g, o))
                }
            });
            t("plant_set_available_food", || {
                for o in &objs {
                    drop(crate::npc::egg::PassiveEnv::plant_set_available_food(&**g, o, 1.0))
                }
            });
            t("freeblock_hovers", || {
                for o in &objs {
                    drop(DwOpsHooks::freeblock_hovers(&**g, o))
                }
            });
            t("tree_height", || {
                for o in &objs {
                    drop(DwOpsHooks::tree_height(&**g, o))
                }
            });
            t("tree_max_height_gene_variation", || {
                for o in &objs {
                    drop(DwOpsHooks::tree_max_height_gene_variation(&**g, o))
                }
            });
            t("tree_growth_rate_gene_variation", || {
                for o in &objs {
                    drop(DwOpsHooks::tree_growth_rate_gene_variation(&**g, o))
                }
            });
            t("interaction_object_width_radius", || {
                for o in &objs {
                    drop(DwOpsHooks::interaction_object_width_radius(&**g, o))
                }
            });
            t("interaction_object_height_radius", || {
                for o in &objs {
                    drop(DwOpsHooks::interaction_object_height_radius(&**g, o))
                }
            });
        }
        // A glue that has a world but no dynamic world.
        let nodw = Glue::new(&f.g.srv().unwrap());
        *nodw.world.borrow_mut() = Rc::downgrade(&f.world);
        for g in [&f.g, &f.dead, &nodw] {
            for kind in [
                SaveLoadKind::TreeOrPlant,
                SaveLoadKind::Shark,
                SaveLoadKind::FreightCar,
                SaveLoadKind::Workbench,
                SaveLoadKind::InteractionObject(1),
                SaveLoadKind::Generic,
            ] {
                for ty in 0..130 {
                    t("construct_from_save_dict", || {
                        drop(g.construct_from_save_dict(kind, ty, &f.dict, Some(&f.dict)))
                    });
                }
            }
            for c in ['p', 'c', 's', 't', 'w', 'x'] {
                t("object_at_pos_for_item", || drop(g.object_at_pos_for_item(c, f.p)));
            }
            for (x, y, create) in [(0, 0, true), (0, 0, false), (1, 1, false), (3, 3, true)] {
                t("load_physical_block", || {
                    g.load_physical_block_at_x_pos_y_pos_create_if_not_created(f.pb, x, y, create)
                });
            }
            let mut avoid = std::collections::VecDeque::new();
            t("freeblock_from_net_data", || drop(g.freeblock_from_net_data(&[0u8; 32], Some(&mut avoid))));
            t("construct_from_net_data", || drop(g.construct_from_net_data(1, Some(1), &[0u8; 32])));
            for ty in [1, 2, 53, 9999] {
                t("construct_from_net_data_plain", || drop(g.construct_from_net_data(ty, None, &[0u8; 32])));
            }
            t("tile_is_lit_for_client", || drop(g.tile_is_lit_for_client(Some("cid"), f.p, f.tile)));
        }
        for _b in [&f.bg, &f.dead_bg, &f.gone] {}
    });
}
