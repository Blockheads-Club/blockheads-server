//! `FishingRod` (NSObject, owned by a Blockhead while it fishes): the cast hook flight, the rope polyline and the
//! reel-in of a hooked ClownFish. The whole simulation runs on the server (including for net players).
//!
//!  FishingRod"
//! (lines 255-382) and its "Synthesis resolutions" (Vector2 default (1, 1)). Client-only members (`cameraPos`, `draw`,
//! `setPaused`, `reelInSound`, shaders, `verts`) are not ported.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::fmath::cconv::f32_to_i32;
use crate::fmath::vector::clamp;
use crate::objects::handle::{ptr_eq, CacheRef, ObjRef, ObjWeak};
use crate::types::structs::Vector2;
use crate::world::helpers::{tile_is_air, tile_is_solid, tile_is_water};
use crate::world::state::World;
use crate::world::tiles::tile_at_world_position_loaded;

crate::hooks::hook_trait! {
/// Seam to modules that did not exist when this one was written.
/// "npc_passive"). The integrator installs the real adapter with [`set_fishing_host`]; the defaults do nothing.
pub trait FishingHost, stub FishingHostStub {
    /// `-[Blockhead fishingRodCast:]` (a no-op for net players,). `vec` is the original unscaled swipe.
    fn blockhead_fishing_rod_cast(&self, _blockhead: &ObjRef, _vec: Vector2) {}
    /// `-[ClownFish die:]` with the fishing Blockhead as killer.
    fn clown_fish_die(&self, _fish: &ObjRef, _killer: Option<&ObjRef>) {}
}
}

struct NullFishingHost;
impl FishingHostStub for NullFishingHost {}

thread_local! {
    static HOST: RefCell<Rc<dyn FishingHost>> = RefCell::new(Rc::new(NullFishingHost));
}

/// Install the fishing seam (see [`FishingHost`]).
pub fn set_fishing_host(h: Rc<dyn FishingHost>) {
    HOST.with(|c| *c.borrow_mut() = h);
}

fn fishing_host() -> Rc<dyn FishingHost> {
    HOST.with(|c| c.borrow().clone())
}

/// Maximum number of rope points.
const MAX_ROPE_POINTS: usize = 127;

/// FishingRod ivars.
pub struct FishingRodState {
    /// 0x08, not retained
    pub world: Weak<World>,
    /// 0x10, stored UNRETAINED by the original; cleared by `blockhead_unloaded` (the original `release` there is a
    /// latent over-release, UB-PARITY, no observable effect)
    pub blockhead: Option<ObjWeak>,
    /// 0x18 ready to cast; YES at init
    pub casting: bool,
    /// 0x20 rope, newest point at the back
    pub line_locations: Vec<Vector2>,
    /// 0x38 (1, 1) after `.cxx_construct`
    pub hook_velocity: Vector2,
    /// 0x40 (1, 1) after `.cxx_construct`
    pub hook_pos: Vector2,
    /// 0x48 (1, 1) after `.cxx_construct`
    pub next_real_in_hook_pos: Vector2,
    /// 0x50
    pub is_in_casting_animation: bool,
    /// 0x54
    pub cast_animation_progress: f32,
    /// 0x58
    pub hook_in_air: bool,
    /// 0x59
    pub casting_right: bool,
    /// 0x78 retained ClownFish
    pub hooked_fish: Option<ObjRef>,
    /// 0x88 accumulates dt while a fish is hooked
    pub real_in_fraction: f32,
    /// 0x98 YES at init
    pub valid: bool,
    /// 0x99 never written by any method; stays false
    pub done: bool,
    /// 0x9a never written by any method: ALWAYS false
    pub is_net: bool,
}

/// `FishingRod: NSObject`.
pub struct FishingRod {
    /// The ivars.
    pub st: RefCell<FishingRodState>,
}

/// `max(0, x)` as the original's if-chain: 0 when `x <= 0`, else `x`.
#[inline]
fn max0(x: f32) -> f32 {
    if x <= 0.0f32 {
        0.0f32
    } else {
        x
    }
}

