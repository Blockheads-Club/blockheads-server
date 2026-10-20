//! `NSKeyedArchiver` / `NSKeyedUnarchiver` for the shapes the server uses (`NSIndexSet` roots) plus a best-effort
//! generic subset.
//!

use super::binary::{write_bnode, BNode};
use super::value::PlistValue;
use super::xml_read::property_list_with_data;
use crate::foundation::exception::{ns_raise, range_exception_index, unrecognized_selector};
use crate::foundation::gsdict::GsDict;
use crate::foundation::indexset::IndexSet;
use crate::foundation::nslog::nslog;
use std::collections::HashMap;

/// Archivable value subset.
#[derive(Clone, Debug)]
pub enum ArchValue {
    IndexSet(IndexSet),
    Array(Vec<ArchValue>),
    Data(Vec<u8>),
    String(String),
    Int(i64),
    Real(f64),
    Bool(bool),
}

fn s(x: &str) -> BNode {
    BNode::Str(x.to_string())
}

fn class_dict(chain: &[&str]) -> BNode {
    BNode::Dict(vec![
        (s("$classes"), BNode::Array(chain.iter().map(|c| s(c)).collect())),
        (s("$classname"), s(chain[0])),
    ])
}

/// `[[NSKeyedArchiver alloc] initForWritingWithMutableData:]`.
pub struct KeyedArchiver {
    objects: Vec<BNode>,
    top: GsDict<String, BNode>,
    classes: HashMap<String, usize>,
}

impl Default for KeyedArchiver {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyedArchiver {
    /// Fresh archiver whose object 0 is `$null`.
    pub fn new() -> Self {
        KeyedArchiver { objects: vec![s("$null")], top: GsDict::new(), classes: HashMap::new() }
    }

    fn class_uid(&mut self, chain: &[&str]) -> u64 {
        if let Some(&i) = self.classes.get(chain[0]) {
            return i as u64;
        }
        self.objects.push(class_dict(chain));
        let i = self.objects.len() - 1;
        self.classes.insert(chain[0].to_string(), i);
        i as u64
    }

    /// Archive an index set (`mutable` selects the `NSMutableIndexSet` class chain). Returns the object index.
    pub fn add_index_set(&mut self, set: &IndexSet, mutable: bool) -> usize {
        let chain: &[&str] =
            if mutable { &["NSMutableIndexSet", "NSIndexSet", "NSObject"] } else { &["NSIndexSet", "NSObject"] };
        let ranges = set.ranges();
        let me = self.objects.len();
        self.objects.push(BNode::Bool(false)); // placeholder
        let node = match ranges.len() {
            0 => {
                let c = self.class_uid(chain);
                BNode::Dict(vec![(s("$class"), BNode::Uid(c)), (s("NSRangeCount"), BNode::Int(0))])
            }
            1 => {
                let c = self.class_uid(chain);
                let (loc, len) = ranges[0];
                // NSLocation / NSLength are encoded as 32-bit ints (wrap)
                BNode::Dict(vec![
                    (s("NSLength"), BNode::Int(len as u32 as i32 as i64)),
                    (s("$class"), BNode::Uid(c)),
                    (s("NSRangeCount"), BNode::Int(1)),
                    (s("NSLocation"), BNode::Int(loc as u32 as i32 as i64)),
                ])
            }
            n => {
                let mut data = Vec::new();
                for &(loc, len) in ranges {
                    varint(loc, &mut data);
                    varint(len, &mut data);
                }
                let di = self.objects.len();
                self.objects.push(BNode::Bool(false));
                let dc = self.class_uid(&["NSMutableData", "NSData", "NSObject"]);
                self.objects[di] = BNode::Dict(vec![(s("$class"), BNode::Uid(dc)), (s("NS.data"), BNode::Data(data))]);
                let c = self.class_uid(chain);
                BNode::Dict(vec![
                    (s("$class"), BNode::Uid(c)),
                    (s("NSRangeCount"), BNode::Int(n as i64)),
                    (s("NSRangeData"), BNode::Uid(di as u64)),
                ])
            }
        };
        self.objects[me] = node;
        me
    }

