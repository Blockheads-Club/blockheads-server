//! `Weather`: the server keeps only the `windMovement` state of this client weather renderer.
//!

use std::cell::Cell;
use std::rc::Rc;

use crate::fmath::libm::fabs;
use crate::objects::handle::CacheRef;
use crate::world::state::World;

/// `Weather: NSObject`. Only `windMovement` (+0x344) carries game state; it is never set on the server.
pub struct Weather {
    wind_movement: Cell<f32>,
}

impl Weather {
    /// `-[Weather initWithCache:world:worldTime:]`. Stores nothing the server can observe
    /// (`randomBirdTimer = 4.0f`, `lastSetUndergroundMix = 0` are client-only), consumes no `rand()`.
    pub fn init_with_cache_world_world_time(_cache: CacheRef, _world: &Rc<World>, _world_time: f32) -> Rc<Weather> {
        Rc::new(Weather { wind_movement: Cell::new(0.0) })
    }

    /// `-[Weather windStrength]`: `clamp((|windMovement| - 0.5) / 32.0, 0.0, 1.0)` evaluated in f64
    /// (both constants are doubles), result cast to f32. The clamp is `r = v; if 1.0 < v { r = 1.0 }; if v < 0.0 { r = 0.0 }`.
    pub fn wind_strength(&self) -> f32 {
        let v: f64 = (fabs(self.wind_movement.get() as f64) - 0.5f64) / 32.0f64;
        let mut r = v;
        if 1.0f64 < v {
            r = 1.0;
        }
        if v < 0.0f64 {
            r = 0.0;
        }
        r as f32
    }

    /// `-[Weather windMovement]`
    pub fn wind_movement(&self) -> f32 {
        self.wind_movement.get()
    }

    /// `-[Weather setWindMovement:]`
    pub fn set_wind_movement(&self, w: f32) {
        self.wind_movement.set(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_strength_is_zero_on_the_server_and_clamps() {
        let w = World::alloc(4);
        let weather = Weather::init_with_cache_world_world_time(None, &w, 12.5);
        assert_eq!(weather.wind_movement(), 0.0);
        assert_eq!(weather.wind_strength(), 0.0);
        weather.set_wind_movement(0.5);
        assert_eq!(weather.wind_strength(), 0.0);
        weather.set_wind_movement(-16.5);
        assert_eq!(weather.wind_strength(), 0.5);
        weather.set_wind_movement(1000.0);
        assert_eq!(weather.wind_strength(), 1.0);
        weather.set_wind_movement(32.5);
        assert_eq!(weather.wind_strength(), 1.0);
    }
}
