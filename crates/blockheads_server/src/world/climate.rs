//! Seasons and temperature.
//!
//!  Climate: seasons and temperature".
//! Float32 unless a constant is a double in the spec; evaluation order exactly as in the original.
//! `world: None` behaves as ObjC nil (`W = 0`, zeroed `CustomRules`). `getWorldUpVectorForX` is client math and
//! is not ported (R5).

use super::state::World;
use crate::fmath::libm::{cosf, fabsf, fmod, fmodf};
use crate::types::structs::{CustomRules, IntPair};
use crate::types::tile::Tile;

const PI: f64 = 3.141592653589793;

fn rules_of(world: Option<&World>) -> CustomRules {
    match world {
        Some(w) => w.custom_rules(),
        None => CustomRules::default(),
    }
}

fn width_macro(world: Option<&World>) -> i32 {
    world.map_or(0, |w| w.world_width_macro)
}

/// `customRulesBaseTemp(world)`: 30.0 when rules are disabled; else by `temperature` (0 -> 0, 1 -> 15,
/// 3 -> 30, 4 -> 37, 5 -> 45, others 30).
pub fn custom_rules_base_temp(world: Option<&World>) -> f32 {
    let r = rules_of(world);
    if r.custom_rules_enabled == 0 {
        return 30.0;
    }
    match r.temperature {
        0 => 0.0,
        1 => 15.0,
        3 => 30.0,
        4 => 37.0,
        5 => 45.0,
        _ => 30.0,
    }
}

/// `customRulesPoleOffset(world)`: 20.0 when enabled and `(uint)(int8)temperature` is 0, 1, 3, 4 or 5; else 45.0.
pub fn custom_rules_pole_offset(world: Option<&World>) -> f32 {
    let r = rules_of(world);
    if world.is_some() && r.custom_rules_enabled != 0 {
        let t = (r.temperature as i32) as u32;
        if t < 2 || t == 3 || t == 4 || t == 5 {
            return 20.0;
        }
    }
    45.0
}

/// `absoluteSeasonFraction(gameTime)` = `(float)fmod((t / 900.0) / 4.0, 1.0)`.
pub fn absolute_season_fraction(game_time: f64) -> f32 {
    fmod((game_time / 900.0) / 4.0, 1.0) as f32
}

/// `seasonForWorldX(x, gameTime, world)`: the middle half of the world is offset by half a year.
pub fn season_for_world_x(x: i32, game_time: f64, world: Option<&World>) -> f32 {
    let wpx = (width_macro(world).wrapping_shl(5)) as f32;
    let xf = x as f32;
    if xf > wpx * 0.25f32 && xf < wpx * 0.75f32 {
        fmodf(absolute_season_fraction(game_time) + 0.5f32, 1.0f32)
    } else {
        absolute_season_fraction(game_time)
    }
}

/// Common sub-expressions of the temperature functions: `(yf, pole, w, day_term, s)`.
struct Terms {
    base: f32,
    yf: f32,
    pole: f32,
    w: f32,
    day_term: f32,
    s: f32,
}

fn terms(x: i32, season: f32, day_fraction: f32, weather_fraction: f32, world: Option<&World>) -> Terms {
    let base = custom_rules_base_temp(world);
    let f = fmodf(((x as f32) / 32.0f32) / (width_macro(world) as f32), 0.5f32);
    let yf = fabsf(f - 0.25f32) / 0.25f32;
    let pole = custom_rules_pole_offset(world);
    let w15 = 15.0f32 * weather_fraction;
    let w = if 5.0f32 <= w15 { 5.0f32 } else { w15 };
    let day_term = 15.0f32 * day_fraction;
    let s = cosf((((season as f64) * PI * 2.0) - PI) as f32);
    Terms { base, yf, pole, w, day_term, s }
}