impl FishingRod {
    /// `-initWithWorld:blockhead:cache:` (client shaders / textures omitted).
    pub fn init_with_world_blockhead_cache(world: &Rc<World>, blockhead: &ObjRef, _cache: CacheRef) -> Rc<FishingRod> {
        let one = Vector2::cxx_new();
        Rc::new(FishingRod {
            st: RefCell::new(FishingRodState {
                world: Rc::downgrade(world),
                blockhead: Some(Rc::downgrade(blockhead)),
                casting: true,
                line_locations: Vec::new(),
                hook_velocity: one,
                hook_pos: one,
                next_real_in_hook_pos: one,
                is_in_casting_animation: false,
                cast_animation_progress: 0.0,
                hook_in_air: false,
                casting_right: false,
                hooked_fish: None,
                real_in_fraction: 0.0,
                valid: true,
                done: false,
                is_net: false,
            }),
        })
    }

    fn world(&self) -> Option<Rc<World>> {
        self.st.borrow().world.upgrade()
    }

    /// `-valid`
    pub fn valid(&self) -> bool {
        self.st.borrow().valid
    }

    /// `-setValid:`: when cleared, the hooked fish is released (the sound stop is a no-op).
    pub fn set_valid(&self, v: bool) {
        let mut st = self.st.borrow_mut();
        st.valid = v;
        if !v {
            st.hooked_fish = None;
        }
    }

    /// `-isCasting`
    pub fn is_casting(&self) -> bool {
        self.st.borrow().casting
    }

    /// `-directionalSwipe:`: the only place `casting` becomes false.
    pub fn directional_swipe(&self, vec: Vector2) {
        if !self.is_casting() {
            return;
        }
        let bh = self.blockhead();
        let base = bh.as_ref().map_or(Vector2::ZERO, |b| b.float_pos());
        {
            let mut st = self.st.borrow_mut();
            st.is_in_casting_animation = true;
            st.casting = false;
            let mut hv = vec.mul_f(0.005f32);
            if hv.length_squared() > 400.0f32 {
                hv = hv.normal().mul_f(20.0f32);
            }
            st.hook_velocity = hv;
            st.casting_right = hv.x > 0.0f32;
            let off = Vector2 { x: if st.casting_right { 1.2f32 } else { -1.2f32 }, y: 1.5f32 };
            st.hook_pos = base.add(off);
            let hp = st.hook_pos;
            st.line_locations.push(hp);
        }
        // `[blockhead fishingRodCast:vec]` with the original unscaled vector (nil-safe)
        if let Some(b) = &bh {
            fishing_host().blockhead_fishing_rod_cast(b, vec);
        }
    }

    /// `finishReel()` of `update`: achievement, `die:`, `stopFishing`. `isNet` is never set, so the `!isNet` part
    /// always runs.
    fn finish_reel(&self) {
        let (is_net, fish, bh, world) = {
            let st = self.st.borrow();
            (st.is_net, st.hooked_fish.clone(), st.blockhead.as_ref().and_then(|w| w.upgrade()), st.world.upgrade())
        };
        if !is_net {
            if let Some(w) = &world {
                w.report_achievement_with_identifier("grp.fishing");
            }
            if let Some(f) = &fish {
                fishing_host().clown_fish_die(f, bh.as_ref());
            }
        }
        if let Some(b) = &bh {
            if let Some(blockhead) = crate::blockhead::blockhead::as_blockhead(b) {
                crate::blockhead::blockhead::host().stop_fishing(blockhead);
            }
        }
    }

