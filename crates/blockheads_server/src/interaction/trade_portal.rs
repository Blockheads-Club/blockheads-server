//! `TradePortal` (DynamicObjectType 50, interaction type 7, item `ITEM_TRADE_PORTAL` 210) and the
//! `PortalChestManager` placeholder.
//!
//! The buy / sell / upgrade logic is unreachable from server code but is implemented for completeness.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dynworld::state::DynamicWorld;
use crate::env::crystal_manager::CrystalManager;
use crate::env::particle_emitter::ParticleEmitter;
use crate::fmath::libm::powf;
use crate::fmath::vector::Vector;
use crate::foundation::md5::string_from_md5;
use crate::interaction::chest::{dict_int, dict_str, gunzip_plist, gz_plist_bytes, subdata_from, Links};
use crate::interaction::trading_post::{blockhead_inventory_api, support_lost};
use crate::items::inventory_item::{InventoryItem, SubSlot};
use crate::items::predicates::item_type_is_sowable;
use crate::lighting::artificial_light::ArtificialLight;
use crate::lighting::sunlight::WorldHelper;
use crate::objects::base::{padded_bytes, DynamicObject, DynamicObjectBase};
use crate::objects::class_info::{ClassInfo, INTERACTION_OBJECT_CLASS};
use crate::objects::handle::{self, CacheRef, ObjRef};
use crate::objects::interaction_object::*;
use crate::plist::value::{PlistDict, PlistValue};
use crate::rng::glibc_rand::{r01_from, rand_tagged};
use crate::types::item_type::ItemType;
use crate::types::netdata::TradePortalNetData;
use crate::types::structs::{CraftableItem, IntPair, WireStruct};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

