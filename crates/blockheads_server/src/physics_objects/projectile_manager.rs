//! `ProjectileManager` (NSObject, World ivar `projectileManager` 0xe10): in-flight "instant" projectiles
//! (arrows, poison darts) that apply damage on arrival.
//!
//! the Rail, Scorpion, Sign
//! and SnowSurfaceBlock parts of that file belong to other modules). No RNG, no network traffic from this class.
//! Projectile state is not saved.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::fmath::cconv::f32_to_i32;
use crate::objects::handle::{CacheRef, ObjRef};
use crate::types::structs::Vector2;
use crate::world::state::World;

/// `ITEM_POISON`.
pub const ITEM_POISON: i32 = 0xff;
/// `ITEM_POISON_ARROW`.
pub const ITEM_POISON_ARROW: i32 = 0x100;

/// `struct Projectile` (0x30 bytes, calloc'ed).
struct Projectile {
    /// 0x00 retained
    at_object: Option<ObjRef>,
    /// 0x08 retained
    firer: Option<ObjRef>,
    /// 0x10
    item_type: i32,
    /// 0x14 current position
    pos: Vector2,
    /// 0x1c
    to_pos: Vector2,
    /// 0x24 unit direction (NaN / inf for a zero-length flight, no guard)
    normal: Vector2,
    /// 0x2c
    distance_left: f32,
}

/// `ProjectileManager: NSObject`. `cache`, `texture`, `shader` are client-only and omitted.
pub struct ProjectileManager {
    /// 0x08, not retained
    world: Weak<World>,
    /// 0x28 `std::vector<Projectile*>`
    projectiles: RefCell<Vec<Projectile>>,
}

impl ProjectileManager {
    /// `-initWithWorld:cache:`. Created once by `World incrementalLoad` stage 11; no RNG.
    pub fn init_with_world_cache(world: &Rc<World>, _cache: CacheRef) -> Rc<ProjectileManager> {
        Rc::new(ProjectileManager { world: Rc::downgrade(world), projectiles: RefCell::new(Vec::new()) })
    }

    /// Number of projectiles in flight (diagnostics helper; no ObjC counterpart).
    pub fn len(&self) -> usize {
        self.projectiles.borrow().len()
    }

    /// `-update:`, called every tick by `World update` before `DynamicWorld update`.
    pub fn update(&self, dt: f32) {
        let mut i = 0usize;
        loop {
            // The length is re-read every iteration: the hit callback may re-enter `fire_projectile...`.
            let arrived = {
                let mut v = self.projectiles.borrow_mut();
                if i >= v.len() {
                    break;
                }
                let p = &mut v[i];
                let distance = 20.0f32 * dt;
                p.distance_left = p.distance_left - distance;
                if 0.0f32 < p.distance_left {
                    p.pos = p.pos.add(p.normal.mul_f(distance));
                    i += 1;
                    None
                } else {
                    // Removed before the callback ("Re-entrancy").
                    Some(v.remove(i))
                }
            };
            let Some(p) = arrived else { continue };
            let mut force = 2.0f32;
            if p.item_type == ITEM_POISON || p.item_type == ITEM_POISON_ARROW {
                force = 10.0f32;
            }
            let expert = self.world.upgrade().map_or(false, |w| w.expert_mode());
            if expert {
                force = force / 2.0f32;
            }
            let target_pos = match &p.at_object {
                Some(o) => o.float_pos(),
                None => Vector2 { x: 0.0, y: 0.0 },
            };
            let d = target_pos.sub(p.to_pos);
            let len_sq = d.length_squared();
            if len_sq < 16.0f32 {
                if let Some(target) = &p.at_object {
                    if let Some(h) = target.as_harmable() {
                        h.hit_with_force_blockhead(target, f32_to_i32(force), p.firer.as_ref());
                    }
                }
            }
            // `at_object` / `firer` are released when `p` drops here.
        }
    }

    /// `-fireProjectileFrom:to:at:fireItemType:firer:`. `distance_vec / len` is a true f32 division
    /// (NaN for a zero-length flight, so the projectile arrives on the first update).
    pub fn fire_projectile_from_to_at_fire_item_type_firer(
        &self,
        from_pos: Vector2,
        to_pos: Vector2,
        at_object: Option<&ObjRef>,
        fire_item_type: i32,
        firer: Option<&ObjRef>,
    ) {
        let distance_vec = to_pos.sub(from_pos);
        let len = distance_vec.length();
        let p = Projectile {
            at_object: at_object.cloned(),
            firer: firer.cloned(),
            item_type: fire_item_type,
            pos: from_pos,
            to_pos,
            normal: distance_vec.div_f(len),
            distance_left: len,
        };
        self.projectiles.borrow_mut().push(p);
    }

