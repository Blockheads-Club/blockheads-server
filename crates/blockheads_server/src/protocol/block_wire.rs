//! Block (0x04), light block (0x28), block-not-loaded (0x27) and block request (0x03/0x0d) wire forms.
//!

use super::codec::{self, frame, gunz, gz, plist_from, plist_xml, t, Rd, Wr};
use crate::foundation::gsdict::GsDict;
use crate::foundation::md5;
use crate::plist::value::PlistValue as PV;
use crate::types::structs::WireStruct;

pub use crate::types::netdata::ClientMacroBlockRequest;

pub const TILE_BYTES: usize = 0x40;
pub const BLOCK_TILES: usize = 1024;
pub const TILES_LEN: usize = 0x10000;
/// tiles + version u8 + updateIndex u32.
pub const TILES_BLOB_LEN: usize = 0x10005;
pub const LIGHT_LEN: usize = 0x400;

/// "t" blob before gzip: tiles ++ [version] ++ updateIndex LE.
pub fn tiles_blob(tiles: &[u8; TILES_LEN], version: u8, update_index: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(TILES_BLOB_LEN);
    v.extend_from_slice(tiles);
    v.push(version);
    v.extend_from_slice(&update_index.to_le_bytes());
    v
}

/// Fill-receipt hash: lowercase md5 hex of `"<playerID>_<x>_<y>_<adminByte>"` (`%d` of block xPos/yPos).
pub fn block_hash(player_id: &str, x: i32, y: i32, admin: bool) -> String {
    md5::string_from_md5(&format!("{}_{}_{}_{}", player_id, x, y, admin as u8))
}

/// Extra-data plist of the "r" blob.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtraData {
    pub hash: String,
    /// array of unsigned ints; key absent when `None`.
    pub receipts: Option<Vec<u32>>,
}

impl ExtraData {
    /// The plist dictionary `{hash, receipts?}`.
    pub fn to_plist(&self) -> PV {
        let mut m: GsDict<String, PV> = GsDict::new();
        m.insert("hash".to_string(), PV::string(self.hash.clone()));
        if let Some(r) = &self.receipts {
            m.insert("receipts".to_string(), PV::array(r.iter().map(|&x| PV::int(x as i64)).collect()));
        }
        PV::dict(m)
    }
}

/// "r" blob before gzip: light(0x400) ++ XML plist {hash, receipts?}. `None` if the plist helper fails.
pub fn light_blob(light: &[u8; LIGHT_LEN], extra: &ExtraData) -> Option<Vec<u8>> {
    let p = plist_xml(&extra.to_plist())?;
    let mut v = Vec::with_capacity(LIGHT_LEN + p.len());
    v.extend_from_slice(light);
    v.extend_from_slice(&p);
    Some(v)
}

/// 0x04 SERVER_BLOCK: `[04][u32 macroIndex][XML plist {"r"?: data, "t": data}]`.
pub fn server_block(macro_index: u32, t_gz: &[u8], r_gz: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut m: GsDict<String, PV> = GsDict::new();
    if let Some(r) = r_gz {
        m.insert("r".to_string(), PV::data(r.to_vec()));
    }
    m.insert("t".to_string(), PV::data(t_gz.to_vec()));
    let p = plist_xml(&PV::dict(m))?;
    let mut w = Wr::with_type(t::SERVER_BLOCK);
    w.u32(macro_index).bytes(&p);
    Some(w.finish())
}

/// Convenience doing the gzip steps in the order of `sendBlockToClientWithoutSaving...`:
/// tiles blob -> gz, light blob -> gz, then the packet.
pub fn server_block_from_parts(
    macro_index: u32,
    tiles: &[u8; TILES_LEN],
    version: u8,
    update_index: u32,
    light: Option<(&[u8; LIGHT_LEN], &ExtraData)>,
) -> Option<Vec<u8>> {
    let t_gz = gz(&tiles_blob(tiles, version, update_index));
    let r_gz = match light {
        Some((l, e)) => Some(gz(&light_blob(l, e)?)),
        None => None,
    };
    server_block(macro_index, &t_gz, r_gz.as_deref())
}

/// 0x27 SERVER_BLOCK_NOT_LOADED: `[27][u64 index][u32 index]` (13 bytes).
pub fn block_not_loaded(index: u64) -> Vec<u8> {
    let mut w = Wr::with_type(t::SERVER_BLOCK_NOT_LOADED);
    w.u64(index).u32(index as u32);
    w.finish()
}

