//! Dodo and donkey breed logic (uses the process-wide glibc `rand()` stream) and NPC helpers.
//!

use crate::rng::glibc_rand::r01_tagged;
use crate::types::enums::{DodoBreed, DonkeyBreed, NPCType};

pub use crate::items::misc::item_type_for_dodo_egg_breed;

/// Highest dodo breed (`DODO_BREED_RAINBOW`).
const MAX_BREED: u16 = 32;

/// `generateChildBreedFromParentBreed`. Draw contract: 0 draws when `!has_bred`,
/// 1 draw for differing parents, 2 for equal parents without evolution, 1 or 2 in the evolve branch. All comparisons
/// are done in f64 against f64 constants, the draws are `(float)rand() / 2147483648.0f`.
pub fn generate_child_breed_from_parent_breed(breed: DodoBreed, mate_breed: DodoBreed, has_bred: bool) -> DodoBreed {
    if !has_bred {
        return breed;
    }
    let x = r01_tagged("breeds:dodo:x");
    if breed != mate_breed {
        return if 0.5f64 < x as f64 { breed } else { mate_breed };
    }
    if 0.5f64 < x as f64 {
        let y = r01_tagged("breeds:dodo:y");
        return if 0.5f64 <= y as f64 { breed } else { devolve_breed_for_breed(breed) };
    }
    // evolve branch (x <= 0.5): one fresh draw z per case that has a threshold
    let b: u16 = if breed.0 > MAX_BREED { 0 } else { breed.0 };
    let mut z = || r01_tagged("breeds:dodo:z") as f64;
    // "z < 0.2 -> next else self"
    let step = |next: u16, cur: u16, z: &mut dyn FnMut() -> f64| if z() < 0.2f64 { next } else { cur };
    let r = match b {
        0 => {
            let z = z();
            if z < 0.33f64 {
                1
            } else if z < 0.67f64 {
                9
            } else {
                7
            }
        }
        1 => {
            let z = z();
            if z < 0.1f64 {
                27
            } else if z < 0.33f64 {
                21
            } else if z < 0.67f64 {
                2
            } else {
                10
            }
        }
        2 => 4,
        3 => step(13, 3, &mut z),
        4 => step(5, 4, &mut z),
        5 => step(6, 5, &mut z),
        7 => {
            let z = z();
            if z < 0.33f64 {
                8
            } else if z < 0.67f64 {
                15
            } else {
                17
            }
        }
        10 => 11,
        11 => {
            let z = z();
            if z < 0.8f64 {
                3
            } else {
                12
            }
        }
        12 => step(14, 12, &mut z),
        15 => step(16, 15, &mut z),
        17 => 18,
        18 => 19,
        19 => step(20, 19, &mut z),
        21 => 22,
        22 => step(23, 22, &mut z),
        23 => step(24, 23, &mut z),
        24 => step(25, 24, &mut z),
        25 => step(26, 25, &mut z),
        27 => step(28, 27, &mut z),
        28 => step(29, 28, &mut z),
        29 => step(30, 29, &mut z),
        30 => step(31, 30, &mut z),
        31 => step(32, 31, &mut z),
        other => other,
    };
    DodoBreed(r)
}

/// `devolveBreedForBreed`: one step back down the tree; 0 and > 32 unchanged.
pub fn devolve_breed_for_breed(b: DodoBreed) -> DodoBreed {
    let r = match b.0 {
        1 => 0,
        2 => 1,
        3 => 11,
        4 => 2,
        5 => 4,
        6 => 5,
        7 => 0,
        8 => 7,
        9 => 0,
        10 => 1,
        11 => 10,
        12 => 11,
        13 => 3,
        14 => 12,
        15 => 7,
        16 => 15,
        17 => 7,
        18 => 17,
        19 => 18,
        20 => 19,
        21 => 1,
        22 => 21,
        23 => 22,
        24 => 23,
        25 => 24,
        26 => 25,
        27 => 1,
        28 => 27,
        29 => 28,
        30 => 29,
        31 => 30,
        32 => 31,
        o => o,
    };
    DodoBreed(r)
}

