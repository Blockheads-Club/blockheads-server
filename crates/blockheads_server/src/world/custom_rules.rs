//! Custom rules: option tables, `default_custom_rules`, `custom_rules_from_dict`, seed parsing and the
//! `impl World` custom rule methods.
//!
//! Exceptions the original raises inside `customRulesFromDict` are raised with `foundation::exception`
//! (they unwind to the `doIdleSafe` catch site).

use super::state::World;
use crate::fmath::cconv::{f32_to_i32, f64_to_u32};
use crate::fmath::vector::clampi;
use crate::foundation::exception::{invalid_argument, range_exception_index, unrecognized_selector};
use crate::foundation::gzip::gzip_inflate;
use crate::foundation::nsstring::{gs_hash, length as ns_length};
use crate::items::predicates::item_type_is_stackable;
use crate::nslog;
use crate::plist::value::{ClassTag, PlistArray, PlistDict, PlistValue};
use crate::types::item_type::ItemType;
use crate::types::structs::CustomRules;

// =====================================================================================================
// Option tables
// =====================================================================================================

/// One `NSDictionary` of `GENERATE_CUSTOM_RULE_*`.
pub struct CustomRuleOption {
    /// JSON key (`name`).
    pub name: &'static str,
    pub category_key: i32,
    /// `None` for "spawn items" and "sun color" (no options/default).
    pub default: Option<&'static str>,
    pub expert_default: Option<&'static str>,
    /// Index == stored value (max blockheads stores index + 1).
    pub options: &'static [&'static str],
    /// Only "seed": `["custom"]`.
    pub text_entry_options: &'static [&'static str],
}

const fn opt(
    name: &'static str,
    category_key: i32,
    default: &'static str,
    options: &'static [&'static str],
) -> CustomRuleOption {
    CustomRuleOption {
        name,
        category_key,
        default: Some(default),
        expert_default: None,
        options,
        text_entry_options: &[],
    }
}

/// `GENERATE_CUSTOM_RULE_WORLD_GEN_OPTIONS` array, 14 entries in table order.
pub static CUSTOM_RULE_WORLD_GEN_OPTIONS: [CustomRuleOption; 14] = [
    CustomRuleOption {
        name: "seed",
        category_key: 1,
        default: Some("random"),
        expert_default: None,
        options: &["random", "custom"],
        text_entry_options: &["custom"],
    },
    opt("oceans", 2, "normal", &["no oceans", "small oceans", "normal", "large oceans", "ocean world"]),
    opt("temperature", 3, "normal", &["very cold", "cold", "normal", "warm", "hot", "very hot"]),
    opt("rainfall", 4, "normal", &["very dry", "dry", "normal", "wet", "very wet"]),
    opt("plants", 5, "normal", &["no plants", "few plants", "normal", "more plants"]),
    opt(
        "trees",
        6,
        "normal",
        &["no trees", "gem trees only", "sky island trees only", "few trees", "normal", "more trees"],
    ),
    opt("terrain", 7, "normal", &["totally flat", "quite flat", "normal", "mountainous"]),
    opt("caves", 8, "normal", &["no caves", "normal", "more caves"]),
    opt("lakes", 9, "normal", &["no lakes", "normal", "more lakes"]),
    opt("sky islands", 10, "normal", &["no sky islands", "less sky islands", "normal", "more sky islands"]),
    opt("ore quantity", 11, "normal", &["no ore", "less ore", "normal", "more ore"]),
    opt("ore distribution", 12, "normal", &["small veins", "normal", "large veins"]),
    opt("gems", 13, "normal", &["no gems", "less gems", "normal", "more gems"]),
    opt("treasure", 14, "normal", &["no treasure", "less treasure", "normal", "more treasure"]),
];

/// `GENERATE_CUSTOM_RULE_GAME_OPTIONS` array, 19 entries in table order.
/// exactly two options).
pub static CUSTOM_RULE_GAME_OPTIONS: [CustomRuleOption; 19] = [
    opt("health", 999, "normal", &["one hit kill", "hard", "normal", "easy", "infinite health"]),
    opt("hunger", 1000, "normal", &["hungrier", "normal", "fuller", "always full"]),
    opt("happiness", 1002, "normal", &["sadder", "normal", "happier", "always happy"]),
    opt("clothing decay", 1007, "normal", &["no decay", "slower", "normal", "faster"]),
    opt("tool decay", 1008, "normal", &["no decay", "slower", "normal", "faster"]),
    opt("death", 1009, "normal", &["no death", "no RIP", "normal", "no regeneration", "die on exit"]),
    CustomRuleOption {
        name: "death drops",
        category_key: 1010,
        default: Some("some items"),
        expert_default: Some("all items"),
        options: &["all items", "some items", "no items"],
        text_entry_options: &[],
    },
    opt("max blockheads", 1011, "5", &["1", "2", "3", "4", "5"]),
    CustomRuleOption {
        name: "spawn items",
        category_key: 1001,
        default: None,
        expert_default: None,
        options: &[],
        text_entry_options: &[],
    },
    opt("npc/mob difficulty", 1006, "normal", &["hard", "normal", "easy", "passive"]),
    opt(
        "trade portals",
        1003,
        "normal",
        &["normal", "disabled", "buy only", "sell only", "everything free", "jobs only"],
    ),
    opt(
        "trade prices",
        1004,
        "4x higher when buying",
        &["4x higher when buying", "10% higher when buying", "buy and sell prices equal"],
    ),
    opt("trade portal items", 1005, "require discovery", &["require discovery", "all items available"]),
    opt("day/night cycle", 1012, "normal", &["slower", "normal", "faster"]),
    CustomRuleOption {
        name: "sun color",
        category_key: 1013,
        default: None,
        expert_default: None,
        options: &[],
        text_entry_options: &[],
    },
    opt("world modification", 1014, "enabled", &["enabled", "admin only", "disabled"]),
    opt("pole items", 1015, "respawns, wears out", &["respawns, wears out", "one only, lasts forever"]),
    CustomRuleOption {
        name: "teleportation",
        category_key: 1016,
        default: Some("enabled"),
        expert_default: Some("requires diamond portal"),
        options: &["enabled", "requires diamond portal", "disabled"],
        text_entry_options: &[],
    },
    CustomRuleOption {
        name: "meditation",
        category_key: 1017,
        default: Some("enabled"),
        expert_default: Some("disabled"),
        options: &["enabled", "disabled"],
        text_entry_options: &[],
    },
];