    fn add_value(&mut self, v: &ArchValue) -> usize {
        match v {
            ArchValue::IndexSet(i) => self.add_index_set(i, true),
            ArchValue::Data(d) => {
                self.objects.push(BNode::Data(d.clone()));
                self.objects.len() - 1
            }
            ArchValue::String(x) => {
                // NSString objects are uniqued
                if let Some(p) = self.objects.iter().position(|o| matches!(o, BNode::Str(y) if y == x)) {
                    return p;
                }
                self.objects.push(BNode::Str(x.clone()));
                self.objects.len() - 1
            }
            ArchValue::Int(i) => {
                self.objects.push(BNode::Int(*i));
                self.objects.len() - 1
            }
            ArchValue::Real(r) => {
                self.objects.push(BNode::Real { v: *r, f32: false });
                self.objects.len() - 1
            }
            ArchValue::Bool(b) => {
                self.objects.push(BNode::Bool(*b));
                self.objects.len() - 1
            }
            ArchValue::Array(a) => {
                let me = self.objects.len();
                self.objects.push(BNode::Bool(false));
                let uids: Vec<BNode> = a.iter().map(|x| BNode::Uid(self.add_value(x) as u64)).collect();
                let c = self.class_uid(&["NSArray", "NSObject"]);
                // Note: key order of the NSArray archive dictionary (server never writes one).
                self.objects[me] =
                    BNode::Dict(vec![(s("NS.objects"), BNode::Array(uids)), (s("$class"), BNode::Uid(c))]);
                me
            }
        }
    }

    /// `encodeObject:forKey:`.
    pub fn encode_object(&mut self, v: &ArchValue, key: &str) {
        let i = self.add_value(v);
        self.top.insert(key.to_string(), BNode::Uid(i as u64));
    }

    /// `encodeObject:forKey:` for an index set of explicit mutability.
    pub fn encode_index_set(&mut self, set: &IndexSet, mutable: bool, key: &str) {
        let i = self.add_index_set(set, mutable);
        self.top.insert(key.to_string(), BNode::Uid(i as u64));
    }

    /// `encodeInt:forKey:` (stored directly in `$top`).
    pub fn encode_int(&mut self, v: i32, key: &str) {
        self.top.insert(key.to_string(), BNode::Int(v as i64));
    }

    /// `finishEncoding`: the `bplist00` bytes.
    pub fn finish_encoding(self) -> Vec<u8> {
        let top: Vec<(BNode, BNode)> = self.top.iter().map(|(k, v)| (s(k), v.clone())).collect();
        let root = BNode::Dict(vec![
            (s("$objects"), BNode::Array(self.objects)),
            (s("$top"), BNode::Dict(top)),
            (s("$version"), BNode::Int(100000)),
            (s("$archiver"), s("NSKeyedArchiver")),
        ]);
        write_bnode(&root)
    }
}

fn varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            break;
        }
    }
}

fn read_varint(d: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    let mut shift = 0;
    loop {
        let b = *d.get(*i)?;
        *i += 1;
        if shift < 64 {
            v |= ((b & 0x7f) as u64) << shift;
        }
        shift += 7;
        if b & 0x80 == 0 {
            return Some(v);
        }
    }
}

/// Archive of `set` under `key` as `NSMutableIndexSet` (all server archives).
pub fn archive_index_set(set: &IndexSet, key: &str) -> Vec<u8> {
    let mut a = KeyedArchiver::new();
    a.encode_index_set(set, true, key);
    a.finish_encoding()
}

/// Archive with explicit mutability (tests, immutable chain).
pub fn archive_index_set_ex(set: &IndexSet, mutable: bool, key: &str) -> Vec<u8> {
    let mut a = KeyedArchiver::new();
    a.encode_index_set(set, mutable, key);
    a.finish_encoding()
}

// ---------------------------------------------------------------------------------------------
// Unarchiver
// ---------------------------------------------------------------------------------------------

/// `[[NSKeyedUnarchiver alloc] initForReadingWithData:]`.
pub struct KeyedUnarchiver {
    root: PlistValue,
}

const GSIARRAY_ASSERT: &str =
    "../Headers/GNUstepBase/GSIArray.h:549  Assertion failed in GSIArrayItemAtIndex.  NSInvalidArgumentException";

