//! Food helpers.

use crate::types::enums::FoodType;
use crate::types::item_type::ItemType;

type I = ItemType;

/// `foodTypeForItemType`. `data_b` matters only for DODO_EGG (food only when 0). Domain is the
/// unsigned `(id - 21) <= 0x133`; everything else is FOOD_NONE (32).
pub fn food_type_for_item_type(t: ItemType, data_b: i32) -> FoodType {
    if t.0.wrapping_sub(21) > 0x133 {
        return FoodType::FOOD_NONE;
    }
    match t {
        I::ITEM_APPLE => FoodType::FOOD_APPLE,
        I::ITEM_MANGO => FoodType::FOOD_MANGO,
        I::ITEM_MAPLE_SEED => FoodType::FOOD_MAPLE_SEED,
        I::ITEM_PRICKLY_PEAR => FoodType::FOOD_PRICKLY_PEAR,
        I::ITEM_RAW_DODO_MEAT => FoodType::FOOD_RAW_DODO_MEAT,
        I::ITEM_COCONUT => FoodType::FOOD_COCONUT,
        I::ITEM_FLAX_SEED => FoodType::FOOD_FLAX_SEED,
        I::ITEM_COOKED_DODO_MEAT => FoodType::FOOD_COOKED_DODO_MEAT,
        I::ITEM_ORANGE => FoodType::FOOD_ORANGE,
        I::ITEM_SUNFLOWER_SEED => FoodType::FOOD_SUNFLOWER_SEED,
        I::ITEM_CORN => FoodType::FOOD_CORN,
        I::ITEM_CARROT => FoodType::FOOD_CARROT,
        I::ITEM_CHERRY => FoodType::FOOD_CHERRY,
        I::ITEM_COFFEE_CHERRY => FoodType::FOOD_COFFEE_CHERRY,
        I::ITEM_COFFEE_CUP => FoodType::FOOD_COFFEE,
        I::ITEM_DODO_EGG => {
            if data_b == 0 {
                FoodType::FOOD_DODO_EGG
            } else {
                FoodType::FOOD_NONE
            }
        }
        I::ITEM_CHILLI => FoodType::FOOD_CHILLI,
        I::ITEM_KELP => FoodType::FOOD_KELP,
        I::ITEM_RAW_FISH => FoodType::FOOD_RAW_FISH,
        I::ITEM_COOKED_FISH => FoodType::FOOD_COOKED_FISH,
        I::ITEM_WORM => FoodType::FOOD_WORM,
        I::ITEM_LIME => FoodType::FOOD_LIME,
        I::ITEM_FISH_CURRY => FoodType::FOOD_FISH_CURRY,
        I::ITEM_DODO_STEW => FoodType::FOOD_DODO_STEW,
        I::ITEM_BREAD => FoodType::FOOD_BREAD,
        I::ITEM_TOMATO => FoodType::FOOD_TOMATO,
        I::ITEM_PIZZA => FoodType::FOOD_PIZZA,
        I::ITEM_FLATBREAD => FoodType::FOOD_FLATBREAD,
        I::ITEM_MILK_BUCKET => FoodType::FOOD_MILK,
        I::ITEM_MOZZARELLA => FoodType::FOOD_MOZZARELLA,
        I::ITEM_RAINBOW_CAKE => FoodType::FOOD_RAINBOW_CAKE,
        _ => FoodType::FOOD_NONE,
    }
}

/// `itemTypeFromFoodType`: inverse mapping; default ITEM_NONE.
pub fn item_type_from_food_type(f: FoodType) -> ItemType {
    match f {
        FoodType::FOOD_APPLE => I::ITEM_APPLE,
        FoodType::FOOD_MANGO => I::ITEM_MANGO,
        FoodType::FOOD_PRICKLY_PEAR => I::ITEM_PRICKLY_PEAR,
        FoodType::FOOD_RAW_DODO_MEAT => I::ITEM_RAW_DODO_MEAT,
        FoodType::FOOD_COOKED_DODO_MEAT => I::ITEM_COOKED_DODO_MEAT,
        FoodType::FOOD_COCONUT => I::ITEM_COCONUT,
        FoodType::FOOD_ORANGE => I::ITEM_ORANGE,
        FoodType::FOOD_FLAX_SEED => I::ITEM_FLAX_SEED,
        FoodType::FOOD_SUNFLOWER_SEED => I::ITEM_SUNFLOWER_SEED,
        FoodType::FOOD_CORN => I::ITEM_CORN,
        FoodType::FOOD_CARROT => I::ITEM_CARROT,
        FoodType::FOOD_CHERRY => I::ITEM_CHERRY,
        FoodType::FOOD_COFFEE_CHERRY => I::ITEM_COFFEE_CHERRY,
        FoodType::FOOD_DODO_EGG => I::ITEM_DODO_EGG,
        FoodType::FOOD_CHILLI => I::ITEM_CHILLI,
        FoodType::FOOD_COFFEE => I::ITEM_COFFEE_CUP,
        FoodType::FOOD_KELP => I::ITEM_KELP,
        FoodType::FOOD_RAW_FISH => I::ITEM_RAW_FISH,
        FoodType::FOOD_COOKED_FISH => I::ITEM_COOKED_FISH,
        FoodType::FOOD_WORM => I::ITEM_WORM,
        FoodType::FOOD_LIME => I::ITEM_LIME,
        FoodType::FOOD_FISH_CURRY => I::ITEM_FISH_CURRY,
        FoodType::FOOD_DODO_STEW => I::ITEM_DODO_STEW,
        FoodType::FOOD_MAPLE_SEED => I::ITEM_MAPLE_SEED,
        FoodType::FOOD_TOMATO => I::ITEM_TOMATO,
        FoodType::FOOD_BREAD => I::ITEM_BREAD,
        FoodType::FOOD_PIZZA => I::ITEM_PIZZA,
        FoodType::FOOD_FLATBREAD => I::ITEM_FLATBREAD,
        FoodType::FOOD_MILK => I::ITEM_MILK_BUCKET,
        FoodType::FOOD_MOZZARELLA => I::ITEM_MOZZARELLA,
        FoodType::FOOD_RAINBOW_CAKE => I::ITEM_RAINBOW_CAKE,
        _ => I::ITEM_NONE,
    }
}

