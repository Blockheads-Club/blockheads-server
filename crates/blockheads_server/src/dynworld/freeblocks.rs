//! Free block (dropped item) creation: client packet 0x14, the server-side factory overloads and lookup helpers.
//!
//! Sound playback is client presentation (decision 1); only its state effects are kept.

use super::state::{DynamicWorld, FreeBlockInit};
use crate::foundation::gzip::gzip_inflate;
use crate::items::inventory_item::InventoryItem;
use crate::items::misc::item_type_for_workbench_type_and_level;
use crate::items::predicates::{item_type_is_column, item_type_is_painting, item_type_is_stairs, item_type_is_torch};
use crate::objects::handle::ObjRef;
use crate::plist::helpers::property_list_from_data_opt;
use crate::plist::value::PlistValue;
use crate::types::enums::WorkbenchType;
use crate::types::item_type::ItemType;
use crate::types::structs::IntPair;
use crate::world::helpers::tile_is_workbench;
use crate::world::loot::item_type_from_tile_is_foregorund;
use crate::world::state::TileRef;

impl DynamicWorld {
    /// `-createClientFreeblocksWithData:` (packet 0x14 payload after the type byte): `gzipInflate`, plist array of
    /// NSData, one FreeBlock each. Inserted directly into `dynamic_objects[14]` (`operator[]`, overwriting).
    pub fn create_client_freeblocks_with_data(&self, net_data: &[u8]) {
        let hooks = self.hooks();
        let arr = property_list_from_data_opt(gzip_inflate(net_data).as_deref());
        let items: Vec<PlistValue> = match arr {
            Some(PlistValue::Array(a)) => a.items,
            _ => Vec::new(),
        };
        for item in items {
            let PlistValue::Data(d) = item else { continue };
            // the ring is handed to the constructor by pointer in the original; moved out for the call
            let mut ring = std::mem::take(&mut *self.avoid_freeblock_dupe_object_ids.borrow_mut());
            let fb = hooks.freeblock_from_net_data(&d.bytes, Some(&mut ring));
            *self.avoid_freeblock_dupe_object_ids.borrow_mut() = ring;
            if let Some(fb) = fb {
                self.do_set(14, fb.unique_id(), fb.clone());
                let key = self.world_index_key(fb.pos());
                self.free_blocks_insert(key, &fb);
                self.send_net_data_if_needed_for_object_is_creation(&fb, true);
            }
        }
    }

