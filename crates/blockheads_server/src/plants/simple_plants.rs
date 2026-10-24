//! owned by plants
//!
//! The 7 `NormalPlant` configuration subclasses (Flax, Sunflower, Corn, Carrot, Chilli, Wheat, Tomato) as a
//! `const` table, the two light functions and the per-kind `ClassInfo` statics.
//!
//! (incl. `-lightFactor`, `-lightColor`);;;
//! `# 3. Plants` and `## 3.1 SunflowerPlant light`.

use crate::fmath::cconv::f32_to_i32;
use crate::objects::class_info::ClassInfo;

use super::normal_plant::NORMAL_PLANT_CLASS;

/// Which of the seven configuration subclasses a [`super::normal_plant::NormalPlant`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalPlantKind {
    Flax,
    Sunflower,
    Corn,
    Carrot,
    Chilli,
    Wheat,
    Tomato,
}

/// When `-emitsLight` is YES.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitsLight {
    Never,
    Always,
    /// Sunflower: `emitsLight == flowering`.
    WhenFlowering,
}

/// One row of the configuration table.
pub struct NormalPlantConfig {
    pub kind: NormalPlantKind,
    pub class: &'static ClassInfo,
    pub object_type: i32,
    pub plant_type: i32,
    pub max_age_base: f32,
    pub seed_item_type: i32,
    pub folliage_item_type: i32,
    pub min_allowed_temperature: i32,
    pub food_to_remove_when_spawning_npc: f32,
    pub npc_spawn_type: i32,
    pub flowering_season: fn(i32) -> bool,
    pub can_die_season: fn(i32) -> bool,
    pub required_soil: &'static [u8],
    pub emits_light: EmitsLight,
}

macro_rules! class {
    ($name:ident, $s:expr) => {
        #[doc = concat!("`", $s, " : NormalPlant` class descriptor.")]
        pub static $name: ClassInfo = ClassInfo { name: $s, superclass: Some(&NORMAL_PLANT_CLASS), protocols: &[] };
    };
}
class!(FLAX_PLANT_CLASS, "FlaxPlant");
class!(SUNFLOWER_PLANT_CLASS, "SunflowerPlant");
class!(CORN_PLANT_CLASS, "CornPlant");
class!(CARROT_PLANT_CLASS, "CarrotPlant");
class!(CHILLI_PLANT_CLASS, "ChilliPlant");
class!(WHEAT_PLANT_CLASS, "WheatPlant");
class!(TOMATO_PLANT_CLASS, "TomatoPlant");

fn always(_s: i32) -> bool {
    true
}
fn s23(s: i32) -> bool {
    s == 2 || s == 3
}
fn s45(s: i32) -> bool {
    s == 4 || s == 5
}
fn s67(s: i32) -> bool {
    s == 6 || s == 7
}
fn s12(s: i32) -> bool {
    s == 1 || s == 2
}
fn is0(s: i32) -> bool {
    s == 0
}
fn is5(s: i32) -> bool {
    s == 5
}
fn is6(s: i32) -> bool {
    s == 6
}
fn is7(s: i32) -> bool {
    s == 7
}

/// `Plant` default soil set `{DIRT, DIRT_GRASS, DIRT_GRASS_FROZEN, COMPOST, COMPOST_GRASS, COMPOST_GRASS_FROZEN}`.
const SOIL_DEFAULT: &[u8] = &[6, 27, 28, 48, 49, 50];
/// Flax additionally grows on SAND, SAND_BEACH, BLACK_SAND.
const SOIL_FLAX: &[u8] = &[6, 27, 28, 48, 49, 50, 7, 8, 58];
/// Chilli and Tomato: no frozen variants.
const SOIL_NO_FROZEN: &[u8] = &[6, 27, 48, 49];

