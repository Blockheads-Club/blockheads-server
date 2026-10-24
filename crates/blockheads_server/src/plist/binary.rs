//! `bplist00` writer (GNUstep 1.24 `NSPropertyListBinaryFormat_v1_0`) and reader.
//!

use super::value::{ClassTag, Origin, PlistArray, PlistData, PlistDict, PlistStr, PlistValue};
use crate::foundation::exception::NsException;
use crate::foundation::gsdict::GsDict;
use std::collections::HashMap;

/// Writer-side object model: dictionaries are ordered pair lists so callers (the keyed archiver) can fix key order.
#[derive(Clone, Debug)]
pub enum BNode {
    Bool(bool),
    Int(i64),
    Real { v: f64, f32: bool },
    Date(f64),
    Data(Vec<u8>),
    Str(String),
    Uid(u64),
    Array(Vec<BNode>),
    Dict(Vec<(BNode, BNode)>),
}

/// Minimal big-endian byte count (1, 2, 4 or 8).
fn nbytes(v: u64) -> usize {
    if v <= 0xff {
        1
    } else if v <= 0xffff {
        2
    } else if v <= 0xffff_ffff {
        4
    } else {
        8
    }
}

fn size_code(k: usize) -> u8 {
    match k {
        1 => 0,
        2 => 1,
        4 => 2,
        _ => 3,
    }
}

fn be(v: u64, k: usize) -> Vec<u8> {
    v.to_be_bytes()[8 - k..].to_vec()
}

fn lenhdr(m: u8, n: usize) -> Vec<u8> {
    if n < 15 {
        vec![m | n as u8]
    } else {
        let k = nbytes(n as u64);
        let mut v = vec![m | 0x0f, 0x10 | size_code(k)];
        v.extend(be(n as u64, k));
        v
    }
}

