//! The Blockheads dedicated server (v1.7.1 compatible), clean-room Rust reimplementation.
//! owned by skeleton: module declarations only.

pub mod app;
pub mod blockhead;
pub mod building;
pub mod crafting;
pub mod dynworld;
pub mod env;
pub mod fmath;
pub mod foundation;
pub mod host;
pub mod interaction;
pub mod items;
pub mod lighting;
pub mod net;
pub mod npc;
pub mod objects;
pub mod path;
pub mod physics_objects;
pub mod plants;
pub mod plist;
pub mod protocol;
pub mod rng;
pub mod server;
pub mod storage;
#[cfg(test)]
pub(crate) mod testenv;
pub mod trains;
pub mod trees;
pub mod types;
pub mod world;
pub mod worldgen;

pub mod prelude;