static CONFIGS: [NormalPlantConfig; 7] = [
    NormalPlantConfig {
        kind: NormalPlantKind::Flax,
        class: &FLAX_PLANT_CLASS,
        object_type: 10,
        plant_type: 1,
        max_age_base: 14400.0,
        seed_item_type: 54,
        folliage_item_type: 55,
        min_allowed_temperature: -20,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 0,
        flowering_season: s23,
        can_die_season: always,
        required_soil: SOIL_FLAX,
        emits_light: EmitsLight::Never,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Sunflower,
        class: &SUNFLOWER_PLANT_CLASS,
        object_type: 11,
        plant_type: 2,
        max_age_base: 7200.0,
        seed_item_type: 61,
        folliage_item_type: 0,
        min_allowed_temperature: -5,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 0,
        flowering_season: s45,
        can_die_season: is6,
        required_soil: SOIL_DEFAULT,
        emits_light: EmitsLight::WhenFlowering,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Corn,
        class: &CORN_PLANT_CLASS,
        object_type: 12,
        plant_type: 3,
        max_age_base: 7200.0,
        seed_item_type: 62,
        folliage_item_type: 0,
        min_allowed_temperature: -10,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 0,
        flowering_season: s23,
        can_die_season: is7,
        required_soil: SOIL_DEFAULT,
        emits_light: EmitsLight::Never,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Carrot,
        class: &CARROT_PLANT_CLASS,
        object_type: 27,
        plant_type: 4,
        max_age_base: 7200.0,
        seed_item_type: 71,
        folliage_item_type: 0,
        min_allowed_temperature: -20,
        food_to_remove_when_spawning_npc: 1800.0,
        npc_spawn_type: 3,
        flowering_season: s67,
        can_die_season: is0,
        required_soil: SOIL_DEFAULT,
        emits_light: EmitsLight::Never,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Chilli,
        class: &CHILLI_PLANT_CLASS,
        object_type: 33,
        plant_type: 5,
        max_age_base: 7200.0,
        seed_item_type: 112,
        folliage_item_type: 0,
        min_allowed_temperature: -1,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 0,
        flowering_season: s12,
        can_die_season: is5,
        required_soil: SOIL_NO_FROZEN,
        emits_light: EmitsLight::Always,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Wheat,
        class: &WHEAT_PLANT_CLASS,
        object_type: 61,
        plant_type: 9,
        max_age_base: 7200.0,
        seed_item_type: 310,
        folliage_item_type: 0,
        min_allowed_temperature: -15,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 8,
        flowering_season: s23,
        can_die_season: is7,
        required_soil: SOIL_DEFAULT,
        emits_light: EmitsLight::Never,
    },
    NormalPlantConfig {
        kind: NormalPlantKind::Tomato,
        class: &TOMATO_PLANT_CLASS,
        object_type: 62,
        plant_type: 10,
        max_age_base: 7200.0,
        seed_item_type: 316,
        folliage_item_type: 0,
        min_allowed_temperature: -2,
        food_to_remove_when_spawning_npc: 900.0,
        npc_spawn_type: 0,
        flowering_season: s12,
        can_die_season: is5,
        required_soil: SOIL_NO_FROZEN,
        emits_light: EmitsLight::Never,
    },
];

/// The table of (one row per kind).
pub fn config_for_kind(k: NormalPlantKind) -> &'static NormalPlantConfig {
    match k {
        NormalPlantKind::Flax => &CONFIGS[0],
        NormalPlantKind::Sunflower => &CONFIGS[1],
        NormalPlantKind::Corn => &CONFIGS[2],
        NormalPlantKind::Carrot => &CONFIGS[3],
        NormalPlantKind::Chilli => &CONFIGS[4],
        NormalPlantKind::Wheat => &CONFIGS[5],
        NormalPlantKind::Tomato => &CONFIGS[6],
    }
}

/// `DynamicObjectType` -> kind (10, 11, 12, 27, 33, 61, 62).
pub fn kind_for_object_type(object_type: i32) -> Option<NormalPlantKind> {
    CONFIGS.iter().find(|c| c.object_type == object_type).map(|c| c.kind)
}

/// `PlantType` -> kind (1, 2, 3, 4, 5, 9, 10).
pub fn kind_for_plant_type(plant_type: i32) -> Option<NormalPlantKind> {
    CONFIGS.iter().find(|c| c.plant_type == plant_type).map(|c| c.kind)
}

/// `-lightFactor` (Chilli and Sunflower): `(0.1f + (age / maxAgeBase) * 0.5f) + (flowering ? 0.4f: 0.0f)`;
/// 0.0 for the kinds that never emit.
pub fn light_factor(kind: NormalPlantKind, age: f32, flowering: bool) -> f32 {
    match kind {
        NormalPlantKind::Chilli | NormalPlantKind::Sunflower => {
            let m = config_for_kind(kind).max_age_base;
            let bonus = if flowering { 0.4f32 } else { 0.0f32 };
            (0.1f32 + (age / m) * 0.5f32) + bonus
        }
        _ => 0.0,
    }
}