/// Structural key used for uniquing.
fn canon(n: &BNode, out: &mut Vec<u8>) {
    match n {
        BNode::Bool(b) => out.extend_from_slice(&[b'b', *b as u8]),
        BNode::Int(i) => {
            out.push(b'i');
            out.extend_from_slice(&i.to_be_bytes());
        }
        BNode::Real { v, f32 } => {
            out.push(b'r');
            out.push(*f32 as u8);
            // value equality (so 0.0 == -0.0), as the tuple comparison of the reference model
            let nv = if *v == 0.0 { 0.0f64 } else { *v };
            out.extend_from_slice(&nv.to_bits().to_be_bytes());
        }
        BNode::Date(d) => {
            out.push(b'd');
            out.extend_from_slice(&d.to_bits().to_be_bytes());
        }
        BNode::Data(d) => {
            out.push(b'x');
            out.extend_from_slice(&(d.len() as u64).to_be_bytes());
            out.extend_from_slice(d);
        }
        BNode::Str(s) => {
            out.push(b's');
            out.extend_from_slice(&(s.len() as u64).to_be_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        BNode::Uid(u) => {
            out.push(b'u');
            out.extend_from_slice(&u.to_be_bytes());
        }
        BNode::Array(a) => {
            out.push(b'[');
            out.extend_from_slice(&(a.len() as u64).to_be_bytes());
            for x in a {
                canon(x, out);
            }
        }
        BNode::Dict(d) => {
            out.push(b'{');
            out.extend_from_slice(&(d.len() as u64).to_be_bytes());
            for (k, v) in d {
                canon(k, out);
                canon(v, out);
            }
        }
    }
}

fn children(n: &BNode) -> Vec<&BNode> {
    match n {
        BNode::Array(a) => a.iter().collect(),
        BNode::Dict(d) => d.iter().map(|(k, _)| k).chain(d.iter().map(|(_, v)| v)).collect(),
        _ => Vec::new(),
    }
}

fn encode(n: &BNode, rsz: usize, refs: &mut dyn FnMut(&BNode) -> usize) -> Vec<u8> {
    match n {
        BNode::Bool(b) => vec![if *b { 0x09 } else { 0x08 }],
        BNode::Int(v) => {
            let k = if (0..=0xffff_ffff).contains(v) { nbytes(*v as u64) } else { 8 };
            let mut o = vec![0x10 | size_code(k)];
            o.extend(be(*v as u64, k));
            o
        }
        BNode::Real { v, f32 } => {
            if *f32 {
                let mut o = vec![0x22];
                o.extend_from_slice(&(*v as f32).to_be_bytes());
                o
            } else {
                let mut o = vec![0x23];
                o.extend_from_slice(&v.to_be_bytes());
                o
            }
        }
        BNode::Date(d) => {
            let mut o = vec![0x33];
            o.extend_from_slice(&d.to_be_bytes());
            o
        }
        BNode::Data(d) => {
            let mut o = lenhdr(0x40, d.len());
            o.extend_from_slice(d);
            o
        }
        BNode::Str(s) => {
            if s.is_ascii() {
                let mut o = lenhdr(0x50, s.len());
                o.extend_from_slice(s.as_bytes());
                o
            } else {
                let u: Vec<u16> = s.encode_utf16().collect();
                let mut o = lenhdr(0x60, u.len());
                for x in u {
                    o.extend_from_slice(&x.to_be_bytes());
                }
                o
            }
        }
        BNode::Uid(u) => {
            let k = nbytes(*u);
            let mut o = vec![0x80 | (k as u8 - 1)];
            o.extend(be(*u, k));
            o
        }
        BNode::Array(a) => {
            let mut o = lenhdr(0xa0, a.len());
            for x in a {
                o.extend(be(refs(x) as u64, rsz));
            }
            o
        }
        BNode::Dict(d) => {
            let mut o = lenhdr(0xd0, d.len());
            for (k, _) in d {
                o.extend(be(refs(k) as u64, rsz));
            }
            for (_, v) in d {
                o.extend(be(refs(v) as u64, rsz));
            }
            o
        }
    }
}

/// Serialise an object tree: BFS object table with structural uniquing, trailer.
pub fn write_bnode(root: &BNode) -> Vec<u8> {
    let mut objs: Vec<&BNode> = vec![root];
    let mut idx: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut key = Vec::new();
    canon(root, &mut key);
    idx.insert(key, 0);
    let mut i = 0;
    while i < objs.len() {
        for c in children(objs[i]) {
            let mut k = Vec::new();
            canon(c, &mut k);
            if let std::collections::hash_map::Entry::Vacant(e) = idx.entry(k) {
                e.insert(objs.len());
                objs.push(c);
            }
        }
        i += 1;
    }
    let n = objs.len();
    let rsz = if n > 1 { nbytes((n - 1) as u64) } else { 1 };
    let mut body = Vec::new();
    let mut offs = Vec::with_capacity(n);
    let mut lookup = |x: &BNode| -> usize {
        let mut k = Vec::new();
        canon(x, &mut k);
        idx[&k]
    };
    for o in &objs {
        offs.push(8 + body.len());
        let e = encode(o, rsz, &mut lookup);
        body.extend(e);
    }
    let tab = 8 + body.len();
    let osz = nbytes(tab as u64);
    let mut out = b"bplist00".to_vec();
    out.extend(body);
    for o in offs {
        out.extend(be(o as u64, osz));
    }
    out.extend_from_slice(&[0; 6]);
    out.push(osz as u8);
    out.push(rsz as u8);
    out.extend_from_slice(&(n as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&(tab as u64).to_be_bytes());
    out
}

fn to_bnode(v: &PlistValue) -> BNode {
    match v {
        PlistValue::String(s) => BNode::Str(s.s.clone()),
        PlistValue::Int(i) => BNode::Int(*i),
        PlistValue::UInt(u) => BNode::Int(*u as i64),
        PlistValue::Real { value, is_float } => BNode::Real { v: *value, f32: *is_float },
        PlistValue::Bool(b) => BNode::Bool(*b),
        PlistValue::Date(d) => BNode::Date(*d),
        PlistValue::Data(d) => BNode::Data(d.bytes.clone()),
        PlistValue::Array(a) => BNode::Array(a.items.iter().map(to_bnode).collect()),
        PlistValue::Dict(d) => {
            // a CF$UID dictionary is how the generic reader presents a UID; write it back as one is NOT done: keep as dict
            BNode::Dict(d.map.iter().map(|(k, v)| (BNode::Str(k.clone()), to_bnode(v))).collect())
        }
        // Note: NSNull has no binary encoding in the spec; written as its description string.
        PlistValue::Null => BNode::Str("<null>".to_string()),
    }
}

/// GNUstep binary writer (format 200); dictionary keys in `GsDict` enumeration order.
pub fn write_bplist(v: &PlistValue) -> Vec<u8> {
    write_bnode(&to_bnode(v))
}

// ---------------------------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------------------------

struct Reader<'a> {
    d: &'a [u8],
    offs: Vec<usize>,
    rsz: usize,
    ctag: ClassTag,
    ltag: ClassTag,
}

fn be_read(b: &[u8]) -> u64 {
    b.iter().fold(0u64, |a, &x| (a << 8) | x as u64)
}

impl<'a> Reader<'a> {
    fn get(&self, off: usize, n: usize) -> Option<&'a [u8]> {
        self.d.get(off..off.checked_add(n)?)
    }

    /// Length nibble handling; returns (length, offset after header).
    fn length(&self, off: usize, low: u8) -> Option<(usize, usize)> {
        if low != 0x0f {
            return Some((low as usize, off + 1));
        }
        let m = *self.d.get(off + 1)?;
        if m >> 4 != 1 {
            return None;
        }
        let k = 1usize << (m & 0xf);
        if k > 8 {
            return None;
        }
        let v = be_read(self.get(off + 2, k)?);
        Some((usize::try_from(v).ok()?, off + 2 + k))
    }

    fn object(&self, idx: usize, depth: u32) -> Option<PlistValue> {
        if depth > 256 {
            return None;
        }
        let off = *self.offs.get(idx)?;
        let b = *self.d.get(off)?;
        let (hi, lo) = (b >> 4, b & 0xf);
        match hi {
            0 => match b {
                0x08 => Some(PlistValue::Bool(false)),
                0x09 => Some(PlistValue::Bool(true)),
                _ => None,
            },
            1 => {
                let k = 1usize << lo;
                if k > 8 {
                    return None;
                }
                let raw = be_read(self.get(off + 1, k)?);
                let v = if k == 8 { raw as i64 } else { raw as i64 };
                Some(PlistValue::Int(v))
            }
            2 => match lo {
                2 => Some(PlistValue::Real {
                    value: f32::from_be_bytes(self.get(off + 1, 4)?.try_into().ok()?) as f64,
                    is_float: true,
                }),
                3 => Some(PlistValue::Real {
                    value: f64::from_be_bytes(self.get(off + 1, 8)?.try_into().ok()?),
                    is_float: false,
                }),
                _ => None,
            },
            3 => {
                if lo != 3 {
                    return None;
                }
                Some(PlistValue::Date(f64::from_be_bytes(self.get(off + 1, 8)?.try_into().ok()?)))
            }
            4 => {
                let (n, p) = self.length(off, lo)?;
                Some(PlistValue::Data(PlistData { bytes: self.get(p, n)?.to_vec(), tag: self.ltag }))
            }
            5 => {
                let (n, p) = self.length(off, lo)?;
                let s = self.get(p, n)?.iter().map(|&c| c as char).collect::<String>();
                Some(PlistValue::String(PlistStr { s, tag: self.ltag }))
            }
            6 => {
                let (n, p) = self.length(off, lo)?;
                let raw = self.get(p, n.checked_mul(2)?)?;
                let u: Vec<u16> = raw.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                Some(PlistValue::String(PlistStr { s: String::from_utf16_lossy(&u), tag: self.ltag }))
            }
            8 => {
                let k = lo as usize + 1;
                let v = be_read(self.get(off + 1, k)?);
                let mut m = GsDict::plist_parsed();
                m.insert("CF$UID".to_string(), PlistValue::Int(v as i64));
                Some(PlistValue::Dict(PlistDict {
                    map: m,
                    tag: ClassTag { origin: Origin::Binary, mutable: false, alt: false },
                }))
            }
            0xa => {
                let (n, p) = self.length(off, lo)?;
                let raw = self.get(p, n.checked_mul(self.rsz)?)?;
                let mut items = Vec::with_capacity(n.min(1 << 16));
                for c in raw.chunks(self.rsz) {
                    items.push(self.object(be_read(c) as usize, depth + 1)?);
                }
                Some(PlistValue::Array(PlistArray { items, tag: self.ctag }))
            }
            0xd => {
                let (n, p) = self.length(off, lo)?;
                let raw = self.get(p, n.checked_mul(self.rsz)?.checked_mul(2)?)?;
                let (kr, vr) = raw.split_at(n * self.rsz);
                let mut pairs = Vec::with_capacity(n.min(1 << 16));
                for (kc, vc) in kr.chunks(self.rsz).zip(vr.chunks(self.rsz)) {
                    let k = self.object(be_read(kc) as usize, depth + 1)?;
                    let v = self.object(be_read(vc) as usize, depth + 1)?;
                    let PlistValue::String(k) = k else { return None };
                    pairs.push((k.s, v));
                }
                // `dictionaryWithObjects:forKeys:count:` (capacity = count, keys in file order): the GSIMap enumeration
                // order of `plist_golden/xml_read_expected/dict_*.bin.out` / `ka_*.bin.out`.
                let m = GsDict::from_pairs(pairs);
                Some(PlistValue::Dict(PlistDict { map: m, tag: self.ctag }))
            }
            _ => None,
        }
    }
}

/// GNUstep binary reader. `Ok(None)` is nil (bad header/body), `Err` a raised exception.
pub fn read_bplist(d: &[u8], opts: u32) -> Result<Option<PlistValue>, NsException> {
    if d.len() < 40 {
        return Ok(None);
    }
    let t = &d[d.len() - 32..];
    let osz = t[6] as usize;
    let rsz = t[7] as usize;
    if !(1..=8).contains(&osz) {
        return Err(NsException {
            name: "NSGenericException".to_string(),
            reason: Some(format!("Unknown offset size {}", t[6])),
        });
    }
    if !(1..=8).contains(&rsz) {
        return Ok(None);
    }
    let nobj = be_read(&t[8..16]) as usize;
    let top = be_read(&t[16..24]) as usize;
    let tab = be_read(&t[24..32]) as usize;
    let total = match nobj.checked_mul(osz) {
        Some(x) => x,
        None => return Ok(None),
    };
    let Some(tabbytes) = tab.checked_add(0).and_then(|t0| d.get(t0..t0.checked_add(total)?)) else {
        return Ok(None);
    };
    if nobj == 0 || top >= nobj {
        return Ok(None);
    }
    let offs: Vec<usize> = tabbytes.chunks(osz).map(|c| be_read(c) as usize).collect();
    let r = Reader {
        d,
        offs,
        rsz,
        ctag: ClassTag { origin: Origin::Binary, mutable: opts != 0, alt: false },
        ltag: ClassTag { origin: Origin::Binary, mutable: opts == 2, alt: false },
    };
    Ok(r.object(top, 0))
}

#[cfg(test)]
mod tests {
    use super::super::value::PlistValue as V;
    use super::*;

    fn arr(items: Vec<V>) -> V {
        V::array(items)
    }

    #[test]
    fn round_trip_and_classes() {
        let mut m = GsDict::new();
        m.insert("i".to_string(), V::int(1));
        m.insert("d".to_string(), V::real_f64(1.5));
        m.insert("b".to_string(), V::Bool(true));
        m.insert("s".to_string(), V::string("s"));
        m.insert("x".to_string(), V::data(vec![0, 0]));
        m.insert("u".to_string(), V::string("h\u{e9}llo \u{20ac}"));
        let bytes = write_bplist(&V::dict(m));
        let v = read_bplist(&bytes, 0).unwrap().unwrap();
        assert_eq!(v.class_name(), "GSDictionary");
        assert_eq!(v.object_for_key("s").unwrap().class_name(), "GSCBufferString");
        assert_eq!(v.object_for_key("u").unwrap().as_str(), Some("h\u{e9}llo \u{20ac}"));
        assert_eq!(v.object_for_key("i").unwrap().class_name(), "NSIntNumber");
        let a = read_bplist(&write_bplist(&arr(vec![V::int(1)])), 0).unwrap().unwrap();
        assert_eq!(a.class_name(), "GSInlineArray");
        let a = read_bplist(&write_bplist(&arr(vec![V::int(1)])), 1).unwrap().unwrap();
        assert_eq!(a.class_name(), "GSMutableArray");
    }

    #[test]
    fn errors() {
        assert!(read_bplist(b"bplist00\0", 0).unwrap().is_none());
        let mut bad = write_bplist(&arr(vec![V::int(1)]));
        let n = bad.len();
        bad[n - 26] = 0;
        let e = read_bplist(&bad, 0).unwrap_err();
        assert_eq!(e.name, "NSGenericException");
        assert_eq!(e.reason.as_deref(), Some("Unknown offset size 0"));
        bad[n - 26] = 255;
        assert_eq!(read_bplist(&bad, 0).unwrap_err().reason.as_deref(), Some("Unknown offset size 255"));
    }
}

#[cfg(test)]
mod edge_cases {
    use super::super::value::PlistValue as V;
    use super::*;

    /// A bplist whose objects are the given encodings (1-byte offsets and references), `top` as the root.
    fn mk(objs: &[Vec<u8>], top: u64) -> Vec<u8> {
        let mut d = b"bplist00".to_vec();
        let mut offs = vec![];
        for o in objs {
            offs.push(d.len() as u8);
            d.extend_from_slice(o);
        }
        let tab = d.len() as u64;
        d.extend_from_slice(&offs);
        trailer(&mut d, 1, 1, objs.len() as u64, top, tab);
        d
    }
    fn trailer(d: &mut Vec<u8>, osz: u8, rsz: u8, nobj: u64, top: u64, tab: u64) {
        d.extend_from_slice(&[0, 0, 0, 0, 0, 0, osz, rsz]);
        for v in [nobj, top, tab] {
            d.extend_from_slice(&v.to_be_bytes());
        }
    }
    fn read1(o: &[u8]) -> Option<V> {
        let mut d = vec![0u8; 32]; // padding so the file reaches the 40 byte minimum
        d.clear();
        d.extend_from_slice(&mk(&[o.to_vec()], 0));
        read_bplist(&d, 0).unwrap()
    }

    #[test]
    fn writer_value_kinds() {
        assert_eq!(size_code(8), 3);
        let long = V::array((0..300).map(V::int).collect());
        let V::Array(back) = read_bplist(&write_bplist(&long), 0).unwrap().unwrap() else { panic!() };
        assert_eq!(back.items.len(), 300);
        assert_eq!(nbytes(1 << 40), 8);
        let a = V::array(vec![V::real_f32(1.5), V::Date(12.5), V::UInt(7), V::Null, V::Bool(false), V::Bool(true)]);
        let back = read_bplist(&write_bplist(&a), 0).unwrap().unwrap();
        let V::Array(items) = back else { panic!() };
        assert!(matches!(items.items[0], V::Real { value, is_float: true } if value == 1.5));
        assert!(matches!(items.items[1], V::Date(d) if d == 12.5));
        assert!(matches!(items.items[2], V::Int(7)));
        assert_eq!(items.items[3].as_str(), Some("<null>"));
        assert!(matches!(items.items[4], V::Bool(false)));
        assert!(matches!(items.items[5], V::Bool(true)));
        let uid = write_bnode(&BNode::Array(vec![BNode::Uid(u64::MAX), BNode::Uid(3)]));
        let V::Array(items) = read_bplist(&uid, 0).unwrap().unwrap() else { panic!() };
        assert_eq!(items.items[0].object_for_key("CF$UID").map(|v| v.long_long_value()), Some(-1));
    }

    #[test]
    fn reader_object_kinds() {
        assert!(matches!(read1(&[0x08]), Some(V::Bool(false))));
        assert!(read1(&[0x00]).is_none());
        assert!(read1(&[0x14, 0, 0]).is_none());
        assert!(read1(&[0x20]).is_none());
        assert!(read1(&[0x32, 0]).is_none());
        assert!(matches!(read1(&[0x33, 0, 0, 0, 0, 0, 0, 0, 0]), Some(V::Date(_))));
        assert!(read1(&[0x70]).is_none());
        assert!(read1(&[0xaf, 0x20]).is_none());
        assert!(read1(&[0xaf, 0x14]).is_none());
        assert!(read1(&[0xaf, 0x10]).is_none());
    }

    #[test]
    fn reader_depth_limit() {
        // object i is an array holding object i + 1: 300 levels deep trips the guard
        let n = 300usize;
        let mut d = b"bplist00".to_vec();
        let mut offs = vec![];
        for i in 0..n {
            offs.push(d.len() as u16);
            d.extend_from_slice(&[0xa1]);
            d.extend_from_slice(&((i + 1) as u16).to_be_bytes());
        }
        offs.push(d.len() as u16);
        d.push(0x09);
        let tab = d.len() as u64;
        for o in &offs {
            d.extend_from_slice(&o.to_be_bytes());
        }
        trailer(&mut d, 2, 2, (n + 1) as u64, 0, tab);
        assert!(read_bplist(&d, 0).unwrap().is_none());
    }

    #[test]
    fn reader_trailer_checks() {
        let base = || b"bplist00".to_vec();
        let tail = |osz: u8, rsz, nobj, top, tab| {
            let mut d = base();
            d.extend_from_slice(&[0x09, 8, 0, 0]);
            trailer(&mut d, osz, rsz, nobj, top, tab);
            d
        };
        assert!(read_bplist(&tail(1, 1, 1, 0, 9), 0).unwrap().is_some());
        assert!(read_bplist(&tail(1, 0, 1, 0, 9), 0).unwrap().is_none());
        assert!(read_bplist(&tail(2, 1, u64::MAX, 0, 9), 0).unwrap().is_none());
        assert!(read_bplist(&tail(1, 1, 1, 0, 1 << 40), 0).unwrap().is_none());
        assert!(read_bplist(&tail(1, 1, 0, 0, 9), 0).unwrap().is_none());
        assert!(read_bplist(&tail(1, 1, 1, 1, 9), 0).unwrap().is_none());
        assert_eq!(read_bplist(&tail(0, 1, 1, 0, 9), 0).unwrap_err().name, "NSGenericException");
    }
}

#[cfg(test)]
#[path = "binary_golden.rs"]
mod golden;