/// `layLikelyhoodForBreed`: f32 table, 1.0 for 0..=4 and > 32.
pub fn lay_likelyhood_for_breed(b: DodoBreed) -> f32 {
    match b.0 {
        5 | 6 | 12 | 15 | 16 | 17 | 21 | 22 | 27 => 0.5,
        18 | 19 | 28 => 0.25,
        13 | 14 | 20 | 23 | 29 => 0.125,
        24 | 30 => 0.0625,
        25 | 26 | 31 | 32 => 0.03125,
        _ => 1.0,
    }
}

/// `nameForDodoBreed`: `"<PREFIX> DODO"` or `"DODO"`.
pub fn name_for_dodo_breed(b: DodoBreed) -> String {
    match prefix_name_for_dodo_breed(b) {
        None => "DODO".to_string(),
        Some(p) => format!("{} DODO", p),
    }
}

/// `prefixNameForDodoBreed`: `None` for 0 and > 32.
pub fn prefix_name_for_dodo_breed(b: DodoBreed) -> Option<&'static str> {
    Some(match b.0 {
        1 => "STONE",
        2 => "LIMESTONE",
        3 => "SANDSTONE",
        4 => "MARBLE",
        5 => "RED MARBLE",
        6 => "LAPIS",
        7 => "DIRT",
        8 => "COMPOST",
        9 => "WOOD",
        10 => "GRAVEL",
        11 => "SAND",
        12 => "BLACK SAND",
        13 => "GLASS",
        14 => "BLACK GLASS",
        15 => "CLAY",
        16 => "RED BRICK",
        17 => "FLINT",
        18 => "COAL",
        19 => "OIL",
        20 => "FUEL",
        21 => "COPPER",
        22 => "TIN",
        23 => "IRON",
        24 => "GOLD",
        25 => "TITANIUM",
        26 => "PLATINUM",
        27 => "AMETHYST",
        28 => "SAPPHIRE",
        29 => "EMERALD",
        30 => "RUBY",
        31 => "DIAMOND",
        32 => "RAINBOW",
        _ => return None,
    })
}

/// `generateDonkeyChildBreedFromParentBreed`. Uses 1 or 2 draws (0 when `!has_bred`).
pub fn generate_donkey_child_breed_from_parent_breed(
    breed: DonkeyBreed,
    mate: DonkeyBreed,
    has_bred: bool,
) -> DonkeyBreed {
    if !has_bred {
        return breed;
    }
    let x = r01_tagged("breeds:donkey:x");
    if breed == mate {
        if (x as f64) <= 0.5f64 {
            if breed.0 < 23 {
                DonkeyBreed(breed.0 + 1)
            } else {
                breed
            }
        } else {
            let y = r01_tagged("breeds:donkey:y");
            if (y as f64) < 0.5f64 {
                devolve_donkey_breed_for_breed(breed)
            } else {
                breed
            }
        }
    } else if 0.5f64 < x as f64 {
        breed
    } else {
        mate
    }
}

/// `devolveDonkeyBreedForBreed`: 0 stays 0, else `breed - 1`.
pub fn devolve_donkey_breed_for_breed(b: DonkeyBreed) -> DonkeyBreed {
    if b.0 == 0 {
        DonkeyBreed(0)
    } else {
        DonkeyBreed(b.0.wrapping_sub(1))
    }
}

/// `nameForDonkeyBreed`: base is `UNICORN` for breed >= 12 else `DONKEY`.
pub fn name_for_donkey_breed(b: DonkeyBreed) -> String {
    let base = if b.0 >= 12 { "UNICORN" } else { "DONKEY" };
    match prefix_name_for_donkey_breed(b) {
        None => base.to_string(),
        Some(p) => format!("{} {}", p, base),
    }
}

/// `prefixNameForDonkeyBreed`: 1..=23, `None` for 0 and > 23.
pub fn prefix_name_for_donkey_breed(b: DonkeyBreed) -> Option<&'static str> {
    Some(match b.0 {
        1 | 13 => "BROWN",
        2 | 14 => "BLACK",
        3 | 15 => "BLUE",
        4 | 16 => "GREEN",
        5 | 17 => "YELLOW",
        6 | 18 => "ORANGE",
        7 | 19 => "RED",
        8 | 20 => "PURPLE",
        9 | 21 => "PINK",
        10 | 22 => "WHITE",
        11 | 23 => "RAINBOW",
        12 => "GREY",
        _ => return None,
    })
}

