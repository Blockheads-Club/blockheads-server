//! `WirePathCreator`: the uniform-cost flood fill that connects a power consumer to power sources through
//! electricity-conducting tiles, plus the wire-path `testTile`.
//!
//!
//! `ParticleEmitter.addElectricityParticleWithPath:size:` is a NO-OP on the dedicated server: no
//! net message 0x2c is emitted and the particle path is not built (building it only re-reads already-known
//! slots and loaded tiles, which is unobservable).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::foundation::indexset::IndexSet;
use crate::objects::handle::ObjRef;
use crate::types::structs::IntPair;
use crate::world::helpers::tile_conducts_electricity;
use crate::world::state::{TileRef, World};
use crate::world::tiles::{get_world_pos_for_world_index, tile_at_world_index_loaded, world_index_at_world_position};

use super::path_creator::{dw_workbench_at_pos, ns_index};

/// Maximum number of derived-tile slots per search (`derivedTileCount == 0x1ff` => NULL).
pub const MAX_WIRE_TILES: usize = 0x1ff;

/// The `Workbench` messages the wire path sends (`availableElectricity`, `generatesElectricity`,
/// `isStorageDevice`, `conductsElectricity`, `subtractElectricty:`). Implemented by the workbench class
/// (building_objects, /) and handed out by [`super::path_creator::PathDynamicWorldHooks::workbench_at_pos`].
pub trait WirePowerSource {
    /// `-availableElectricity` (uint16)
    fn available_electricity(&self) -> u16;
    /// `-generatesElectricity`
    fn generates_electricity(&self) -> bool;
    /// `-isStorageDevice`
    fn is_storage_device(&self) -> bool;
    /// `-conductsElectricity`
    fn conducts_electricity(&self) -> bool;
    /// `-subtractElectricty:` (original spelling)
    fn subtract_electricty(&self, amount: u16) -> bool;
}

/// `WirePathTileProperties` (0xc): `F`, `G`, `parentIndex`.
#[derive(Clone, Copy, Default, Debug)]
pub struct WirePathTileProperties {
    pub f: i32,
    pub g: i32,
    pub parent_index: i32,
}

struct WireState {
    open: Option<IndexSet>,
    closed: Option<IndexSet>,
    start_index: i32,
    /// index 0 unused (slot numbers start at 1)
    slots: Vec<WirePathTileProperties>,
    count: usize,
    map: HashMap<i32, usize>,
}

/// Owned by DynamicWorld (`wirePathCreator`).
pub struct WirePathCreator {
    world: Weak<World>,
    st: RefCell<WireState>,
}

impl WirePathCreator {
    /// `-initWithWorld:`.
    pub fn init_with_world(world: &Rc<World>) -> Rc<WirePathCreator> {
        Rc::new(WirePathCreator {
            world: Rc::downgrade(world),
            st: RefCell::new(WireState {
                open: None,
                closed: None,
                start_index: 0,
                slots: vec![WirePathTileProperties::default(); MAX_WIRE_TILES + 1],
                count: 0,
                map: HashMap::new(),
            }),
        })
    }

    /// `-dealloc`.
    pub fn dealloc(&self) {
        let mut st = self.st.borrow_mut();
        st.open = None;
        st.closed = None;
        st.map.clear();
        st.count = 0;
    }

    /// `-tileDerivedPropertiesAtWorldIndex:`: the cap check (`count == 0x1ff`) comes FIRST, so once 511 slots are
    /// used even known indices yield `None`. Returns the slot number (1-based).
    pub fn tile_derived_properties_at_world_index(&self, world_index: i32) -> Option<usize> {
        let mut st = self.st.borrow_mut();
        if st.count == MAX_WIRE_TILES {
            return None;
        }
        let slot = st.map.get(&world_index).copied().unwrap_or(0);
        if slot == 0 {
            st.count += 1;
            let n = st.count;
            st.map.insert(world_index, n);
            Some(n)
        } else {
            Some(slot)
        }
    }

    /// Copy of a slot (test/inspection helper).
    pub fn slot(&self, s: usize) -> WirePathTileProperties {
        self.st.borrow().slots[s]
    }