/// `min(255, x)` as the binary does it: `255.0 <= x ? 255.0: x`.
fn cap255(x: f32) -> f32 {
    if 255.0f32 <= x {
        255.0f32
    } else {
        x
    }
}

/// `-lightColor` as `[R, G, B, heat]` floats holding whole numbers.
pub fn light_color(kind: NormalPlantKind, lf: f32) -> [f32; 4] {
    match kind {
        NormalPlantKind::Chilli => {
            let rv = cap255(255.0f32 * lf);
            let gv = cap255(75.0f32 * lf);
            [f32_to_i32(rv) as f32, f32_to_i32(gv) as f32, 0.0, lf + lf]
        }
        NormalPlantKind::Sunflower => {
            let c = cap255(255.0f32 * lf);
            let x = f32_to_i32(c) as f32;
            let z = crate::fmath::cconv::f64_to_i32((f32_to_i32(c) as f64) * 0.25f64) as f32;
            [x, x, z, 0.0]
        }
        _ => [0.0; 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    ///  U1: the table equals the specs; season predicates by index.
    #[test]
    fn table() {
        let c = config_for_kind(NormalPlantKind::Carrot);
        assert_eq!((c.object_type, c.plant_type, c.seed_item_type, c.npc_spawn_type), (27, 4, 71, 3));
        assert_eq!(c.food_to_remove_when_spawning_npc, 1800.0);
        let seasons = |f: fn(i32) -> bool| (0..8).filter(|s| f(*s)).collect::<Vec<_>>();
        assert_eq!(seasons(c.flowering_season), vec![6, 7]);
        assert_eq!(seasons(c.can_die_season), vec![0]);
        assert_eq!(seasons(config_for_kind(NormalPlantKind::Flax).can_die_season).len(), 8);
        assert_eq!(seasons(config_for_kind(NormalPlantKind::Sunflower).flowering_season), vec![4, 5]);
        assert_eq!(seasons(config_for_kind(NormalPlantKind::Chilli).flowering_season), vec![1, 2]);
        assert_eq!(seasons(config_for_kind(NormalPlantKind::Tomato).can_die_season), vec![5]);
        assert_eq!(config_for_kind(NormalPlantKind::Chilli).required_soil, &[6, 27, 48, 49]);
        assert_eq!(config_for_kind(NormalPlantKind::Wheat).npc_spawn_type, 8);
        for k in [
            NormalPlantKind::Flax,
            NormalPlantKind::Sunflower,
            NormalPlantKind::Corn,
            NormalPlantKind::Carrot,
            NormalPlantKind::Chilli,
            NormalPlantKind::Wheat,
            NormalPlantKind::Tomato,
        ] {
            let c = config_for_kind(k);
            assert_eq!(kind_for_object_type(c.object_type), Some(k));
            assert_eq!(kind_for_plant_type(c.plant_type), Some(k));
            assert!(c.class.is_kind_of(&NORMAL_PLANT_CLASS));
        }
        assert_eq!(kind_for_object_type(34), None);
    }

    ///  observed chilli light: age 6410.735 -> lf 0.5448, color (139, 40, 0, heat 1).
    #[test]
    fn chilli_light_golden() {
        let lf = light_factor(NormalPlantKind::Chilli, 6410.735, false);
        assert!((lf - 0.5452).abs() < 1e-3);
        let c = light_color(NormalPlantKind::Chilli, lf);
        assert_eq!((c[0] as i32, c[1] as i32, c[2] as i32, c[3] as i32), (139, 40, 0, 1));
    }

    ///  sunflower colour.
    #[test]
    fn sunflower_light() {
        let lf = light_factor(NormalPlantKind::Sunflower, 0.0, true);
        assert!((lf - 0.5).abs() < 1e-6);
        let c = light_color(NormalPlantKind::Sunflower, lf);
        assert_eq!(c, [127.0, 127.0, 31.0, 0.0]);
        let c = light_color(NormalPlantKind::Sunflower, 3.0);
        assert_eq!(c, [255.0, 255.0, 63.0, 0.0]);
    }
}
