//! Content of the loaded mods (milestone M8): the world's id registry (`<world>/mods/registry.toml`) and the
//! cosmetics the server knows. Built once when the host loads.
//!
//! Safety rule: the sidecar is never reset. If it cannot be read, parsed or written, or a kind has no free id left,
//! the problem is logged and content stays **off** for this session (no registry is sent, nothing can be equipped);
//! the file is left exactly as it was, so fixing it brings the ids back. A server whose mods declare no content
//! touches nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bhmod_content::registry::{declared, Wire};
use bhmod_content::{Kind, Registry};
use bhmod_core::discover::discover;
use bhmod_manifest::content::base_tile;

use super::{ContentTables, VariantInfo};

/// The content of the offered mods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Content {
    /// `MOD_LIST.registry`.
    pub wire: Wire,
    /// Cosmetic id to its slot.
    pub cosmetics: BTreeMap<u64, String>,
    /// The `ItemType`s of the mods' items.
    pub items: BTreeSet<u64>,
    /// Variant blocks by id (only those that name a known `base`).
    pub variants: BTreeMap<u64, VariantInfo>,
}

impl Content {
    /// True when some cosmetic can be worn.
    pub fn has_cosmetics(&self) -> bool {
        !self.cosmetics.is_empty()
    }

    /// What the game rules need to know: the mod items and, when `variants_on`, the variant blocks.
    pub fn tables(&self, variants_on: bool) -> ContentTables {
        ContentTables {
            items: self.items.iter().filter_map(|i| u32::try_from(*i).ok()).collect(),
            variants: if variants_on {
                self.variants.iter().filter_map(|(id, v)| Some((u16::try_from(*id).ok()?, *v))).collect()
            } else {
                BTreeMap::new()
            },
        }
    }

    /// The key of a content id of `kind` in this world.
    pub fn key(&self, kind: Kind, id: u64) -> Option<&str> {
        self.wire.get(kind.name())?.iter().find(|(_, i)| **i == id).map(|(k, _)| k.as_str())
    }

    /// The id of a content key of `kind` in this world.
    pub fn id(&self, kind: Kind, key: &str) -> Option<u64> {
        self.wire.get(kind.name())?.get(key).copied()
    }
}

/// Allocates ids for the content of the `offered` mods (ids of packages in `dir` that are offered to clients) and
/// keeps the sidecar at `sidecar` up to date. Problems go to `log`.
pub fn build(dir: &Path, offered: &[String], sidecar: &Path, log: &mut dyn FnMut(String)) -> Content {
    let (packages, _) = discover(dir);
    let manifests: Vec<_> = packages.iter().map(|p| &p.manifest).filter(|m| offered.contains(&m.id)).collect();
    let wanted = declared(manifests.iter().copied());
    if wanted.is_empty() {
        return Content::default();
    }
    let mut registry = match Registry::read(sidecar) {
        Ok(r) => r,
        Err(e) => {
            log(format!("content is off: {e}; the file was left untouched, fix or remove it to bring content back"));
            return Content::default();
        }
    };
    match registry.allocate(&wanted) {
        Ok(true) => {
            if let Err(e) = registry.write(sidecar) {
                log(format!("content is off: {e}"));
                return Content::default();
            }
        }
        Ok(false) => {}
        Err(e) => {
            log(format!("content is off: {e}"));
            return Content::default();
        }
    }
    let wire = registry.wire(&wanted);
    let mut cosmetics = BTreeMap::new();
    for m in &manifests {
        for c in &m.content.cosmetics {
            if let Some(id) = registry.id(Kind::Cosmetics, &c.key) {
                cosmetics.insert(id, c.slot.clone());
            }
        }
    }
    let mut items = BTreeSet::new();
    let mut variants = BTreeMap::new();
    for m in &manifests {
        items.extend(m.content.items.iter().filter_map(|i| registry.id(Kind::Items, &i.key)));
        for b in &m.content.blocks {
            let (Some(id), Some(base)) = (registry.id(Kind::Blocks, &b.key), b.base.as_deref().and_then(base_tile))
            else {
                continue;
            };
            let drops =
                b.drops.as_deref().and_then(|k| registry.id(Kind::Items, k)).and_then(|i| u32::try_from(i).ok());
            variants.insert(id, VariantInfo { base: base.tile_type, drops });
        }
    }
    Content { wire, cosmetics, items, variants }
}

#[cfg(test)]
mod tests;
