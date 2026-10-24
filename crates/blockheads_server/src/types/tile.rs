//! types_structs: struct Tile (0x40 bytes) and the stored terrain block blob. Generated struct: tile_gen.rs.
use super::structs::WireStruct;

include!("tile_gen.rs");

impl Tile {
    /// Changes the block type. A mod block (`bhmod_content::variant`) keeps its record in `padding`; a change of the
    /// block type ends it, so the padding is cleared (it is zero in every vanilla world, so nothing observable
    /// changes without mods).
    pub fn retype(&mut self, type_index: u8) {
        if self.type_index != type_index {
            self.type_index = type_index;
            self.padding = [0; 16];
        }
    }
}

pub const BLOCK_DIM: usize = 32;
pub const BLOCK_TILES: usize = 1024;
pub const TILE_SIZE: usize = 64;
/// 1024 * 64 + 1 (version) + 4 (updateIndex)
pub const BLOCK_BLOB_LEN: usize = BLOCK_TILES * TILE_SIZE + 5; // 65541

/// index inside a block: `localY * 32 + localX`, localY = 0 is the bottom row
pub fn local_tile_index(local_x: usize, local_y: usize) -> usize {
    local_y * BLOCK_DIM + local_x
}

/// `Tile tiles[1024]` as raw bytes (65536). Padding is written from the stored fields (zero for new tiles).
pub fn tiles_to_bytes(tiles: &[Tile]) -> Vec<u8> {
    assert_eq!(tiles.len(), BLOCK_TILES);
    let mut v = vec![0u8; BLOCK_TILES * TILE_SIZE];
    for (i, t) in tiles.iter().enumerate() {
        t.write_le(&mut v[i * TILE_SIZE..]);
    }
    v
}
pub fn tiles_from_bytes(b: &[u8]) -> Option<Vec<Tile>> {
    if b.len() < BLOCK_TILES * TILE_SIZE {
        return None;
    }
    Some((0..BLOCK_TILES).map(|i| Tile::read_le(&b[i * TILE_SIZE..])).collect())
}

/// Plaintext of an LMDB `blocks` value BEFORE gzip (foundation::gzip does the gzipDeflate):
/// tiles (65536) + version (u8) + updateIndex (u32 LE) = 65541 bytes. The caller increments its in-memory
/// `updateIndex` AFTER serialising.
pub fn serialize_block_plain(tiles: &[Tile], version: u8, update_index: u32) -> Vec<u8> {
    let mut v = tiles_to_bytes(tiles);
    v.push(version);
    v.extend_from_slice(&update_index.to_le_bytes());
    debug_assert_eq!(v.len(), BLOCK_BLOB_LEN);
    v
}
/// inverse; requires exactly 65541 bytes (wrong length -> None; what the loader does with other lengths is the storage/world spec's call)
pub fn parse_block_plain(b: &[u8]) -> Option<(Vec<Tile>, u8, u32)> {
    if b.len() != BLOCK_BLOB_LEN {
        return None;
    }
    let tiles = tiles_from_bytes(b)?;
    let ver = b[BLOCK_TILES * TILE_SIZE];
    let upd = u32::from_le_bytes(b[BLOCK_TILES * TILE_SIZE + 1..].try_into().unwrap());
    Some((tiles, ver, upd))
}

#[cfg(test)]
mod hand_tests {
    use super::*;
    #[test]
    fn block_helpers() {
        assert_eq!(local_tile_index(5, 2), 69);
        assert!(tiles_from_bytes(&[0; 100]).is_none());
        assert!(parse_block_plain(&[0; 100]).is_none());
        let blob = vec![0u8; BLOCK_BLOB_LEN];
        let (tiles, ver, upd) = parse_block_plain(&blob).unwrap();
        assert_eq!((tiles.len(), ver, upd), (BLOCK_TILES, 0, 0));
        assert_eq!(tiles_to_bytes(&tiles).len(), BLOCK_TILES * TILE_SIZE);
    }
    #[test]
    fn tile_field_offsets_in_bytes() {
        let mut t = Tile::default();
        t.type_index = 25;
        t.sun_light = 255;
        t.artificial_heat = -1;
        t.dynamic_object_owner = 0x0102030405060708;
        t.paint_bot = 0xbeef;
        let b = t.to_vec();
        assert_eq!(b.len(), 64);
        assert_eq!((b[0], b[7]), (25, 255));
        assert_eq!(&b[20..22], &[0xff, 0xff]); // artificialHeat @0x14
        assert_eq!(&b[0x24..0x26], &[0xef, 0xbe]); // paintBot @0x24
        assert_eq!(&b[0x28..0x30], &[8, 7, 6, 5, 4, 3, 2, 1]); // dynamicObjectOwner @0x28 LE
        assert!(b[0xd] == 0 && b[0x17] == 0 && b[0x26..0x28] == [0, 0] && b[0x30..].iter().all(|&x| x == 0));
    }
    #[test]
    fn retype_ends_a_mod_record_only_when_the_block_changes() {
        let mut t = Tile::default();
        t.type_index = 11;
        t.padding = [7; 16];
        t.retype(11);
        assert_eq!(t.padding, [7; 16]);
        t.retype(2);
        assert_eq!((t.type_index, t.padding), (2, [0; 16]));
    }
    #[test]
    fn block_blob_roundtrip() {
        let tiles = vec![Tile::default(); BLOCK_TILES];
        let b = serialize_block_plain(&tiles, 8, 0x01020304);
        assert_eq!(b.len(), 65541);
        assert_eq!(&b[65536..], &[8, 4, 3, 2, 1]);
        let (t2, v, u) = parse_block_plain(&b).unwrap();
        assert_eq!((t2.len(), v, u), (1024, 8, 0x01020304));
    }
}