/// `GENERATE_CUSTOM_RULE_WORLD_GEN_OPTIONS()`: the table is a static; kept for call-order parity (idempotent).
pub fn generate_custom_rule_world_gen_options() {}
/// `GENERATE_CUSTOM_RULE_GAME_OPTIONS()`: see [`generate_custom_rule_world_gen_options`].
pub fn generate_custom_rule_game_options() {}

// =====================================================================================================
// defaultCustomRules / customRulesFromDict
// =====================================================================================================

/// `defaultCustomRules(out, expertMode)` starting from an all-zero struct.
pub fn default_custom_rules(expert_mode: bool) -> CustomRules {
    let mut r = CustomRules::default();
    r.custom_rules_enabled = 0;
    r.seed_type = 0;
    r.oceans = 2;
    r.temperature = 2;
    r.rainfall = 2;
    r.plants = 2;
    r.trees = 4;
    r.terrain_type = 2;
    r.caves = 1;
    r.lakes = 1;
    r.sky_islands = 2;
    r.ore_quantity = 2;
    r.ore_distribution = 1;
    r.gems = 2;
    r.treasure = 2;
    r.health = 2;
    r.hunger = 1;
    for i in 0..7 {
        r.spawn_item_counts[i] = 1;
        r.spawn_item_types[i] = 2; // ITEM_DEFAULT_MARKER
    }
    r.happiness = 1;
    r.trade_portals = 0;
    r.clothing_decay = 2;
    r.tool_decay = 2;
    r.death = 2;
    r.max_blockheads = 5;
    r.npc_difficulty = 1;
    r.day_night_cycle = 1;
    r.world_modifcation = 0;
    r.sun_color = [127, 127, 127];
    r.death_drops = if expert_mode { 0 } else { 1 };
    r.portal_prices = 0;
    r.portal_discoverability = 0;
    r.pole_items = 0;
    r.teleportation = if expert_mode { 1 } else { 0 };
    r.meditation = if expert_mode { 1 } else { 0 };
    r
}

/// Store `found` into the field of category `cat`. Categories 1001 and 1013 store nothing.
fn store_category(out: &mut CustomRules, cat: i32, found: i32) {
    let v = found as i8;
    match cat {
        1 => out.seed_type = v,
        2 => out.oceans = v,
        3 => out.temperature = v,
        4 => out.rainfall = v,
        5 => out.plants = v,
        6 => out.trees = v,
        7 => out.terrain_type = v,
        8 => out.caves = v,
        9 => out.lakes = v,
        10 => out.sky_islands = v,
        11 => out.ore_quantity = v,
        12 => out.ore_distribution = v,
        13 => out.gems = v,
        14 => out.treasure = v,
        999 => out.health = v,
        1000 => out.hunger = v,
        1001 => {}
        1002 => out.happiness = v,
        1003 => out.trade_portals = v,
        1004 => out.portal_prices = v,
        1005 => out.portal_discoverability = v,
        1006 => out.npc_difficulty = v,
        1007 => out.clothing_decay = v,
        1008 => out.tool_decay = v,
        1009 => out.death = v,
        1010 => out.death_drops = v,
        1011 => out.max_blockheads = found.wrapping_add(1) as i8,
        1012 => out.day_night_cycle = v,
        1013 => {}
        1014 => out.world_modifcation = v,
        1015 => out.pole_items = v,
        1016 => out.teleportation = v,
        1017 => out.meditation = v,
        _ => {}
    }
}

/// The option loop shared by the world-gen and game tables: no early break, last match wins, a match equal to
/// the default string is NOT written. `default_override` is the effective default for this option.
fn find_option(opt: &CustomRuleOption, sel: &PlistValue, default: Option<&str>) -> i32 {
    let mut found = -1i32;
    if let Some(sel) = sel.as_str() {
        for (i, o) in opt.options.iter().enumerate() {
            if *o == sel && Some(*o) != default {
                found = i as i32;
            }
        }
    }
    found
}

/// `[v intValue]`: strings and numbers answer; other classes do not recognize `intValue`.
fn int_value_or_raise(v: &PlistValue) -> i32 {
    match v {
        PlistValue::String(_)
        | PlistValue::Int(_)
        | PlistValue::UInt(_)
        | PlistValue::Real { .. }
        | PlistValue::Bool(_) => v.int_value(),
        other => unrecognized_selector(other.class_name(), "intValue"),
    }
}

/// `[v intValue]` with nil receiver giving 0.
fn int_value_opt(v: Option<&PlistValue>) -> i32 {
    v.map_or(0, int_value_or_raise)
}

/// `customRulesFromDict(incoming, expertMode)`. `None` -> defaults with
/// `custom_rules_enabled == 0`. May raise `NSInvalidArgumentException` (see 7.2(a)).
pub fn custom_rules_from_dict(incoming: Option<&PlistDict>, expert_mode: bool) -> CustomRules {
    let mut out = default_custom_rules(expert_mode);
    let Some(incoming) = incoming else { return out };
    out.custom_rules_enabled = 1;
    let lookup = |k: &str| incoming.get(&k.to_string());

    for opt in CUSTOM_RULE_WORLD_GEN_OPTIONS.iter() {
        let Some(sel) = lookup(opt.name) else { continue };
        let found = find_option(opt, sel, opt.default);
        if found != -1 && (1..=14).contains(&opt.category_key) {
            store_category(&mut out, opt.category_key, found);
        }
    }

    if out.seed_type == 1 {
        let seed = lookup("custom_seed_value");
        let len_zero = match seed {
            None => true,
            Some(PlistValue::String(s)) => ns_length(&s.s) == 0,
            // Note: NSData also answers `length`; a non-empty one would then fail inside the formatter.
            Some(PlistValue::Data(d)) if d.bytes.is_empty() => true,
            Some(other) => unrecognized_selector(other.class_name(), "length"),
        };
        if len_zero {
            nslog!("Failed to load custom seed. Resetting to random.");
            out.seed_type = 0;
        } else if let Some(PlistValue::String(s)) = seed {
            out.custom_seed_value = seed_from_string(&s.s).value;
        }
    }

    for opt in CUSTOM_RULE_GAME_OPTIONS.iter() {
        let Some(sel) = lookup(opt.name) else { continue };
        let default = if expert_mode { opt.expert_default.or(opt.default) } else { opt.default };
        let found = find_option(opt, sel, default);
        if found != -1 && ((999..=1016).contains(&opt.category_key) || opt.category_key == 1017) {
            store_category(&mut out, opt.category_key, found);
        }
    }

    for i in 0..3 {
        if let Some(v) = lookup(&format!("sunColor_{}", i)) {
            out.sun_color[i] = int_value_or_raise(v) as u8;
        }
    }

    // `[slots count] == 7`
    let slots = lookup("spawnItemSlots");
    let count: usize = match slots {
        None => 0,
        Some(PlistValue::Array(a)) => a.items.len(),
        Some(PlistValue::Dict(d)) => d.len(),
        Some(other) => unrecognized_selector(other.class_name(), "count"),
    };
    if count == 7 {
        for i in 0..7 {
            // `count == 7` was only reached with `slots` present
            let d = match slots.expect("count is 7") {
                PlistValue::Array(a) => &a.items[i],
                other => unrecognized_selector(other.class_name(), "objectAtIndex:"),
            };
            let (ty, ct) = match d {
                PlistValue::Dict(dd) => (dd.get(&"type".to_string()), dd.get(&"count".to_string())),
                other => unrecognized_selector(other.class_name(), "objectForKey:"),
            };
            out.spawn_item_types[i] = int_value_opt(ty) as u16;
            out.spawn_item_counts[i] = int_value_opt(ct) as i8;
        }
    }
    out
}

