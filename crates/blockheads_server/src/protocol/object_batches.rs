//! Object create/update/remove batch packets (0x07/0x46/0x08/0x09 server, 0x0a/0x0b/0x0c/0x45 client).
//!

use super::codec::{gz_plist_from, plist_gz, Malformed, WireResult};
use crate::plist::value::PlistValue as PV;

/// Object types 0..=64.
pub const NUM_TYPE_BUCKETS: usize = 65;

/// Server -> client batch kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatchKind {
    Create = 0x07,
    CreationDataUpdate = 0x46,
    Update = 0x08,
    Remove = 0x09,
}
/// Client -> server batch kinds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClientBatchKind {
    Create = 0x0a,
    Update = 0x0b,
    Remove = 0x0c,
    CreationDataUpdate = 0x45,
}

fn encode(kind: u8, object_type: u8, items: &[Vec<u8>]) -> Option<Vec<u8>> {
    let arr = PV::array(items.iter().map(|b| PV::data(b.clone())).collect());
    let z = plist_gz(&arr)?;
    let mut v = Vec::with_capacity(2 + z.len());
    v.push(kind);
    v.push(object_type);
    v.extend_from_slice(&z);
    Some(v)
}

/// `[kind][objectType][gzip(XML plist array<data>)]`; `None` iff the plist helper fails.
pub fn encode_batch(kind: BatchKind, object_type: u8, items: &[Vec<u8>]) -> Option<Vec<u8>> {
    encode(kind as u8, object_type, items)
}
/// Client form (test client).
pub fn encode_client_batch(kind: ClientBatchKind, object_type: u8, items: &[Vec<u8>]) -> Option<Vec<u8>> {
    encode(kind as u8, object_type, items)
}

/// Decoded batch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Batch {
    pub kind: u8,
    pub object_type: u8,
    pub items: Vec<Vec<u8>>,
}

/// Strip the 2-byte prefix, gunzip, plist-decode, extract the NSData array.
/// `Err(NSMallocException)` for a 1-byte packet (`len-2` negative,);
/// `Ok(None)` = the original's nil array.
pub fn decode_batch(packet: &[u8]) -> WireResult<Option<Batch>> {
    if packet.len() < 2 {
        return Err(Malformed::malloc());
    }
    let (kind, object_type) = (packet[0], packet[1]);
    let Some(PV::Array(a)) = gz_plist_from(&packet[2..]) else {
        return Ok(None);
    };
    let items = a
        .items
        .iter()
        .filter_map(|v| match v {
            PV::Data(d) => Some(d.bytes.clone()),
            _ => None,
        })
        .collect();
    Ok(Some(Batch { kind, object_type, items }))
}

/// First 8 bytes of a blob as u64 LE: the object unique id.
pub fn blob_unique_id(blob: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(blob.get(..8)?.try_into().unwrap()))
}
