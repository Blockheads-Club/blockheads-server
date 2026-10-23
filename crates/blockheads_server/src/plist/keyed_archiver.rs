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

    fn set_of(ranges: &[(u64, u64)]) -> IndexSet {
        let mut s = IndexSet::new();
        for &(l, n) in ranges {
            s.add_indexes_in_range(l, n);
        }
        s
    }

    /// An XML keyed archive: `objects` are XML fragments, `$top["root"]` points at object `root`.
    fn xml_archive(objects: &str, root: i64) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\"?><plist version=\"0.9\"><dict><key>$objects</key><array>{objects}</array><key>$top</key><dict><key>root</key><dict><key>CF$UID</key><integer>{root}</integer></dict></dict></dict></plist>"
        )
        .into_bytes()
    }
    fn uid(i: i64) -> String {
        format!("<dict><key>CF$UID</key><integer>{i}</integer></dict>")
    }
    fn class(name: &str) -> String {
        format!("<dict><key>$classname</key><string>{name}</string></dict>")
    }
    fn obj(class_idx: i64, extra: &str) -> String {
        format!("<dict><key>$class</key>{}{extra}</dict>", uid(class_idx))
    }
    fn decode(d: &[u8]) -> Option<ArchValue> {
        KeyedUnarchiver::init_for_reading(Some(d)).unwrap().decode_object_for_key("root")
    }
    fn exc(d: &[u8]) -> (String, String) {
        let e = catch_ns(|| decode(d)).unwrap_err();
        (e.name, e.reason.unwrap_or_default())
    }

    #[test]
    fn archiver_round_trips_every_value() {
        let mut a = KeyedArchiver::default();
        let items = ArchValue::Array(vec![
            ArchValue::String("a".into()),
            ArchValue::String("a".into()),
            ArchValue::Int(-4),
            ArchValue::Real(2.5),
            ArchValue::Bool(true),
            ArchValue::Data(vec![9, 8]),
            ArchValue::IndexSet(set_of(&[(1, 2), (10, 1)])),
            ArchValue::Array(vec![]),
        ]);
        a.encode_object(&items, "root");
        a.encode_int(5, "n");
        let bytes = a.finish_encoding();
        let u = KeyedUnarchiver::init_for_reading(Some(&bytes)).unwrap();
        let Some(ArchValue::Array(v)) = u.decode_object_for_key("root") else { panic!() };
        assert_eq!(v.len(), 8);
        assert!(matches!(&v[0], ArchValue::String(s) if s == "a"));
        assert!(matches!(&v[1], ArchValue::String(s) if s == "a"));
        assert!(matches!(v[2], ArchValue::Int(-4)));
        assert!(matches!(v[3], ArchValue::Real(r) if r == 2.5));
        assert!(matches!(v[4], ArchValue::Bool(true)));
        assert!(matches!(&v[5], ArchValue::Data(d) if d == &[9, 8]));
        assert!(matches!(&v[6], ArchValue::IndexSet(i) if i.ranges() == set_of(&[(1, 2), (10, 1)]).ranges()));
        assert!(matches!(&v[7], ArchValue::Array(x) if x.is_empty()));
        // `encodeInt:` stores the number directly, which is not an object reference
        assert!(u.decode_object_for_key("n").is_none());
        assert!(u.decode_object_for_key("missing").is_none());
        u.finish_decoding();
    }

    #[test]
    fn index_set_archives_round_trip() {
        for ranges in [vec![], vec![(5, 3)], vec![(1, 2), (10, 1), (100, 5)]] {
            let set = set_of(&ranges);
            for mutable in [true, false] {
                let bytes = archive_index_set_ex(&set, mutable, "root");
                assert_eq!(unarchive_index_set(&bytes, "root").unwrap().ranges(), set.ranges());
            }
            assert_eq!(archive_index_set(&set, "k"), archive_index_set_ex(&set, true, "k"));
        }
        assert!(unarchive_index_set_opt(None, "root").is_none());
        let not_a_set = archive_index_set(&IndexSet::new(), "root");
        assert!(unarchive_index_set(&not_a_set, "other").is_none());
        let mut a = KeyedArchiver::new();
        a.encode_object(&ArchValue::Int(1), "root");
        assert!(unarchive_index_set(&a.finish_encoding(), "root").is_none());
    }

    #[test]
    fn reader_entry_points() {
        assert!(KeyedUnarchiver::init_for_reading(None).is_none());
        assert!(KeyedUnarchiver::init_for_reading(Some(b"<?xml junk")).is_none());
        let raise = || KeyedUnarchiver::init_for_reading(Some(b"<?xml version=\"1.0\"?><plist><array/></plist>"));
        let e = catch_ns(raise).err().unwrap();
        assert_eq!(e.name, "NSInvalidArgumentException");
    }

    #[test]
    fn plain_object_kinds() {
        let a = |o: &str| decode(&xml_archive(&format!("<string>$null</string>{o}"), 1));
        assert!(a("<string>$null</string>").is_none());
        assert!(matches!(a("<string>hi</string>"), Some(ArchValue::String(s)) if s == "hi"));
        assert!(matches!(a("<integer>7</integer>"), Some(ArchValue::Int(7))));
        assert!(matches!(a("<real>1.5</real>"), Some(ArchValue::Real(r)) if r == 1.5));
        assert!(matches!(a("<false/>"), Some(ArchValue::Bool(false))));
        assert!(matches!(a("<data>AQI=</data>"), Some(ArchValue::Data(d)) if d == [1, 2]));
        assert!(a("<array/>").is_none());
        assert!(a("<dict><key>x</key><string>y</string></dict>").is_none());
    }

    #[test]
    fn unsigned_numbers_wrap() {
        let mut m = GsDict::new();
        m.insert("$objects".to_string(), PlistValue::array(vec![PlistValue::UInt(u64::MAX)]));
        let mut uidd = GsDict::new();
        uidd.insert("CF$UID".to_string(), PlistValue::int(0));
        let mut top = GsDict::new();
        top.insert("root".to_string(), PlistValue::dict(uidd));
        m.insert("$top".to_string(), PlistValue::dict(top));
        let u = KeyedUnarchiver { root: PlistValue::dict(m) };
        assert!(matches!(u.decode_object_for_key("root"), Some(ArchValue::Int(-1))));
    }

    #[test]
    fn container_classes() {
        let with = |o: &str| xml_archive(&format!("<string>$null</string>{o}<string>elem</string>{}", class_set()), 1);
        fn class_set() -> String {
            ["NSArray", "NSMutableArray", "NSData", "NSString", "NSNumber", "NSDictionary", "Weird"]
                .iter()
                .map(|c| super::tests::class(c))
                .collect()
        }
        // object 2 is a string, 3.. are the classes: 3 NSArray, 4 NSMutableArray, 5 NSData, 6 NSString, 7 NSNumber, 8 NSDictionary, 9 Weird
        let list = format!("<key>NS.objects</key><array>{}</array>", uid(2));
        assert!(matches!(decode(&with(&obj(3, &list))), Some(ArchValue::Array(v)) if v.len() == 1));
        assert!(matches!(decode(&with(&obj(4, ""))), Some(ArchValue::Array(v)) if v.is_empty()));
        assert!(decode(&with(&obj(3, "<key>NS.objects</key><array><string>x</string></array>"))).is_none());
        assert!(
            matches!(decode(&with(&obj(5, "<key>NS.data</key><data>AQ==</data>"))), Some(ArchValue::Data(d)) if d == [1])
        );
        assert!(matches!(decode(&with(&obj(5, ""))), Some(ArchValue::Data(d)) if d.is_empty()));
        assert!(
            matches!(decode(&with(&obj(6, "<key>NS.string</key><string>s</string>"))), Some(ArchValue::String(s)) if s == "s")
        );
        assert!(matches!(decode(&with(&obj(6, ""))), Some(ArchValue::String(s)) if s.is_empty()));
        assert!(decode(&with(&obj(7, ""))).is_none());
        assert!(decode(&with(&obj(8, ""))).is_none());
        assert!(decode(&with("<dict><key>x</key><string>y</string></dict>")).is_none());
        let (n, r) = exc(&with(&obj(9, "")));
        assert_eq!(n, "NSInvalidUnarchiveOperationException");
        assert!(r.contains("no class for name 'Weird'"), "{r}");
        // a class entry without a name is reported as "(null)"
        let nameless = xml_archive(&format!("<string>$null</string>{}<dict/>", obj(2, "")), 1);
        assert!(exc(&nameless).1.contains("'(null)'"));
        // class index outside $objects
        let (n, _) = exc(&with(&obj(99, "")));
        assert_eq!(n, "NSRangeException");
        let (n, _) = exc(&with(&obj(-1, "")));
        assert_eq!(n, "NSRangeException");
        // a class UID missing is nil
        assert!(decode(&with("<dict><key>$class</key><string>x</string></dict>")).is_none());
    }

    #[test]
    fn deep_and_dangling_references() {
        // an array holding itself stops at the depth limit
        let looped = xml_archive(
            &format!(
                "<string>$null</string>{}{}",
                obj(2, &format!("<key>NS.objects</key><array>{}</array>", uid(1))),
                class("NSArray")
            ),
            1,
        );
        assert!(decode(&looped).is_none());
        for root in [5, -1] {
            let (n, r) = exc(&xml_archive("<string>$null</string>", root));
            assert_eq!(n, "NSInternalInconsistencyException");
            assert!(r.contains("GSIArray.h"));
        }
        let no_objects = b"<?xml version=\"1.0\"?><plist><dict><key>$top</key><dict><key>root</key><dict><key>CF$UID</key><integer>0</integer></dict></dict></dict></plist>";
        assert_eq!(exc(no_objects).0, "NSInternalInconsistencyException");
    }

    #[test]
    fn index_set_range_forms() {
        let set_cls = |extra: &str| {
            xml_archive(
                &format!("<string>$null</string>{}{}{}", obj(2, extra), class("NSIndexSet"), class("NSData")),
                1,
            )
        };
        let ranges = |d: &[u8]| {
            let Some(ArchValue::IndexSet(i)) = decode(d) else { panic!() };
            i.ranges().to_vec()
        };
        let one = |loc: &str, len: &str| {
            format!("<key>NSRangeCount</key><integer>1</integer><key>NSLocation</key><integer>{loc}</integer><key>NSLength</key><integer>{len}</integer>")
        };
        assert!(ranges(&set_cls("")).is_empty());
        assert_eq!(ranges(&set_cls(&one("4", "2"))), vec![(4, 2)]);
        assert!(ranges(&set_cls(&one("4", "0"))).is_empty());
        assert_eq!(exc(&set_cls(&one("3", "18446744073709551615"))).0, "NSRangeException");
        assert_eq!(exc(&set_cls(&one("9223372036854775807", "1"))).0, "NSRangeException");
        // count >= 2: ranges come from varint pairs in NSRangeData (object 4): (5,0) is skipped, (7,2) kept, then a
        // location without a length ends the data
        let data_obj = |b64: &str| {
            let range_data = format!("<key>NSRangeCount</key><integer>3</integer><key>NSRangeData</key>{}", uid(4));
            let cls = |n: &str| class(n);
            let nsdata = format!("<dict><key>$class</key>{}<key>NS.data</key><data>{b64}</data></dict>", uid(3));
            xml_archive(
                &format!(
                    "<string>$null</string>{}{}{}{}",
                    obj(2, &range_data),
                    cls("NSIndexSet"),
                    cls("NSData"),
                    nsdata
                ),
                1,
            )
        };
        assert_eq!(ranges(&data_obj("BQAHAgE=")), vec![(7, 2)]);
        // NSRangeData that is not an object reference or not data leaves the set empty
        assert!(ranges(&set_cls(
            "<key>NSRangeCount</key><integer>2</integer><key>NSRangeData</key><string>x</string>"
        ))
        .is_empty());
        assert!(ranges(&set_cls("<key>NSRangeCount</key><integer>2</integer>")).is_empty());
        assert!(ranges(&set_cls(&format!(
            "<key>NSRangeCount</key><integer>2</integer><key>NSRangeData</key>{}",
            uid(2)
        )))
        .is_empty());
    }

    #[test]
    fn malformed_archive_shapes() {
        let top = |uid_body: &str| {
            format!("<?xml version=\"1.0\"?><plist><dict><key>$objects</key><string>x</string><key>$top</key><dict><key>root</key><dict><key>CF$UID</key>{uid_body}</dict></dict></dict></plist>")
                .into_bytes()
        };
        // $objects that is not an array behaves like a missing table
        assert_eq!(exc(&top("<integer>0</integer>")).0, "NSInternalInconsistencyException");
        // a UID that is not an integer is no reference
        assert!(decode(&top("<string>0</string>")).is_none());
        // unsigned numbers in an index set archive are read like signed ones
        let mut set = GsDict::new();
        set.insert("NSRangeCount".to_string(), PlistValue::UInt(1));
        set.insert("NSLocation".to_string(), PlistValue::UInt(2));
        set.insert("NSLength".to_string(), PlistValue::UInt(3));
        let mut cls = GsDict::new();
        cls.insert("$classname".to_string(), PlistValue::string("NSIndexSet"));
        let mut cref = GsDict::new();
        cref.insert("CF$UID".to_string(), PlistValue::int(1));
        set.insert("$class".to_string(), PlistValue::dict(cref));
        let mut uid0 = GsDict::new();
        uid0.insert("CF$UID".to_string(), PlistValue::int(0));
        let mut tops = GsDict::new();
        tops.insert("root".to_string(), PlistValue::dict(uid0));
        let mut root = GsDict::new();
        root.insert("$objects".to_string(), PlistValue::array(vec![PlistValue::dict(set), PlistValue::dict(cls)]));
        root.insert("$top".to_string(), PlistValue::dict(tops));
        let u = KeyedUnarchiver { root: PlistValue::dict(root) };
        let Some(ArchValue::IndexSet(i)) = u.decode_object_for_key("root") else { panic!() };
        assert_eq!(i.ranges().to_vec(), vec![(2, 3)]);
    }
}

#[cfg(test)]
#[path = "keyed_archiver_golden.rs"]
mod golden;