    /// `-update:accurateDT:isSimulation:` (`accurateDT` and `isSimulation` are unused).
    pub fn update_accurate_dt_is_simulation(&self, dt: f32, _accurate_dt: f32, _is_simulation: bool) {
        let Some(world) = self.world() else { return };
        let tile_at = |p: Vector2| tile_at_world_position_loaded(f32_to_i32(p.x), f32_to_i32(p.y), &world);

        // ---- Phase A: hooked fish present ----
        let fish = self.st.borrow().hooked_fish.clone();
        if let Some(fish) = fish {
            if fish.needs_removed() {
                self.st.borrow_mut().hooked_fish = None;
                return;
            }
            self.st.borrow_mut().real_in_fraction += dt;
            if !self.st.borrow().line_locations.is_empty() {
                let mut distance = 4.0f32 * dt;
                while (distance as f64) > 0.0001f64 {
                    let (back, hook_pos) = {
                        let st = self.st.borrow();
                        match st.line_locations.last() {
                            Some(b) => (*b, st.hook_pos),
                            None => {
                                drop(st);
                                self.finish_reel();
                                return;
                            }
                        }
                    };
                    let v = back.sub(hook_pos);
                    let len = v.length();
                    let reaches = len <= distance;
                    let step = if distance <= len { distance } else { len };
                    {
                        let mut st = self.st.borrow_mut();
                        st.hook_pos = hook_pos.add(v.normal().mul_f(step));
                    }
                    distance -= step;
                    if reaches {
                        let empty = {
                            let mut st = self.st.borrow_mut();
                            st.hook_pos = back;
                            st.line_locations.pop();
                            st.line_locations.is_empty()
                        };
                        if empty {
                            self.finish_reel();
                            return;
                        }
                    }
                }
            }
            if self.st.borrow().line_locations.len() > 1 {
                let hook_pos = self.st.borrow().hook_pos;
                let t = tile_at(hook_pos);
                let water_below_fill = tile_is_water(t.clone())
                    && (t.as_ref().map_or(0u8, |t| t.partial_content_left()) as f32)
                        < (hook_pos.y - (f32_to_i32(hook_pos.y) as f32)) * 255.0f32;
                if tile_is_air(t) || water_below_fill {
                    let mut hp = hook_pos;
                    hp.y -= 2.0f32 * dt;
                    if tile_is_solid(tile_at(hp)) {
                        hp.y += 8.0f32 * dt;
                    }
                    self.st.borrow_mut().hook_pos = hp;
                }
                // rope relax
                let n = self.st.borrow().line_locations.len();
                let mut previous_y = 0.0f32;
                let mut first = true;
                for i in 0..n {
                    let (mut e, next_y) = {
                        let st = self.st.borrow();
                        (st.line_locations[i], if i + 1 < n { st.line_locations[i + 1].y } else { st.hook_pos.y })
                    };
                    if first {
                        previous_y = e.y;
                        first = false;
                        continue;
                    }
                    let old_y = e.y;
                    e.y -= 4.0f32 * dt;
                    if tile_is_solid(tile_at(e)) {
                        e.y = old_y;
                    }
                    let fraction = max0(1.0f32 - 10.0f32 * dt);
                    e.y = old_y * fraction
                        + previous_y * (1.0f32 - fraction) * 0.5f32
                        + next_y * (1.0f32 - fraction) * 0.5f32;
                    if tile_is_solid(tile_at(e)) {
                        e.y = old_y;
                    }
                    previous_y = e.y;
                    self.st.borrow_mut().line_locations[i] = e;
                }
            }
        }

        // ---- Phase B: cast animation timer ----
        {
            let mut st = self.st.borrow_mut();
            if st.is_in_casting_animation {
                st.cast_animation_progress += dt;
                if st.cast_animation_progress > 0.25f32 {
                    st.hook_in_air = true;
                    st.is_in_casting_animation = false;
                    st.cast_animation_progress = 0.0;
                }
            }
        }

        // ---- Phase C: hook flight ----
        {
            let st = self.st.borrow();
            if st.hooked_fish.is_some() {
                return;
            }
            if !st.is_in_casting_animation && !st.hook_in_air {
                return;
            }
        }
        {
            let (old, hook_pos, mut hv) = {
                let mut st = self.st.borrow_mut();
                if st.line_locations.len() > 64 {
                    st.hook_velocity.x *= max0(1.0f32 - 2.0f32 * dt);
                }
                st.hook_velocity.y += dt * -10.0f32;
                let old = st.hook_pos;
                st.hook_pos = st.hook_pos.add(st.hook_velocity.mul_f(dt));
                (old, st.hook_pos, st.hook_velocity)
            };
            let pos = (f32_to_i32(hook_pos.x), f32_to_i32(hook_pos.y));
            let t = tile_at_world_position_loaded(pos.0, pos.1, &world);
            let mut new_hook_pos = hook_pos;
            if !tile_is_solid(t.clone()) {
                if tile_is_water(t) {
                    hv.x *= max0(1.0f32 - 2.0f32 * dt);
                    hv.x = clamp(hv.x, -1.0f32, 1.0f32);
                    hv.y = -1.0f32;
                }
            } else {
                new_hook_pos = old;
                let p2 = (f32_to_i32(old.x), f32_to_i32(old.y));
                if p2.0 != pos.0 {
                    hv.x = 0.0;
                }
                if p2.1 != pos.1 {
                    hv.y = 0.0;
                }
            }
            let mut st = self.st.borrow_mut();
            st.hook_velocity = hv;
            st.hook_pos = new_hook_pos;
        }
        // rope growth: `lineLocations.at(size - 1)` throws std::out_of_range when empty (cannot happen after a cast)
        {
            let mut st = self.st.borrow_mut();
            let last = *st.line_locations.last().expect("std::out_of_range: FishingRod lineLocations is empty"); // UB-PARITY
            let hp = st.hook_pos;
            let d = hp.sub(last);
            let close = d.length_squared() <= 4.0f32 && (hp.x - last.x).abs() <= 0.5f32;
            if !close && st.line_locations.len() < MAX_ROPE_POINTS {
                st.line_locations.push(hp);
            }
        }
        // rope relax variant 2
        let n = self.st.borrow().line_locations.len();
        if n > 1 {
            for i in 1..n {
                let (mut e, next_y) = {
                    let st = self.st.borrow();
                    (st.line_locations[i], if i + 1 < n { st.line_locations[i + 1].y } else { st.hook_pos.y })
                };
                let old_y = e.y;
                e.y -= 4.0f32 * dt;
                let t = tile_at(e);
                if tile_is_solid(t.clone()) {
                    e.y = old_y;
                } else if tile_is_water(t.clone())
                    && ((e.y - (f32_to_i32(e.y) as f32)) * 255.0f32
                        <= (t.as_ref().map_or(0u8, |t| t.partial_content_left()) as f32))
                {
                    let fraction = max0(1.0f32 - dt);
                    e.y = old_y * fraction + next_y * (1.0f32 - fraction);
                }
                self.st.borrow_mut().line_locations[i] = e;
            }
        }
    }