    /// `-dealloc`: empties the vector.
    pub fn dealloc(&self) {
        self.projectiles.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::base::{DynamicObject, DynamicObjectBase};
    use crate::objects::class_info::{ClassInfo, DYNAMIC_OBJECT_CLASS};
    use crate::objects::handle::alloc_obj;
    use crate::objects::protocols::HarmableDynamicObject;

    /// A target that records `hitWithForce:`.
    struct Target {
        base: RefCell<DynamicObjectBase>,
        hits: RefCell<Vec<i32>>,
    }
    static TARGET_CLASS: ClassInfo =
        ClassInfo { name: "Target", superclass: Some(&DYNAMIC_OBJECT_CLASS), protocols: &["HarmableDynamicObject"] };
    impl DynamicObject for Target {
        fn base(&self) -> &RefCell<DynamicObjectBase> {
            &self.base
        }
        fn class_info(&self) -> &'static ClassInfo {
            &TARGET_CLASS
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_harmable(&self) -> Option<&dyn HarmableDynamicObject> {
            Some(self)
        }
    }
    impl HarmableDynamicObject for Target {
        fn tap_is_within_body_radius(&self, _tap: Vector2) -> bool {
            false
        }
        fn hit_with_force_blockhead(&self, _me: &ObjRef, force: i32, _b: Option<&ObjRef>) {
            self.hits.borrow_mut().push(force);
        }
        fn will_die_if_hit_by_force(&self, _force: i32) -> bool {
            false
        }
        fn is_visible(&self) -> bool {
            true
        }
        fn center(&self) -> Vector2 {
            Vector2 { x: 0.0, y: 0.0 }
        }
    }
    fn target(x: f32, y: f32) -> Rc<Target> {
        let t = alloc_obj(|w| Target {
            base: RefCell::new(DynamicObjectBase::alloc_zeroed_with(w)),
            hits: RefCell::new(vec![]),
        });
        t.base.borrow_mut().float_pos = Vector2 { x, y };
        t
    }
    fn mgr(expert: bool) -> (Rc<World>, ProjectileManager) {
        let w = World::alloc(2);
        w.set_expert_mode(expert);
        let m = ProjectileManager { world: Rc::downgrade(&w), projectiles: RefCell::new(vec![]) };
        (w, m)
    }

    /// : an arrow arrives after ceil(10 / (20 * 0.2)) ticks and deals 2.
    #[test]
    fn arrow_hits_with_force_two() {
        let (_w, m) = mgr(false);
        let t = target(10.0, 0.0);
        let o: ObjRef = t.clone();
        m.fire_projectile_from_to_at_fire_item_type_firer(
            Vector2 { x: 0.0, y: 0.0 },
            Vector2 { x: 10.0, y: 0.0 },
            Some(&o),
            0x50,
            None,
        );
        m.update(0.2);
        m.update(0.2);
        assert_eq!(m.len(), 1);
        assert!(t.hits.borrow().is_empty());
        m.update(0.2);
        assert_eq!(m.len(), 0);
        assert_eq!(*t.hits.borrow(), vec![2]);
    }

    #[test]
    fn poison_force_and_expert_halving() {
        for (expert, item, want) in [(false, 255, 10), (false, 256, 10), (true, 255, 5), (true, 7, 1)] {
            let (_w, m) = mgr(expert);
            let t = target(1.0, 0.0);
            let o: ObjRef = t.clone();
            m.fire_projectile_from_to_at_fire_item_type_firer(
                Vector2 { x: 0.0, y: 0.0 },
                Vector2 { x: 1.0, y: 0.0 },
                Some(&o),
                item,
                None,
            );
            m.update(1.0);
            assert_eq!(*t.hits.borrow(), vec![want], "expert={expert} item={item}");
        }
    }

    /// A target 4.0 units from `to_pos` is not hit (`lenSq < 16` strictly); 3.9 is.
    #[test]
    fn miss_when_target_moved() {
        let (_w, m) = mgr(false);
        let t = target(14.0, 0.0);
        let o: ObjRef = t.clone();
        m.fire_projectile_from_to_at_fire_item_type_firer(
            Vector2 { x: 0.0, y: 0.0 },
            Vector2 { x: 10.0, y: 0.0 },
            Some(&o),
            0x50,
            None,
        );
        m.update(1.0);
        assert_eq!(m.len(), 0);
        assert!(t.hits.borrow().is_empty());
        let t2 = target(13.9, 0.0);
        let o2: ObjRef = t2.clone();
        m.fire_projectile_from_to_at_fire_item_type_firer(
            Vector2 { x: 0.0, y: 0.0 },
            Vector2 { x: 10.0, y: 0.0 },
            Some(&o2),
            0x50,
            None,
        );
        m.update(1.0);
        assert_eq!(*t2.hits.borrow(), vec![2]);
    }

    /// `from == to` gives a NaN normal and arrives on the next update.
    #[test]
    fn zero_length_flight() {
        let (_w, m) = mgr(false);
        let t = target(0.0, 0.0);
        let o: ObjRef = t.clone();
        let p = Vector2 { x: 3.0, y: 3.0 };
        m.fire_projectile_from_to_at_fire_item_type_firer(p, p, Some(&o), 0x50, None);
        assert!(m.projectiles.borrow()[0].normal.x.is_nan());
        m.update(0.05);
        assert_eq!(m.len(), 0);
        // the target at (0,0) is 18 (squared) from (3,3): no hit
        assert!(t.hits.borrow().is_empty());
        // nil target: nothing happens, projectile removed
        m.fire_projectile_from_to_at_fire_item_type_firer(p, p, None, 0x50, None);
        m.update(0.05);
        assert_eq!(m.len(), 0);
    }
}