    /// `-createFreeBlockAtPosition:ofType:dataA:dataB:subItems:dynamicObjectSaveDict:hovers:playSound:priorityBlockhead:`
    /// (server branch; `client` is nil).
    #[allow(clippy::too_many_arguments)]
    pub fn create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
        &self,
        position: IntPair,
        item_type: ItemType,
        data_a: u16,
        data_b: u16,
        sub_items: Option<Vec<Vec<InventoryItem>>>,
        dynamic_object_save_dict: Option<PlistValue>,
        hovers: bool,
        play_sound: i32,
        priority_blockhead: Option<&ObjRef>,
    ) {
        if item_type == ItemType::ITEM_TIME_CRYSTAL {
            return;
        }
        let hooks = self.hooks();
        let fb = hooks.freeblock_from_position(FreeBlockInit {
            position,
            item_type: item_type.0,
            data_a,
            data_b,
            sub_items,
            dynamic_object_save_dict,
            hovers,
            priority_blockhead,
        });
        // UB-PARITY: the original has NO nil check (a nil FreeBlock would be stored under key 0 at pos {0,0}).
        let Some(fb) = fb else { return };
        if play_sound != 0 {
            let world = self.world();
            let t = world.world_time() as f32; // cvtsd2ss
            hooks.freeblock_set_creation_sound(&fb, t, play_sound);
            // sound selection (pop/craftCreate/drop/sale) is client presentation; the delay bump is state
            if self.free_block_sound_delay.get() < 1.0f32 {
                self.free_block_sound_delay.set(self.free_block_sound_delay.get() + 0.05f32);
            }
        }
        self.do_set(14, fb.unique_id(), fb.clone());
        let key = self.world_index_key(fb.pos());
        self.free_blocks_insert(key, &fb);
        self.send_net_data_if_needed_for_object_is_creation(&fb, true);
    }

    /// `-createFreeBlockAtPosition:forForegroundContents:forTile:priorityBlockhead:`: drops the item of a mined tile.
    pub fn create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
        &self,
        position: IntPair,
        for_foreground_contents: bool,
        tile: Option<TileRef>,
        priority_blockhead: Option<&ObjRef>,
    ) {
        let hooks = self.hooks();
        let world = self.world();
        // `tile == nil ||` leads to a NULL tile dereference in itemTypeFromTileIsForegorund: UB-PARITY, return.
        let Some(tile) = tile else { return };
        if !(tile.type_index() == 0x10 && !for_foreground_contents) {
            let item = item_type_from_tile_is_foregorund(tile, position, for_foreground_contents, &world);
            if item == ItemType::ITEM_NONE {
                if !tile_is_workbench(Some(tile)) {
                    if tile.contents() == 0x30 {
                        let io = hooks.interaction_object_at_pos(position);
                        if let Some(io) = io {
                            let dit = hooks.destroy_item_type(&io);
                            if dit != 0 {
                                let sd = Some(PlistValue::Dict(io.get_save_dict(&io)));
                                self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                                    position,
                                    ItemType(dit as u32),
                                    0,
                                    0,
                                    None,
                                    sd,
                                    false,
                                    1,
                                    priority_blockhead,
                                );
                            }
                            io.set_needs_removed(&io, true);
                        }
                    }
                } else if let Some(wb) = hooks.workbench_at_pos(position) {
                    let it = item_type_for_workbench_type_and_level(
                        WorkbenchType(hooks.workbench_type(&wb) as u32),
                        hooks.workbench_level(&wb),
                    );
                    let sd = Some(PlistValue::Dict(wb.get_save_dict(&wb)));
                    self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        position,
                        it,
                        0,
                        0,
                        None,
                        sd,
                        false,
                        1,
                        priority_blockhead,
                    );
                    wb.set_needs_removed(&wb, true);
                }
            } else {
                let obj: Option<ObjRef> = if item_type_is_painting(item) {
                    hooks.object_at_pos_for_item('p', position)
                } else if item_type_is_column(item) {
                    hooks.object_at_pos_for_item('c', position)
                } else if item_type_is_stairs(item) {
                    hooks.object_at_pos_for_item('s', position)
                } else if item_type_is_torch(item) {
                    hooks.object_at_pos_for_item('t', position)
                } else if item == ItemType::ITEM_COPPER_WIRE {
                    hooks.object_at_pos_for_item('w', position)
                } else {
                    None
                };
                let save_dict = obj.as_ref().map(|o| PlistValue::Dict(o.get_save_dict(o)));
                let client = priority_blockhead.and_then(|b| hooks.blockhead_client_id(b));
                let lit = hooks.tile_is_lit_for_client(client.as_deref(), position, tile);
                self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                    position,
                    item,
                    0,
                    0,
                    None,
                    save_dict,
                    false,
                    if lit { 1 } else { 0 },
                    priority_blockhead,
                );
            }
        }
    }

    /// `-freeblockWithUniqueID:`: `dynamicObjects[14]`, then `dynamicObjectsToAdd[14]`.
    pub fn freeblock_with_unique_id(&self, unique_id: u64) -> Option<ObjRef> {
        self.do_get(14, unique_id).or_else(|| self.ta_get(14, unique_id))
    }

    /// `-createBackgroundContentFreeBlockAtPosition:forTile:removeBlockhead:`.
    pub fn create_background_content_free_block_at_position_for_tile_remove_blockhead(
        &self,
        position: IntPair,
        tile: TileRef,
        remove_blockhead: Option<&ObjRef>,
    ) {
        let hooks = self.hooks();
        let bg = tile.background_contents();
        let play_sound = if tile.light() > 0xAA { 1 } else { 0 };
        match bg {
            0x46 | 0x4B => {
                if let Some(d) = hooks.remove_door_at_pos(position) {
                    let it = ItemType(hooks.item_type(&d) as u32);
                    let sd = Some(PlistValue::Dict(d.get_save_dict(&d)));
                    self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        position, it, 0, 0, None, sd, false, play_sound, remove_blockhead,
                    );
                }
            }
            0x45 | 0x5F => {
                if let Some(w) = hooks.remove_window_at_pos(position) {
                    let it = if bg == 0x45 { ItemType::ITEM_WINDOW } else { ItemType::ITEM_BLACK_WINDOW };
                    let sd = Some(PlistValue::Dict(w.get_save_dict(&w)));
                    self.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        position, it, 0, 0, None, sd, false, play_sound, remove_blockhead,
                    );
                }
            }
            _ => {}
        }
    }

    /// `-playTimeCrystalReceivedSoundAtPos:`: the only server effect is the `+0.05f` bump (when `< 1.0f`).
    pub fn play_time_crystal_received_sound_at_pos(&self, _position: IntPair) {
        if self.free_block_sound_delay.get() < 1.0f32 {
            self.free_block_sound_delay.set(self.free_block_sound_delay.get() + 0.05f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynworld::state::testutil::*;

    use std::cell::RefCell;
    use std::rc::Rc;

    struct Hk {
        log: Rc<RefCell<Vec<String>>>,
        next: std::cell::Cell<u64>,
    }
    impl crate::dynworld::state::DwHooksStub for Hk {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.next.set(self.next.get() + 1);
            let o: ObjRef = Obj::new(14, self.next.get(), self.log.clone());
            o.base().borrow_mut().pos = init.position;
            Some(o)
        }
        fn freeblock_from_net_data(
            &self,
            net: &[u8],
            avoid: Option<&mut std::collections::VecDeque<u64>>,
        ) -> Option<ObjRef> {
            let id = u64::from_le_bytes(net[0..8].try_into().unwrap());
            let a = avoid.expect("packet 0x14 passes the ring");
            if a.contains(&id) {
                return None;
            }
            a.push_back(id);
            Some(Obj::new(14, id, self.log.clone()))
        }
    }

    /// create: time crystal is skipped; others go straight into dynamic_objects[14] and the position set.
    #[test]
    fn create_free_block() {
        let d = dw("fb_create", 1);
        let log = Rc::new(RefCell::new(vec![]));
        d.set_hooks(Rc::new(Hk { log: log.clone(), next: std::cell::Cell::new(100) }));
        d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            IntPair { x: 5, y: 6 }, ItemType::ITEM_TIME_CRYSTAL, 0, 0, None, None, false, 0, None,
        );
        assert_eq!(d.do_len(14), 0);
        d.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            IntPair { x: 5, y: 6 }, ItemType(4), 0, 0, None, None, false, 1, None,
        );
        assert_eq!(d.do_len(14), 1);
        assert!(d.freeblock_with_unique_id(101).is_some());
        let key = d.world_index_key(IntPair { x: 5, y: 6 });
        assert_eq!(d.free_blocks_by_position.borrow()[&key].len(), 1);
        // playSound bumps the delay
        assert_eq!(d.free_block_sound_delay.get(), 0.05f32);
    }

    /// packet 0x14: the dupe ring drops a repeated id.
    #[test]
    fn client_freeblocks_ring() {
        let d = dw("fb_pkt", 1);
        let log = Rc::new(RefCell::new(vec![]));
        d.set_hooks(Rc::new(Hk { log: log.clone(), next: std::cell::Cell::new(0) }));
        let a = 77u64.to_le_bytes().to_vec();
        let arr = PlistValue::array(vec![PlistValue::data(a.clone()), PlistValue::data(a.clone())]);
        let xml = crate::plist::helpers::data_from_property_list(&arr).unwrap();
        let gz = crate::foundation::gzip::gzip_deflate(&xml);
        d.create_client_freeblocks_with_data(&gz);
        assert_eq!(d.do_len(14), 1);
        assert!(d.do_contains(14, 77));
    }

    #[test]
    fn crystal_sound_bump() {
        let d = dw("fb_cry", 1);
        d.play_time_crystal_received_sound_at_pos(IntPair { x: 0, y: 0 });
        d.play_time_crystal_received_sound_at_pos(IntPair { x: 0, y: 0 });
        assert_eq!(d.free_block_sound_delay.get(), 0.05f32 + 0.05f32);
        d.free_block_sound_delay.set(1.0);
        d.play_time_crystal_received_sound_at_pos(IntPair { x: 0, y: 0 });
        assert_eq!(d.free_block_sound_delay.get(), 1.0);
    }
}