    /// `-hookedFishDirection`.
    pub fn hooked_fish_direction(&self) -> Vector2 {
        let st = self.st.borrow();
        let n = st.line_locations.len();
        if n < 2 {
            return Vector2 { x: 1.0, y: 0.0 };
        }
        let mut d = st.line_locations[n - 1].sub(st.hook_pos);
        if (d.length_squared() as f64) < 0.1f64 {
            d = st.line_locations[n - 2].sub(st.line_locations[n - 1]);
        }
        d.normal()
    }

    /// `-setHookedFish:` (sounds omitted). Called by ClownFish.
    pub fn set_hooked_fish(&self, fish: Option<ObjRef>) {
        let same = {
            let st = self.st.borrow();
            match (&st.hooked_fish, &fish) {
                (None, None) => true,
                (Some(a), Some(b)) => ptr_eq(a, b),
                _ => false,
            }
        };
        if same {
            return;
        }
        let mut st = self.st.borrow_mut();
        let is_some = fish.is_some();
        st.hooked_fish = fish;
        if is_some {
            // `lineLocations.at(size - 1)` throws std::out_of_range when empty (UB-PARITY)
            let last = *st.line_locations.last().expect("std::out_of_range: FishingRod lineLocations is empty");
            st.next_real_in_hook_pos = last;
        }
    }

    /// `-hookedFish`
    pub fn hooked_fish(&self) -> Option<ObjRef> {
        self.st.borrow().hooked_fish.clone()
    }

    /// `-hookPos`
    pub fn hook_pos(&self) -> Vector2 {
        self.st.borrow().hook_pos
    }

    /// `-setHookPos:`
    pub fn set_hook_pos(&self, v: Vector2) {
        self.st.borrow_mut().hook_pos = v;
    }

    /// `-blockhead`
    pub fn blockhead(&self) -> Option<ObjRef> {
        self.st.borrow().blockhead.as_ref().and_then(|w| w.upgrade())
    }

    /// `-blockheadUnloaded:`: clears the blockhead if it is the same object.
    pub fn blockhead_unloaded(&self, bh: &ObjRef) {
        let same = self.blockhead().map_or(false, |b| ptr_eq(&b, bh));
        if same {
            self.st.borrow_mut().blockhead = None;
        }
    }