    fn with_slot<R>(&self, s: usize, f: impl FnOnce(&mut WirePathTileProperties) -> R) -> R {
        f(&mut self.st.borrow_mut().slots[s])
    }

    /// `-findAndSubtractAllPowerUpTo:forUser:`: drains up to `required_power` from the reachable sources and
    /// returns the amount actually drained (`requiredPower - amountLeft`).
    pub fn find_and_subtract_all_power_up_to_for_user(&self, required_power: u16, user: Option<&ObjRef>) -> i32 {
        let world = self.world.upgrade().expect("World already deallocated");
        let mut open = IndexSet::new();
        let mut closed = IndexSet::new();
        {
            let mut st = self.st.borrow_mut();
            st.open = None;
            st.closed = None;
            st.map.clear();
            st.count = 0;
        }
        let pos = user.map_or(IntPair { x: 0, y: 0 }, |u| u.pos());
        let consumer_is_storage_device = user.and_then(|u| u.as_power_user()).is_some_and(|p| p.is_storage_device());

        let start_index = world_index_at_world_position(pos.x, pos.y, &world);
        self.st.borrow_mut().start_index = start_index;
        let finish = |this: &WirePathCreator, open: IndexSet, closed: IndexSet, r: i32| {
            let mut st = this.st.borrow_mut();
            st.open = Some(open);
            st.closed = Some(closed);
            r
        };
        let start_tile = tile_at_world_index_loaded(start_index, &world);
        if start_tile.is_none() {
            return finish(self, open, closed, 0);
        }
        // the slot count was reset above, so the cap check cannot refuse the first index
        let sp = self.tile_derived_properties_at_world_index(start_index).expect("slot cap on an empty map");
        self.with_slot(sp, |p| {
            p.g = 0;
            p.f = 0;
            p.parent_index = -1;
        });
        open.add_index(ns_index(start_index));
        let mut amount_left: u16 = required_power;

        loop {
            if open.count() == 0 {
                return finish(self, open, closed, required_power as i32 - amount_left as i32);
            }
            // lowest F first; ties: lowest world index (ascending enumeration, strict '<')
            let mut lowest_f: i32 = 9999999;
            let mut lowest_idx: i32 = 0;
            let mut lowest_tile: Option<TileRef> = None;
            let mut lowest_slot: Option<usize> = None;
            let indices: Vec<u64> = open.iter().collect();
            for idx in indices {
                let idx = idx as i32;
                let tile = tile_at_world_index_loaded(idx, &world);
                let props = self.tile_derived_properties_at_world_index(idx);
                if let (Some(s), Some(_)) = (props, tile) {
                    let f = self.slot(s).f;
                    if f < lowest_f {
                        lowest_f = f;
                        lowest_idx = idx;
                        lowest_tile = tile;
                        lowest_slot = Some(s);
                    }
                }
            }
            let Some(lowest_slot) = lowest_slot else {
                return finish(self, open, closed, required_power as i32 - amount_left as i32);
            };
            let b = get_world_pos_for_world_index(lowest_idx, &world);
            open.remove_index(ns_index(lowest_idx));
            closed.add_index(ns_index(lowest_idx));
            for (nx, ny) in [
                (b.x.wrapping_sub(1), b.y),
                (b.x.wrapping_add(1), b.y),
                (b.x, b.y.wrapping_sub(1)),
                (b.x, b.y.wrapping_add(1)),
            ] {
                let wb = self.wire_test_tile(
                    nx,
                    ny,
                    &mut open,
                    &mut closed,
                    lowest_idx,
                    lowest_tile,
                    lowest_slot,
                    b.x,
                    b.y,
                    10,
                    &world,
                    consumer_is_storage_device,
                    user,
                );
                if let Some(wb) = wb {
                    let avail = wb.available_electricity();
                    // only a source with at least the 10 units the tile test asks for gets here, and `amount_left`
                    // is not zero inside the loop, so something is always taken
                    let take = avail.min(amount_left);
                    wb.subtract_electricty(take);
                    amount_left -= take;
                    // addElectricityParticleWithPath:size: is a no-op on the dedicated server
                    if amount_left == 0 {
                        return finish(self, open, closed, required_power as i32);
                    }
                }
            }
        }
    }