// =====================================================================================================
// Seed parsing (7.2(b))
// =====================================================================================================

/// Which path `seed_from_string` took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedKind {
    /// `numberFromString:` produced a number (`Loaded numeric seed...`).
    Numeric,
    /// It returned nil and `[seed hash]` was used (`Loaded hash seed...`).
    Hash,
}

/// Result of [`seed_from_string`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedFromString {
    pub kind: SeedKind,
    pub value: u32,
}

/// A number produced by the lenient GNUstep `NSNumberFormatter -numberFromString:`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParsedNumber {
    Int(i64),
    Double(f64),
    NaN,
}

impl ParsedNumber {
    /// `-unsignedIntValue`: integer -> low 32 bits; double -> `f64_to_u32` (i64 overflow gives 0); NaN -> 0.
    pub fn unsigned_int_value(self) -> u32 {
        match self {
            ParsedNumber::Int(v) => v as u32,
            ParsedNumber::Double(d) => f64_to_u32(d),
            ParsedNumber::NaN => 0,
        }
    }
}

/// Zero code points of the Unicode decimal digit (Nd) blocks accepted as digits (observed: U+0663 parses as 3).
const DIGIT_ZEROS: &[u32] = &[
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6, 0x0D66, 0x0DE6,
    0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40,
    0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x11066, 0x1D7CE, 0x1D7D8,
    0x1D7E2, 0x1D7EC, 0x1D7F6,
];

fn digit_value(c: char) -> Option<u32> {
    let cp = c as u32;
    DIGIT_ZEROS.iter().find(|z| cp >= **z && cp < **z + 10).map(|z| cp - z)
}

/// Lenient `numberFromString:` (rules fitted to 70 observed strings, (b)):
/// optional single `-`, then `digits [. digits] [(e|E)[+|-]digits]`, parsing stops at the first character
/// that does not fit; the exact string `NaN` is a number (unsigned value 0); everything else is nil.
pub fn number_from_string(s: &str) -> Option<ParsedNumber> {
    if s == "NaN" {
        return Some(ParsedNumber::NaN);
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    let mut negative = false;
    if chars.first() == Some(&'-') {
        negative = true;
        i = 1;
    }
    let mut int_digits: Vec<u32> = Vec::new();
    while i < chars.len() {
        match digit_value(chars[i]) {
            Some(d) => {
                int_digits.push(d);
                i += 1;
            }
            None => break,
        }
    }
    let mut frac_digits: Vec<u32> = Vec::new();
    let mut has_point = false;
    if i < chars.len() && chars[i] == '.' {
        has_point = true;
        i += 1;
        while i < chars.len() {
            match digit_value(chars[i]) {
                Some(d) => {
                    frac_digits.push(d);
                    i += 1;
                }
                None => break,
            }
        }
    }
    if int_digits.is_empty() && frac_digits.is_empty() {
        return None;
    }
    // exponent: `e|E [+|-] digits`; an `e` not followed by digits is ignored
    let mut exp: Option<i64> = None;
    if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
        let mut j = i + 1;
        let mut exp_neg = false;
        if j < chars.len() && (chars[j] == '+' || chars[j] == '-') {
            exp_neg = chars[j] == '-';
            j += 1;
        }
        let mut e: i64 = 0;
        let mut any = false;
        while j < chars.len() {
            match digit_value(chars[j]) {
                Some(d) => {
                    any = true;
                    e = (e * 10 + d as i64).min(1_000_000);
                    j += 1;
                }
                None => break,
            }
        }
        if any {
            exp = Some(if exp_neg { -e } else { e });
        }
    }
    if !has_point && exp.is_none() {
        // plain integer: must fit i64
        let mut v: i128 = 0;
        for d in &int_digits {
            v = v * 10 + *d as i128;
            if v > (i64::MAX as i128) + 1 {
                return None;
            }
        }
        if negative {
            v = -v;
        }
        if v > i64::MAX as i128 || v < i64::MIN as i128 {
            return None;
        }
        return Some(ParsedNumber::Int(v as i64));
    }
    let mut text = String::new();
    if negative {
        text.push('-');
    }
    if int_digits.is_empty() {
        text.push('0');
    }
    for d in &int_digits {
        text.push(char::from_digit(*d, 10).unwrap());
    }
    if !frac_digits.is_empty() {
        text.push('.');
        for d in &frac_digits {
            text.push(char::from_digit(*d, 10).unwrap());
        }
    }
    if let Some(e) = exp {
        text.push('e');
        text.push_str(&e.to_string());
    }
    let v: f64 = text.parse().ok()?;
    // Note: exact cutoff between 9.3e18 and 1e19 unmeasured; `>= 1e19` is used.
    if v.abs() >= 1e19 {
        return None;
    }
    Some(ParsedNumber::Double(v))
}

/// The seed decision without logging: numeric if `numberFromString:` yields a number, else `[seed hash]`.
pub fn parse_seed(seed: &str) -> SeedFromString {
    match number_from_string(seed) {
        Some(n) => SeedFromString { kind: SeedKind::Numeric, value: n.unsigned_int_value() },
        None => SeedFromString { kind: SeedKind::Hash, value: gs_hash(seed) },
    }
}

/// Seed string -> number, including the NSLog line of the taken path:
/// `Loaded numeric seed from string:%@, integer:%u` or `Loaded hash seed from string:%@ converted to integer:%u.`
pub fn seed_from_string(seed: &str) -> SeedFromString {
    let r = parse_seed(seed);
    match r.kind {
        SeedKind::Hash => nslog!("Loaded hash seed from string:{} converted to integer:{}.", seed, r.value),
        SeedKind::Numeric => nslog!("Loaded numeric seed from string:{}, integer:{}", seed, r.value),
    }
    r
}

