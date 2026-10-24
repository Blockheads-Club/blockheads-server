//! Test fixture for the production hook adapters: a `Glue` / `BlockheadGlue` on a small real world + DynamicWorld,
//! and one live object of each class the adapters forward to.

use std::any::Any;
use std::rc::{Rc, Weak};

use super::{BlockheadGlue, Glue, RealDw};
use crate::blockhead::blockhead::{Blockhead, FishingRodRef};
use crate::items::inventory_item::{InventoryItem, ItemRef};
use crate::objects::base::DynamicWorldRef;
use crate::objects::handle::ObjRef;
use crate::plist::value::{PlistDict, PlistValue};
use crate::server::bhserver::test_support::Harness;
use crate::types::item_type::ItemType;
use crate::types::structs::{IntPair, Vector2};
use crate::world::state::{PhysicalBlockRef, TileRef, World};

pub(crate) struct Fx {
    /// Keeps the server (and its world) alive for the fixture's lifetime.
    pub _h: Harness,
    pub g: Rc<Glue>,
    pub bg: BlockheadGlue,
    /// Glue with neither World nor DynamicWorld (and its BlockheadGlue).
    pub dead: Rc<Glue>,
    pub dead_bg: BlockheadGlue,
    /// BlockheadGlue whose Glue is gone.
    pub gone: BlockheadGlue,
    pub world: Rc<World>,
    pub dw: Rc<RealDw>,
    pub bh_obj: ObjRef,
    /// An object of no special class.
    pub stub: ObjRef,
    pub objs: Vec<(&'static str, ObjRef)>,
    pub p: IntPair,
    pub v: Vector2,
    pub tile: TileRef,
    pub pb: PhysicalBlockRef,
    pub item: ItemRef,
    pub pv: PlistValue,
    pub dict: PlistDict,
    pub action: Rc<dyn Any>,
    pub rod: FishingRodRef,
}

impl Fx {
    pub fn bh(&self) -> &Blockhead {
        crate::blockhead::blockhead::as_blockhead(&self.bh_obj).unwrap()
    }
    pub fn obj(&self, name: &str) -> &ObjRef {
        &self.objs.iter().find(|(n, _)| *n == name).unwrap().1
    }
    pub fn dwr(&self) -> DynamicWorldRef {
        self.dw.clone()
    }
}

/// Build the fixture (the seams are thread-locals: run on a fresh thread).
pub(crate) fn fx(tag: &str) -> Fx {
    crate::objects::registry::install();
    let (world, _mock, _) = crate::blockhead::blockhead::testutil::setup();
    *world.save_id.borrow_mut() = Some(tag.to_string());
    std::mem::forget(_mock);
    let dw = crate::blockhead::interaction_rules::testhost::real_dw(&world, tag);
    let h = Harness::new(tag);
    let g = Glue::new(&h.srv);
    *g.world.borrow_mut() = Rc::downgrade(&world);
    *g.dw.borrow_mut() = Some(dw.clone());
    g.install_pre_init(); // the production seams, as the server installs them
    let bg = BlockheadGlue(Rc::downgrade(&g));
    let dead = Glue::new(&h.srv);
    let dead_bg = BlockheadGlue(Rc::downgrade(&dead));
    let gone = BlockheadGlue(Weak::new());
    let dwr: DynamicWorldRef = dw.clone();
    let bh_obj = crate::blockhead::blockhead::testutil::new_bh(&world, &dwr, 0);
    let stub = crate::world::helpers::testutil::new_obj();
    let p = IntPair { x: 5, y: 9 };
    let tile = crate::blockhead::blockhead::testutil::tile(&world, 5, 9);
    let pb = world.macro_tiles.get(0).unwrap().physical_block().unwrap();
    let item = InventoryItem::new(ItemType(1048), 0, 0, None, None);
    let mut dict = crate::objects::base::new_mutable_dict();
    dict.map.insert("outputImageData".into(), PlistValue::data(vec![1, 2, 3]));
    let pv = PlistValue::Dict(dict.clone());
    let action = crate::blockhead::action::Action::init_with_save_dict_inventory_items(&dict, &[]);
    let cast = crate::physics_objects::fishing_rod::FishingRod::init_with_world_blockhead_cache(&world, &bh_obj, None);
    cast.directional_swipe(Vector2 { x: 400.0, y: 300.0 }); // a cast rod has a line
    let rod: FishingRodRef = cast;
    let objs = make_objects(&g, &world, &dw, &bh_obj, tile);
    Fx {
        _h: h,
        g,
        bg,
        dead,
        dead_bg,
        gone,
        world,
        dw,
        bh_obj,
        stub,
        objs,
        p,
        v: Vector2 { x: 5.5, y: 9.5 },
        tile,
        pb,
        item,
        pv,
        dict,
        action,
        rod,
    }
}

/// One live object per class, built through the production constructors where the glue has one.
fn make_objects(
    g: &Rc<Glue>,
    world: &Rc<World>,
    dw: &Rc<RealDw>,
    bh: &ObjRef,
    tile: TileRef,
) -> Vec<(&'static str, ObjRef)> {
    use crate::dynworld::lookups::DwOpsHooks;
    let at = |x: i32| IntPair { x, y: 9 };
    let ops: &dyn DwOpsHooks = &**g;
    for (x, y) in [(17, 9), (17, 10), (6, 9), (6, 8)] {
        crate::blockhead::blockhead::testutil::tile(world, x, y).set_type_index(3);
        // water: fish, boat
    }
    let mut v: Vec<(&'static str, Option<ObjRef>)> = vec![
        ("torch", ops.new_torch(dw, at(1), 17, 0, 0, None, Some("cid"))),
        ("egg", ops.new_egg(dw, at(2), None)),
        ("painting", ops.new_painting(dw, at(3), 212, None, Some("cid"), Some("Name"))),
        ("rail", ops.new_rail(dw, at(4), 199, false)),
        ("boat", ops.new_boat(dw, at(6), None, Some("cid"))),
        ("hand_car", ops.new_train_car(dw, 204, at(7), None, Some("cid"))),
        ("steam_train", ops.new_train_car(dw, 205, at(8), None, Some("cid"))),
        ("freight_car", ops.new_train_car(dw, 206, at(10), None, Some("cid"))),
        ("passenger_car", ops.new_train_car(dw, 208, at(11), None, Some("cid"))),
        ("glow", ops.new_glow_block(dw, at(12), tile)),
        ("workbench", ops.new_workbench(dw, at(13), 1, false, None, Some("cid"), Some("Name"))),
        ("chest", ops.new_chest(dw, at(14), 0x413, false, None, Some("cid"), Some("Name"))),
        ("trade_portal", ops.new_trade_portal(dw, at(15), 210, false, None, Some("cid"), Some("Name"))),
        ("dodo", ops.new_npc(dw, 1, at(16), None, true, false, None)),
        ("clownfish", {
            let mut sd = crate::objects::base::new_mutable_dict();
            sd.map.insert("pos_x".into(), PlistValue::int(17));
            sd.map.insert("pos_y".into(), PlistValue::int(9));
            let w: crate::objects::base::WorldRef = world.clone();
            let d: DynamicWorldRef = dw.clone();
            crate::npc::clown_fish::ClownFish::init_with_world_dynamic_world_save_dict_cache(&w, &d, &sd, None)
        }),
        ("plant", ops.new_plant(dw, 1, at(18), 0, 0, true)),
        ("tree", ops.new_tree(dw, 1, at(19), 5, 5, true, 100.0)),
        (
            "free_block",
            crate::physics_objects::free_block::FreeBlock::new(
                world,
                dw,
                at(20),
                1048,
                0,
                0,
                None,
                None,
                None,
                true,
                None,
            ),
        ),
    ];
    for (name, ot, ty) in [
        ("ladder", 19, 53),
        ("column", 53, 222),
        ("stairs", 54, 229),
        ("wire", 38, 178),
        ("elevator_shaft", 56, 1087),
        ("elevator_motor", 55, 1088),
        ("window", 31, 58),
    ] {
        let x = 21 + v.len() as i32;
        dw.add_standard_object_at_pos_object_type_item_type_save_dict_placed_by_client(
            at(x),
            ot,
            ty,
            None,
            Some("cid"),
        );
        v.push((name, dw.object_of_type_at_pos(ot, at(x))));
    }
    let bed_item = InventoryItem::new(ItemType(63), 0, 0, None, None);
    let bed = crate::building::bed::Bed::init_with_world_dynamic_world_at_position_cache_item_flipped_save_dict_placed_by_client_client_name(
        world,
        dw,
        at(31),
        None,
        Some(&bed_item.borrow()),
        false,
        None,
        Some("cid"),
        None,
    );
    v.push(("bed", bed));
    let door =
        crate::building::door::Door::init_with_world_dynamic_world_at_position_cache_type_save_dict_placed_by_client(
            world,
            dw,
            at(30),
            None,
            304,
            None,
            Some("cid"),
        )
        .map(|d| d as ObjRef);
    v.push(("gate", door));
    let _ = bh;
    v.into_iter().map(|(n, o)| (n, o.unwrap())).collect()
}