    /// The wire-path `testTile`. `parentX/parentY` are unused in the original. Returns the
    /// power-source workbench found on this tile, if any.
    #[allow(clippy::too_many_arguments)]
    fn wire_test_tile(
        &self,
        test_x: i32,
        test_y: i32,
        open_list: &mut IndexSet,
        closed_list: &mut IndexSet,
        parent_index: i32,
        parent_tile: Option<TileRef>,
        parent: usize,
        _parent_x: i32,
        _parent_y: i32,
        required_power: u16,
        world: &Rc<World>,
        consumer_is_storage_device: bool,
        user: Option<&ObjRef>,
    ) -> Option<Rc<dyn WirePowerSource>> {
        let mut result: Option<Rc<dyn WirePowerSource>> = None;
        let idx = world_index_at_world_position(test_x, test_y, world);
        if closed_list.contains_index(ns_index(idx)) {
            return None;
        }
        let tile = tile_at_world_index_loaded(idx, world)?;
        let props = self.tile_derived_properties_at_world_index(idx)?;
        let mut passable = false;
        if tile_conducts_electricity(Some(tile)) {
            passable = true;
        } else if tile.contents() == 0x2e || tile.contents() == 0x2f {
            // CONTENTS_WORKBENCH / CONTENTS_WORKBENCH_SPRITE
            let wb = dw_workbench_at_pos(user, IntPair { x: test_x, y: test_y });
            match wb {
                // messages to nil return 0: availableElectricity 0 < required => passable = conducts(nil) = NO
                None => passable = false,
                Some(wb) => {
                    if wb.available_electricity() < required_power {
                        passable = wb.conducts_electricity();
                    } else {
                        let mut done = false;
                        if !wb.generates_electricity() && (!wb.is_storage_device() || consumer_is_storage_device) {
                            passable = wb.conducts_electricity();
                            done = true;
                        }
                        if !done {
                            passable = true;
                            result = Some(wb);
                        }
                    }
                }
            }
        }
        if !open_list.contains_index(ns_index(idx)) && passable {
            open_list.add_index(ns_index(idx));
            let parent_g = if parent_tile.is_some() { self.slot(parent).g } else { 0 };
            self.with_slot(props, |p| {
                p.parent_index = parent_index;
                p.g = parent_g.wrapping_add(10);
                p.f = p.g;
            });
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::path_creator::testutil::*;
    use crate::path::path_creator::{set_dynamic_world_hooks, PathDynamicWorldHooks};
    use crate::world::tiles::tile_at_world_position_loaded;
    use std::cell::Cell;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    struct Wb {
        avail: Cell<u16>,
        generates: bool,
        storage: bool,
        conducts: bool,
        taken: RefCell<Vec<u16>>,
    }
    impl WirePowerSource for Wb {
        fn available_electricity(&self) -> u16 {
            self.avail.get()
        }
        fn generates_electricity(&self) -> bool {
            self.generates
        }
        fn is_storage_device(&self) -> bool {
            self.storage
        }
        fn conducts_electricity(&self) -> bool {
            self.conducts
        }
        fn subtract_electricty(&self, amount: u16) -> bool {
            self.avail.set(self.avail.get() - amount);
            self.taken.borrow_mut().push(amount);
            true
        }
    }

    struct Hooks {
        wbs: RefCell<Vec<((i32, i32), Rc<Wb>)>>,
    }
    impl PathDynamicWorldHooks for Hooks {
        fn has_lights_to_add(&self) -> bool {
            false
        }
        fn blockhead_occupies_tile_at_pos_ignore_blockhead(&self, _p: IntPair, _i: Option<&ObjRef>) -> bool {
            false
        }
        fn workbench_at_pos(&self, pos: IntPair) -> Option<Rc<dyn WirePowerSource>> {
            self.wbs
                .borrow()
                .iter()
                .find(|(p, _)| *p == (pos.x, pos.y))
                .map(|(_, w)| w.clone() as Rc<dyn WirePowerSource>)
        }
    }

    fn wb(avail: u16, generates: bool, storage: bool, conducts: bool) -> Rc<Wb> {
        Rc::new(Wb { avail: Cell::new(avail), generates, storage, conducts, taken: RefCell::new(vec![]) })
    }

    /// consumer workbench at (100,21); wire tiles (contents 0x60) at x 101..=102; source workbench tile at 103.
    fn setup(src: Rc<Wb>, wire_len: i32) -> (Rc<World>, Rc<WirePathCreator>, ObjRef, Rc<Hooks>) {
        let (world, _l, _d) = new_world(32, 20);
        for x in 101..101 + wire_len {
            edit(&world, x, 21, |t| t.contents = 0x60);
        }
        edit(&world, 101 + wire_len, 21, |t| t.contents = 0x2e);
        let hooks = Rc::new(Hooks { wbs: RefCell::new(vec![((101 + wire_len, 21), src)]) });
        set_dynamic_world_hooks(Some(hooks.clone()));
        let wp = WirePathCreator::init_with_world(&world);
        let user = new_user(ip(100, 21));
        (world, wp, user, hooks)
    }

    #[test]
    fn drains_from_generator() {
        let src = wb(100, true, false, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(30, Some(&user)), 30);
        assert_eq!(src.avail.get(), 70);
        assert_eq!(*src.taken.borrow(), vec![30]);
        set_dynamic_world_hooks(None);
    }

    #[test]
    fn partial_drain_returns_what_was_taken() {
        let src = wb(25, true, false, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        // takes min(avail, left) repeatedly from the same workbench until it is exhausted
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(100, Some(&user)), 25);
        assert_eq!(src.avail.get(), 0);
        set_dynamic_world_hooks(None);
    }

    #[test]
    fn storage_rules() {
        // a battery (storage, non-generating) supplies a normal consumer...
        let src = wb(100, false, true, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(10, Some(&user)), 10);
        // ... but not a storage-device consumer
        let src = wb(100, false, true, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        user_knobs(&user).storage.set(true);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(10, Some(&user)), 0);
        assert_eq!(src.avail.get(), 100);
        // a generator supplies a storage consumer
        let src = wb(100, true, false, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        user_knobs(&user).storage.set(true);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(10, Some(&user)), 10);
        // sources with available < 10 are conductors only
        let src = wb(9, true, false, true);
        let (_w, wp, user, _h) = setup(src.clone(), 2);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(5, Some(&user)), 0);
        set_dynamic_world_hooks(None);
    }

    /// A workbench tile the dynamic world knows nothing about conducts nothing; `dealloc` empties the search state.
    #[test]
    fn unknown_workbench_and_dealloc() {
        let src = wb(100, true, false, true);
        let (_w, wp, user, hooks) = setup(src.clone(), 2);
        hooks.wbs.borrow_mut().clear();
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(10, Some(&user)), 0);
        wp.dealloc();
        assert_eq!(wp.tile_derived_properties_at_world_index(7), Some(1));
        assert!(!hooks.has_lights_to_add());
        assert!(!hooks.blockhead_occupies_tile_at_pos_ignore_blockhead(ip(0, 0), None));
        set_dynamic_world_hooks(None);
    }

    #[test]
    fn slot_cap_and_nil_start() {
        // a wire 600 tiles long: the 511-slot cap stops the search before the source
        let src = wb(100, true, false, true);
        let (_w, wp, user, _h) = setup(src.clone(), 600);
        assert_eq!(wp.find_and_subtract_all_power_up_to_for_user(10, Some(&user)), 0);
        assert_eq!(src.avail.get(), 100);
        // cap semantics: after 511 slots even known indices yield None
        let wp2 = WirePathCreator::init_with_world(&_w);
        for i in 0..511 {
            assert!(wp2.tile_derived_properties_at_world_index(i).is_some());
        }
        assert!(wp2.tile_derived_properties_at_world_index(0).is_none());
        // nil start tile => 0
        let (w, _l, _d) = new_world(32, 20);
        w.set_ops_hooks(None);
        let wp3 = WirePathCreator::init_with_world(&w);
        assert_eq!(wp3.find_and_subtract_all_power_up_to_for_user(10, Some(&new_user(ip(5, 5)))), 0);
        set_dynamic_world_hooks(None);
        let _ = tile_at_world_position_loaded;
    }
}
