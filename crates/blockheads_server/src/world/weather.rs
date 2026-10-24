//! `world_ops` weather: `getWeatherFractionForPos...` (3 overloads), `getDayNightFractionForX:atWorldTime:`,
//! `dayColorForPosition:`, `sandFractionForPos:`, `windStrength`.
//!
//! "Weather / time / misc getters".
use super::climate::absolute_season_fraction;
use super::misc::deps;
use super::state::World;
use crate::fmath::libm;
use crate::fmath::vector::{clamp, linear_interpolate, polar_to_rectangular};
use crate::types::structs::{IntPair, Vector};

type V3 = (f32, f32, f32);

/// `GLKVector3Length`: f64 `sqrt` of the f32 sum of squares, narrowed.
fn glk_len(v: V3) -> f32 {
    libm::sqrt((v.0 * v.0 + v.1 * v.1 + v.2 * v.2) as f64) as f32
}

/// `GLKVector3Normalize`: `s = 1/len; v * s`.
fn glk_norm(v: V3) -> V3 {
    let s = 1.0f32 / glk_len(v);
    (v.0 * s, v.1 * s, v.2 * s)
}

/// `GLKVector3CrossProduct`.
fn glk_cross(l: V3, r: V3) -> V3 {
    (l.1 * r.2 - l.2 * r.1, l.2 * r.0 - l.0 * r.2, l.0 * r.1 - l.1 * r.0)
}

/// Row/column `y` of `GLKMatrix4MultiplyVector3(GLKMatrix4MakeLookAt(eye=0, center=(0,0,-30), up=(up.x, up.y, 0)), sun)`
/// (w = 0). Private GLK subset.
fn glk_look_at_rel_y(sun: V3, up: V3) -> f32 {
    let eye: V3 = (0.0, 0.0, 0.0);
    let center: V3 = (0.0, 0.0, -30.0f32);
    // negate = sign-bit xor; eye + (-center)
    let neg_center: V3 = (-center.0, -center.1, -center.2);
    let n = glk_norm((eye.0 + neg_center.0, eye.1 + neg_center.1, eye.2 + neg_center.2));
    let up3: V3 = (up.0, up.1, 0.0f32);
    let u = glk_norm(glk_cross(up3, n));
    let v = glk_cross(n, u);
    let m01 = v.0;
    let m11 = v.1;
    let m21 = v.2;
    // dot(negate(v), eye)
    let m31 = (-v.0 * eye.0 + -v.1 * eye.1) + -v.2 * eye.2;
    m01 * sun.0 + m11 * sun.1 + m21 * sun.2 + m31 * 0.0f32
}

impl World {
    /// `-[World getWeatherFractionForPos:]`.
    pub fn get_weather_fraction_for_pos(&self, pos: IntPair) -> f32 {
        self.get_weather_fraction_for_pos_at_world_time(pos, self.world_time.get())
    }

    /// `-[World getWeatherFractionForPos:atWorldTime:]`.
    pub fn get_weather_fraction_for_pos_at_world_time(&self, pos: IntPair, time: f64) -> f32 {
        self.get_weather_fraction_for_pos_at_world_time_ignore_sand_fraction(pos, time, false)
    }

    /// `-[World getWeatherFractionForPos:atWorldTime:ignoreSandFraction:]`.
    /// `pos.x / W` is C `int` division.
    pub fn get_weather_fraction_for_pos_at_world_time_ignore_sand_fraction(
        &self,
        pos: IntPair,
        time: f64,
        ignore_sand_fraction: bool,
    ) -> f32 {
        let d = deps();
        let w_tiles = self.world_width_macro << 5;
        let n: f64 = if self.weather_noise_function.borrow().is_some() {
            d.weather_noise_get_x_y_z_octaves(0.0 + (pos.x.wrapping_div(w_tiles)) as f64, 0.324f64, 0.0005f64 * time, 3)
        } else {
            0.0
        };
        let mut w = n as f32;
        let rules = self.custom_rules.get();
        if rules.custom_rules_enabled != 0 {
            if rules.rainfall == 3 {
                w += 0.2f32;
            } else if rules.rainfall == 4 {
                w += 0.5f32;
            }
        }
        let mut sand = 0.0f32;
        if !ignore_sand_fraction {
            // worldTileLoader != nil ? worldTileLoader: clientTileLoader (always nil on the server)
            if self.world_tile_loader.borrow().is_some() {
                sand = d.sand_fraction_for_pos_high_res(pos, false);
            }
        }
        let h = clamp(((pos.y - 0x200) as f32 - 256.0f32) / 256.0f32, 0.0f32, 1.0f32);
        w -= h;
        if sand > 0.3f32 {
            w -= 3.0f32 * sand;
        } else if sand > 0.0f32 {
            w -= sand * sand * 10.0f32;
        } else {
            w -= 0.15f32 * sand;
        }
        let nrt = self.no_rain_timer.get();
        if nrt < 900.0f32 && (rules.custom_rules_enabled == 0 || (rules.rainfall != 4 && rules.rainfall != 3)) {
            let f = clamp((((nrt as f64) - 450.0f64) / 450.0f64) as f32, 0.0f32, 1.0f32);
            w = linear_interpolate(0.0f32, w, f);
        }
        clamp(w, -1.0f32, 2.0f32)
    }