// =====================================================================================================
// impl World
// =====================================================================================================

fn mutable_tag() -> ClassTag {
    ClassTag { mutable: true, ..ClassTag::default() }
}

impl World {
    // `-customRules` / `-setCustomRules:` are the generated Cell accessors in state.rs (64-byte copy).

    /// `-customRulesDict`
    pub fn custom_rules_dict(&self) -> Option<PlistDict> {
        self.custom_rules_dict.borrow().clone()
    }

    /// `-customRuleForOptionNamed:` = `[customRulesDict objectForKey:name]`.
    pub fn custom_rule_for_option_named(&self, name: &str) -> Option<PlistValue> {
        self.custom_rules_dict.borrow().as_ref().and_then(|d| d.get(&name.to_string()).cloned())
    }

    /// `-customRulesChanged`: `[[uiManager regenerateUI] customRulesChanged]`, uiManager is nil on the server.
    pub fn custom_rules_changed(&self) {}

    /// Re-run `customRulesFromDict(customRulesDict, expertMode)` and store the result.
    fn recompute_custom_rules(&self) {
        let dict = self.custom_rules_dict.borrow().clone();
        let r = custom_rules_from_dict(dict.as_ref(), self.expert_mode.get());
        self.custom_rules.set(r);
    }

    /// `-setCustomRule:forOptionNamed:`. Ignored when not admin. A nil `rule` or `option` raises
    /// `NSInvalidArgumentException` from `setObject:forKey:`. No network send on the server (client nil).
    pub fn set_custom_rule_for_option_named(&self, rule: Option<&PlistValue>, option: Option<&str>) {
        if !self.is_admin.get() {
            return;
        }
        {
            let mut d = self.custom_rules_dict.borrow_mut();
            if d.is_some() {
                match (rule, option) {
                    (_, None) => invalid_argument("Tried to add nil key to dictionary"),
                    (None, Some(k)) => {
                        invalid_argument(&format!("Tried to add nil value for key '{}' to dictionary", k))
                    }
                    (Some(v), Some(k)) => {
                        d.as_mut().unwrap().insert(k.to_string(), v.clone());
                    }
                }
            }
        }
        self.recompute_custom_rules();
        self.custom_rules_changed();
    }

    /// `-customRuleDictRecievedFromNet:` (sic). No admin check; `spawnItemSlots` is stored as a mutable array copy.
    pub fn custom_rule_dict_recieved_from_net(&self, rule_dict: &PlistDict) {
        {
            let mut d = self.custom_rules_dict.borrow_mut();
            if let Some(d) = d.as_mut() {
                for k in rule_dict.keys_in_order() {
                    let v = rule_dict.get(&k).cloned();
                    let Some(v) = v else { continue };
                    if k == "spawnItemSlots" {
                        if let PlistValue::Array(a) = &v {
                            d.insert(
                                k.clone(),
                                PlistValue::Array(PlistArray { items: a.items.clone(), tag: mutable_tag() }),
                            );
                            continue;
                        }
                    }
                    d.insert(k.clone(), v);
                }
            }
        }
        self.recompute_custom_rules();
        self.custom_rules_changed();
    }

    /// `-setCustomSlotAtIndex:toItemType:count:`. Out-of-range `slot` raises `NSRangeException`
    /// (`replaceObjectAtIndex:withObject:`).
    pub fn set_custom_slot_at_index_to_item_type_count(&self, slot: i32, item_type: i32, count: i32) {
        let mut count = clampi(count, 1, 99);
        if !item_type_is_stackable(ItemType(item_type as u32), 0, 0) {
            count = 1;
        }
        let mk = |ty: i64, ct: i64| {
            let mut m = PlistValue::mutable_dict();
            if let PlistValue::Dict(d) = &mut m {
                d.insert("type".to_string(), PlistValue::int(ty));
                d.insert("count".to_string(), PlistValue::int(ct));
            }
            m
        };
        let new_dict = mk(item_type as i64, count as i64);
        {
            let mut d = self.custom_rules_dict.borrow_mut();
            // `[NSMutableArray arrayWithArray: customRulesDict[@"spawnItemSlots"]]` (nil source => nil)
            let existing: Option<Vec<PlistValue>> = match d.as_ref().and_then(|d| d.get(&"spawnItemSlots".to_string()))
            {
                Some(PlistValue::Array(a)) => Some(a.items.clone()),
                _ => None,
            };
            let mut slots = match existing {
                Some(v) if !v.is_empty() => v,
                _ => (0..7).map(|_| mk(2, 1)).collect(),
            };
            if slot < 0 || slot as usize >= slots.len() {
                range_exception_index(slot as i64 as u64, slots.len() as u64, "replaceObjectAtIndex:withObject:");
            }
            slots[slot as usize] = new_dict;
            if let Some(d) = d.as_mut() {
                d.insert(
                    "spawnItemSlots".to_string(),
                    PlistValue::Array(PlistArray { items: slots, tag: mutable_tag() }),
                );
            }
        }
        self.recompute_custom_rules();
        self.custom_rules_changed();
    }

    /// `-setSunColorRule:` (MJColor, only r, g, b used): `clampi((int)(255.0f * c), 0, 255)` into
    /// `sunColor_0..2`.
    pub fn set_sun_color_rule(&self, color: [f32; 4]) {
        let comp = |c: f32| clampi(f32_to_i32(255.0f32 * c), 0, 255);
        {
            let mut d = self.custom_rules_dict.borrow_mut();
            if let Some(d) = d.as_mut() {
                for i in 0..3 {
                    d.insert(format!("sunColor_{}", i), PlistValue::int(comp(color[i]) as i64));
                }
            }
        }
        self.recompute_custom_rules();
        self.custom_rules_changed();
    }

    /// `-sendUpdatedSunColor`: `client == nil` on the server, nothing to do.
    pub fn send_updated_sun_color(&self) {}
    /// `-sendUpdatedCustomSlots`: `client == nil` on the server, nothing to do.
    pub fn send_updated_custom_slots(&self) {}

