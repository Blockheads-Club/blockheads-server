//! owned by skeleton: module declarations only. Do not edit; update the module table first.

pub mod glibc_rand;
pub mod mt19937;
pub mod trace;

//  enable once `glibc_rand` (owner: rng) publishes these items.
// Left commented because glibc_rand.rs is still a placeholder and the line would break `cargo check`.
// pub use glibc_rand::{srand, rand, rand_tagged, r01, r01_tagged};