/// `baseTemperatureForWorldPos(pos, season, dayFraction, weatherFraction, world)`.
pub fn base_temperature_for_world_pos(
    pos: IntPair,
    season: f32,
    day_fraction: f32,
    weather_fraction: f32,
    world: Option<&World>,
) -> f32 {
    let t = terms(pos.x, season, day_fraction, weather_fraction, world);
    let mut temp = (t.base - t.yf * t.pole) - t.w;
    if pos.y > 512 {
        temp -= 50.0f32 * (((pos.y as f32) - 512.0f32) / 512.0f32);
    }
    let k = 1.0f64 - (t.yf as f64) * 0.8f64;
    let r: f64 = (temp as f64) + (t.day_term as f64) * k;
    ((r as f32) as f64 + ((t.s * 5.0f32) as f64) * k) as f32
}

/// `freezingLevelForWorldX(worldX, season, dayFraction, weatherFraction, world)`. No caller besides its
/// definition in the original; ported anyway.
pub fn freezing_level_for_world_x(
    world_x: i32,
    season: f32,
    day_fraction: f32,
    weather_fraction: f32,
    world: Option<&World>,
) -> f32 {
    let t = terms(world_x, season, day_fraction, weather_fraction, world);
    let inner: f64 = ((t.base - t.yf * t.pole) - t.w) as f64
        + (t.day_term as f64) * (1.0f64 - (t.yf as f64) * 0.8f64)
        + ((t.s * 5.0f32) as f64);
    512.0f32 + 512.0f32 * ((inner / 50.0f64) as f32)
}