    /// `-isInCastingAnimation`
    pub fn is_in_casting_animation(&self) -> bool {
        self.st.borrow().is_in_casting_animation
    }
    /// `-setIsInCastingAnimation:`
    pub fn set_is_in_casting_animation(&self, v: bool) {
        self.st.borrow_mut().is_in_casting_animation = v;
    }
    /// `-castAnimationProgress`
    pub fn cast_animation_progress(&self) -> f32 {
        self.st.borrow().cast_animation_progress
    }
    /// `-setCastAnimationProgress:`
    pub fn set_cast_animation_progress(&self, v: f32) {
        self.st.borrow_mut().cast_animation_progress = v;
    }
    /// `-hookInAir`
    pub fn hook_in_air(&self) -> bool {
        self.st.borrow().hook_in_air
    }
    /// `-setHookInAir:`
    pub fn set_hook_in_air(&self, v: bool) {
        self.st.borrow_mut().hook_in_air = v;
    }
    /// `-castingRight`
    pub fn casting_right(&self) -> bool {
        self.st.borrow().casting_right
    }
    /// `-setCastingRight:`
    pub fn set_casting_right(&self, v: bool) {
        self.st.borrow_mut().casting_right = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::testkit::tile;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use crate::objects::handle::alloc_obj;
    use crate::physics_objects::free_block::testkit::flat;

    struct Dummy {
        base: RefCell<DynamicObjectBase>,
    }
    static DUMMY: ClassInfo = ClassInfo { name: "Dummy", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &[] };
    impl DynamicObject for Dummy {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &DUMMY
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
    fn dummy(x: f32, y: f32) -> ObjRef {
        let d = alloc_obj(|w| Dummy { base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)) });
        d.base.borrow_mut().float_pos = Vector2 { x, y };
        d
    }

    #[derive(Default)]
    struct Rec {
        log: RefCell<Vec<String>>,
    }
    impl FishingHostStub for Rec {
        fn blockhead_fishing_rod_cast(&self, _b: &ObjRef, v: Vector2) {
            self.log.borrow_mut().push(format!("cast({},{})", v.x, v.y));
        }
        fn clown_fish_die(&self, _f: &ObjRef, k: Option<&ObjRef>) {
            self.log.borrow_mut().push(format!("die(killer={})", k.is_some()));
        }
    }

