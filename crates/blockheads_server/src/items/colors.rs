//! Pigments, dye colour packing and paint mixing.
//!
//!  (`colorIndexForPigments`.

use crate::types::enums::PigmentColor;
use crate::types::item_type::ItemType;
use crate::types::structs::Vector;

pub use crate::fmath::vector::simple_smooth_step;
pub use crate::items::misc::{mj_color8_make, mj_color8_make3};

/// `pigmentColorForPigmentIndex`: ids 1..=8 are the pigments, everything else is all-zero including
/// alpha.
pub fn pigment_color_for_pigment_index(p: u16) -> Vector {
    let (r, g, b, a) = match p {
        1 => (1.0f32, 1.0f32, 1.0f32, 1.0f32),
        2 => (0.0, 0.0, 0.0, 1.0),
        3 => (0.76, 0.11, 0.01, 1.0),
        4 => (0.91, 0.83, 0.15, 1.0),
        5 => (0.01, 0.26, 0.71, 1.0),
        6 => (0.01, 0.90, 0.40, 1.0),
        7 => (0.47, 0.06, 0.61, 1.0),
        8 => (0.00, 0.87, 0.95, 1.0),
        _ => (0.0, 0.0, 0.0, 0.0),
    };
    Vector::new4(r, g, b, a)
}

/// `defaultColorForItemType`: only items 83..=170 are special.
pub fn default_color_for_item_type(item: ItemType) -> Vector {
    match item.0 {
        83 | 84 | 85 | 115 | 117 | 169 => Vector::new4(0.8, 0.82, 0.75, 1.0),
        122 | 124 | 130 => Vector::new4(0.35, 0.15, 0.0, 1.0),
        126 | 127 => Vector::new4(0.15, 0.05, 0.0, 1.0),
        170 => Vector::new4(0.05, 0.01, 0.0, 1.0),
        _ => Vector::new4(1.0, 1.0, 1.0, 1.0),
    }
}

/// `1 - pigment` term used by the subtractive mix: x and y via f64, z via f32.
fn sub_term(pigment: u16) -> [f32; 3] {
    let c = pigment_color_for_pigment_index(pigment).v;
    [(1.0f64 - c[0] as f64) as f32, (1.0f64 - c[1] as f64) as f32, 1.0f32 - c[2]]
}

/// `mixedColorForIndex`: subtractive mix of up to three pigment nibbles (bits 15..12, 11..8, 7..4).
/// B and C are only examined when A is non-zero; no pigments gives the item's default colour.
pub fn mixed_color_for_index(color_index: u16, item: ItemType) -> Vector {
    if item.0 == 278 {
        return Vector::new4(1.0, 1.0, 1.0, 1.0);
    }
    let mut sub = [1.0f32; 3];
    let mut count: i32 = 0;
    if (color_index & 0xF000) != 0 {
        let a = (color_index & 0xF000) >> 12;
        sub = sub_term(a);
        count = 1;
        let b = (color_index & 0x0F00) >> 8;
        if b != 0 {
            let t = sub_term(b);
            for k in 0..3 {
                sub[k] += t[k];
            }
            count += 1;
        }
        let c = (color_index & 0x00F0) >> 4;
        if c != 0 {
            let t = sub_term(c);
            for k in 0..3 {
                sub[k] += t[k];
            }
            count += 1;
        }
    }
    if count == 0 {
        return default_color_for_item_type(item);
    }
    let mut out = [0.0f32; 3];
    for k in 0..3 {
        let avg = sub[k] / (count as f32);
        let m: f64 = if (avg as f64) >= 1.0 { 1.0 } else { avg as f64 };
        out[k] = (1.0f64 - m) as f32;
    }
    Vector::new4(out[0], out[1], out[2], 1.0)
}

/// `colorIndexForPigments`: `(a << 12) | (b << 8) | (c << 4)` truncated to 16 bits.
pub fn color_index_for_pigments(a: PigmentColor, b: PigmentColor, c: PigmentColor) -> u16 {
    (((a.0 & 0xffff) << 12) | ((b.0 & 0xffff) << 8) | ((c.0 & 0xffff) << 4)) as u16
}