/// `currentTemperatureForTileAtWorldPos(tile, pos, season, dayFraction, weatherFraction, world)`.
/// `tile.artificialHeat` is a signed 16-bit value.
pub fn current_temperature_for_tile_at_world_pos(
    tile: &Tile,
    pos: IntPair,
    season: f32,
    day_fraction: f32,
    weather_fraction: f32,
    world: Option<&World>,
) -> f32 {
    let b = base_temperature_for_world_pos(pos, season, day_fraction, weather_fraction, world);
    let k = ((tile.sun_light as f32) / 255.0f32) * 0.8f32 + 0.2f32;
    ((b * k) as f64 + (20.0f32 as f64) * (1.0f64 - (k as f64)) + ((tile.artificial_heat as i32) as f64)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(x: i32, y: i32) -> IntPair {
        IntPair { x, y }
    }

    fn world_with_rules(f: impl FnOnce(&mut CustomRules)) -> std::rc::Rc<World> {
        let w = World::alloc(32);
        let mut r = CustomRules::default();
        f(&mut r);
        w.set_custom_rules(r);
        w
    }

    #[test]
    fn season_fraction() {
        assert_eq!(absolute_season_fraction(900.0 * 4.0), 0.0);
        assert_eq!(absolute_season_fraction(900.0 * 2.0), 0.5);
        assert_eq!(absolute_season_fraction(900.0 * 5.0), 0.25);
        assert_eq!(absolute_season_fraction(0.0), 0.0);
    }

    #[test]
    fn season_for_world_x_boundaries() {
        let w = World::alloc(32); // Wt = 1024
        let t = 900.0 * 2.0; // abs = 0.5
        let wt = 1024;
        // strict comparisons at 0.25*Wt and 0.75*Wt
        assert_eq!(season_for_world_x(wt / 4, t, Some(&w)), 0.5);
        assert_eq!(season_for_world_x(wt / 4 + 1, t, Some(&w)), 0.0); // fmodf(1.0, 1.0)
        assert_eq!(season_for_world_x(wt * 3 / 4 - 1, t, Some(&w)), 0.0);
        assert_eq!(season_for_world_x(wt * 3 / 4, t, Some(&w)), 0.5);
        // nil world: W = 0, never in the middle
        assert_eq!(season_for_world_x(5, t, None), 0.5);
    }

    #[test]
    fn custom_rule_base_and_pole() {
        assert_eq!(custom_rules_base_temp(None), 30.0);
        assert_eq!(custom_rules_pole_offset(None), 45.0);
        let table = [
            (0i8, 0.0f32, 20.0f32),
            (1, 15.0, 20.0),
            (2, 30.0, 45.0),
            (3, 30.0, 20.0),
            (4, 37.0, 20.0),
            (5, 45.0, 20.0),
            (6, 30.0, 45.0),
            (-1, 30.0, 45.0),
        ];
        for (t, base, pole) in table {
            let w = world_with_rules(|r| {
                r.custom_rules_enabled = 1;
                r.temperature = t;
            });
            assert_eq!(custom_rules_base_temp(Some(&w)), base, "temp {}", t);
            assert_eq!(custom_rules_pole_offset(Some(&w)), pole, "temp {}", t);
        }
        let w = world_with_rules(|r| r.temperature = 0); // disabled
        assert_eq!(custom_rules_base_temp(Some(&w)), 30.0);
        assert_eq!(custom_rules_pole_offset(Some(&w)), 45.0);
    }

    #[test]
    fn base_temperature_special_points() {
        let w = World::alloc(32);
        // x = 0: f = 0, yf = 1; pole 45 => (30 - 45) - 0 = -15; day 0.25 adds 3.75 * 0.2; cos(-pi/2) ~ 0
        let t = base_temperature_for_world_pos(ip(0, 0), 0.25, 0.25, 0.0, Some(&w));
        assert!((t - (-14.25)).abs() < 1e-4, "{}", t);
        // x = 8 macro: u = 0.25 /... f = 0.25 => yf = 0 (equator): 30 + 15*day
        let t = base_temperature_for_world_pos(ip(8 * 32, 0), 0.25, 0.0, 0.0, Some(&w));
        assert!((t - 30.0).abs() < 1e-4, "{}", t);
        // weather caps at 5
        let a = base_temperature_for_world_pos(ip(8 * 32, 0), 0.25, 0.0, 1.0, Some(&w));
        assert!((a - 25.0).abs() < 1e-4);
        let b = base_temperature_for_world_pos(ip(8 * 32, 0), 0.25, 0.0, 0.2, Some(&w));
        assert!((b - 27.0).abs() < 1e-4);
        // above y=512 the altitude term cools by up to 50
        let c = base_temperature_for_world_pos(ip(8 * 32, 1024), 0.25, 0.0, 0.0, Some(&w));
        assert!((c - (-20.0)).abs() < 1e-3);
        // season 0 vs 0.5: cos(-pi) = -1, cos(0) = 1 => +-5 at the equator
        let s0 = base_temperature_for_world_pos(ip(8 * 32, 0), 0.0, 0.0, 0.0, Some(&w));
        let s5 = base_temperature_for_world_pos(ip(8 * 32, 0), 0.5, 0.0, 0.0, Some(&w));
        assert!((s0 - 25.0).abs() < 1e-4 && (s5 - 35.0).abs() < 1e-4);
        // nil world: W = 0 => NaN
        assert!(base_temperature_for_world_pos(ip(0, 0), 0.25, 0.25, 0.0, None).is_nan());
    }

    #[test]
    fn current_temperature_and_freezing() {
        let w = World::alloc(32);
        let mut tile = Tile::default();
        tile.sun_light = 255;
        tile.artificial_heat = 0;
        let pos = ip(8 * 32, 0);
        let b = base_temperature_for_world_pos(pos, 0.25, 0.0, 0.0, Some(&w));
        assert_eq!(current_temperature_for_tile_at_world_pos(&tile, pos, 0.25, 0.0, 0.0, Some(&w)), b);
        tile.sun_light = 0;
        tile.artificial_heat = -10;
        // k = 0.2: b*0.2 + 20*0.8 - 10
        let r = current_temperature_for_tile_at_world_pos(&tile, pos, 0.25, 0.0, 0.0, Some(&w));
        assert!((r - (b * 0.2 + 16.0 - 10.0)).abs() < 1e-3);
        // freezing level at the equator, base 30, season 0.25: 512 + 512 * (30 / 50)
        let f = freezing_level_for_world_x(8 * 32, 0.25, 0.0, 0.0, Some(&w));
        assert!((f - (512.0 + 512.0 * 0.6)).abs() < 1e-2, "{}", f);
    }
}