    /// `-verifyClientCustomRulesData:`: gunzip (nil/empty => false), overlay the first `min(len, 0x40)` bytes on
    /// `defaultCustomRules(expertMode)`, then compare the listed fields and the first 7 spawn slots.
    pub fn verify_client_custom_rules_data(&self, custom_rules_data: &[u8]) -> bool {
        let Some(unzipped) = gzip_inflate(custom_rules_data) else { return false };
        if unzipped.is_empty() {
            return false;
        }
        let mut client = default_custom_rules(self.expert_mode.get());
        client.overlay_bytes(&unzipped);
        let me = self.custom_rules.get();
        if client.custom_rules_enabled != me.custom_rules_enabled
            || client.temperature != me.temperature
            || client.rainfall != me.rainfall
            || client.health != me.health
            || client.hunger != me.hunger
            || client.happiness != me.happiness
            || client.trade_portals != me.trade_portals
            || client.npc_difficulty != me.npc_difficulty
            || client.clothing_decay != me.clothing_decay
            || client.tool_decay != me.tool_decay
            || client.death != me.death
            || client.max_blockheads != me.max_blockheads
            || client.day_night_cycle != me.day_night_cycle
            || client.world_modifcation != me.world_modifcation
            || client.death_drops != me.death_drops
            || client.portal_prices != me.portal_prices
            || client.portal_discoverability != me.portal_discoverability
            || client.pole_items != me.pole_items
            || client.teleportation != me.teleportation
            || client.meditation != me.meditation
        {
            return false;
        }
        for i in 0..7 {
            if client.spawn_item_counts[i] != me.spawn_item_counts[i]
                || client.spawn_item_types[i] != me.spawn_item_types[i]
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;
    use crate::foundation::gzip::gzip_deflate;
    use crate::plist::json::json_object_with_data;
    use crate::types::structs::WireStruct;

    fn dict(json: &str) -> PlistDict {
        let v = json_object_with_data(json.as_bytes()).value.expect("json");
        let PlistValue::Dict(d) = v else { panic!() };
        d
    }
    fn reason_of(json: &str) -> String {
        let d = dict(json);
        let e = catch_ns(|| custom_rules_from_dict(Some(&d), false)).expect_err("expected an exception");
        assert_eq!(e.name, "NSInvalidArgumentException");
        e.reason.unwrap()
    }

    #[test]
    fn table_shapes() {
        assert_eq!(CUSTOM_RULE_WORLD_GEN_OPTIONS.len(), 14);
        assert_eq!(CUSTOM_RULE_GAME_OPTIONS.len(), 19);
        let pole = CUSTOM_RULE_GAME_OPTIONS.iter().find(|o| o.name == "pole items").unwrap();
        assert_eq!(pole.options, &["respawns, wears out", "one only, lasts forever"]);
        assert_eq!(CUSTOM_RULE_WORLD_GEN_OPTIONS[0].text_entry_options, &["custom"]);
        let cats: Vec<i32> = CUSTOM_RULE_WORLD_GEN_OPTIONS.iter().map(|o| o.category_key).collect();
        assert_eq!(cats, (1..=14).collect::<Vec<_>>());
        // every default is one of its options
        for o in CUSTOM_RULE_WORLD_GEN_OPTIONS.iter().chain(CUSTOM_RULE_GAME_OPTIONS.iter()) {
            if let Some(d) = o.default {
                assert!(o.options.contains(&d), "{}", o.name);
            }
            if let Some(d) = o.expert_default {
                assert!(o.options.contains(&d), "{}", o.name);
            }
        }
        generate_custom_rule_world_gen_options();
        generate_custom_rule_game_options();
    }

    #[test]
    fn default_image() {
        for expert in [false, true] {
            let r = default_custom_rules(expert);
            assert_eq!(
                (
                    r.custom_rules_enabled,
                    r.seed_type,
                    r.oceans,
                    r.temperature,
                    r.rainfall,
                    r.plants,
                    r.trees,
                    r.terrain_type
                ),
                (0, 0, 2, 2, 2, 2, 4, 2)
            );
            assert_eq!(
                (r.caves, r.lakes, r.sky_islands, r.ore_quantity, r.ore_distribution, r.gems, r.treasure),
                (1, 1, 2, 2, 1, 2, 2)
            );
            assert_eq!(
                (r.health, r.hunger, r.happiness, r.trade_portals, r.clothing_decay, r.tool_decay, r.death),
                (2, 1, 1, 0, 2, 2, 2)
            );
            assert_eq!((r.max_blockheads, r.npc_difficulty, r.day_night_cycle, r.world_modifcation), (5, 1, 1, 0));
            assert_eq!(r.sun_color, [127, 127, 127]);
            assert_eq!(r.spawn_item_counts, [1, 1, 1, 1, 1, 1, 1, 0]);
            assert_eq!(r.spawn_item_types, [2, 2, 2, 2, 2, 2, 2, 0]);
            assert_eq!(r.death_drops, if expert { 0 } else { 1 });
            assert_eq!(r.teleportation, expert as i8);
            assert_eq!(r.meditation, expert as i8);
            assert_eq!((r.portal_prices, r.portal_discoverability, r.pole_items, r.custom_seed_value), (0, 0, 0, 0));
            assert_eq!(r.to_vec().len(), 64);
        }
    }

    #[test]
    fn dict_basics() {
        assert_eq!(custom_rules_from_dict(None, false).custom_rules_enabled, 0);
        assert_eq!(custom_rules_from_dict(Some(&dict("{}")), false).custom_rules_enabled, 1);
        assert_eq!(
            custom_rules_from_dict(Some(&dict("{}")), false).to_vec()[1..],
            default_custom_rules(false).to_vec()[1..]
        );
    }

    #[test]
    fn every_world_gen_option_selects_its_index() {
        for o in CUSTOM_RULE_WORLD_GEN_OPTIONS.iter().skip(1) {
            for (i, name) in o.options.iter().enumerate() {
                let d = dict(&format!("{{\"{}\":\"{}\"}}", o.name, name));
                let r = custom_rules_from_dict(Some(&d), false);
                let base = default_custom_rules(false);
                let got = field_by_cat(&r, o.category_key);
                let want = if Some(*name) == o.default { field_by_cat(&base, o.category_key) } else { i as i8 };
                assert_eq!(got, want, "{} = {}", o.name, name);
            }
        }
    }

    fn field_by_cat(r: &CustomRules, cat: i32) -> i8 {
        let table = [
            (1, r.seed_type),
            (2, r.oceans),
            (3, r.temperature),
            (4, r.rainfall),
            (5, r.plants),
            (6, r.trees),
            (7, r.terrain_type),
            (8, r.caves),
            (9, r.lakes),
            (10, r.sky_islands),
            (11, r.ore_quantity),
            (12, r.ore_distribution),
            (13, r.gems),
            (14, r.treasure),
            (999, r.health),
            (1000, r.hunger),
            (1002, r.happiness),
            (1003, r.trade_portals),
            (1004, r.portal_prices),
            (1005, r.portal_discoverability),
            (1006, r.npc_difficulty),
            (1007, r.clothing_decay),
            (1008, r.tool_decay),
            (1009, r.death),
            (1010, r.death_drops),
            (1011, r.max_blockheads),
            (1012, r.day_night_cycle),
            (1014, r.world_modifcation),
            (1015, r.pole_items),
            (1016, r.teleportation),
            (1017, r.meditation),
        ];
        table.iter().find(|(c, _)| *c == cat).map(|(_, v)| *v).expect("the category is in the table")
    }

    #[test]
    fn every_game_option_both_modes() {
        for expert in [false, true] {
            for o in CUSTOM_RULE_GAME_OPTIONS.iter() {
                if o.options.is_empty() {
                    // categories 1001 and 1013 store nothing
                    let d = dict(&format!("{{\"{}\":\"x\"}}", o.name));
                    let r = custom_rules_from_dict(Some(&d), expert);
                    let mut want = default_custom_rules(expert);
                    want.custom_rules_enabled = 1;
                    assert_eq!(r, want);
                    continue;
                }
                let base = default_custom_rules(expert);
                let eff_default = if expert { o.expert_default.or(o.default) } else { o.default };
                for (i, name) in o.options.iter().enumerate() {
                    let d = dict(&format!("{{\"{}\":\"{}\"}}", o.name, name));
                    let r = custom_rules_from_dict(Some(&d), expert);
                    let got = field_by_cat(&r, o.category_key);
                    let want = if Some(*name) == eff_default {
                        field_by_cat(&base, o.category_key)
                    } else if o.category_key == 1011 {
                        i as i8 + 1
                    } else {
                        i as i8
                    };
                    assert_eq!(got, want, "expert={} {} = {}", expert, o.name, name);
                }
            }
        }
        // the quirks spelled out in
        let r = custom_rules_from_dict(Some(&dict("{\"death drops\":\"some items\"}")), true);
        assert_eq!(r.death_drops, 1);
        let r = custom_rules_from_dict(Some(&dict("{\"death drops\":\"all items\"}")), true);
        assert_eq!(r.death_drops, 0);
        let r = custom_rules_from_dict(Some(&dict("{\"death drops\":\"all items\"}")), false);
        assert_eq!(r.death_drops, 0);
        let r = custom_rules_from_dict(Some(&dict("{\"max blockheads\":\"1\"}")), false);
        assert_eq!(r.max_blockheads, 1);
        let r = custom_rules_from_dict(Some(&dict("{\"max blockheads\":\"5\"}")), false);
        assert_eq!(r.max_blockheads, 5);
    }

    #[test]
    fn non_string_and_unknown_values_are_ignored() {
        for j in [
            "{\"oceans\":1}",
            "{\"temperature\":[\"hot\"]}",
            "{\"oceans\":{\"a\":1}}",
            "{\"nonsense\":\"x\"}",
            "{\"oceans\":\"nope\"}",
        ] {
            let r = custom_rules_from_dict(Some(&dict(j)), false);
            let mut want = default_custom_rules(false);
            want.custom_rules_enabled = 1;
            assert_eq!(r, want, "{}", j);
        }
    }

    #[test]
    fn sun_color_and_slots() {
        let r = custom_rules_from_dict(
            Some(&dict("{\"sunColor_0\":\"300\",\"sunColor_1\":255.7,\"sunColor_2\":-1}")),
            false,
        );
        assert_eq!(r.sun_color, [44, 255, 255]);
        let slots = "[{\"type\":5,\"count\":3},{\"type\":70000,\"count\":200},{},{\"type\":9},{\"count\":4},{\"type\":1,\"count\":-1},{\"type\":2,\"count\":1}]";
        let r = custom_rules_from_dict(Some(&dict(&format!("{{\"spawnItemSlots\":{}}}", slots))), false);
        assert_eq!(r.spawn_item_types[..7], [5, (70000u32 & 0xffff) as u16, 0, 9, 0, 1, 2]);
        assert_eq!(r.spawn_item_counts[..7], [3, 200u8 as i8, 0, 0, 4, -1, 1]);
        assert_eq!(r.spawn_item_types[7], 0);
        // wrong length: ignored
        let r = custom_rules_from_dict(Some(&dict("{\"spawnItemSlots\":[{\"type\":5}]}")), false);
        assert_eq!(r.spawn_item_types[0], 2);
    }

    #[test]
    fn exception_reasons() {
        assert_eq!(
            reason_of("{\"seed\":\"custom\",\"custom_seed_value\":12345}"),
            "Can not determine type information for -[NSDoubleNumber (null)]"
        );
        assert_eq!(
            reason_of("{\"seed\":\"custom\",\"custom_seed_value\":[\"a\"]}"),
            "Can not determine type information for -[GSArray (null)]"
        );
        assert_eq!(
            reason_of("{\"seed\":\"custom\",\"custom_seed_value\":{\"a\":1}}"),
            "Can not determine type information for -[GSDictionary (null)]"
        );
        assert_eq!(
            reason_of("{\"spawnItemSlots\":5}"),
            "Can not determine type information for -[NSDoubleNumber (null)]"
        );
        assert_eq!(reason_of("{\"sunColor_0\":[1]}"), "GSArray(instance) does not recognize intValue");
        assert_eq!(reason_of("{\"sunColor_0\":{\"a\":1}}"), "GSDictionary(instance) does not recognize intValue");
        assert_eq!(
            reason_of("{\"spawnItemSlots\":[1,2,3,4,5,6,7]}"),
            "NSDoubleNumber(instance) does not recognize objectForKey:"
        );
        assert_eq!(
            reason_of("{\"spawnItemSlots\":[[1],[1],[1],[1],[1],[1],[1]]}"),
            "GSArray(instance) does not recognize objectForKey:"
        );
        assert!(reason_of("{\"spawnItemSlots\":[\"a\",\"a\",\"a\",\"a\",\"a\",\"a\",\"a\"]}")
            .ends_with("(instance) does not recognize objectForKey:"));
        assert_eq!(
            reason_of("{\"spawnItemSlots\":{\"1\":1,\"2\":1,\"3\":1,\"4\":1,\"5\":1,\"6\":1,\"7\":1}}"),
            "GSDictionary(instance) does not recognize objectAtIndex:"
        );
        assert!(reason_of("{\"spawnItemSlots\":\"abcdefg\"}").starts_with("Can not determine type information for -["));
    }

    fn capture<T>(f: impl FnOnce() -> T) -> (T, String) {
        use std::sync::{Arc, Mutex};
        let _g = crate::foundation::nslog::TEST_SINK_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let sink = Arc::new(Mutex::new(Vec::new()));
        crate::foundation::nslog::set_test_sink(Some(sink.clone()));
        let r = f();
        crate::foundation::nslog::set_test_sink(None);
        let out = String::from_utf8_lossy(&sink.lock().unwrap().clone()).to_string();
        (r, out)
    }

    #[test]
    fn seed_resets_to_random_when_missing_or_empty() {
        for j in ["{\"seed\":\"custom\"}", "{\"seed\":\"custom\",\"custom_seed_value\":\"\"}"] {
            let d = dict(j);
            let (r, log) = capture(|| custom_rules_from_dict(Some(&d), false));
            assert_eq!(r.seed_type, 0);
            assert!(log.contains("Failed to load custom seed. Resetting to random.\n"), "{}", log);
        }
    }

    #[test]
    fn seed_lines_in_dict() {
        let d = dict("{\"seed\":\"custom\",\"custom_seed_value\":\"-5\"}");
        let (r, log) = capture(|| custom_rules_from_dict(Some(&d), false));
        assert_eq!((r.seed_type, r.custom_seed_value), (1, 4294967291));
        assert!(log.contains("Loaded numeric seed from string:-5, integer:4294967291\n"), "{}", log);
        let d = dict("{\"seed\":\"custom\",\"custom_seed_value\":\"hello world\"}");
        let (r, log) = capture(|| custom_rules_from_dict(Some(&d), false));
        assert_eq!(r.custom_seed_value, 166199516);
        assert!(log.contains("Loaded hash seed from string:hello world converted to integer:166199516.\n"), "{}", log);
    }

    #[test]
    fn seed_golden_vectors() {
        let numeric: &[(&str, u32)] = &[
            ("12345", 12345),
            ("-5", 4294967291),
            ("3.5", 3),
            ("1e3", 1000),
            ("1E3", 1000),
            ("1e+3", 1000),
            ("1e10", 1410065408),
            ("1e15", 2764472320),
            ("1e18", 2808348672),
            ("1e-2", 0),
            ("1.5e2", 150),
            ("0012", 12),
            ("12abc", 12),
            ("12 34", 12),
            ("5 ", 5),
            ("5e", 5),
            ("1e", 1),
            ("1.", 1),
            (".5", 0),
            ("0.5", 0),
            ("-0", 0),
            ("-3.7", 4294967293),
            ("0x10", 0),
            ("0x", 0),
            ("1,000", 1),
            ("12.9", 12),
            ("7.99999", 7),
            ("1.2.3", 1),
            ("0.1e1", 1),
            ("4294967295", 4294967295),
            ("4294967296", 0),
            ("2147483648", 2147483648),
            ("-2147483649", 2147483647),
            ("-4294967295", 1),
            ("123456789012", 3197704724),
            ("9223372036854775807", 4294967295),
            ("9.3e18", 0),
            ("00000000000000000000001", 1),
            ("NaN", 0),
            ("\u{663}", 3),
        ];
        for (s, v) in numeric {
            let r = parse_seed(s);
            assert_eq!((r.kind, r.value), (SeedKind::Numeric, *v), "{:?}", s);
        }
        let hashed: &[(&str, u32)] = &[
            (" 12", 36515),
            ("+5", 1472),
            ("e5", 3386),
            ("--5", 50543),
            ("- 5", 50114),
            ("-", 45),
            ("inf", 118077),
            ("nan", 123101),
            ("NAN", 87165),
            ("\u{221e}", 8734),
            ("1e19", 1872576),
            ("1e20", 1872600),
            ("99999999999999999999", 6233652),
            ("9223372036854775808", 100571465),
            ("18446744073709551615", 215918199),
            ("12345678901234567890", 31724954),
            ("hello world", 166199516),
            ("\u{e9}", 233),
            ("e\u{301}", 4102),
            ("Zo\u{eb}", 101908),
            ("\u{15f}", 351),
            ("a_very_long_name_with_numbers_123", 92269824),
        ];
        for (s, v) in hashed {
            let r = parse_seed(s);
            assert_eq!((r.kind, r.value), (SeedKind::Hash, *v), "{:?}", s);
        }
        let long = format!("x{}", "a".repeat(299));
        assert_eq!(parse_seed(&long), SeedFromString { kind: SeedKind::Hash, value: 70549411 });
    }

    #[test]
    fn seed_from_string_logs_both_paths() {
        let (r, log) = capture(|| seed_from_string("12abc"));
        assert_eq!(r.value, 12);
        assert!(log.contains("Loaded numeric seed from string:12abc, integer:12\n"), "{}", log);
        let (r, log) = capture(|| seed_from_string("e5"));
        assert_eq!(r.value, 3386);
        assert!(log.contains("Loaded hash seed from string:e5 converted to integer:3386.\n"), "{}", log);
    }

    fn admin_world(expert: bool, json: Option<&str>) -> std::rc::Rc<World> {
        let w = World::alloc(32);
        w.set_is_admin(true);
        w.set_expert_mode(expert);
        *w.custom_rules_dict.borrow_mut() = json.map(dict);
        w.set_custom_rules(custom_rules_from_dict(w.custom_rules_dict().as_ref(), expert));
        w
    }

    #[test]
    fn set_custom_rule() {
        let w = admin_world(false, Some("{}"));
        w.set_custom_rule_for_option_named(Some(&PlistValue::string("hard")), Some("health"));
        assert_eq!(w.custom_rules().health, 1);
        assert_eq!(w.custom_rule_for_option_named("health").unwrap().as_str(), Some("hard"));
        assert!(w.custom_rule_for_option_named("zzz").is_none());
        // not admin: ignored
        w.set_is_admin(false);
        w.set_custom_rule_for_option_named(Some(&PlistValue::string("easy")), Some("health"));
        assert_eq!(w.custom_rules().health, 1);
        // nil arguments raise
        w.set_is_admin(true);
        let e = catch_ns(|| w.set_custom_rule_for_option_named(None, Some("health"))).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("Tried to add nil value for key 'health' to dictionary"));
        let e = catch_ns(|| w.set_custom_rule_for_option_named(Some(&PlistValue::string("x")), None)).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("Tried to add nil key to dictionary"));
        // no dict: nothing stored, rules fall back to defaults
        let w = admin_world(false, None);
        w.set_custom_rule_for_option_named(Some(&PlistValue::string("hard")), Some("health"));
        assert_eq!(w.custom_rules().custom_rules_enabled, 0);
    }