    /// `-[World getDayNightFractionForX:atWorldTime:]`: sun elevation relative to the local up vector,
    /// `0.25 + 0.25 * rel.y`.
    pub fn get_day_night_fraction_for_x_at_world_time(&self, x: f32, time: f64) -> f32 {
        let py = absolute_season_fraction(time);
        let pd = libm::fmodf((time / 900.0f64) as f32, 1.0f32);
        let lat = (libm::cos((0.5f64 - py as f64) * 6.283185307179586f64) * 0.40927970959267024f64) as f32;
        let dd = (0.5f64 - pd as f64) * 3.141592653589793f64;
        let lon = (dd + dd) as f32;
        let sun = polar_to_rectangular(lat as f64, lon as f64);
        // getWorldUpVectorForX(x)
        let a = ((((x / ((self.world_width_macro << 5) as f32)) as f64) * 3.141592653589793f64 * 2.0f64)
            - 1.5707963267948966f64) as f32;
        let (up_lat, up_lon): (f32, f32) = if (a as f64) <= 1.5707963267948966f64 {
            (-a, 1.5707963705062866f32)
        } else {
            (((a as f64) - 3.141592653589793f64) as f32, -1.5707963705062866f32)
        };
        let up = polar_to_rectangular(up_lat as f64, up_lon as f64);
        let rel_y = glk_look_at_rel_y((sun.x, sun.y, sun.z), (up.x, up.y, up.z));
        0.25f32 + 0.25f32 * rel_y
    }

    /// `-[World dayColorForPosition:]`: ignores `pos`, returns (1,1,1,1).
    pub fn day_color_for_position(&self, _pos: IntPair) -> Vector {
        Vector::new4(1.0, 1.0, 1.0, 1.0)
    }

    /// `-[World sandFractionForPos:]`: `worldTileLoader == nil ? 0: [worldTileLoader sandFractionForPos:pos highRes:YES]`.
    pub fn sand_fraction_for_pos(&self, pos: IntPair) -> f32 {
        if self.world_tile_loader.borrow().is_none() {
            0.0
        } else {
            deps().sand_fraction_for_pos_high_res(pos, true)
        }
    }