    #[test]
    fn initial_state_and_swipe() {
        let (w, _dw, _r) = flat("rod_init", 5);
        let rec = Rc::new(Rec::default());
        set_fishing_host(rec.clone());
        let bh = dummy(50.0, 10.0);
        let rod = FishingRod::init_with_world_blockhead_cache(&w, &bh, None);
        assert!(rod.is_casting() && rod.valid());
        assert_eq!(rod.hook_pos(), Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(rod.st.borrow().next_real_in_hook_pos, Vector2 { x: 1.0, y: 1.0 });
        assert_eq!(rod.hooked_fish_direction(), Vector2 { x: 1.0, y: 0.0 });
        rod.directional_swipe(Vector2 { x: 2000.0, y: 1000.0 });
        assert!(!rod.is_casting());
        assert!(rod.is_in_casting_animation());
        assert!(rod.casting_right());
        assert_eq!(rod.hook_pos(), Vector2 { x: 51.2, y: 11.5 });
        assert_eq!(rod.st.borrow().hook_velocity, Vector2 { x: 2000.0f32 * 0.005f32, y: 1000.0f32 * 0.005f32 });
        assert_eq!(rod.st.borrow().line_locations.len(), 1);
        assert_eq!(*rec.log.borrow(), vec!["cast(2000,1000)".to_string()]);
        // a second swipe is ignored
        rod.directional_swipe(Vector2 { x: -5.0, y: 0.0 });
        assert_eq!(rec.log.borrow().len(), 1);
        // fast swipe is normalised to speed 20
        let rod2 = FishingRod::init_with_world_blockhead_cache(&w, &bh, None);
        rod2.directional_swipe(Vector2 { x: -100000.0, y: 0.0 });
        assert_eq!(rod2.st.borrow().hook_velocity, Vector2 { x: -20.0, y: 0.0 });
        assert!(!rod2.casting_right());
        assert_eq!(rod2.hook_pos(), Vector2 { x: 48.8, y: 11.5 });
    }

    /// : after 0.25 s the hook is in the air; the rope grows; water slows the hook.
    #[test]
    fn cast_flight_and_rope() {
        let (w, _dw, _r) = flat("rod_flight", 5);
        let bh = dummy(20.0, 10.0);
        let rod = FishingRod::init_with_world_blockhead_cache(&w, &bh, None);
        rod.directional_swipe(Vector2 { x: 4000.0, y: 1000.0 });
        for _ in 0..2 {
            rod.update_accurate_dt_is_simulation(0.1, 0.1, false);
        }
        assert!(rod.is_in_casting_animation());
        rod.update_accurate_dt_is_simulation(0.1, 0.1, false);
        assert!(rod.hook_in_air());
        assert!(!rod.is_in_casting_animation());
        let mut grew = false;
        for _ in 0..40 {
            rod.update_accurate_dt_is_simulation(0.05, 0.05, false);
            if rod.st.borrow().line_locations.len() > 1 {
                grew = true;
            }
        }
        assert!(grew);
        // hits the solid ground at y <= 5: stops there
        assert!(rod.hook_pos().y >= 5.9);
        // water: velocity.y forced to -1, x clamped
        tile(&w, 60, 12).update(|t| t.type_index = 3);
        let rod2 = FishingRod::init_with_world_blockhead_cache(&w, &dummy(0.0, 0.0), None);
        {
            let mut st = rod2.st.borrow_mut();
            st.hook_in_air = true;
            st.casting = false;
            st.hook_pos = Vector2 { x: 60.5, y: 12.5 };
            st.hook_velocity = Vector2 { x: 5.0, y: 0.0 };
            st.line_locations.push(Vector2 { x: 60.5, y: 12.5 });
        }
        rod2.update_accurate_dt_is_simulation(0.01, 0.01, false);
        assert_eq!(rod2.st.borrow().hook_velocity.y, -1.0);
        assert_eq!(rod2.st.borrow().hook_velocity.x, 1.0);
    }

    /// Reeling in a hooked fish: at the last rope point `die` and `stopFishing` run, in order.
    #[test]
    fn reel_in_finishes() {
        let (w, _dw, _r) = flat("rod_reel", 5);
        let rec = Rc::new(Rec::default());
        set_fishing_host(rec.clone());
        let bh = dummy(20.0, 10.0);
        let rod = FishingRod::init_with_world_blockhead_cache(&w, &bh, None);
        {
            let mut st = rod.st.borrow_mut();
            st.casting = false;
            st.line_locations = vec![Vector2 { x: 20.0, y: 20.0 }, Vector2 { x: 22.0, y: 20.0 }];
            st.hook_pos = Vector2 { x: 24.0, y: 20.0 };
        }
        let fish = dummy(0.0, 0.0);
        rod.set_hooked_fish(Some(fish.clone()));
        assert_eq!(rod.st.borrow().next_real_in_hook_pos, Vector2 { x: 22.0, y: 20.0 });
        for _ in 0..100 {
            rod.update_accurate_dt_is_simulation(0.1, 0.1, false);
            if !rec.log.borrow().is_empty() {
                break;
            }
        }
        assert_eq!(*rec.log.borrow(), vec!["die(killer=true)".to_string()]);
        assert!(rod.st.borrow().line_locations.is_empty());
        // removed fish are dropped without a reel
        let rod2 = FishingRod::init_with_world_blockhead_cache(&w, &bh, None);
        rod2.st.borrow_mut().line_locations.push(Vector2 { x: 1.0, y: 1.0 });
        let f2 = dummy(0.0, 0.0);
        rod2.set_hooked_fish(Some(f2.clone()));
        f2.set_needs_removed(&f2, true);
        rod2.update_accurate_dt_is_simulation(0.1, 0.1, false);
        assert!(rod2.hooked_fish().is_none());
        rod2.set_valid(false);
        assert!(!rod2.valid());
        rod2.blockhead_unloaded(&bh);
        assert!(rod2.blockhead().is_none());
    }
}