    #[test]
    fn net_dict_slots_and_sun() {
        let w = admin_world(false, Some("{}"));
        w.custom_rule_dict_recieved_from_net(&dict("{\"temperature\":\"hot\",\"sunColor_0\":10}"));
        assert_eq!(w.custom_rules().temperature, 4);
        assert_eq!(w.custom_rules().sun_color[0], 10);
        w.set_sun_color_rule([1.0, 0.5, 2.0, 1.0]);
        assert_eq!(w.custom_rules().sun_color, [255, 127, 255]);
        w.set_sun_color_rule([-1.0, 0.0, 0.0, 1.0]);
        assert_eq!(w.custom_rules().sun_color[0], 0);
        // slot edit: placeholder fill then replace; ITEM 3 (flint) count clamps to 99 when stackable
        w.set_custom_slot_at_index_to_item_type_count(2, 3, 500);
        let r = w.custom_rules();
        assert_eq!(r.spawn_item_types[2], 3);
        assert!(r.spawn_item_counts[2] == 99 || r.spawn_item_counts[2] == 1);
        assert_eq!((r.spawn_item_types[0], r.spawn_item_counts[0]), (2, 1));
        w.set_custom_slot_at_index_to_item_type_count(0, 4, -5);
        assert_eq!(w.custom_rules().spawn_item_counts[0], 1);
        let e = catch_ns(|| w.set_custom_slot_at_index_to_item_type_count(7, 3, 1)).unwrap_err();
        assert_eq!(e.name, "NSRangeException");
        assert_eq!(e.reason.as_deref(), Some("Index 7 is out of range 7 (in 'replaceObjectAtIndex:withObject:')"));
        w.send_updated_sun_color();
        w.send_updated_custom_slots();
        w.custom_rules_changed();
    }