    /// `-[World windStrength]`: `weather == nil ? 0: [weather windStrength]`.
    pub fn wind_strength(&self) -> f32 {
        if self.weather.borrow().is_none() {
            0.0
        } else {
            deps().weather_wind_strength()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::{NoiseFunction, WorldTileLoader};
    use std::rc::Rc;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    /// W6-style table on the formula: no noise function (n = 0), ramp, custom rainfall.
    #[test]
    fn weather_fraction_formula() {
        let rec = testkit::install();
        let w = World::alloc(32);
        w.no_rain_timer.set(10000.0);
        // n = 0, h = clamp(((0-512)-256)/256) = 0, sand 0 -> 0
        assert_eq!(w.get_weather_fraction_for_pos(ip(10, 0)), 0.0);
        // high up: h = clamp((1023-512-256)/256) = 255/256
        let hi = w.get_weather_fraction_for_pos(ip(10, 1023));
        assert_eq!(hi, -(255.0f32 / 256.0f32));
        // noise + loader present
        *w.weather_noise_function.borrow_mut() = Some(Rc::new(NoiseFunction {}));
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
        rec.noise.set(0.5);
        rec.sand.set(0.5);
        let v = w.get_weather_fraction_for_pos_at_world_time(ip(40, 600), 1000.0);
        let h = clamp(((600 - 512) as f32 - 256.0f32) / 256.0f32, 0.0, 1.0);
        let mut expect = 0.5f32 as f32 - h;
        expect -= 3.0f32 * 0.5f32;
        assert_eq!(v, clamp(expect, -1.0, 2.0));
        let log = rec.take();
        assert!(log.contains(&"noise(0,0.324,0.5,3)".to_string()), "{:?}", log);
        // pos.x / W is int division: x = 1024 with W = 1024 -> 1.0
        let _ = w.get_weather_fraction_for_pos_at_world_time(ip(1024, 600), 1000.0);
        assert!(rec.take().contains(&"noise(1,0.324,0.5,3)".to_string()));
        // ignore sand
        rec.sand.set(0.5);
        let _ = w.get_weather_fraction_for_pos_at_world_time_ignore_sand_fraction(ip(1, 600), 0.0, true);
        assert!(!rec.take().iter().any(|l| l.starts_with("sand")));
        // ramp: noRainTimer < 900 -> 0 below 450
        w.no_rain_timer.set(300.0);
        assert_eq!(w.get_weather_fraction_for_pos_at_world_time_ignore_sand_fraction(ip(1, 600), 0.0, true), 0.0);
        // rainfall 4 disables the ramp and adds 0.5
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        cr.rainfall = 4;
        w.custom_rules.set(cr);
        rec.noise.set(0.0);
        let v = w.get_weather_fraction_for_pos_at_world_time_ignore_sand_fraction(ip(1, 512), 0.0, true);
        assert_eq!(v, 0.5f32 - clamp((0.0f32 - 256.0) / 256.0, 0.0, 1.0));
        install_deps(Rc::new(NullDeps));
    }

    #[test]
    fn day_night_fraction_range_and_symmetry() {
        let w = World::alloc(32);
        let mut min = 1.0f32;
        let mut max = 0.0f32;
        for i in 0..100 {
            let t = i as f64 * 9.0;
            let f = w.get_day_night_fraction_for_x_at_world_time(8.0 * 32.0, t);
            assert!((0.0..=1.0).contains(&f), "{} {}", t, f);
            min = min.min(f);
            max = max.max(f);
        }
        // the sun goes through night and day
        assert!(min < 0.1 && max > 0.4, "{} {}", min, max);
        // deterministic
        assert_eq!(
            w.get_day_night_fraction_for_x_at_world_time(0.0, 123.4).to_bits(),
            w.get_day_night_fraction_for_x_at_world_time(0.0, 123.4).to_bits()
        );
    }

    #[test]
    fn simple_getters() {
        let _r = testkit::install();
        let w = World::alloc(32);
        assert_eq!(w.day_color_for_position(ip(3, 4)).v, [1.0; 4]);
        assert_eq!(w.sand_fraction_for_pos(ip(0, 0)), 0.0);
        assert_eq!(w.wind_strength(), 0.0);
        install_deps(Rc::new(NullDeps));
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::world::misc::{install_deps, testkit, NullDeps};
    use crate::world::state::{Weather, WorldTileLoader};
    use std::rc::Rc;

    #[test]
    fn sand_rain_and_wind_branches() {
        let rec = testkit::install();
        let w = World::alloc(32);
        w.no_rain_timer.set(10000.0);
        *w.world_tile_loader.borrow_mut() = Some(Rc::new(WorldTileLoader {}));
        let mut cr = w.custom_rules.get();
        cr.custom_rules_enabled = 1;
        for rainfall in [3, 4, 2] {
            cr.rainfall = rainfall;
            w.custom_rules.set(cr);
            for sand in [0.0f32, 0.1, 0.5] {
                rec.sand.set(sand);
                let _ = w.get_weather_fraction_for_pos_at_world_time(IntPair { x: 40, y: 600 }, 0.0);
            }
        }
        // sand fraction and wind for a world with and without the pieces
        assert_eq!(w.sand_fraction_for_pos(IntPair { x: 1, y: 1 }), 0.5);
        assert_eq!(w.wind_strength(), 0.0);
        *w.weather.borrow_mut() = Some(Rc::new(Weather {}));
        assert_eq!(w.wind_strength(), 0.0);
        *w.world_tile_loader.borrow_mut() = None;
        assert_eq!(w.sand_fraction_for_pos(IntPair { x: 1, y: 1 }), 0.0);
        // the far side of the world has its sun the other way up
        let near = w.get_day_night_fraction_for_x_at_world_time(100.0, 450.0);
        let far = w.get_day_night_fraction_for_x_at_world_time(800.0, 450.0);
        assert!(near.is_finite() && far.is_finite());
        install_deps(Rc::new(NullDeps));
    }
}