/// 0x28 SERVER_LIGHT_BLOCK: `[28][i32 macroIndex][gzip(light ++ u32 updateIndex)]`.
pub fn light_block(macro_index: i32, light: &[u8; LIGHT_LEN], update_index: u32) -> Vec<u8> {
    let mut blob = Vec::with_capacity(LIGHT_LEN + 4);
    blob.extend_from_slice(light);
    blob.extend_from_slice(&update_index.to_le_bytes());
    let mut w = Wr::with_type(t::SERVER_LIGHT_BLOCK);
    w.i32(macro_index).bytes(&gz(&blob));
    w.finish()
}

/// 0x03 BLOCK_REQUEST (client side): `[03][i32][u8][3 zero pad]` (9 bytes).
pub fn block_request(r: ClientMacroBlockRequest) -> Vec<u8> {
    let mut w = Wr::with_type(t::CLIENT_BLOCK_REQUEST);
    w.i32(r.macro_index).u8(r.create_if_not_created).zeros(3);
    w.finish()
}

/// 0x0d BLOCK_REMOVAL_NOTIFICATION (client side): `[0d][u32]`.
pub fn block_removal_notification(macro_index: u32) -> Vec<u8> {
    frame(t::CLIENT_BLOCK_REMOVAL_NOTIFICATION, &macro_index.to_le_bytes())
}

/// Decoded 0x04.
#[derive(Clone, Debug)]
pub struct BlockMsg {
    pub macro_index: u32,
    /// 0x10000 bytes
    pub tiles: Vec<u8>,
    pub version: u8,
    pub update_index: u32,
    /// 0x400 bytes (zeros when "r" is absent)
    pub light: Vec<u8>,
    /// decoded extra plist `{hash, receipts?}`
    pub extra: Option<PV>,
}

/// Decode a 0x04 payload (packet minus type). Needs the "t" inflate to be exactly 0x10005 bytes.
pub fn decode_server_block(payload: &[u8]) -> Option<BlockMsg> {
    let mut r = Rd::new(payload);
    let macro_index = r.u32()?;
    let dict = plist_from(r.rest())?;
    let t_gz = match dict.object_for_key("t")? {
        PV::Data(d) => d.bytes.clone(),
        _ => return None,
    };
    let tb = gunz(&t_gz)?;
    if tb.len() != TILES_BLOB_LEN {
        return None;
    }
    let tiles = tb[..TILES_LEN].to_vec();
    let version = tb[TILES_LEN];
    let update_index = u32::from_le_bytes(tb[TILES_LEN + 1..TILES_LEN + 5].try_into().unwrap());
    let (light, extra) = match dict.object_for_key("r") {
        Some(PV::Data(d)) => {
            let rb = gunz(&d.bytes)?;
            if rb.len() < LIGHT_LEN {
                return None;
            }
            (rb[..LIGHT_LEN].to_vec(), plist_from(&rb[LIGHT_LEN..]))
        }
        _ => (vec![0u8; LIGHT_LEN], None),
    };
    Some(BlockMsg { macro_index, tiles, version, update_index, light, extra })
}

/// Decoded 0x28.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightBlockMsg {
    pub macro_index: u32,
    pub light: Vec<u8>,
    pub update_index: u32,
}
/// Decode a 0x28 payload (packet minus type).
pub fn decode_light_block(payload: &[u8]) -> Option<LightBlockMsg> {
    let mut r = Rd::new(payload);
    let macro_index = r.u32()?;
    let blob = gunz(r.rest())?;
    if blob.len() < LIGHT_LEN + 4 {
        return None;
    }
    Some(LightBlockMsg {
        macro_index,
        light: blob[..LIGHT_LEN].to_vec(),
        update_index: u32::from_le_bytes(blob[LIGHT_LEN..LIGHT_LEN + 4].try_into().unwrap()),
    })
}
/// Decode a 0x27 payload: `(u64 index, u32 index)`.
pub fn decode_block_not_loaded(payload: &[u8]) -> Option<(u64, u32)> {
    let mut r = Rd::new(payload);
    Some((r.u64()?, r.u32()?))
}

#[allow(unused)]
fn _uses() {
    let _ = codec::split_type;
    let _ = ClientMacroBlockRequest::SIZE;
}