/// Scenario tests of the mined-tile drops.
#[cfg(test)]
mod scenario_tests {
    use super::*;
    use crate::dynworld::state::testutil::*;
    use crate::dynworld::state::DwHooksStub;
    use crate::trees::tree::testenv::{env, ip};
    use crate::world::tiles::tile_at_world_position_loaded;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    struct Fh {
        log: Log,
        next: Cell<u64>,
        io: RefCell<Option<ObjRef>>,
        wb: RefCell<Option<ObjRef>>,
        destroy: Cell<i32>,
        lit: Cell<bool>,
        door: RefCell<Option<ObjRef>>,
        item: Cell<i32>,
    }
    impl DwHooksStub for Fh {
        fn freeblock_from_position(&self, init: FreeBlockInit<'_>) -> Option<ObjRef> {
            self.next.set(self.next.get() + 1);
            self.log.borrow_mut().push(format!("drop({},{})", init.item_type, init.dynamic_object_save_dict.is_some()));
            let o = tobj(14, self.next.get(), &self.log, None, None, None);
            o.base().borrow_mut().pos = init.position;
            Some(o)
        }
        fn interaction_object_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.io.borrow().clone()
        }
        fn workbench_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.wb.borrow().clone()
        }
        fn workbench_type(&self, _w: &ObjRef) -> i32 {
            3
        }
        fn destroy_item_type(&self, _o: &ObjRef) -> i32 {
            self.destroy.get()
        }
        fn object_at_pos_for_item(&self, which: char, _p: IntPair) -> Option<ObjRef> {
            self.log.borrow_mut().push(format!("lookup({which})"));
            Some(tobj(1, 1, &self.log, None, None, None))
        }
        fn tile_is_lit_for_client(&self, _c: Option<&str>, _p: IntPair, _t: TileRef) -> bool {
            self.lit.get()
        }
        fn remove_door_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.door.borrow().clone()
        }
        fn remove_window_at_pos(&self, _p: IntPair) -> Option<ObjRef> {
            self.door.borrow().clone()
        }
        fn item_type(&self, _o: &ObjRef) -> i32 {
            self.item.get()
        }
    }

    fn fixture(name: &str) -> (Rc<DynamicWorld>, Rc<Fh>, Log) {
        let (_w, d) = env(name);
        let log: Log = Rc::new(RefCell::new(vec![]));
        let h = Rc::new(Fh {
            log: log.clone(),
            next: Cell::new(100),
            io: RefCell::new(None),
            wb: RefCell::new(None),
            destroy: Cell::new(0),
            lit: Cell::new(false),
            door: RefCell::new(None),
            item: Cell::new(52),
        });
        d.set_hooks(h.clone());
        (d, h, log)
    }

    fn drop_at(d: &Rc<DynamicWorld>, p: IntPair, fg: bool) {
        let tile = tile_at_world_position_loaded(p.x, p.y, &d.world());
        d.create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(p, fg, tile, None);
    }

    #[test]
    fn mined_tiles_drop_their_items() {
        let (d, h, log) = fixture("fbs_mined");
        let w = d.world();
        let tile = |x, y| tile_at_world_position_loaded(x, y, &w).unwrap();
        let count = |what: &str| log.borrow().iter().filter(|l| l.starts_with(what)).count();
        // no tile, and the unmined-water guard
        d.create_free_block_at_position_for_foreground_contents_for_tile_priority_blockhead(
            ip(1, 1),
            false,
            None,
            None,
        );
        tile(5, 5).update(|t| t.type_index = 0x10);
        drop_at(&d, ip(5, 5), false);
        assert_eq!(count("drop"), 0);
        // ordinary blocks drop their item; a lit tile makes the sound
        drop_at(&d, ip(6, 5), false);
        assert_eq!(count("drop"), 1);
        h.lit.set(true);
        drop_at(&d, ip(6, 5), false);
        assert_eq!(count("drop"), 2);
        // nothing to drop and not an interaction object
        tile(7, 12).update(|t| t.contents = 0x01);
        tile(7, 12).update(|t| t.type_index = 2);
        drop_at(&d, ip(7, 14), false);
        // interaction objects drop what they are made of and go away
        tile(8, 12).update(|t| t.contents = 0x30);
        drop_at(&d, ip(8, 12), false);
        let chest = tobj(46, 5, &log, None, None, None);
        *h.io.borrow_mut() = Some(chest.clone());
        drop_at(&d, ip(8, 12), false);
        assert!(chest.needs_removed());
        assert_eq!(count("drop"), 2);
        h.destroy.set(1043);
        let chest2 = tobj(46, 6, &log, None, None, None);
        *h.io.borrow_mut() = Some(chest2.clone());
        drop_at(&d, ip(8, 12), false);
        assert_eq!(count("drop(1043,true)"), 1);
        // workbenches drop the item of their type and level
        tile(9, 12).update(|t| t.contents = 0x2E);
        drop_at(&d, ip(9, 12), false);
        let bench = tobj(45, 7, &log, None, None, None);
        *h.wb.borrow_mut() = Some(bench.clone());
        drop_at(&d, ip(9, 12), false);
        assert!(bench.needs_removed());
        // foreground contents: torches, ladders and the pieces looked up in the world
        for (fg, which) in [(0x31u8, Some('t')), (0x42, None), (0x60, Some('w'))] {
            tile(10, 12).update(|t| {
                t.type_index = 2;
                t.foreground_contents = fg;
            });
            drop_at(&d, ip(10, 12), true);
            if let Some(c) = which {
                assert!(log.borrow().contains(&format!("lookup({c})")));
            }
        }
        // painting, column and stairs items found in the foreground and background
        for fg in [0x63u8, 0x64] {
            tile(11, 12).update(|t| t.foreground_contents = fg);
            drop_at(&d, ip(11, 12), true);
        }
        let _ = (count("drop"), h.lit.get());
    }

    #[test]
    fn background_items_come_back_as_free_blocks() {
        let (d, h, log) = fixture("fbs_bg");
        let w = d.world();
        let tile = |x, y| tile_at_world_position_loaded(x, y, &w).unwrap();
        let door = tobj(20, 8, &log, None, None, None);
        *h.door.borrow_mut() = Some(door);
        for (bg, light) in [(0x46u8, 0u8), (0x4B, 255), (0x45, 255), (0x5F, 0), (0x01, 0)] {
            tile(12, 12).update(|t| {
                t.background_contents = bg;
                t.sun_light = light;
            });
            d.create_background_content_free_block_at_position_for_tile_remove_blockhead(
                ip(12, 12),
                tile(12, 12),
                None,
            );
        }
        let l = log.borrow().clone();
        assert!(l.iter().filter(|s| s.starts_with("drop(")).count() >= 4, "{l:?}");
        // without a door or window there is nothing to drop
        *h.door.borrow_mut() = None;
        let n = log.borrow().len();
        for bg in [0x46u8, 0x45] {
            tile(12, 12).update(|t| t.background_contents = bg);
            d.create_background_content_free_block_at_position_for_tile_remove_blockhead(
                ip(12, 12),
                tile(12, 12),
                None,
            );
        }
        assert_eq!(log.borrow().len(), n);
    }

    #[test]
    fn client_packets_ignore_bad_payloads() {
        let (d, _h, _log) = fixture("fbs_pkt");
        d.create_client_freeblocks_with_data(&[1, 2, 3]);
        let not_array = crate::foundation::gzip::gzip_deflate(
            &crate::plist::helpers::data_from_property_list(&plist_dict(vec![])).unwrap(),
        );
        d.create_client_freeblocks_with_data(&not_array);
        let with_junk = crate::foundation::gzip::gzip_deflate(
            &crate::plist::helpers::data_from_property_list(&PlistValue::array(vec![
                PlistValue::int(1),
                PlistValue::data(vec![1u8; 8]),
            ]))
            .unwrap(),
        );
        d.create_client_freeblocks_with_data(&with_junk);
        assert_eq!(d.do_len(14), 0);
    }

    #[test]
    fn paintings_columns_and_stairs_drop_their_item() {
        use crate::world::helpers::testutil::{dw_obj, FakeDw};
        let (d, _h, log) = fixture("fbs_pieces");
        let w = d.world();
        let fake = Rc::new(FakeDw::default());
        *w.dynamic_world.borrow_mut() = Some(Rc::new(crate::world::state::DynamicWorld::new(Box::new(fake.clone()))));
        let spot = (13, 12);
        fake.painting.borrow_mut().insert(spot, dw_obj(212, 0, 0, false, true));
        fake.column.borrow_mut().insert(spot, dw_obj(222, 0, 0, false, true));
        fake.stairs.borrow_mut().insert(spot, dw_obj(229, 0, 0, false, true));
        let tile = tile_at_world_position_loaded(13, 12, &w).unwrap();
        tile.update(|t| t.type_index = 2);
        for (fg, contents, is_fg) in [(0x63u8, 0u8, true), (0x64, 0, true), (0, 0x65, false)] {
            tile.update(|t| {
                t.foreground_contents = fg;
                t.contents = contents;
            });
            drop_at(&d, ip(13, 12), is_fg);
        }
        let l = log.borrow().clone();
        for which in ['p', 'c', 's'] {
            assert!(l.contains(&format!("lookup({which})")), "{l:?}");
        }
    }
}