/// `tulipGeneOrderForPigment`: 99 means "not a gene pigment".
pub fn tulip_gene_order_for_pigment(p: u16) -> i32 {
    match p {
        1 => 3,
        2 => 5,
        3 => 0,
        4 => 1,
        5 => 2,
        6 => 6,
        7 => 7,
        8 => 4,
        _ => 99,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Straightforward reference implementation (independent of the module's helper).
    fn reference(ci: u16, item: u32) -> [f32; 4] {
        if item == 278 {
            return [1.0; 4];
        }
        let pc = |p: u16| -> [f32; 4] {
            match p {
                1 => [1.0, 1.0, 1.0, 1.0],
                2 => [0.0, 0.0, 0.0, 1.0],
                3 => [0.76, 0.11, 0.01, 1.0],
                4 => [0.91, 0.83, 0.15, 1.0],
                5 => [0.01, 0.26, 0.71, 1.0],
                6 => [0.01, 0.90, 0.40, 1.0],
                7 => [0.47, 0.06, 0.61, 1.0],
                8 => [0.00, 0.87, 0.95, 1.0],
                _ => [0.0; 4],
            }
        };
        let mut sx = 1.0f32;
        let mut sy = 1.0f32;
        let mut sz = 1.0f32;
        let mut count = 0;
        if ci & 0xF000 != 0 {
            let c = pc((ci & 0xF000) >> 12);
            sx = (1.0f64 - c[0] as f64) as f32;
            sy = (1.0f64 - c[1] as f64) as f32;
            sz = 1.0f32 - c[2];
            count = 1;
            for sh in [8u16, 4] {
                let n = (ci >> sh) & 0xF;
                if n != 0 {
                    let c = pc(n);
                    sx += (1.0f64 - c[0] as f64) as f32;
                    sy += (1.0f64 - c[1] as f64) as f32;
                    sz += 1.0f32 - c[2];
                    count += 1;
                }
            }
        }
        if count == 0 {
            return default_color_for_item_type(ItemType(item)).v;
        }
        let f = |s: f32| {
            let avg = s / count as f32;
            let m = if avg as f64 >= 1.0 { 1.0f64 } else { avg as f64 };
            (1.0f64 - m) as f32
        };
        [f(sx), f(sy), f(sz), 1.0]
    }

    /// Checklist 6.1 #11.
    #[test]
    fn mix_matches_reference() {
        for item in [278u32, 83, 122, 126, 170, 0, 5000] {
            for ci in 0..=u16::MAX {
                let got = mixed_color_for_index(ci, ItemType(item)).v;
                let exp = reference(ci, item);
                for k in 0..4 {
                    assert_eq!(got[k].to_bits(), exp[k].to_bits(), "ci={ci:#x} item={item}");
                }
            }
        }
    }

    #[test]
    fn special_cases() {
        let white = [1.0f32; 4];
        assert_eq!(mixed_color_for_index(0x0210, ItemType(83)).v, [0.8, 0.82, 0.75, 1.0]);
        assert_eq!(mixed_color_for_index(0x0000, ItemType(0)).v, white);
        assert_eq!(mixed_color_for_index(0x1000, ItemType(5000)).v, white);
        assert_eq!(mixed_color_for_index(0x1200, ItemType(0)).v, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!(mixed_color_for_index(0x1f00, ItemType(0)).v[3], 1.0);
        for ci in [0x1230u16, 0x2000, 0x4000] {
            assert_eq!(mixed_color_for_index(ci, ItemType(278)).v, white);
        }
        assert_eq!(pigment_color_for_pigment_index(0).v, [0.0; 4]);
        assert_eq!(pigment_color_for_pigment_index(9).v, [0.0; 4]);
        assert_eq!(default_color_for_item_type(ItemType(170)).v, [0.05, 0.01, 0.0, 1.0]);
        assert_eq!(default_color_for_item_type(ItemType(171)).v, white);
    }

    #[test]
    fn packing_and_tulip() {
        assert_eq!(color_index_for_pigments(PigmentColor(1), PigmentColor(2), PigmentColor(3)), 0x1230);
        assert_eq!(color_index_for_pigments(PigmentColor(0x13), PigmentColor(0), PigmentColor(0)), 0x3000);
        assert_eq!(tulip_gene_order_for_pigment(0), 99);
        assert_eq!(tulip_gene_order_for_pigment(8), 4);
        assert_eq!(tulip_gene_order_for_pigment(9), 99);
        assert_eq!(
            mj_color8_make(1.5, -1.0, 0.5, 1.0),
            crate::types::structs::MJColor8 { r: 255, g: 0, b: 127, a: 255 }
        );
    }
}