/// `TradePortal: InteractionObject` (instance size 0xe0).
pub static TRADE_PORTAL_CLASS: ClassInfo =
    ClassInfo { name: "TradePortal", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

/// `DYNAMIC_OBJECT_TYPE_TRADE_PORTAL`
pub const DYNAMIC_OBJECT_TYPE_TRADE_PORTAL: i32 = 50;
/// `INTERACTION_OBJECT_TYPE_TRADE_PORTAL`
pub const INTERACTION_OBJECT_TYPE_TRADE_PORTAL: u16 = 7;
/// `ITEM_TRADE_PORTAL`
pub const ITEM_TRADE_PORTAL: i32 = 210;
const ITEM_MONEY: u32 = 298;
const ITEM_COPPER_COIN: i32 = 166;
const ITEM_GOLD_COIN: i32 = 167;
const ITEM_PLATINUM_COIN: i32 = 260;
const ITEM_GOLD_COIN_TRADE_FOR_COPPER_COINS: i32 = 1092;
const ITEM_DODO_EGG: i32 = 91;
const ITEM_TIME_CRYSTAL: i32 = 11;
/// Price multiplier step (double).
const PRICE_STEP: f64 = 0.997;
const CONTENTS_INTERACTION_OBJECT: u8 = 0x30;

/// Placeholder: the class has no implementation in the server binary (resolution T2 / C1).
/// `World.portal_chest_manager` is always `None`.
pub struct PortalChestManager;
impl PortalChestManager {
    /// `-portalChestInventoryItems`: always nil.
    pub fn portal_chest_inventory_items(&self) -> Option<Vec<SubSlot>> {
        None
    }
}

/// TradePortal ivars 0xa0..0xde (client-only ones omitted).
pub struct TradePortalState {
    /// 0xa0, `parentObject = self`
    pub light: Option<Rc<ArtificialLight>>,
    /// 0xac = `rand() % 8` (kept: it is a draw)
    pub animation_loop_index: i32,
    /// 0xb8
    pub paused: bool,
    /// 0xd0; values `Real{ is_float: true }` (numberWithFloat: from randomize) or `is_float: false`
    pub local_price_offsets: Option<PlistDict>,
    /// 0xd8, 0..5
    pub level: i32,
    /// 0xdc
    pub is_sell_interaction: bool,
    /// 0xdd
    pub is_mission_interaction: bool,
}

/// `TradePortal: InteractionObject`.
pub struct TradePortal {
    base: RefCell<DynamicObjectBase>,
    io: RefCell<InteractionObjectState>,
    /// The TradePortal ivars.
    pub st: RefCell<TradePortalState>,
    links: Links,
}

/// `(a >= 0.5) ? min(a, 2.0): 0.5` (NaN -> 0.5).
fn clamp_offset(v: f64) -> f64 {
    if v >= 0.5 {
        if v > 2.0 {
            2.0
        } else {
            v
        }
    } else {
        0.5
    }
}

/// `TILE_TRADE_PORTAL_BASE_STONE + level` for level 0..5, else unchanged.
fn base_stone_for_level(level: i32) -> Option<u8> {
    if (0..=5).contains(&level) {
        Some((0x3c + level) as u8)
    } else {
        None
    }
}

impl TradePortal {
    fn alloc(world: &Rc<World>, dw: &Rc<DynamicWorld>) -> Rc<TradePortal> {
        let links = Links::new(world, dw);
        handle::alloc_obj(|w| TradePortal {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            io: RefCell::new(InteractionObjectState::default()),
            st: RefCell::new(TradePortalState {
                light: None,
                animation_loop_index: 0,
                paused: false,
                local_price_offsets: None,
                level: 0,
                is_sell_interaction: false,
                is_mission_interaction: false,
            }),
            links,
        })
    }

    fn cache(&self) -> CacheRef {
        self.base.borrow().cache.clone()
    }

    /// `-initSubDerivedItems`: ONE `rand()` draw per constructor (T7).
    fn init_sub_derived_items(&self) {
        self.st.borrow_mut().animation_loop_index = rand_tagged("TradePortal.initSubDerivedItems") % 8;
        let world = self.links.world();
        world.update_trade_prices_if_needed();
        let pos = self.base.borrow().pos;
        if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y, &world) {
            t.set_contents(CONTENTS_INTERACTION_OBJECT);
        }
        if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y + 1, &world) {
            t.set_contents(CONTENTS_INTERACTION_OBJECT);
        }
    }

    /// `-loadPriceOffsets:`: merge, clamp to [0.5, 2.0], store as double.
    pub fn load_price_offsets(&self, saved: &PlistDict) {
        let mut st = self.st.borrow_mut();
        let dict = st.local_price_offsets.get_or_insert_with(crate::objects::base::new_mutable_dict);
        for k in saved.map.keys_in_order() {
            let v = saved.map.get(&k).map_or(0.0, |v| v.double_value());
            dict.map.insert(k, PlistValue::real_f64(clamp_offset(v)));
        }
    }

    /// `-updatePortalLight` (placement constructor only).
    pub fn update_portal_light(&self, me: &ObjRef) {
        let old = self.st.borrow_mut().light.take();
        if let Some(l) = old {
            l.remove_from_tiles();
            l.dealloc_light();
        }
        let pos = me.pos();
        let (world, dw) = (self.links.world(), self.links.dw());
        let light = ArtificialLight::init_with_world_dynamic_world_at_position_cache_parent_object_color_r_color_g_color_b_heat_radius_light_direction(
            &world,
            &(dw as crate::objects::base::DynamicWorldRef),
            IntPair { x: pos.x, y: pos.y - 1 },
            self.cache(),
            Some(me),
            255,
            246,
            64,
            0,
            12,
            0,
        );
        self.st.borrow_mut().light = light;
    }

    /// Alias for [`TradePortal::new_at_position_item_flipped_save_dict_placed_by_client_client_name`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<TradePortal>> {
        Self::new_at_position_item_flipped_save_dict_placed_by_client_client_name(
            world,
            dw,
            pos,
            cache,
            item,
            flipped,
            save_dict,
            client_id,
            client_name,
        )
    }

    /// `-[TradePortal initWithWorld:dynamicWorld:atPosition:cache:item:flipped:saveDict:placedByClient:clientName:]`
    ///  (trap 10: stone rewrite and sun light update; offsets stay empty unless the save dict has them).
    #[allow(clippy::too_many_arguments)]
    pub fn new_at_position_item_flipped_save_dict_placed_by_client_client_name(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        pos: IntPair,
        cache: CacheRef,
        item: Option<&InventoryItem>,
        flipped: bool,
        save_dict: Option<&PlistDict>,
        client_id: Option<&str>,
        client_name: Option<&str>,
    ) -> Option<Rc<TradePortal>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
            &me_obj, &w, &d, pos, cache, item, flipped, save_dict, client_id, client_name,
        )
            .then_some(())?;
        me.st.borrow_mut().local_price_offsets = Some(crate::objects::base::new_mutable_dict());
        if let Some(sd) = save_dict {
            if let Some(PlistValue::Dict(saved)) = sd.map.get(&"localPriceOffsets".to_string()) {
                me.load_price_offsets(saved);
            }
            me.st.borrow_mut().level = dict_int(sd, "level");
        }
        let p = me_obj.pos();
        let level = me.st.borrow().level;
        let stone = tile_at_world_position_loaded(p.x, p.y - 1, world);
        if let Some(t) = stone {
            t.set_contents(0);
            if t.back_wall_type_index() == 2 {
                t.set_back_wall_type_index(10);
            }
            if let Some(ty) = base_stone_for_level(level) {
                t.set_type_index(ty);
            }
        }
        WorldHelper::update_sun_light_removed_for_tile_at_pos_world(None, IntPair { x: p.x, y: p.y - 1 }, world);
        me.init_sub_derived_items();
        me.update_portal_light(&me_obj);
        Some(me)
    }

    /// `-[TradePortal initWithWorld:dynamicWorld:saveDict:cache:]` (no stone rewrite; light from `lightDict`).
    pub fn from_save_dict(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        save_dict: &PlistDict,
        cache: CacheRef,
    ) -> Option<Rc<TradePortal>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        // the base initialiser always succeeds here, `None` is only the ObjC `self = nil` shape
        interaction_object_init_with_world_dynamic_world_save_dict_cache(&me_obj, &w, &d, save_dict, cache.clone())
            .then_some(())?;
        me.st.borrow_mut().local_price_offsets = Some(crate::objects::base::new_mutable_dict());
        if let Some(PlistValue::Dict(saved)) = save_dict.map.get(&"localPriceOffsets".to_string()) {
            me.load_price_offsets(saved);
        }
        me.st.borrow_mut().level = dict_int(save_dict, "level");
        if let Some(PlistValue::Dict(ld)) = save_dict.map.get(&"lightDict".to_string()) {
            let dwr: crate::objects::base::DynamicWorldRef = dw.clone();
            let light = ArtificialLight::init_with_world_dynamic_world_save_dict_cache_parent_object(
                world,
                &dwr,
                ld,
                cache,
                Some(&me_obj),
            );
            me.st.borrow_mut().light = light;
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-[TradePortal initWithWorld:dynamicWorld:cache:netData:]` (no light is created).
    pub fn from_net_data(
        world: &Rc<World>,
        dw: &Rc<DynamicWorld>,
        cache: CacheRef,
        net_data: &[u8],
    ) -> Option<Rc<TradePortal>> {
        let me = Self::alloc(world, dw);
        let me_obj: ObjRef = me.clone();
        let (w, d) = me.links.refs();
        if !interaction_object_init_with_world_dynamic_world_cache_net_data(&me_obj, &w, &d, cache, net_data) {
            return None;
        }
        let hdr = TradePortalNetData::read_le(&padded_bytes(net_data, 0x30));
        me.st.borrow_mut().level = hdr.level as i32;
        let dict = gunzip_plist(subdata_from(net_data, 0x30));
        me_obj.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
        me.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
        me.st.borrow_mut().local_price_offsets = Some(crate::objects::base::new_mutable_dict());
        if let Some(PlistValue::Dict(saved)) = dict.as_ref().and_then(|d| d.object_for_key("localPriceOffsets")) {
            me.load_price_offsets(saved);
        }
        for b in dw.blockheads() {
            if let Some(bh) = handle::downcast::<crate::blockhead::blockhead::Blockhead>(&b) {
                bh.net_interaction_object_was_loaded(&me_obj);
            }
        }
        me.init_sub_derived_items();
        Some(me)
    }

    /// `-level`
    pub fn level(&self) -> i32 {
        self.st.borrow().level
    }
    /// `-localPriceOffsets` (a copy).
    pub fn local_price_offsets(&self) -> Option<PlistDict> {
        self.st.borrow().local_price_offsets.clone()
    }
    /// `-isSellInteraction`
    pub fn is_sell_interaction(&self) -> bool {
        self.st.borrow().is_sell_interaction
    }
    /// `-isMissionInteraction`
    pub fn is_mission_interaction(&self) -> bool {
        self.st.borrow().is_mission_interaction
    }

    /// `-randomizeLocalTradeOffsets`: exactly 308 `rand()` draws (categories 1..8, levels 0..5, table
    /// order); values are `numberWithFloat:`.
    pub fn randomize_local_trade_offsets(&self, me: &ObjRef) {
        for cat in ITEM_CATEGORIES.iter().skip(1) {
            for level in cat.iter() {
                for &it in level.iter() {
                    let r = rand_tagged("TradePortal.randomizeLocalTradeOffsets");
                    let f = powf(0.8f32, (r01_from(r) - 0.5f32) * 10.0f32);
                    let mut st = self.st.borrow_mut();
                    let dict = st.local_price_offsets.get_or_insert_with(crate::objects::base::new_mutable_dict);
                    dict.map.insert(format!("{}", it), PlistValue::real_f32(f));
                }
            }
        }
        self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), DYNAMIC_OBJECT_TYPE_TRADE_PORTAL);
        me.set_update_needs_to_be_sent(true);
    }

    /// The 8 particle iterations x 7 `rand()` draws of `remoteUpdate:` / `upgradeToNextLevel` (the emitter is a no-op,
    /// the draws are observable).
    fn level_up_effects(&self, pos: IntPair) {
        let emitter = ParticleEmitter::instance();
        let hit = Vector::new(pos.x as f32 + 0.5f32, pos.y as f32, -0.5f32);
        for _ in 0..8 {
            let b = (r01_from(rand_tagged("TradePortal.particle")) as f64 * 1.5) as f32;
            let color = Vector::new4(b + 0.231373f64 as f32, b + 0.347059f64 as f32, 1.0, 0.5);
            let off = Vector::new(
                r01_from(rand_tagged("TradePortal.particle")) - 0.5f32,
                2.0f32 * r01_from(rand_tagged("TradePortal.particle")),
                0.0,
            );
            let vel = Vector::new(
                (r01_from(rand_tagged("TradePortal.particle")) - 0.5f32) * 4.0f32,
                (r01_from(rand_tagged("TradePortal.particle")) + (-0.3f32)) * 4.0f32,
                0.0,
            );
            let life = r01_from(rand_tagged("TradePortal.particle")) * 4.0f32 + 1.0f32;
            let scale = r01_from(rand_tagged("TradePortal.particle")) * 4.0f32 + 4.0f32;
            emitter.add_particle_at_pos_velocity_color_gravity_type_life_scale_center(
                Vector::new(hit.x + off.x, hit.y + off.y, hit.z + off.z),
                vel,
                color,
                2,
                life,
                scale,
                hit,
            );
        }
    }

    fn rewrite_base_stone(&self, pos: IntPair, level: i32) {
        let world = self.links.world();
        if let Some(t) = tile_at_world_position_loaded(pos.x, pos.y - 1, &world) {
            if (1..=5).contains(&level) {
                t.set_type_index((0x3c + level) as u8);
            }
        }
    }

    /// `-upgradeToNextLevel` (only sent by `craftItemFinished` to a Workbench; unreachable here, T1).
    pub fn upgrade_to_next_level(&self, me: &ObjRef) {
        if self.level() < 5 {
            let level = {
                let mut st = self.st.borrow_mut();
                st.level += 1;
                st.level
            };
            self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), DYNAMIC_OBJECT_TYPE_TRADE_PORTAL);
            me.set_update_needs_to_be_sent(true);
            self.level_up_effects(me.pos());
            self.rewrite_base_stone(me.pos(), level);
        }
    }

    fn offset_for(&self, key: &str) -> f64 {
        self.st
            .borrow()
            .local_price_offsets
            .as_ref()
            .and_then(|d| d.map.get(&key.to_string()))
            .map_or(1.0, |v| v.double_value())
    }
    fn set_offset(&self, key: &str, v: f64) {
        let mut st = self.st.borrow_mut();
        let dict = st.local_price_offsets.get_or_insert_with(crate::objects::base::new_mutable_dict);
        dict.map.insert(key.to_string(), PlistValue::real_f64(clamp_offset(v)));
    }

    /// `-sellItem:atTotalPrice:count:usageMultiplier:` (unreachable on the server, T1).
    pub fn sell_item_at_total_price_count_usage_multiplier(
        &self,
        me: &ObjRef,
        item_type: i32,
        listed_price: i32,
        count: i32,
        usage_multiplier: f32,
    ) {
        let Some(bh) = me.current_blockhead() else { return };
        if matches!(
            item_type,
            ITEM_GOLD_COIN | ITEM_GOLD_COIN_TRADE_FOR_COPPER_COINS | ITEM_COPPER_COIN | ITEM_PLATINUM_COIN
        ) {
            return;
        }
        let required_data_b = if item_type == ITEM_DODO_EGG { 0 } else { -1 };
        let removed = blockhead_inventory_api().subtract_items_from_inventory_of_type_count_data_b(
            &bh,
            item_type,
            count,
            required_data_b,
        );
        if removed != count {
            crate::nslog!("Error trying to subtract items to sell.");
            return;
        }
        let controlled = bh.as_blockhead().is_some_and(|b| b.is_client_blockhead_being_controlled_by_server());
        if !self.base.borrow().is_net && !controlled {
            self.links.world().report_achievement_with_identifier("grp.trade");
        }
        let key = format!("{}", item_type);
        let cur = self.offset_for(&key) * PRICE_STEP;
        self.set_offset(&key, cur);
        self.links.world().update_price_for_item_bought_or_sold_of_type_key_sold_count(&key, usage_multiplier);
        let platinum = listed_price / 10000;
        let copper = listed_price % 10000;
        let pos = me.pos();
        self.links.dw().create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            IntPair { x: pos.x, y: pos.y + 1 },
            ItemType(ITEM_MONEY),
            (platinum & 0xffff) as u16,
            (copper & 0xffff) as u16,
            None,
            None,
            false,
            4,
            Some(&bh),
        );
        self.links.dw().dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_TRADE_PORTAL);
        me.set_update_needs_to_be_sent(true);
    }

    /// `-buyItem:atTotalPrice:count:` (unreachable on the server, T1).
    pub fn buy_item_at_total_price_count(&self, me: &ObjRef, item_type: i32, listed_price: i32, count: i32) {
        let Some(bh) = me.current_blockhead() else { return };
        let api = blockhead_inventory_api();
        let (dw, pos) = (self.links.dw(), me.pos());
        let drop_at = IntPair { x: pos.x, y: pos.y + 1 };
        if matches!(
            item_type,
            ITEM_GOLD_COIN | ITEM_GOLD_COIN_TRADE_FOR_COPPER_COINS | ITEM_COPPER_COIN | ITEM_PLATINUM_COIN
        ) {
            let (use_type, count_to_use, receive_type, count_to_receive) = match item_type {
                ITEM_COPPER_COIN => (ITEM_GOLD_COIN, 1, ITEM_COPPER_COIN, 100),
                ITEM_PLATINUM_COIN => (ITEM_GOLD_COIN, 100, ITEM_PLATINUM_COIN, 1),
                ITEM_GOLD_COIN_TRADE_FOR_COPPER_COINS => (ITEM_COPPER_COIN, 100, ITEM_GOLD_COIN, 1),
                _ => (ITEM_PLATINUM_COIN, 1, ITEM_GOLD_COIN, 100),
            };
            let have = api.count_of_inventory_items_of_type_include_actions(&bh, use_type, false);
            if count_to_use <= have {
                let removed = api.subtract_items_from_inventory_of_type_count_data_b(&bh, use_type, count_to_use, -1);
                if removed == count_to_use {
                    for _ in 0..count_to_receive {
                        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                            drop_at,
                            ItemType(receive_type as u32),
                            0,
                            0,
                            None,
                            None,
                            false,
                            4,
                            Some(&bh),
                        );
                    }
                } else {
                    crate::nslog!("Error trying to subtract coins to trade.");
                }
            }
            return;
        }
        let cash = api.total_cash(&bh);
        if listed_price <= cash {
            let controlled = bh.as_blockhead().is_some_and(|b| b.is_client_blockhead_being_controlled_by_server());
            if !self.base.borrow().is_net && !controlled {
                self.links.world().report_achievement_with_identifier("grp.trade");
            }
            api.subtract_cash(&bh, listed_price);
            let key = format!("{}", item_type);
            let cur = self.offset_for(&key) / PRICE_STEP;
            self.set_offset(&key, cur);
            self.links.world().update_price_for_item_bought_or_sold_of_type_key_sold_count(&key, -1.0f32);
            let (mut da, mut db) = (0u16, 0u16);
            if item_type_is_sowable(ItemType(item_type as u32)) {
                da = 0x7f;
                db = 0x7f;
            }
            for _ in 0..count.max(0) {
                if item_type != ITEM_TIME_CRYSTAL {
                    dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
                        drop_at,
                        ItemType(item_type as u32),
                        da,
                        db,
                        None,
                        None,
                        false,
                        4,
                        Some(&bh),
                    );
                }
            }
            dw.dynamic_world_changed_at_pos_object_type(pos, DYNAMIC_OBJECT_TYPE_TRADE_PORTAL);
            me.set_update_needs_to_be_sent(true);
        }
    }

    /// `-upgradeCraftableItem`: only the documented fields are set.
    pub fn upgrade_craftable_item(&self) -> CraftableItem {
        let expert = self.links.world().expert_mode();
        let mut c = CraftableItem::read_le(&[0u8; 0x7c]);
        c.crafted_item_type = 132;
        c.crafted_item_count = 1;
        c.number_of_required_items = 3;
        c.required_items[1] = ITEM_GOLD_COIN as u32;
        c.required_items[2] = ITEM_TIME_CRYSTAL as u32;
        c.required_item_counts[0] = if expert { 10 } else { 1 };
        match self.level() {
            0 => {
                c.required_items[0] = 87;
                c.required_item_counts[1] = if expert { 50 } else { 10 };
                c.required_item_counts[2] = 10;
            }
            1 => {
                c.required_items[0] = 86;
                c.required_item_counts[1] = if expert { 99 } else { 20 };
                c.required_item_counts[2] = 20;
            }
            2 => {
                c.required_items[0] = 76;
                if expert {
                    c.required_items[1] = ITEM_PLATINUM_COIN as u32;
                    c.required_item_counts[1] = 5;
                } else {
                    c.required_item_counts[1] = 50;
                }
                c.required_item_counts[2] = 50;
            }
            3 => {
                c.required_items[0] = 75;
                c.required_items[1] = ITEM_PLATINUM_COIN as u32;
                c.required_item_counts[1] = if expert { 20 } else { 1 };
                c.required_item_counts[2] = 100;
            }
            4 => {
                c.required_items[0] = 88;
                c.required_items[1] = ITEM_PLATINUM_COIN as u32;
                c.required_item_counts[1] = if expert { 99 } else { 10 };
                c.required_item_counts[2] = 200;
            }
            _ => {}
        }
        c
    }

    /// `-takeItemsFromBlockheadForUpgradeToNextLevel` (unreachable on the server, T1).
    pub fn take_items_from_blockhead_for_upgrade_to_next_level(&self, me: &ObjRef) -> bool {
        let c = self.upgrade_craftable_item();
        let Some(bh) = me.current_blockhead() else { return false };
        let controlled = bh.as_blockhead().is_some_and(|b| b.is_client_blockhead_being_controlled_by_server());
        let cm = CrystalManager::instance();
        for i in 0..c.number_of_required_items.max(0) as usize {
            if c.required_items[i] != ITEM_TIME_CRYSTAL as u32 {
                continue;
            }
            if controlled {
                continue;
            }
            let mut ccount = cm.amount();
            let ok = cm.amount_string().as_deref()
                == Some(string_from_md5(&format!("7acfe93afc08{}c65ae2c54ecaf07f", ccount)).as_str());
            if !ok || cm.amount_string().is_none() {
                ccount = 0;
            }
            let req = c.required_item_counts[i];
            if ccount < req {
                return false;
            }
            if req > 0 {
                let amt = cm.amount();
                let token = string_from_md5(&format!("7acfe93afc08c{}65ae2c54ecaf07f", (amt - req) + 0x49));
                cm.modify_modify_string(req, &token);
            }
        }
        let api = blockhead_inventory_api();
        for i in 0..c.number_of_required_items.max(0) as usize {
            if c.required_items[i] == ITEM_TIME_CRYSTAL as u32 {
                continue;
            }
            let req = c.required_item_counts[i];
            let removed =
                api.subtract_items_from_inventory_of_type_count_data_b(&bh, c.required_items[i] as i32, req, -1);
            if req - removed > 0 {
                return false;
            }
        }
        true
    }

    /// `customRules` bytes: `(customRulesEnabled, tradePortals)`.
    fn rules(&self) -> (bool, i32) {
        let r = self.links.world().custom_rules();
        (r.custom_rules_enabled != 0, r.trade_portals as i32)
    }

    /// `-actionTitle` (option 1; `None` = nil).
    pub fn action_title(&self) -> Option<String> {
        let (enabled, mode) = self.rules();
        if enabled {
            if mode == 1 {
                return None;
            }
            if mode == 3 {
                return Some("SELL".to_string());
            }
            if mode == 5 {
                return Some("JOB LISTINGS".to_string());
            }
        }
        Some("BUY".to_string())
    }

    /// `-secondOptionTitle`.
    pub fn second_option_title(&self) -> Option<String> {
        let (enabled, mode) = self.rules();
        if !enabled || !(1..=5).contains(&mode) {
            return Some("SELL".to_string());
        }
        if mode != 1 && mode != 5 {
            return Some("JOB LISTINGS".to_string());
        }
        None
    }

    /// `-thirdOptionTitle`.
    pub fn third_option_title(&self) -> Option<String> {
        let (enabled, mode) = self.rules();
        if !enabled || !(1..=5).contains(&mode) {
            return Some("JOB LISTINGS".to_string());
        }
        None
    }

    /// `-setWorkbenchChoiceUIOption:`: resets both flags first, then classifies the title. Live on the
    /// server (Blockhead `startInteractingWithTileAtIndex:...`, interaction type 7).
    pub fn set_workbench_choice_ui_option(&self, option: i32) {
        {
            let mut st = self.st.borrow_mut();
            st.is_sell_interaction = false;
            st.is_mission_interaction = false;
        }
        let title = match option {
            0 => Self::action_title(self),
            1 => self.second_option_title(),
            2 => self.third_option_title(),
            _ => None,
        };
        let mut st = self.st.borrow_mut();
        match title.as_deref() {
            Some("SELL") => st.is_sell_interaction = true,
            Some("JOB LISTINGS") => st.is_mission_interaction = true,
            _ => {}
        }
    }

    /// `-currentBlockheadCash`
    pub fn current_blockhead_cash(&self, me: &ObjRef) -> i32 {
        me.current_blockhead().map_or(0, |b| blockhead_inventory_api().total_cash(&b))
    }

    /// `-currentBlockheadCountOfInventoryItemsOfType:`
    pub fn current_blockhead_count_of_inventory_items_of_type(&self, me: &ObjRef, item_type: i32) -> i32 {
        let Some(b) = me.current_blockhead() else { return 0 };
        let api = blockhead_inventory_api();
        if item_type == ITEM_DODO_EGG {
            api.count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(
                &b,
                ITEM_DODO_EGG,
                0,
                false,
            )
        } else {
            api.count_of_inventory_items_of_type_include_actions(&b, item_type, false)
        }
    }

    /// `-currentBlockheadUsageMultiplierForFirstItemOfType:`
    pub fn current_blockhead_usage_multiplier_for_first_item_of_type(&self, me: &ObjRef, item_type: i32) -> f32 {
        me.current_blockhead()
            .map_or(1.0, |b| blockhead_inventory_api().usage_multiplier_for_first_item_of_type(&b, item_type))
    }
}

impl DynamicObject for TradePortal {
    fn base(&self) -> &RefCell<DynamicObjectBase> {
        &self.base
    }
    fn class_info(&self) -> &'static ClassInfo {
        &TRADE_PORTAL_CLASS
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_interaction_object(&self) -> Option<&RefCell<InteractionObjectState>> {
        Some(&self.io)
    }
    fn as_interaction_like(&self) -> Option<&dyn InteractionObjectLike> {
        Some(self)
    }
    fn should_add_to_macro_block(&self) -> bool {
        interaction_object_should_add_to_macro_block(self)
    }
    /// `-objectType`.
    fn object_type(&self) -> i32 {
        DYNAMIC_OBJECT_TYPE_TRADE_PORTAL
    }
    /// `-getLightRGB` (client light query; kept for completeness).
    fn get_light_rgb(&self) -> [f32; 3] {
        [255.0, 246.0, 64.0]
    }
    /// `-occupiesNormalContents`.
    fn occupies_normal_contents(&self) -> bool {
        true
    }
    /// `-freeblockCreationItemType`.
    fn freeblock_creation_item_type(&self) -> i32 {
        ITEM_TRADE_PORTAL
    }
    /// `-freeBlockCreationSaveDict`.
    fn free_block_creation_save_dict(&self, me: &ObjRef) -> Option<PlistDict> {
        Some(self.get_save_dict(me))
    }
    /// `-getSaveDict` (trap 11: the live offsets dictionary is copied).
    fn get_save_dict(&self, me: &ObjRef) -> PlistDict {
        let mut d = interaction_object_get_save_dict(me);
        let (offsets, level, light) = {
            let st = self.st.borrow();
            (st.local_price_offsets.clone(), st.level, st.light.clone())
        };
        if let Some(o) = offsets {
            d.map.insert("localPriceOffsets".to_string(), PlistValue::Dict(o));
        }
        d.map.insert("level".to_string(), PlistValue::int(level as i64));
        if let Some(l) = light {
            let lo: ObjRef = l;
            let ld = lo.get_save_dict(&lo);
            d.map.insert("lightDict".to_string(), PlistValue::Dict(ld));
        }
        d
    }
    /// `-creationNetDataForClient:` is not overridden: delegates to the virtual update (decision 2.4).
    fn creation_net_data_for_client(&self, me: &ObjRef, client: Option<&str>) -> Option<Vec<u8>> {
        self.update_net_data_for_client(me, client)
    }
    /// `-updateNetDataForClient:`.
    fn update_net_data_for_client(&self, me: &ObjRef, _client: Option<&str>) -> Option<Vec<u8>> {
        let mut s = TradePortalNetData::default();
        s.interaction_object_creation_net_data = interaction_object_interaction_object_creation_net_data(me);
        s.level = self.level() as u8;
        let mut out = s.to_vec();
        let mut dd = crate::objects::base::new_mutable_dict();
        if let Some(o) = me.owner_id() {
            dd.map.insert("ownerID".to_string(), PlistValue::string(o));
        }
        if let Some(n) = self.io.borrow().owner_name.clone() {
            dd.map.insert("ownerName".to_string(), PlistValue::string(n));
        }
        if let Some(o) = self.local_price_offsets() {
            dd.map.insert("localPriceOffsets".to_string(), PlistValue::Dict(o));
        }
        out.extend_from_slice(&gz_plist_bytes(&PlistValue::Dict(dd)));
        Some(out)
    }
    /// `-remoteUpdate:` (T5: the level-up branch is unguarded, 56 draws; the owner / offsets block always runs).
    fn remote_update(&self, me: &ObjRef, net_data: &[u8]) {
        interaction_object_remote_update(me, net_data);
        self.links.dw().dynamic_world_changed_at_pos_object_type(me.pos(), DYNAMIC_OBJECT_TYPE_TRADE_PORTAL);
        let upd = TradePortalNetData::read_le(&padded_bytes(net_data, 0x30));
        if self.level() < upd.level as i32 {
            self.st.borrow_mut().level = upd.level as i32;
            self.level_up_effects(me.pos());
            self.rewrite_base_stone(me.pos(), upd.level as i32);
        }
        if self.io.borrow().current_blockhead.is_none() {
            let dict = gunzip_plist(subdata_from(net_data, 0x30));
            me.set_owner_id(dict_str(dict.as_ref(), "ownerID"));
            self.io.borrow_mut().owner_name = dict_str(dict.as_ref(), "ownerName");
            if let Some(PlistValue::Dict(saved)) = dict.as_ref().and_then(|d| d.object_for_key("localPriceOffsets")) {
                self.st.borrow_mut().local_price_offsets = Some(crate::objects::base::new_mutable_dict());
                self.load_price_offsets(saved);
            }
        }
    }
    fn update_accurate_dt_is_simulation(&self, me: &ObjRef, dt: f32, accurate_dt: f32, is_simulation: bool) {
        if !self.base.borrow().needs_removed {
            interaction_object_update_accurate_dt_is_simulation(me, dt, accurate_dt, is_simulation);
        }
    }
    /// `-worldChanged:`: the light first (after the `isNet` return), then the support check (trap 9).
    fn world_changed(&self, me: &ObjRef, positions: &mut Vec<IntPair>) {
        if self.base.borrow().is_net {
            return;
        }
        let light = self.st.borrow().light.clone();
        if let Some(l) = light {
            l.world_changed_positions(positions);
        }
        let world = self.links.world();
        let pos = self.base.borrow().pos;
        if support_lost(&world, pos, positions) {
            self.remove(me, None);
        }
    }
    /// `-worldContentsChanged:` (empty).
    fn world_contents_changed(&self, _me: &ObjRef, _positions: &mut Vec<IntPair>) {}
    /// `-setNeedsRemoved:`.
    fn set_needs_removed(&self, _me: &ObjRef, v: bool) {
        self.base.borrow_mut().needs_removed = v;
        if v {
            let light = self.st.borrow().light.clone();
            if let Some(l) = light {
                l.remove_from_tiles();
            }
        }
    }
    /// `-addArtificialLightContributionForPhysicalBlockLoadedAtXPos:yPos:`.
    fn add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(
        &self,
        _me: &ObjRef,
        macro_x: i32,
        macro_y: i32,
    ) {
        let light = self.st.borrow().light.clone();
        if let Some(l) = light {
            l.add_contribution_for_physical_block_loaded_at_x_pos_y_pos(macro_x, macro_y);
        }
    }
    /// `-dealloc`: releases the offsets and the light.
    fn dealloc(&self, me: &ObjRef) {
        let light = {
            let mut st = self.st.borrow_mut();
            st.local_price_offsets = None;
            st.light.take()
        };
        if let Some(l) = light {
            l.dealloc_light();
        }
        interaction_object_dealloc(me)
    }
    fn blockheads_loaded(&self, me: &ObjRef) {
        interaction_object_blockheads_loaded(me)
    }
    fn blockhead_unloaded(&self, me: &ObjRef, b: &ObjRef) {
        interaction_object_blockhead_unloaded(me, b)
    }
}

impl InteractionObjectLike for TradePortal {
    fn io_state(&self) -> &RefCell<InteractionObjectState> {
        &self.io
    }
    fn interaction_object_type(&self) -> u16 {
        INTERACTION_OBJECT_TYPE_TRADE_PORTAL
    }
    fn is_double_height(&self) -> bool {
        true
    }
    fn destroy_item_type(&self) -> i32 {
        ITEM_TRADE_PORTAL
    }
    /// `-title`.
    fn title(&self) -> String {
        format!("TRADE PORTAL LEVEL {}", self.level() + 1)
    }
    /// `-actionTitle` (nil modelled as the empty string).
    fn action_title(&self) -> String {
        TradePortal::action_title(self).unwrap_or_default()
    }
    fn requires_human_interaction(&self) -> bool {
        true
    }
    fn can_be_used_in_expert_mode_when_not_owned(&self) -> bool {
        true
    }
    fn set_paused(&self, _me: &ObjRef, paused: bool) {
        self.st.borrow_mut().paused = paused;
    }
    /// `-remove:` (order: free block, tiles y, y-1, y+1, needsRemoved).
    fn remove(&self, me: &ObjRef, remove_blockhead: Option<&ObjRef>) {
        let (needs_removed, is_net) = {
            let b = self.base.borrow();
            (b.needs_removed, b.is_net)
        };
        if needs_removed || is_net {
            return;
        }
        let (in_use, cur, remote) = {
            let io = self.io.borrow();
            (io.is_in_use, io.current_blockhead.clone(), io.remote_blockhead_in_use_unique_id)
        };
        if in_use {
            if cur.as_ref().is_some_and(|c| c.is_net()) || remote != 0 {
                self.io.borrow_mut().needs_to_be_removed_when_interaction_ends = true;
                return;
            }
            if let Some(bh) = cur.as_ref().and_then(|c| c.as_blockhead()) {
                bh.stop_interacting();
            }
        }
        let pos = self.base.borrow().pos;
        let dict = self.get_save_dict(me);
        let (world, dw) = (self.links.world(), self.links.dw());
        dw.create_free_block_at_position_of_type_data_a_data_b_sub_items_dynamic_object_save_dict_hovers_play_sound_priority_blockhead(
            pos,
            ItemType(ITEM_TRADE_PORTAL as u32),
            0,
            0,
            None,
            Some(PlistValue::Dict(dict)),
            false,
            1,
            remove_blockhead,
        );
        for y in [pos.y, pos.y - 1, pos.y + 1] {
            world.remove_tile_at_world_x_world_y_create_contents_freeblock_count_create_foreground_contents_freeblock_count_remove_blockhead(pos.x, y, 0, 0, remove_blockhead);
        }
        self.set_needs_removed(me, true);
    }
}

/// `ITEM_CATEGORIES[9][6][..]` rand-order data of `randomizeLocalTradeOffsets`.
/// Rows are slices here (the original terminates each row with ITEM_NONE).
pub static ITEM_CATEGORIES: [[&[i32]; 6]; 9] = [
    [&[], &[], &[], &[], &[], &[]],
    [
        &[1048, 1049, 242, 251, 1024, 222, 229, 1051],
        &[1028, 223, 230, 1036, 225, 232, 1030, 224, 231, 1026, 243, 252],
        &[1094, 1075, 1062, 1103, 332, 333, 1042, 266, 265, 1076, 268, 267, 1060, 244, 253, 1057],
        &[1054, 227, 234, 1038, 226, 233, 1063, 228, 235],
        &[1066, 236, 245, 1067, 237, 246, 1068, 238, 247],
        &[
            1069, 239, 248, 1070, 240, 249, 1045, 241, 250, 1090, 284, 283, 1091, 282, 281, 1089, 264, 263, 1098, 334,
            339, 1099, 335, 340, 1100, 336, 341, 1101, 337, 342, 1102, 338, 343, 1105,
        ],
    ],
    [
        &[4, 155, 27],
        &[21, 22, 23, 24, 46, 54, 60, 160, 61, 62, 71, 77, 112, 310, 316],
        &[144, 55, 295, 91, 30, 29, 151],
        &[120, 121, 78, 311, 312, 313, 319, 321, 323],
        &[59, 152, 79, 314, 320],
        &[82, 81, 181, 182, 255, 315, 318, 317, 329, 328],
    ],
    [
        &[16, 6, 7, 8, 25],
        &[64, 33, 34],
        &[42, 40, 43, 49, 50],
        &[66, 67, 68, 70],
        &[209, 93, 322],
        &[89, 90, 286, 285, 257],
    ],
    [
        &[115, 117, 85, 84],
        &[122, 130, 124],
        &[126, 127, 154],
        &[189, 188, 187, 190],
        &[193, 192, 191, 194],
        &[197, 196, 195, 198, 293, 292, 291, 294, 289, 288, 287, 290],
    ],
    [
        &[3, 28, 48],
        &[51, 31, 36, 32, 305, 87],
        &[73, 35, 37, 306, 86],
        &[202, 201, 38, 307, 76],
        &[261, 279, 65, 308, 75],
        &[92, 72, 184, 185, 186, 262, 280, 309, 88],
    ],
    [
        &[12, 13, 14, 17],
        &[1043, 52, 69, 53, 58, 161, 163, 63, 304],
        &[169, 44, 47, 74, 80, 83, 102, 111, 133, 153, 156, 302],
        &[105, 150, 254, 258, 180, 56, 57, 99, 100, 101, 109, 110, 119, 331],
        &[1072, 1104, 1065, 164, 165, 174, 207, 158, 159, 183],
        &[145, 146, 147, 148, 149],
    ],
    [
        &[],
        &[],
        &[],
        &[],
        &[176, 175, 177, 178],
        &[179, 1077, 1082, 1083, 1087, 1088, 199, 203, 204, 205, 206, 208, 269, 272, 271, 273, 274, 275, 276, 277, 278],
    ],
    [&[260, 167, 1092, 166], &[], &[], &[], &[], &[]],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::sign::testkit::*;
    use crate::rng::glibc_rand::{rand, srand};

    fn mk(name: &str, sd: Option<PlistDict>) -> (Rc<World>, Rc<DynamicWorld>, Rc<TradePortal>) {
        let (w, d, pb) = world_with_block(name);
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let it = it.borrow();
        let p = TradePortal::new(
            &w,
            &d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it),
            false,
            sd.as_ref(),
            Some("cid"),
            Some("Bob"),
        )
        .unwrap();
        (w, d, p)
    }

    /// The table holds the 308 rand-order items of categories 1..8 (88+49+25+30+33+54+25+4... see).
    #[test]
    fn category_table_shape() {
        let total: usize = ITEM_CATEGORIES.iter().flat_map(|c| c.iter()).map(|l| l.len()).sum();
        assert_eq!(total, 308);
        assert!(ITEM_CATEGORIES[0].iter().all(|l| l.is_empty()));
        assert_eq!(ITEM_CATEGORIES[1][0][0], 1048);
        assert_eq!(ITEM_CATEGORIES[1][5].len(), 34);
        assert_eq!(ITEM_CATEGORIES[8][0], &[260, 167, 1092, 166]);
        let per_cat: Vec<usize> = ITEM_CATEGORIES.iter().map(|c| c.iter().map(|l| l.len()).sum()).collect();
        assert_eq!(per_cat, vec![0, 88, 49, 25, 30, 33, 54, 25, 4]);
    }

    /// First `n` draws of `srand(seed)` (the thread-local generator is re-seeded: call before the code under test).
    fn seq(seed: u32, n: usize) -> Vec<i32> {
        srand(seed);
        (0..n).map(|_| rand()).collect()
    }

    /// constructor = exactly 1 draw; randomize = 308 draws with the documented values.
    #[test]
    fn rng_draw_counts_and_values() {
        let (w, d, pb) = world_with_block("tpo_a");
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let it = it.borrow();
        let reference = seq(1, 400);
        srand(1);
        let p =
            TradePortal::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it), false, None, Some("cid"), Some("Bob"))
                .unwrap();
        assert_eq!(rand(), reference[1], "the constructor draws exactly once");
        assert_eq!(p.st.borrow().animation_loop_index, reference[0] % 8);
        let me: ObjRef = p.clone();
        let reference = seq(5, 400);
        srand(5);
        p.randomize_local_trade_offsets(&me);
        assert_eq!(rand(), reference[308], "308 draws");
        let offs = p.local_price_offsets().unwrap();
        assert_eq!(offs.map.len(), 308);
        // the first item of the table (ITEM_DIRT 1048) consumed the first draw
        let want = powf(0.8f32, (r01_from(reference[0]) - 0.5f32) * 10.0f32);
        let entry = offs.map.get(&"1048".to_string());
        let Some(PlistValue::Real { value, is_float }) = entry else { unreachable!() };
        assert!(*is_float);
        assert_eq!(*value as f32, want);
        assert!(me.update_needs_to_be_sent());
    }

    /// placement tiles, stone, save dict keys; load clamps offsets.
    #[test]
    fn placement_tiles_save_dict_and_clamp() {
        let (_w, _d, pb) = world_with_block("tpo_b");
        let mut sd = crate::objects::base::new_mutable_dict();
        let mut offs = crate::objects::base::new_mutable_dict();
        offs.map.insert("5".to_string(), PlistValue::real_f64(9.0));
        offs.map.insert("6".to_string(), PlistValue::real_f64(0.1));
        offs.map.insert("7".to_string(), PlistValue::real_f64(f64::NAN));
        offs.map.insert("8".to_string(), PlistValue::real_f64(1.25));
        sd.map.insert("localPriceOffsets".to_string(), PlistValue::Dict(offs));
        sd.map.insert("level".to_string(), PlistValue::int(2));
        tile(pb, 10, 9).set_contents(0x55);
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let it = it.borrow();
        let p = TradePortal::new(
            &_w,
            &_d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it),
            false,
            Some(&sd),
            Some("cid"),
            Some("Bob"),
        )
        .unwrap();
        let me: ObjRef = p.clone();
        assert_eq!(tile(pb, 10, 10).contents(), 0x30);
        assert_eq!(tile(pb, 10, 11).contents(), 0x30);
        assert_eq!(tile(pb, 10, 9).contents(), 0);
        assert_eq!(tile(pb, 10, 9).back_wall_type_index(), 10);
        assert_eq!(tile(pb, 10, 9).type_index(), 0x3e);
        assert!(p.st.borrow().light.is_some());
        let o = p.local_price_offsets().unwrap();
        let g = |k: &str| o.map.get(&k.to_string()).unwrap().double_value();
        assert_eq!((g("5"), g("6"), g("7"), g("8")), (2.0, 0.5, 0.5, 1.25));
        let d = me.get_save_dict(&me);
        for k in ["localPriceOffsets", "level", "lightDict", "interactionObjectType"] {
            assert!(d.map.get(&k.to_string()).is_some(), "{k}");
        }
        assert_eq!(me.object_type(), 50);
        assert_eq!(p.interaction_object_type(), 7);
        assert_eq!(p.destroy_item_type(), 210);
        assert!(p.is_double_height());
        assert_eq!(p.title(), "TRADE PORTAL LEVEL 3");
        // load: no stone rewrite, light from lightDict
        tile(pb, 10, 9).set_type_index(6);
        let p2 = TradePortal::from_save_dict(&_w, &_d, &d, None).unwrap();
        assert_eq!(tile(pb, 10, 9).type_index(), 6);
        assert!(p2.st.borrow().light.is_some());
        assert_eq!(p2.level(), 2);
    }

    /// remote update with a higher level draws 56 times and rewrites the stone; equal level draws none.
    #[test]
    fn remote_update_level_up() {
        let (_w, _d, p) = mk("tpo_c", None);
        let me: ObjRef = p.clone();
        let mut hdr = TradePortalNetData::default();
        hdr.level = 3;
        let mut upd = hdr.to_vec();
        let mut dict = PlistValue::mutable_dict();
        if let PlistValue::Dict(dd) = &mut dict {
            dd.map.insert("ownerID".to_string(), PlistValue::string("zed"));
            let mut o = crate::objects::base::new_mutable_dict();
            o.map.insert("9".to_string(), PlistValue::real_f64(3.0));
            dd.map.insert("localPriceOffsets".to_string(), PlistValue::Dict(o));
        }
        upd.extend_from_slice(&gz_plist_bytes(&dict));
        let reference = seq(7, 100);
        srand(7);
        p.remote_update(&me, &upd);
        assert_eq!(rand(), reference[56], "56 particle draws");
        assert_eq!(p.level(), 3);
        assert_eq!(me.owner_id().as_deref(), Some("zed"));
        assert_eq!(p.local_price_offsets().unwrap().map.get(&"9".to_string()).unwrap().double_value(), 2.0);
        // same level: no draws
        srand(7);
        p.remote_update(&me, &upd);
        assert_eq!(rand(), reference[0]);
    }

    /// option titles by rule mode and the flag classification.
    #[test]
    fn titles_and_choice_option() {
        let (w, _d, p) = mk("tpo_d", None);
        let set = |enabled: u8, mode: u8| {
            let mut r = w.custom_rules();
            r.custom_rules_enabled = enabled;
            r.trade_portals = mode as i8;
            w.set_custom_rules(r);
        };
        let t = |p: &TradePortal| (TradePortal::action_title(p), p.second_option_title(), p.third_option_title());
        let s = |x: &str| Some(x.to_string());
        set(0, 0);
        assert_eq!(t(&p), (s("BUY"), s("SELL"), s("JOB LISTINGS")));
        set(1, 0);
        assert_eq!(t(&p), (s("BUY"), s("SELL"), s("JOB LISTINGS")));
        set(1, 1);
        assert_eq!(t(&p), (None, None, None));
        set(1, 2);
        assert_eq!(t(&p), (s("BUY"), s("JOB LISTINGS"), None));
        set(1, 3);
        assert_eq!(t(&p), (s("SELL"), s("JOB LISTINGS"), None));
        set(1, 4);
        assert_eq!(t(&p), (s("BUY"), s("JOB LISTINGS"), None));
        set(1, 5);
        assert_eq!(t(&p), (s("JOB LISTINGS"), None, None));
        set(0, 0);
        p.set_workbench_choice_ui_option(1);
        assert!(p.is_sell_interaction() && !p.is_mission_interaction());
        p.set_workbench_choice_ui_option(2);
        assert!(!p.is_sell_interaction() && p.is_mission_interaction());
        p.set_workbench_choice_ui_option(0);
        assert!(!p.is_sell_interaction() && !p.is_mission_interaction());
        p.set_workbench_choice_ui_option(7);
        assert!(!p.is_sell_interaction() && !p.is_mission_interaction());
    }

    #[test]
    fn net_data_layout() {
        let (_w, _d, p) = mk("tpo_e", None);
        let me: ObjRef = p.clone();
        p.st.borrow_mut().level = 4;
        let b = me.creation_net_data_for_client(&me, None).unwrap();
        assert_eq!(b[0x28], 4);
        assert!(b[0x29..0x30].iter().all(|x| *x == 0));
        let d = gunzip_plist(&b[0x30..]).unwrap();
        assert_eq!(dict_str(Some(&d), "ownerID").as_deref(), Some("cid"));
        assert!(d.object_for_key("localPriceOffsets").is_some());
    }

    #[test]
    fn upgrade_recipe_and_portal_chest_manager() {
        let (w, _d, p) = mk("tpo_f", None);
        let c = p.upgrade_craftable_item();
        assert_eq!((c.crafted_item_type, c.crafted_item_count, c.number_of_required_items), (132, 1, 3));
        assert_eq!((c.required_items[0], c.required_item_counts[1], c.required_item_counts[2]), (87, 10, 10));
        p.st.borrow_mut().level = 3;
        let c = p.upgrade_craftable_item();
        assert_eq!(
            (c.required_items[0], c.required_items[1], c.required_item_counts[1], c.required_item_counts[2]),
            (75, 260, 1, 100)
        );
        w.set_expert_mode(true);
        let c = p.upgrade_craftable_item();
        assert_eq!((c.required_item_counts[0], c.required_item_counts[1]), (10, 20));
        assert!(PortalChestManager.portal_chest_inventory_items().is_none());
    }

    #[test]
    fn upgrade_to_next_level_caps_at_five() {
        let (_w, _d, p) = mk("tpo_g", None);
        let me: ObjRef = p.clone();
        for _ in 0..8 {
            p.upgrade_to_next_level(&me);
        }
        assert_eq!(p.level(), 5);
        assert_eq!(p.title(), "TRADE PORTAL LEVEL 6");
    }

    use crate::crafting::workbench_craft::{crystal_amount, set_crystal_wallet};
    use crate::interaction::trading_post::{set_blockhead_inventory_api, BlockheadInventoryApi};
    use crate::objects::base::{BlockheadApi, DynamicObjectBase};
    use crate::objects::class_info::DYNAMIC_OBJECT_CLASS;
    use crate::objects::interaction_object::testutil::TestBh;
    use std::cell::Cell;
    use std::collections::HashMap;

    /// A blockhead that may be controlled by the server.
    struct CtlBh {
        base: RefCell<DynamicObjectBase>,
        controlled: bool,
    }
    impl DynamicObject for CtlBh {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DYNAMIC_OBJECT_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_blockhead(&self) -> Option<&dyn BlockheadApi> {
            Some(self)
        }
    }
    impl BlockheadApi for CtlBh {
        fn client_id(&self) -> Option<String> {
            None
        }
        fn local_net_id(&self) -> Option<String> {
            None
        }
        fn is_client_blockhead_being_controlled_by_server(&self) -> bool {
            self.controlled
        }
        fn is_interacting_with_any_interaction_object(&self) -> bool {
            true
        }
        fn set_interaction_object(&self, _o: Option<&ObjRef>) {}
        fn stop_interacting(&self) {}
    }
    #[test]
    fn controlled_blockhead_double() {
        let b = ctl_bh(true);
        let api = b.as_blockhead().unwrap();
        assert_eq!(b.class_info().name, "DynamicObject");
        assert_eq!(b.base().borrow().unique_id, 0);
        assert!(b.as_any().downcast_ref::<CtlBh>().is_some());
        assert!(api.client_id().is_none() && api.local_net_id().is_none());
        assert!(api.is_client_blockhead_being_controlled_by_server());
        assert!(api.is_interacting_with_any_interaction_object());
        api.set_interaction_object(None);
        api.stop_interacting();
    }

    fn ctl_bh(controlled: bool) -> ObjRef {
        handle::alloc_obj(|w| CtlBh { base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)), controlled })
    }

    /// Stock and cash double; removing gives back `min(stock, count)` unless `short` is set.
    struct Stock {
        cash: Cell<i32>,
        spent: Cell<i32>,
        have: RefCell<HashMap<i32, i32>>,
        short: Cell<bool>,
        mult: Cell<f32>,
    }
    impl BlockheadInventoryApi for Stock {
        fn total_cash(&self, _b: &ObjRef) -> i32 {
            self.cash.get()
        }
        fn subtract_cash(&self, _b: &ObjRef, amount: i32) {
            self.spent.set(self.spent.get() + amount);
        }
        fn count_of_inventory_items_of_type_include_actions(&self, _b: &ObjRef, t: i32, _a: bool) -> i32 {
            self.have.borrow().get(&t).copied().unwrap_or(0)
        }
        fn count_of_inventory_items_with_specific_data_b_of_type_data_b_include_actions(
            &self,
            _b: &ObjRef,
            t: i32,
            data_b: u16,
            _a: bool,
        ) -> i32 {
            self.have.borrow().get(&t).copied().unwrap_or(0) + data_b as i32 + 1000
        }
        fn usage_multiplier_for_first_item_of_type(&self, _b: &ObjRef, _t: i32) -> f32 {
            self.mult.get()
        }
        fn subtract_items_from_inventory_of_type_count_data_b(&self, _b: &ObjRef, t: i32, count: i32, _d: i32) -> i32 {
            if self.short.get() {
                return count - 1;
            }
            let mut h = self.have.borrow_mut();
            let e = h.entry(t).or_insert(0);
            let n = (*e).min(count);
            *e -= n;
            n
        }
    }
    fn stock() -> Rc<Stock> {
        let s = Rc::new(Stock {
            cash: Cell::new(0),
            spent: Cell::new(0),
            have: RefCell::new(HashMap::new()),
            short: Cell::new(false),
            mult: Cell::new(1.0),
        });
        set_blockhead_inventory_api(s.clone());
        s
    }

    #[test]
    fn selling_to_the_portal() {
        let (_w, _d, p) = mk("tpo_sell", None);
        let me: ObjRef = p.clone();
        let st = stock();
        // nobody is there
        p.sell_item_at_total_price_count_usage_multiplier(&me, 5, 100, 2, 1.0);
        assert!(p.local_price_offsets().unwrap().map.get(&"5".to_string()).is_none());
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(3, false));
        // coins cannot be sold; a short stock aborts
        p.sell_item_at_total_price_count_usage_multiplier(&me, 166, 100, 2, 1.0);
        st.have.borrow_mut().insert(5, 3);
        st.short.set(true);
        p.sell_item_at_total_price_count_usage_multiplier(&me, 5, 100, 2, 1.0);
        assert!(p.local_price_offsets().unwrap().map.get(&"5".to_string()).is_none());
        st.short.set(false);
        // a proper sale lowers the item's price a little and pays platinum + copper
        p.sell_item_at_total_price_count_usage_multiplier(&me, 5, 25_007, 2, 0.5);
        let k = p.local_price_offsets().unwrap().map.get(&"5".to_string()).unwrap().double_value();
        assert!((k - 0.997).abs() < 1e-12);
        assert_eq!(st.have.borrow()[&5], 1);
        assert!(me.update_needs_to_be_sent());
        // dodo eggs only sell their plain kind; replicas and controlled blockheads earn no achievement
        st.have.borrow_mut().insert(91, 2);
        p.io.borrow_mut().current_blockhead = Some(ctl_bh(true));
        p.sell_item_at_total_price_count_usage_multiplier(&me, 91, 10, 1, 1.0);
        p.base.borrow_mut().is_net = true;
        p.io.borrow_mut().current_blockhead = Some(ctl_bh(false));
        p.sell_item_at_total_price_count_usage_multiplier(&me, 91, 10, 1, 1.0);
        assert_eq!(st.have.borrow()[&91], 0);
    }

    #[test]
    fn buying_from_the_portal() {
        let (_w, _d, p) = mk("tpo_buy", None);
        let me: ObjRef = p.clone();
        let st = stock();
        p.buy_item_at_total_price_count(&me, 5, 100, 1);
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(3, false));
        // too expensive
        p.buy_item_at_total_price_count(&me, 5, 100, 1);
        assert_eq!(st.spent.get(), 0);
        st.cash.set(1000);
        p.buy_item_at_total_price_count(&me, 5, 100, 2);
        assert_eq!(st.spent.get(), 100);
        let k = p.local_price_offsets().unwrap().map.get(&"5".to_string()).unwrap().double_value();
        assert!((k - 1.0 / 0.997).abs() < 1e-12);
        // sowable goods come with a full growth value; crystals are never handed out
        p.buy_item_at_total_price_count(&me, 22, 10, 1);
        p.buy_item_at_total_price_count(&me, 11, 10, 3);
        // controlled blockheads and replicas earn no achievement
        p.io.borrow_mut().current_blockhead = Some(ctl_bh(true));
        p.buy_item_at_total_price_count(&me, 5, 1, 1);
        p.base.borrow_mut().is_net = true;
        p.io.borrow_mut().current_blockhead = Some(ctl_bh(false));
        p.buy_item_at_total_price_count(&me, 5, 1, 1);
        // coin changing, all four directions: enough stock, short stock, a failing removal
        p.base.borrow_mut().is_net = false;
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(4, false));
        for (coin, give, stock_n) in [(166, 167, 5), (260, 167, 500), (1092, 166, 500), (167, 260, 5)] {
            st.have.borrow_mut().insert(give, stock_n);
            p.buy_item_at_total_price_count(&me, coin, 0, 1);
            assert!(st.have.borrow()[&give] < stock_n, "coin {coin}");
            st.have.borrow_mut().insert(give, 0);
            p.buy_item_at_total_price_count(&me, coin, 0, 1);
        }
        st.have.borrow_mut().insert(167, 5);
        st.short.set(true);
        p.buy_item_at_total_price_count(&me, 166, 0, 1);
        assert_eq!(st.have.borrow()[&167], 5);
    }

    #[test]
    fn paying_for_a_level_upgrade() {
        let (w, _d, p) = mk("tpo_upg", None);
        let me: ObjRef = p.clone();
        let st = stock();
        // levels have their own recipes, in normal and expert mode
        for level in 0..=5 {
            for expert in [false, true] {
                w.set_expert_mode(expert);
                p.st.borrow_mut().level = level;
                let c = p.upgrade_craftable_item();
                assert_eq!(c.crafted_item_type, 132);
                assert_eq!(c.required_item_counts[0], if expert { 10 } else { 1 });
            }
        }
        w.set_expert_mode(false);
        p.st.borrow_mut().level = 0;
        // nobody present
        assert!(!p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(3, false));
        // an untouched wallet carries no signature and counts as empty
        assert!(!p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        // not enough crystals
        set_crystal_wallet(5);
        assert!(!p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        // enough crystals but no items
        set_crystal_wallet(100);
        assert!(!p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        assert_eq!(crystal_amount(), 90);
        // everything there
        st.have.borrow_mut().extend([(87, 1), (167, 10)]);
        assert!(p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        assert_eq!(crystal_amount(), 80);
        // controlled blockheads pay no crystals
        p.io.borrow_mut().current_blockhead = Some(ctl_bh(true));
        st.have.borrow_mut().extend([(87, 1), (167, 10)]);
        assert!(p.take_items_from_blockhead_for_upgrade_to_next_level(&me));
        assert_eq!(crystal_amount(), 80);
    }

    #[test]
    fn blockhead_queries_and_object_views() {
        let (w, d, pb) = world_with_block("tpo_views");
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let p =
            TradePortal::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let me: ObjRef = p.clone();
        let st = stock();
        assert_eq!(p.current_blockhead_cash(&me), 0);
        assert_eq!(p.current_blockhead_count_of_inventory_items_of_type(&me, 5), 0);
        assert_eq!(p.current_blockhead_usage_multiplier_for_first_item_of_type(&me, 5), 1.0);
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(3, false));
        st.cash.set(42);
        st.mult.set(0.5);
        st.have.borrow_mut().insert(5, 7);
        assert_eq!(p.current_blockhead_cash(&me), 42);
        assert_eq!(p.current_blockhead_count_of_inventory_items_of_type(&me, 5), 7);
        assert_eq!(p.current_blockhead_count_of_inventory_items_of_type(&me, 91), 1000);
        assert_eq!(p.current_blockhead_usage_multiplier_for_first_item_of_type(&me, 5), 0.5);
        // object hooks
        assert_eq!(me.class_info().name, "TradePortal");
        assert!(me.as_any().downcast_ref::<TradePortal>().is_some());
        assert_eq!(me.get_light_rgb(), [255.0, 246.0, 64.0]);
        assert!(me.occupies_normal_contents() && me.should_add_to_macro_block());
        assert_eq!(me.freeblock_creation_item_type(), 210);
        assert!(me.free_block_creation_save_dict(&me).is_some());
        assert_eq!(InteractionObjectLike::action_title(&*p), "BUY");
        assert!(p.requires_human_interaction() && p.can_be_used_in_expert_mode_when_not_owned());
        p.set_paused(&me, true);
        assert!(p.st.borrow().paused);
        assert!(p.io_state().borrow().current_blockhead.is_some());
        me.update_accurate_dt_is_simulation(&me, 0.1, 0.1, false);
        me.world_contents_changed(&me, &mut vec![]);
        me.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&me, 0, 0);
        me.blockheads_loaded(&me);
        let other: ObjRef = TestBh::new(9, false);
        me.blockhead_unloaded(&me, &other);
        // a replica keeps its place, a light-less one tolerates the hooks
        let nd = me.update_net_data_for_client(&me, None).unwrap();
        let (bw, _bd, bdw) = crate::blockhead::blockhead::testutil::setup();
        d.blockheads.borrow_mut().push(crate::blockhead::blockhead::testutil::new_bh(&bw, &bdw, 0));
        let rep = TradePortal::from_net_data(&w, &d, None, &nd).unwrap();
        d.blockheads.borrow_mut().clear();
        let rme: ObjRef = rep.clone();
        assert!(rep.st.borrow().light.is_none());
        rme.add_artificial_light_contribution_for_physical_block_loaded_at_x_pos_y_pos(&rme, 0, 0);
        rme.world_changed(&rme, &mut vec![IntPair { x: 10, y: 9 }]);
        rep.remove(&rme, None);
        let mut far = nd.clone();
        far[8..12].copy_from_slice(&900u32.to_le_bytes());
        far[12..16].copy_from_slice(&900u32.to_le_bytes());
        assert!(TradePortal::from_net_data(&w, &d, None, &far).is_none());
        // remote updates with the user present keep the owner; dealloc releases the light
        p.remote_update(&me, &nd);
        me.dealloc(&me);
        assert!(p.st.borrow().light.is_none() && p.local_price_offsets().is_none());
    }

    #[test]
    fn support_and_removal() {
        let (w, d, pb) = world_with_block("tpo_rm");
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let p =
            TradePortal::new(&w, &d, IntPair { x: 10, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let me: ObjRef = p.clone();
        // a user from another machine postpones the removal
        p.io.borrow_mut().is_in_use = true;
        p.io.borrow_mut().current_blockhead = Some(TestBh::new(2, true));
        p.remove(&me, None);
        assert!(!me.needs_removed());
        p.io.borrow_mut().current_blockhead = None;
        p.io.borrow_mut().remote_blockhead_in_use_unique_id = 8;
        p.remove(&me, None);
        assert!(!me.needs_removed());
        p.io.borrow_mut().remote_blockhead_in_use_unique_id = 0;
        // a local user is stopped; the portal's light goes with it
        let local = TestBh::new(3, false);
        p.io.borrow_mut().current_blockhead = Some(local.clone());
        p.remove(&me, None);
        assert!(me.needs_removed());
        assert!(local.log.borrow().iter().any(|l| l == "stop"));
        p.remove(&me, None);
        // support lost through the world
        let q =
            TradePortal::new(&w, &d, IntPair { x: 20, y: 10 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let qm: ObjRef = q.clone();
        // the portal built its own base stone: take it away again
        tile(pb, 20, 9).set_type_index(2);
        tile(pb, 20, 9).set_back_wall_type_index(2);
        tile(pb, 20, 10).set_back_wall_type_index(2);
        qm.world_changed(&qm, &mut vec![IntPair { x: 20, y: 9 }]);
        assert!(qm.needs_removed());
        // a portal without a stone tile below (unloaded) still works
        let edge =
            TradePortal::new(&w, &d, IntPair { x: 24, y: 0 }, None, Some(&it.borrow()), false, None, Some("c"), None)
                .unwrap();
        let em: ObjRef = edge.clone();
        edge.upgrade_to_next_level(&em);
        assert_eq!(edge.level(), 1);
        em.set_needs_removed(&em, false);
        assert!(!em.needs_removed());
    }

    #[test]
    fn levels_outside_the_stone_table_and_relighting() {
        let (w, d, pb) = world_with_block("tpo_lvl");
        tile(pb, 10, 9).set_back_wall_type_index(2);
        let mut sd = crate::objects::base::new_mutable_dict();
        sd.map.insert("level".to_string(), PlistValue::int(9));
        let it = InventoryItem::new(ItemType(210), 0, 0, None, None);
        let p = TradePortal::new(
            &w,
            &d,
            IntPair { x: 10, y: 10 },
            None,
            Some(&it.borrow()),
            false,
            Some(&sd),
            Some("c"),
            None,
        )
        .unwrap();
        // the base stone keeps its type
        assert_eq!(tile(pb, 10, 9).type_index(), 2);
        let me: ObjRef = p.clone();
        assert!(p.st.borrow().light.is_some());
        p.update_portal_light(&me);
        assert!(p.st.borrow().light.is_some());
        assert_eq!(base_stone_for_level(9), None);
        assert_eq!(base_stone_for_level(-1), None);
        assert_eq!(base_stone_for_level(0), Some(0x3c));
    }
}
