//! `ParticleEmitter`: the headless particle singleton. Every particle method is empty in the server binary; the
//! only server-visible method forwards electricity paths to the world for networking.
//!
//!  ParticleEmitter" (4.1, 4.2);
//! sections 3.6, 6 (R5) and 7 item 13. Callers evaluate their `rand()` arguments BEFORE calling the empty methods.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::fmath::vector::Vector;
use crate::lighting::artificial_light::services;
use crate::types::structs::ElectrictyParticlePathIndex;
use crate::world::state::World;

/// ParticleEmitter ivars that matter on the server (world @0x08, stopAllParticles @0xac, worldWidthMacro @0xb0).
pub struct ParticleEmitterState {
    /// `world` (assign, not retained)
    pub world: Weak<World>,
    /// `stopAllParticles`
    pub stop_all_particles: bool,
    /// `worldWidthMacro`
    pub world_width_macro: i32,
}

/// `ParticleEmitter: NSObject` singleton (instance size 0xb8).
pub struct ParticleEmitter {
    st: RefCell<ParticleEmitterState>,
    adds: Cell<u32>,
}

thread_local! {
    static INSTANCE: RefCell<Option<Rc<ParticleEmitter>>> = const { RefCell::new(None) };
}

impl ParticleEmitter {
    /// `+[ParticleEmitter instance]`: lazy singleton; `-init` is `[super init]` only.
    pub fn instance() -> Rc<ParticleEmitter> {
        INSTANCE.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(Rc::new(ParticleEmitter {
                    st: RefCell::new(ParticleEmitterState {
                        world: Weak::new(),
                        stop_all_particles: false,
                        world_width_macro: 0,
                    }),
                    adds: Cell::new(0),
                }));
            }
            c.as_ref().unwrap().clone()
        })
    }

    /// `-[ParticleEmitter setWorld:]`: assign, not retained.
    pub fn set_world(&self, w: &Rc<World>) {
        self.st.borrow_mut().world = Rc::downgrade(w);
    }

    /// `-[ParticleEmitter reset]`: all four index sets are nil on the server, every message is a no-op.
    pub fn reset(&self) {}

    /// `-[ParticleEmitter setStopAllParticles:]`
    pub fn set_stop_all_particles(&self, b: bool) {
        self.st.borrow_mut().stop_all_particles = b;
    }
    /// `-[ParticleEmitter stopAllParticles]`
    pub fn stop_all_particles(&self) -> bool {
        self.st.borrow().stop_all_particles
    }
    /// `-[ParticleEmitter setWorldWidthMacro:]`
    pub fn set_world_width_macro(&self, w: i32) {
        self.st.borrow_mut().world_width_macro = w;
    }
    /// `-[ParticleEmitter worldWidthMacro]`
    pub fn world_width_macro(&self) -> i32 {
        self.st.borrow().world_width_macro
    }

    /// `-addParticleAtPos:velocity:color:gravityType:life:scale:`: empty (client only).
    pub fn add_particle_at_pos_velocity_color_gravity_type_life_scale(
        &self,
        _pos: Vector,
        _vel: Vector,
        _color: Vector,
        _gravity_type: i32,
        _life: f32,
        _scale: f32,
    ) {
        self.adds.set(self.adds.get().wrapping_add(1));
    }
    /// `-addParticleAtPos:velocity:color:gravityType:life:scale:center:`: empty.
    #[allow(clippy::too_many_arguments)]
    pub fn add_particle_at_pos_velocity_color_gravity_type_life_scale_center(
        &self,
        _pos: Vector,
        _vel: Vector,
        _color: Vector,
        _gravity_type: i32,
        _life: f32,
        _scale: f32,
        _center: Vector,
    ) {
        self.adds.set(self.adds.get().wrapping_add(1));
    }
    /// `-addParticleAtPos:velocity:color:gravityType:life:scale:center:connectionGoal:`: empty.
    #[allow(clippy::too_many_arguments)]
    pub fn add_particle_at_pos_velocity_color_gravity_type_life_scale_center_connection_goal(
        &self,
        _pos: Vector,
        _vel: Vector,
        _color: Vector,
        _gravity_type: i32,
        _life: f32,
        _scale: f32,
        _center: Vector,
        _connection_goal: Vector,
    ) {
        self.adds.set(self.adds.get().wrapping_add(1));
    }
    /// `-addBonusParticleAtPos:color:bonusMultiplier:`: empty.
    pub fn add_bonus_particle_at_pos_color_bonus_multiplier(
        &self,
        _pos: Vector,
        _color: Vector,
        _bonus_multiplier: f32,
    ) {
        self.adds.set(self.adds.get().wrapping_add(1));
    }
    /// `-doAddElectricityParticleWithPath:size:`: empty.
    pub fn do_add_electricity_particle_with_path_size(&self, _path: Vec<ElectrictyParticlePathIndex>, _size: f32) {
        self.adds.set(self.adds.get().wrapping_add(1));
    }

    /// `-addElectricityParticleWithPath:size:`, the ONE server-visible method:
    /// `[world sendNetDataForElectricityParticlePathIfRequired:copy(path) size:size ignoreClient:nil]`, then
    /// `if !stopAllParticles { [self doAddElectricityParticleWithPath:copy(path) size:size] }`. A nil world is a no-op send.
    pub fn add_electricity_particle_with_path_size(&self, path: Vec<ElectrictyParticlePathIndex>, size: f32) {
        let (world, stop) = {
            let st = self.st.borrow();
            (st.world.upgrade(), st.stop_all_particles)
        };
        if let Some(w) = world {
            services().send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
                &w,
                path.clone(),
                size,
                None,
            );
        }
        if !stop {
            self.do_add_electricity_particle_with_path_size(path, size);
        }
    }

    /// Number of calls into the (empty) particle methods; test observability only.
    pub fn noop_call_count(&self) -> u32 {
        self.adds.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::artificial_light::{install_services, LightingServices};

    struct Count(Cell<u32>);
    impl LightingServices for Count {
        fn send_net_data_for_electricity_particle_path_if_required_size_ignore_client(
            &self,
            _w: &World,
            path: Vec<ElectrictyParticlePathIndex>,
            size: f32,
            ignore: Option<&str>,
        ) {
            assert_eq!(path.len(), 2);
            assert_eq!(size, 1.5);
            assert!(ignore.is_none());
            self.0.set(self.0.get() + 1);
        }
    }

    /// item 13: state accessors, no-op methods, electricity hook called exactly once.
    #[test]
    fn electricity_hook_and_state() {
        let e = ParticleEmitter::instance();
        assert!(Rc::ptr_eq(&e, &ParticleEmitter::instance()));
        e.set_world_width_macro(8);
        assert_eq!(e.world_width_macro(), 8);
        e.reset();
        let c = Rc::new(Count(Cell::new(0)));
        install_services(c.clone());
        let w = World::alloc(4);
        e.set_world(&w);
        let p = vec![
            ElectrictyParticlePathIndex { index: 1, solid: 0, padding: [0; 3] },
            ElectrictyParticlePathIndex { index: 2, solid: 1, padding: [0; 3] },
        ];
        let before = e.noop_call_count();
        e.add_electricity_particle_with_path_size(p.clone(), 1.5);
        assert_eq!(c.0.get(), 1);
        assert_eq!(e.noop_call_count(), before + 1);
        e.set_stop_all_particles(true);
        assert!(e.stop_all_particles());
        e.add_electricity_particle_with_path_size(p, 1.5);
        assert_eq!(c.0.get(), 2);
        assert_eq!(e.noop_call_count(), before + 1); // stopAllParticles suppresses the (empty) do-add
        e.set_stop_all_particles(false);
        let z = Vector::new(0.0, 0.0, 0.0);
        e.add_particle_at_pos_velocity_color_gravity_type_life_scale(z, z, z, 3, 8.0, 0.0);
        e.add_bonus_particle_at_pos_color_bonus_multiplier(z, z, 1.0);
        let adds = e.noop_call_count();
        e.add_particle_at_pos_velocity_color_gravity_type_life_scale_center(z, z, z, 3, 8.0, 0.0, z);
        e.add_particle_at_pos_velocity_color_gravity_type_life_scale_center_connection_goal(z, z, z, 3, 8.0, 0.0, z, z);
        assert_eq!(e.noop_call_count(), adds + 2);
        install_services(Rc::new(crate::lighting::artificial_light::NullServices));
    }
}