/// `nourishmentFractionForFoodType`: hunger-bar fraction restored (float32 literals).
pub fn nourishment_fraction_for_food_type(f: FoodType) -> f32 {
    match f {
        FoodType::FOOD_APPLE
        | FoodType::FOOD_MANGO
        | FoodType::FOOD_PRICKLY_PEAR
        | FoodType::FOOD_COCONUT
        | FoodType::FOOD_ORANGE
        | FoodType::FOOD_CORN
        | FoodType::FOOD_CARROT
        | FoodType::FOOD_CHERRY
        | FoodType::FOOD_CHILLI
        | FoodType::FOOD_LIME
        | FoodType::FOOD_TOMATO
        | FoodType::FOOD_DODO_EGG
        | FoodType::FOOD_MOZZARELLA => 0.1f32,
        FoodType::FOOD_RAW_DODO_MEAT => 0.15f32,
        FoodType::FOOD_COOKED_DODO_MEAT => 0.25f32,
        FoodType::FOOD_FLAX_SEED | FoodType::FOOD_SUNFLOWER_SEED | FoodType::FOOD_MAPLE_SEED => 0.02f32,
        FoodType::FOOD_COFFEE_CHERRY | FoodType::FOOD_COFFEE | FoodType::FOOD_KELP => 0.05f32,
        FoodType::FOOD_RAW_FISH | FoodType::FOOD_MILK => 0.2f32,
        FoodType::FOOD_COOKED_FISH | FoodType::FOOD_FLATBREAD => 0.3f32,
        FoodType::FOOD_WORM => 0.015f32,
        FoodType::FOOD_FISH_CURRY | FoodType::FOOD_DODO_STEW | FoodType::FOOD_BREAD => 0.4f32,
        FoodType::FOOD_PIZZA | FoodType::FOOD_RAINBOW_CAKE => 0.8f32,
        _ => 0.0f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mappings_round_trip() {
        let mut n = 0;
        for f in 0..31u32 {
            let it = item_type_from_food_type(FoodType(f));
            assert_ne!(it, I::ITEM_NONE, "food {f}");
            assert_eq!(food_type_for_item_type(it, 0), FoodType(f));
            n += 1;
        }
        assert_eq!(n, 31);
        assert_eq!(item_type_from_food_type(FoodType::FOOD_NONE), I::ITEM_NONE);
        assert_eq!(item_type_from_food_type(FoodType(31)), I::ITEM_NONE);
    }

    #[test]
    fn domain_and_dodo_egg() {
        assert_eq!(food_type_for_item_type(I::ITEM_DODO_EGG, 0), FoodType(13));
        assert_eq!(food_type_for_item_type(I::ITEM_DODO_EGG, 5), FoodType(32));
        assert_eq!(food_type_for_item_type(ItemType(20), 0), FoodType(32));
        assert_eq!(food_type_for_item_type(ItemType(329), 0), FoodType(32));
        assert_eq!(food_type_for_item_type(ItemType(21), 0), FoodType(0));
        assert_eq!(food_type_for_item_type(ItemType(328), 0), FoodType(30));
        assert_eq!(food_type_for_item_type(ItemType((-1i32) as u32), 0), FoodType(32));
    }

    #[test]
    fn nourishment_bits() {
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_APPLE).to_bits(), 0x3dcccccd);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_RAW_DODO_MEAT).to_bits(), 0x3e19999a);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_MAPLE_SEED).to_bits(), 0x3ca3d70a);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_KELP).to_bits(), 0x3d4ccccd);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_MILK).to_bits(), 0x3e4ccccd);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_FLATBREAD).to_bits(), 0x3e99999a);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_WORM).to_bits(), 0x3c75c28f);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_BREAD).to_bits(), 0x3ecccccd);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_PIZZA).to_bits(), 0x3f4ccccd);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_COOKED_DODO_MEAT), 0.25);
        assert_eq!(nourishment_fraction_for_food_type(FoodType::FOOD_NONE), 0.0);
        assert_eq!(nourishment_fraction_for_food_type(FoodType(31)), 0.0);
    }
}