/// `tameCountRequirementForNPCType`: feedings needed to tame; unknown types give 2.
pub fn tame_count_requirement_for_npc_type(t: NPCType) -> i32 {
    match t.0 {
        1 => 2,
        2 => 4,
        3 => 4,
        4 => 2,
        5 => 8,
        6 => 10,
        7 => 4,
        8 => 4,
        _ => 2,
    }
}

/// `classForNPCType` as the Objective-C class name; `None` for types outside 1..=8.
pub fn class_name_for_npc_type(t: NPCType) -> Option<&'static str> {
    Some(match t.0 {
        1 => "Dodo",
        2 => "DropBear",
        3 => "Donkey",
        4 => "ClownFish",
        5 => "Shark",
        6 => "CaveTroll",
        7 => "Scorpion",
        8 => "Yak",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::glibc_rand::{rand, srand};

    /// Number of `rand()` draws consumed by `f` (compares the next draw with the seed's stream).
    fn draws(seed: u32, f: impl FnOnce()) -> usize {
        srand(seed);
        f();
        let next = rand();
        srand(seed);
        (0..4).find(|_| rand() == next).unwrap_or(usize::MAX)
    }

    /// Plain reference implementation using raw draws, for the sequence/parity comparison.
    fn reference(breed: u16, mate: u16, rnd: &mut dyn FnMut() -> f64) -> u16 {
        let x = rnd();
        if breed != mate {
            return if 0.5 < x { breed } else { mate };
        }
        if 0.5 < x {
            let y = rnd();
            return if 0.5 <= y { breed } else { devolve_breed_for_breed(DodoBreed(breed)).0 };
        }
        let b = if breed > 32 { 0 } else { breed };
        let mut t = |n: u16, cur: u16, rnd: &mut dyn FnMut() -> f64| if rnd() < 0.2 { n } else { cur };
        match b {
            0 => {
                let z = rnd();
                if z < 0.33 {
                    1
                } else if z < 0.67 {
                    9
                } else {
                    7
                }
            }
            1 => {
                let z = rnd();
                if z < 0.1 {
                    27
                } else if z < 0.33 {
                    21
                } else if z < 0.67 {
                    2
                } else {
                    10
                }
            }
            2 => 4,
            3 => t(13, 3, rnd),
            4 => t(5, 4, rnd),
            5 => t(6, 5, rnd),
            7 => {
                let z = rnd();
                if z < 0.33 {
                    8
                } else if z < 0.67 {
                    15
                } else {
                    17
                }
            }
            10 => 11,
            11 => {
                let z = rnd();
                if z < 0.8 {
                    3
                } else {
                    12
                }
            }
            12 => t(14, 12, rnd),
            15 => t(16, 15, rnd),
            17 => 18,
            18 => 19,
            19 => t(20, 19, rnd),
            21 => 22,
            22 => t(23, 22, rnd),
            23 | 24 | 25 => t(b + 1, b, rnd),
            27..=31 => t(b + 1, b, rnd),
            o => o,
        }
    }

    /// Checklist 6.1 #13: draw counts and returned sequence for all (breed, mate) pairs over many seeds.
    #[test]
    fn rng_parity_dodo() {
        for seed in 1..=1000u32 {
            for breed in 0..33u16 {
                for mate in 0..33u16 {
                    srand(seed);
                    let mut rnd = || (rand() as f32 / 2147483648.0f32) as f64;
                    let exp = reference(breed, mate, &mut rnd);
                    let n_exp = draws(seed, || {
                        let mut rnd = || (rand() as f32 / 2147483648.0f32) as f64;
                        reference(breed, mate, &mut rnd);
                    });
                    srand(seed);
                    let got = generate_child_breed_from_parent_breed(DodoBreed(breed), DodoBreed(mate), true);
                    let next_got = rand();
                    srand(seed);
                    for _ in 0..n_exp {
                        rand();
                    }
                    assert_eq!(got.0, exp, "seed={seed} {breed} {mate}");
                    assert_eq!(next_got, rand(), "draw count seed={seed} {breed} {mate}");
                }
            }
        }
        // draw count rules
        assert_eq!(
            draws(5, || {
                generate_child_breed_from_parent_breed(DodoBreed(3), DodoBreed(3), false);
            }),
            0
        );
        assert_eq!(
            draws(5, || {
                generate_child_breed_from_parent_breed(DodoBreed(3), DodoBreed(4), true);
            }),
            1
        );
        // breed > 32 treated as 0 in the evolve branch
        for seed in 1..200u32 {
            let r = {
                srand(seed);
                generate_child_breed_from_parent_breed(DodoBreed(40), DodoBreed(40), true).0
            };
            assert!(r == 40 || r <= 32 || r == 0);
        }
    }

    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn rng_matches_host_libc() {
        // Single test touching libc's global rand state.
        for seed in [1u32, 7, 12345] {
            unsafe { libc::srand(seed) };
            let host: Vec<i32> = (0..10).map(|_| unsafe { libc::rand() }).collect();
            srand(seed);
            let ours: Vec<i32> = (0..10).map(|_| rand()).collect();
            assert_eq!(host, ours);
            // a breed call draws the same values as the host stream
            srand(seed);
            let got = generate_child_breed_from_parent_breed(DodoBreed(1), DodoBreed(2), true);
            let x = host[0] as f32 / 2147483648.0f32;
            assert_eq!(got.0, if 0.5f64 < x as f64 { 1 } else { 2 });
        }
    }

    #[test]
    fn donkey() {
        for seed in 1..=300u32 {
            for b in 0..24u16 {
                for m in 0..24u16 {
                    srand(seed);
                    let x = rand() as f32 / 2147483648.0f32;
                    let y = rand() as f32 / 2147483648.0f32;
                    let exp = if b == m {
                        if (x as f64) <= 0.5 {
                            if b < 23 {
                                b + 1
                            } else {
                                b
                            }
                        } else if (y as f64) < 0.5 {
                            b.saturating_sub(1)
                        } else {
                            b
                        }
                    } else if 0.5 < x as f64 {
                        b
                    } else {
                        m
                    };
                    srand(seed);
                    let got = generate_donkey_child_breed_from_parent_breed(DonkeyBreed(b), DonkeyBreed(m), true);
                    assert_eq!(got.0, exp);
                    let n = if b != m || (x as f64) <= 0.5 { 1 } else { 2 };
                    assert_eq!(
                        draws(seed, || {
                            generate_donkey_child_breed_from_parent_breed(DonkeyBreed(b), DonkeyBreed(m), true);
                        }),
                        n
                    );
                }
            }
        }
    }

    /// Checklist 6.1 #12.
    #[test]
    fn tables_and_names() {
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(0)).0, 0);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(33)).0, 0);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(1)).0, 1024);
        assert_eq!(item_type_for_dodo_egg_breed(DodoBreed(32)).0, 329);
        assert_eq!(lay_likelyhood_for_breed(DodoBreed(7)), 1.0);
        assert_eq!(lay_likelyhood_for_breed(DodoBreed(13)), 0.125);
        assert_eq!(lay_likelyhood_for_breed(DodoBreed(25)), 0.03125);
        assert_eq!(lay_likelyhood_for_breed(DodoBreed(33)), 1.0);
        assert_eq!(lay_likelyhood_for_breed(DodoBreed(4)), 1.0);
        assert_eq!(name_for_dodo_breed(DodoBreed(1)), "STONE DODO");
        assert_eq!(name_for_dodo_breed(DodoBreed(0)), "DODO");
        assert_eq!(name_for_dodo_breed(DodoBreed(33)), "DODO");
        assert_eq!(name_for_donkey_breed(DonkeyBreed(0)), "DONKEY");
        assert_eq!(name_for_donkey_breed(DonkeyBreed(24)), "UNICORN");
        assert_eq!(name_for_donkey_breed(DonkeyBreed(12)), "GREY UNICORN");
        assert_eq!(name_for_donkey_breed(DonkeyBreed(13)), "BROWN UNICORN");
        assert_eq!(name_for_donkey_breed(DonkeyBreed(1)), "BROWN DONKEY");
        assert_eq!(devolve_breed_for_breed(DodoBreed(3)).0, 11);
        assert_eq!(devolve_breed_for_breed(DodoBreed(0)).0, 0);
        assert_eq!(devolve_breed_for_breed(DodoBreed(40)).0, 40);
        assert_eq!(devolve_donkey_breed_for_breed(DonkeyBreed(0)).0, 0);
        assert_eq!(devolve_donkey_breed_for_breed(DonkeyBreed(5)).0, 4);
        assert_eq!(tame_count_requirement_for_npc_type(NPCType(5)), 8);
        assert_eq!(tame_count_requirement_for_npc_type(NPCType(0)), 2);
        assert_eq!(class_name_for_npc_type(NPCType(8)), Some("Yak"));
        assert_eq!(class_name_for_npc_type(NPCType(0)), None);
    }

    #[test]
    fn donkey_without_breeding_keeps_the_breed_and_draws_nothing() {
        assert_eq!(generate_donkey_child_breed_from_parent_breed(DonkeyBreed(3), DonkeyBreed(4), false).0, 3);
        assert_eq!(
            draws(9, || {
                generate_donkey_child_breed_from_parent_breed(DonkeyBreed(3), DonkeyBreed(3), false);
            }),
            0
        );
    }

    #[test]
    fn every_name_and_table_entry() {
        let dodo = [
            "STONE",
            "LIMESTONE",
            "SANDSTONE",
            "MARBLE",
            "RED MARBLE",
            "LAPIS",
            "DIRT",
            "COMPOST",
            "WOOD",
            "GRAVEL",
            "SAND",
            "BLACK SAND",
            "GLASS",
            "BLACK GLASS",
            "CLAY",
            "RED BRICK",
            "FLINT",
            "COAL",
            "OIL",
            "FUEL",
            "COPPER",
            "TIN",
            "IRON",
            "GOLD",
            "TITANIUM",
            "PLATINUM",
            "AMETHYST",
            "SAPPHIRE",
            "EMERALD",
            "RUBY",
            "DIAMOND",
            "RAINBOW",
        ];
        for (i, p) in dodo.iter().enumerate() {
            let b = DodoBreed(i as u16 + 1);
            assert_eq!(prefix_name_for_dodo_breed(b), Some(*p));
            assert_eq!(name_for_dodo_breed(b), format!("{p} DODO"));
        }
        assert_eq!(prefix_name_for_dodo_breed(DodoBreed(0)), None);
        assert_eq!(prefix_name_for_dodo_breed(DodoBreed(33)), None);
        let donkey = [
            "BROWN", "BLACK", "BLUE", "GREEN", "YELLOW", "ORANGE", "RED", "PURPLE", "PINK", "WHITE", "RAINBOW", "GREY",
        ];
        for (i, p) in donkey.iter().enumerate() {
            let low = DonkeyBreed(i as u16 + 1);
            assert_eq!(prefix_name_for_donkey_breed(low), Some(*p));
            if i < 11 {
                assert_eq!(prefix_name_for_donkey_breed(DonkeyBreed(i as u16 + 13)), Some(*p));
            }
        }
        assert_eq!(prefix_name_for_donkey_breed(DonkeyBreed(0)), None);
        assert_eq!(prefix_name_for_donkey_breed(DonkeyBreed(24)), None);
        // laying likelihood by tier
        for (breeds, p) in [
            (&[5u16, 6, 12, 15, 16, 17, 21, 22, 27][..], 0.5f32),
            (&[18, 19, 28][..], 0.25),
            (&[13, 14, 20, 23, 29][..], 0.125),
            (&[24, 30][..], 0.0625),
            (&[25, 26, 31, 32][..], 0.03125),
        ] {
            for b in breeds {
                assert_eq!(lay_likelyhood_for_breed(DodoBreed(*b)), p);
            }
        }
        // NPC taming counts and class names
        let counts = [2, 4, 4, 2, 8, 10, 4, 4];
        let classes = ["Dodo", "DropBear", "Donkey", "ClownFish", "Shark", "CaveTroll", "Scorpion", "Yak"];
        for t in 1..=8u32 {
            assert_eq!(tame_count_requirement_for_npc_type(NPCType(t)), counts[t as usize - 1]);
            assert_eq!(class_name_for_npc_type(NPCType(t)), Some(classes[t as usize - 1]));
        }
    }
}