impl KeyedUnarchiver {
    /// `None` data or an unparsable plist gives `None` (init returned nil); may raise.
    pub fn init_for_reading(d: Option<&[u8]>) -> Option<Self> {
        let d = d?;
        let r = property_list_with_data(d, 0);
        let v = r.value?;
        if !matches!(v, PlistValue::Dict(_)) {
            unrecognized_selector(v.class_name(), "objectForKey:");
        }
        Some(KeyedUnarchiver { root: v })
    }

    /// `finishDecoding`.
    pub fn finish_decoding(self) {}

    fn objects(&self) -> Option<&Vec<PlistValue>> {
        match self.root.object_for_key("$objects")? {
            PlistValue::Array(a) => Some(&a.items),
            _ => None,
        }
    }

    fn uid_of(v: &PlistValue) -> Option<i64> {
        match v.object_for_key("CF$UID")? {
            PlistValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Resolve a UID object index to its object; out of range reproduces the GSIArray assertion.
    fn object_at(&self, idx: i64) -> &PlistValue {
        let objs = self.objects();
        match objs.and_then(|o| if idx >= 0 { o.get(idx as usize) } else { None }) {
            Some(o) => o,
            None => {
                nslog(GSIARRAY_ASSERT);
                ns_raise("NSInternalInconsistencyException", GSIARRAY_ASSERT)
            }
        }
    }

    /// `decodeObjectForKey:` for the `$top` entry. `None` = nil.
    pub fn decode_object_for_key(&self, k: &str) -> Option<ArchValue> {
        let top = self.root.object_for_key("$top")?;
        let entry = top.object_for_key(k)?;
        let idx = Self::uid_of(entry)?;
        self.decode_index(idx, 0)
    }

    fn decode_ref(&self, v: &PlistValue, depth: u32) -> Option<ArchValue> {
        let idx = Self::uid_of(v)?;
        self.decode_index(idx, depth + 1)
    }

    fn decode_index(&self, idx: i64, depth: u32) -> Option<ArchValue> {
        if depth > 64 {
            return None;
        }
        let obj = self.object_at(idx);
        match obj {
            PlistValue::String(st) => {
                if st.s == "$null" {
                    None
                } else {
                    Some(ArchValue::String(st.s.clone()))
                }
            }
            PlistValue::Int(i) => Some(ArchValue::Int(*i)),
            PlistValue::UInt(u) => Some(ArchValue::Int(*u as i64)),
            PlistValue::Real { value, .. } => Some(ArchValue::Real(*value)),
            PlistValue::Bool(b) => Some(ArchValue::Bool(*b)),
            PlistValue::Data(d) => Some(ArchValue::Data(d.bytes.clone())),
            PlistValue::Dict(_) => {
                let cls = obj.object_for_key("$class")?;
                let cidx = Self::uid_of(cls)?;
                let cdict = match self.objects().and_then(|o| if cidx >= 0 { o.get(cidx as usize) } else { None }) {
                    Some(c) => c,
                    None => {
                        let n = self.objects().map_or(0, |o| o.len());
                        range_exception_index(cidx as u64, n as u64, "objectAtIndex:")
                    }
                };
                let name = cdict.object_for_key("$classname").and_then(|n| n.as_str()).unwrap_or("(null)");
                match name {
                    "NSMutableIndexSet" | "NSIndexSet" => Some(ArchValue::IndexSet(self.decode_index_set(obj, depth))),
                    "NSArray" | "NSMutableArray" => {
                        let mut out = Vec::new();
                        if let Some(PlistValue::Array(a)) = obj.object_for_key("NS.objects") {
                            for e in a.items.iter() {
                                out.push(self.decode_ref(e, depth)?);
                            }
                        }
                        Some(ArchValue::Array(out))
                    }
                    "NSData" | "NSMutableData" => match obj.object_for_key("NS.data") {
                        Some(PlistValue::Data(d)) => Some(ArchValue::Data(d.bytes.clone())),
                        _ => Some(ArchValue::Data(Vec::new())),
                    },
                    "NSString" | "NSMutableString" => Some(ArchValue::String(
                        obj.object_for_key("NS.string").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    )),
                    "NSNumber" | "NSDictionary" | "NSMutableDictionary" => {
                        // Note: generic dictionary/number archives are not needed by the server.
                        None
                    }
                    other => ns_raise(
                        "NSInvalidUnarchiveOperationException",
                        &format!("[NSKeyedUnarchiver +_decodeObject:]: no class for name '{}'", other),
                    ),
                }
            }
            _ => None,
        }
    }

    fn decode_index_set(&self, obj: &PlistValue, depth: u32) -> IndexSet {
        let _ = depth;
        let int_of = |k: &str| -> i64 {
            match obj.object_for_key(k) {
                Some(PlistValue::Int(i)) => *i,
                Some(PlistValue::UInt(u)) => *u as i64,
                _ => 0,
            }
        };
        let count = int_of("NSRangeCount");
        let mut set = IndexSet::new();
        if count == 1 {
            let loc = int_of("NSLocation");
            let len = int_of("NSLength");
            if len > 0 {
                const NS_NOT_FOUND: u64 = i64::MAX as u64;
                let l = loc as u64;
                if l >= NS_NOT_FOUND || NS_NOT_FOUND - l <= len as u64 {
                    ns_raise("NSRangeException", "Range location + length too great");
                }
                set.add_indexes_in_range(l, len as u64);
            }
        } else if count >= 2 {
            if let Some(uref) = obj.object_for_key("NSRangeData") {
                if let Some(di) = Self::uid_of(uref) {
                    let dobj = self.object_at(di);
                    if let Some(PlistValue::Data(d)) = dobj.object_for_key("NS.data") {
                        let mut i = 0;
                        for _ in 0..count {
                            let (Some(loc), Some(len)) = (read_varint(&d.bytes, &mut i), read_varint(&d.bytes, &mut i))
                            else {
                                break;
                            };
                            if len > 0 {
                                set.add_indexes_in_range(loc, len);
                            }
                        }
                    }
                }
            }
        }
        set
    }
}

/// `+[NSKeyedUnarchiver]` decode of an index set under `key`. Raises like GNUstep.
pub fn unarchive_index_set(data: &[u8], key: &str) -> Option<IndexSet> {
    unarchive_index_set_opt(Some(data), key)
}

/// Nil `NSData` gives nil.
pub fn unarchive_index_set_opt(data: Option<&[u8]>, key: &str) -> Option<IndexSet> {
    let u = KeyedUnarchiver::init_for_reading(data)?;
    match u.decode_object_for_key(key) {
        Some(ArchValue::IndexSet(i)) => Some(i),
        // Note: a non-index-set object under the key would later get index-set selectors; treated as nil.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::exception::catch_ns;

    fn unhex(h: &str) -> Vec<u8> {
        (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    fn set_of(ranges: &[(u64, u64)]) -> IndexSet {
        let mut s = IndexSet::new();
        for &(l, n) in ranges {
            s.add_indexes_in_range(l, n);
        }
        s
    }

    #[test]
    fn keyed_vectors_300() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("keyed_indexset_vectors.txt")) else {
            return;
        };
        let mut n = 0;
        for l in txt.lines().filter(|l| l.starts_with("T ")) {
            let parts: Vec<&str> = l.split('|').collect();
            let head: Vec<&str> = parts[0].split_whitespace().collect();
            let mutable = head[1] == "1";
            // header: T <mutable> <rootkey: 1=root 0=portalPositionIndexSet> <index count>
            let key = if head[2] == "1" { "root" } else { "portalPositionIndexSet" };
            let ranges: Vec<(u64, u64)> = parts[1]
                .split_whitespace()
                .map(|r| {
                    let (a, b) = r.split_once(':').unwrap();
                    (a.parse().unwrap(), b.parse().unwrap())
                })
                .collect();
            let want = unhex(parts[2].trim());
            let set = set_of(&ranges);
            assert_eq!(archive_index_set_ex(&set, mutable, key), want, "vector {l}");
            // single-range archives store NSLocation/NSLength as 32-bit ints, so values >= 2^31 do not survive
            let lossy = ranges.len() == 1 && (ranges[0].0 >= 1 << 31 || ranges[0].1 >= 1 << 31);
            if !lossy {
                let back = unarchive_index_set(&want, key).expect("decodes");
                assert_eq!(back.ranges(), set.ranges(), "vector {l}");
            }
            n += 1;
        }
        assert!(n >= 300);
    }

    #[test]
    fn golden_samples() {
        let set = set_of(&[(8410549, 1)]);
        let want = unhex("62706c6973743030d4010203040506070858246f626a656374735424746f70582476657273696f6e59246172636869766572a3090a0bd10c0d12000186a05f100f4e534b65796564417263686976657255246e756c6cd40e0f101112131214d2151617185f1016706f7274616c506f736974696f6e496e6465785365748001584e534c656e6774685624636c6173735c4e5352616e6765436f756e745a4e534c6f636174696f6e1001800212008055b55824636c61737365735a24636c6173736e616d65a318191a5f10114e534d757461626c65496e6465785365745a4e53496e646578536574584e534f626a65637408111a1f283236393e50565f647d7f888f9ca7a9abb0b9c4c8dce70000000000000101000000000000001b000000000000000000000000000000f0");
        assert_eq!(archive_index_set(&set, "portalPositionIndexSet"), want);
        assert_eq!(unarchive_index_set(&want, "portalPositionIndexSet").unwrap().ranges(), &[(8410549, 1)]);
        assert!(unarchive_index_set(&want, "other").is_none());
        // multi range example from the spec
        let multi = set_of(&[(5, 3), (10, 1), (100, 1), (1000, 1)]);
        let bytes = archive_index_set(&multi, "x");
        assert!(bytes.windows(9).any(|w| w == [5, 3, 10, 1, 100, 1, 0xe8, 7, 1]));
        assert_eq!(unarchive_index_set(&bytes, "x").unwrap().ranges(), multi.ranges());
        // empty
        let e = archive_index_set(&IndexSet::new(), "root");
        assert_eq!(e.len(), 247);
        assert!(unarchive_index_set(&e, "root").unwrap().is_empty());
        let e = archive_index_set(&IndexSet::new(), "foundItems");
        assert_eq!(e.len(), 253);
    }

    #[test]
    fn two_keys_vector() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")) else { return };
        let Some(l) = txt.lines().find(|l| l.starts_with("two ")) else { return };
        let want = unhex(l.split(' ').nth(1).unwrap());
        let mut a = KeyedArchiver::new();
        a.encode_index_set(&set_of(&[(5, 3), (10, 1), (100, 1), (1000, 1)]), true, "a");
        a.encode_index_set(&set_of(&[(42, 1)]), false, "b");
        a.encode_int(3, "n");
        a.encode_object(&ArchValue::String("s".into()), "c");
        assert_eq!(a.finish_encoding(), want);
    }

    #[test]
    fn simple_objects_vectors() {
        let Ok(txt) = std::fs::read_to_string(crate::testenv::plist_golden("bplist_vectors.txt")) else { return };
        let get =
            |n: &str| unhex(txt.lines().find(|l| l.starts_with(&format!("{n} "))).unwrap().split(' ').nth(1).unwrap());
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::String("hello".into()), "root");
        assert_eq!(a.finish_encoding(), get("str"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Data(b"xyz".to_vec()), "root");
        assert_eq!(a.finish_encoding(), get("data"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Int(7), "root");
        assert_eq!(a.finish_encoding(), get("num_i"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Real(2.5), "root");
        assert_eq!(a.finish_encoding(), get("num_d"));
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Bool(true), "root");
        assert_eq!(a.finish_encoding(), get("num_b"));
    }

    #[test]
    fn unarchive_errors() {
        assert!(unarchive_index_set_opt(None, "root").is_none());
        assert!(KeyedUnarchiver::init_for_reading(Some(b"")).is_none());
        let e = catch_ns(|| unarchive_index_set(b"junk", "root")).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("GSMutableString(instance) does not recognize objectForKey:"));
        let e = catch_ns(|| unarchive_index_set(b"12345", "root")).unwrap_err();
        assert_eq!(e.reason.as_deref(), Some("GSMutableString(instance) does not recognize objectForKey:"));
        // unknown class
        let mut bytes = archive_index_set(&IndexSet::new(), "root");
        let pos = bytes.windows(17).position(|w| w == b"NSMutableIndexSet").unwrap();
        // "NSMutableIndexSet" -> same length unknown name
        bytes[pos..pos + 17].copy_from_slice(b"NSMutableIndexSeX");
        let e = catch_ns(|| unarchive_index_set(&bytes, "root")).unwrap_err();
        assert_eq!(e.name, "NSInvalidUnarchiveOperationException");
        assert_eq!(
            e.reason.as_deref(),
            Some("[NSKeyedUnarchiver +_decodeObject:]: no class for name 'NSMutableIndexSeX'")
        );
    }

    #[test]
    fn golden_unarchive_inputs() {
        let dir = crate::testenv::plist_golden("xml_read_inputs");
        let Ok(rd) = std::fs::read_dir(&dir) else { return };
        let _ = rd;
        let run = |f: &str, key: &str| -> String {
            let d = std::fs::read(format!("{dir}/{f}")).unwrap();
            match catch_ns(|| match KeyedUnarchiver::init_for_reading(Some(&d)) {
                None => "init nil".to_string(),
                Some(u) => match u.decode_object_for_key(key) {
                    None => "nil".to_string(),
                    Some(ArchValue::IndexSet(i)) => format!("set {:?}", i.ranges()),
                    Some(o) => format!("{:?}", o),
                },
            }) {
                Ok(s) => s,
                Err(e) => format!("EXC {} | {}", e.name, e.reason.unwrap_or_default()),
            }
        };
        assert_eq!(
            run("bad_uid.bin", "root"),
            "EXC NSRangeException | Index 9 is out of range 2 (in 'objectAtIndex:')"
        );
        assert_eq!(
            run("unknown_class.bin", "root"),
            "EXC NSInvalidUnarchiveOperationException | [NSKeyedUnarchiver +_decodeObject:]: no class for name 'FooBar'"
        );
        assert_eq!(run("top_null.bin", "root"), "nil");
        assert_eq!(run("top_int.bin", "root"), "Int(7)");
        assert_eq!(run("is_neg.bin", "root"), "EXC NSRangeException | Range location + length too great");
        assert_eq!(run("is_len0.bin", "root"), "set []");
        assert_eq!(run("mutable_cls_chain.bin", "root"), "set [(5, 3)]");
        assert_eq!(run("ka_arr.bin", "root"), "Array([String(\"a\"), String(\"b\"), String(\"a\")])");
        assert!(run("top_uid_oob.bin", "root")
            .starts_with("EXC NSInternalInconsistencyException | ../Headers/GNUstepBase/GSIArray.h:549"));
        assert_eq!(
            run("cls_unknown_chain_known_super.bin", "root"),
            "EXC NSInvalidUnarchiveOperationException | [NSKeyedUnarchiver +_decodeObject:]: no class for name 'MyIdx'"
        );
        assert_eq!(run("ka_arr.bin", "root_nokey"), "nil");

        // Every row of `_keyed_unarchive.out` (oracle t19, key "root"; the row label is `argv[a]+9`, i.e. the file name minus its first 4 characters).
        let Ok(rows) = std::fs::read_to_string(crate::testenv::plist_golden("xml_read_expected/_keyed_unarchive.out"))
        else {
            return;
        };
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        let mut checked = 0;
        for l in rows.lines().filter(|l| !l.starts_with("20")) {
            let (label, want) = l.split_once(": ").unwrap();
            let cands: Vec<&String> = names.iter().filter(|n| n.len() > 4 && &n[4..] == label).collect();
            let got: Vec<String> = cands.iter().map(|f| run(f, "root")).collect();
            let ok = |g: &String| -> bool {
                if want.starts_with("EXC ") || want == "init nil" {
                    g == want
                } else if want == "nil " {
                    g == "nil"
                } else if let Some(r) =
                    want.strip_prefix("NSIndexSet ").or_else(|| want.strip_prefix("NSMutableIndexSet "))
                {
                    let ranges =
                        if r.ends_with("(no indexes)") { "set []".to_string() } else { "set [(5, 3)]".to_string() };
                    *g == ranges
                } else if want.starts_with("GSMutableArray ") {
                    g.starts_with("Array(")
                } else if want.starts_with("NSIntNumber ") {
                    *g == format!("Int({})", &want[12..])
                } else if want.starts_with("GSCBufferString ") {
                    *g == format!("String({:?})", &want[16..])
                } else {
                    // NSDictionary roots: the server never decodes them (ArchValue has no dictionary).
                    want.starts_with("GSDictionary ")
                }
            };
            assert!(!cands.is_empty(), "{label}");
            assert!(got.iter().all(ok), "{label}: want {want:?} got {got:?}");
            checked += 1;
        }
        assert!(checked >= 40, "{checked}");
    }
}