    #[test]
    fn verify_client_rules() {
        for expert in [false, true] {
            let w = admin_world(expert, Some("{\"health\":\"hard\",\"day/night cycle\":\"faster\"}"));
            let me = w.custom_rules();
            let ok = gzip_deflate(&me.to_vec());
            assert!(w.verify_client_custom_rules_data(&ok));
            assert!(!w.verify_client_custom_rules_data(&[]));
            assert!(!w.verify_client_custom_rules_data(&[1, 2, 3]));
            assert!(!w.verify_client_custom_rules_data(&gzip_deflate(&[])));
            // compared fields flip => false; ignored fields flip => true
            let mut bytes = me.to_vec();
            for off in [
                0usize, 3, 8, 0x13, 0x14, 0x15, 0x16, 0x17, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x39, 0x3a, 0x3b, 0x3c,
                0x3d, 0x3e, 0x18, 0x1e, 0x20, 0x2c,
            ] {
                let mut b = bytes.clone();
                b[off] ^= 1;
                assert!(!w.verify_client_custom_rules_data(&gzip_deflate(&b)), "offset {:#x}", off);
            }
            for off in [1usize, 2, 9, 0x12, 0x19 + 6, 0x36, 0x37, 0x38, 0x1f, 0x2e] {
                let mut b = bytes.clone();
                b[off] ^= 1;
                assert!(w.verify_client_custom_rules_data(&gzip_deflate(&b)), "offset {:#x}", off);
            }
            // short data: remaining bytes keep the expert-mode defaults
            bytes.truncate(1);
            let _ = w.verify_client_custom_rules_data(&gzip_deflate(&bytes));
        }
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::plist::json::json_object_with_data;

    fn dict(json: &str) -> PlistDict {
        let PlistValue::Dict(d) = json_object_with_data(json.as_bytes()).value.expect("json") else { panic!("dict") };
        d
    }

    #[test]
    fn const_helpers_and_categories_that_store_nothing() {
        let o = opt("x", 3, "a", &["a", "b"]);
        assert_eq!((o.name, o.category_key, o.default, o.options.len()), ("x", 3, Some("a"), 2));
        let mut r = default_custom_rules(false);
        let before = r;
        for cat in [1001, 1013, 99] {
            store_category(&mut r, cat, 1);
        }
        assert_eq!(r.health, before.health);
    }

    #[test]
    fn an_empty_data_seed_resets_to_random_and_odd_exponents_parse() {
        let mut d = dict("{\"seed\":\"custom\"}");
        d.insert("custom_seed_value".to_string(), PlistValue::data(Vec::<u8>::new()));
        let r = custom_rules_from_dict(Some(&d), false);
        assert_eq!(r.seed_type, 0);
        for s in ["1e5x", "1.5e+3z", "2e", "3E-2q", "9e99999999999"] {
            let _ = parse_seed(s);
        }
    }

    #[test]
    fn net_rules_with_slot_arrays_sun_colours_and_unstackable_slots() {
        let w = World::alloc(32);
        w.set_is_admin(true);
        // no dictionary: nothing to update
        w.set_sun_color_rule([1.0, 0.5, 0.0, 1.0]);
        w.custom_rule_dict_recieved_from_net(&dict("{\"health\":\"hard\"}"));
        *w.custom_rules_dict.borrow_mut() = Some(dict("{}"));
        let mut net = dict("{\"health\":\"hard\",\"spawnItemSlots\":[{\"type\":3,\"count\":2}]}");
        net.insert("other".to_string(), PlistValue::int(1));
        w.custom_rule_dict_recieved_from_net(&net);
        assert!(matches!(
            w.custom_rules_dict.borrow().as_ref().unwrap().get(&"spawnItemSlots".to_string()),
            Some(PlistValue::Array(_))
        ));
        // slots that are not a list are stored as sent
        let _ = crate::foundation::exception::catch_ns(|| {
            w.custom_rule_dict_recieved_from_net(&dict("{\"spawnItemSlots\":\"none\"}"));
        });
        *w.custom_rules_dict.borrow_mut() = Some(dict("{}"));
        // a slot item that cannot stack keeps a count of one
        let unstackable = (1..400).find(|&i| !item_type_is_stackable(ItemType(i), 0, 0)).unwrap() as i32;
        w.set_custom_slot_at_index_to_item_type_count(0, unstackable, 9);
        let d = w.custom_rules_dict.borrow();
        let slots = d.as_ref().unwrap().get(&"spawnItemSlots".to_string());
        assert!(
            matches!(slots, Some(PlistValue::Array(a)) if a.items[0].object_for_key("count").unwrap().int_value() == 1)
        );
    }
}
