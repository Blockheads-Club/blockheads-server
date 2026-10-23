// SPDX-License-Identifier: GPL-3.0-or-later AND LGPL-2.1-or-later
// Contains derived work; see THIRD_PARTY_NOTICES.md.
//! GNUstep-base 1.24 `GSIMapTable` emulation for dictionaries and sets whose enumeration order is game-visible.
//!

use crate::foundation::nsstring;

/// Key hashing/equality like the ObjC key class.
pub trait GsKey: Clone {
    /// `-hash` (unsigned 64-bit; bucket = hash % bucket_count).
    fn gs_hash(&self) -> u64;
    /// `-isEqual:`.
    fn gs_eq(&self, other: &Self) -> bool;
}

impl GsKey for String {
    fn gs_hash(&self) -> u64 {
        nsstring::gs_hash(self) as u64
    }
    fn gs_eq(&self, other: &Self) -> bool {
        self == other
    }
}

/// `fib_odd(target)`: next bucket count.
pub fn fib_odd(target: usize) -> usize {
    let (mut size, mut old) = (1usize, 1usize);
    while size < target {
        let n = size + old;
        old = size;
        size = n;
    }
    if size != 1 && size % 2 == 0 {
        size += 1;
    }
    size
}

#[derive(Clone, Debug)]
struct Node<K, V> {
    key: K,
    val: V,
}

/// Dictionary with GNUstep enumeration order.
#[derive(Clone, Debug)]
pub struct GsDict<K: GsKey, V> {
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    /// Each chain stores node indices with the HEAD of the chain as the LAST element.
    buckets: Vec<Vec<usize>>,
    count: usize,
}

impl<K: GsKey, V> Default for GsDict<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: GsKey, V> GsDict<K, V> {
    /// `[NSMutableDictionary dictionary]` (capacity 0 -> 1 bucket).
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    /// `initWithCapacity:n`: `resize(3*n/4 + 1)`.
    pub fn with_capacity(n: usize) -> Self {
        let mut d = GsDict { nodes: Vec::new(), free: Vec::new(), buckets: Vec::new(), count: 0 };
        d.resize(3 * n / 4 + 1);
        d
    }

    /// `dictionaryWithObjects:forKeys:` (capacity = count, forward order).
    pub fn from_pairs<I: IntoIterator<Item = (K, V)>>(it: I) -> Self {
        let v: Vec<(K, V)> = it.into_iter().collect();
        let mut d = Self::with_capacity(v.len());
        for (k, val) in v {
            d.insert(k, val);
        }
        d
    }

    /// Dictionary created by the XML plist / old-style parser (capacity 8 -> 9 buckets).
    pub fn plist_parsed() -> Self {
        Self::with_capacity(8)
    }

    /// Dictionary created by the JSON parser (1 bucket).
    pub fn json_parsed() -> Self {
        Self::new()
    }

    /// `mutableCopy` / `dictionaryWithDictionary:`: capacity = source count, enumerate source, insert.
    pub fn copy_from(src: &Self) -> Self
    where
        V: Clone,
    {
        let mut d = Self::with_capacity(src.len());
        for (k, v) in src.iter() {
            d.insert(k.clone(), v.clone());
        }
        d
    }

    fn resize(&mut self, target: usize) {
        let new = fib_odd(target);
        if new == self.buckets.len() {
            return;
        }
        let mut nb: Vec<Vec<usize>> = vec![Vec::new(); new];
        for chain in &self.buckets {
            // walk head -> tail, push_front into the new chain
            for &ni in chain.iter().rev() {
                let h = self.nodes[ni].as_ref().expect("live node").key.gs_hash();
                nb[(h % new as u64) as usize].push(ni);
            }
        }
        self.buckets = nb;
    }

    fn find(&self, k: &K) -> Option<usize> {
        let b = (k.gs_hash() % self.buckets.len() as u64) as usize;
        self.buckets[b].iter().rev().copied().find(|&ni| self.nodes[ni].as_ref().map_or(false, |n| n.key.gs_eq(k)))
    }

    fn add_new_node(&mut self, k: K, v: V) {
        if 3 * self.count >= 4 * self.buckets.len() {
            self.resize(3 * self.count / 4 + 1);
        }
        let b = (k.gs_hash() % self.buckets.len() as u64) as usize;
        let node = Node { key: k, val: v };
        let ni = if let Some(i) = self.free.pop() {
            self.nodes[i] = Some(node);
            i
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        };
        self.buckets[b].push(ni);
        self.count += 1;
    }

    /// `setObject:forKey:`; an existing key keeps its position and gets the new value.
    pub fn insert(&mut self, k: K, v: V) -> Option<V> {
        if let Some(ni) = self.find(&k) {
            let n = self.nodes[ni].as_mut().unwrap();
            return Some(std::mem::replace(&mut n.val, v));
        }
        self.add_new_node(k, v);
        None
    }

    /// `removeObjectForKey:`; the table never shrinks.
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let b = (k.gs_hash() % self.buckets.len() as u64) as usize;
        let pos = self.buckets[b].iter().position(|&ni| self.nodes[ni].as_ref().map_or(false, |n| n.key.gs_eq(k)))?;
        let ni = self.buckets[b].remove(pos);
        let n = self.nodes[ni].take().unwrap();
        self.free.push(ni);
        self.count -= 1;
        Some(n.val)
    }

    /// `removeAllObjects`; keeps the bucket array.
    pub fn remove_all(&mut self) {
        self.nodes.clear();
        self.free.clear();
        for c in &mut self.buckets {
            c.clear();
        }
        self.count = 0;
    }

    /// `objectForKey:`.
    pub fn get(&self, k: &K) -> Option<&V> {
        self.find(k).map(|ni| &self.nodes[ni].as_ref().unwrap().val)
    }

    /// Mutable `objectForKey:`.
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self.find(k) {
            Some(ni) => Some(&mut self.nodes[ni].as_mut().unwrap().val),
            None => None,
        }
    }

    /// Key membership.
    pub fn contains_key(&self, k: &K) -> bool {
        self.find(k).is_some()
    }

    /// `count`.
    pub fn len(&self) -> usize {
        self.count
    }

    /// `count == 0`.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Node indices in enumeration order: buckets ascending, each chain head to tail.
    fn order(&self) -> Vec<usize> {
        let mut v = Vec::with_capacity(self.count);
        for c in &self.buckets {
            v.extend(c.iter().rev().copied());
        }
        v
    }

    /// `allKeys` == `keyEnumerator` == fast enumeration.
    pub fn keys_in_order(&self) -> Vec<K> {
        self.order().into_iter().map(|ni| self.nodes[ni].as_ref().unwrap().key.clone()).collect()
    }

    /// `allValues` in enumeration order.
    pub fn values_in_order(&self) -> Vec<&V> {
        self.order().into_iter().map(|ni| &self.nodes[ni].as_ref().unwrap().val).collect()
    }

    /// `(key, value)` pairs in enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.order().into_iter().map(move |ni| {
            let n = self.nodes[ni].as_ref().unwrap();
            (&n.key, &n.val)
        })
    }

    /// Number of buckets (tests).
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }
}

/// `NSSet` / `NSMutableSet` with GNUstep enumeration order.
#[derive(Clone, Debug)]
pub struct GsSet<K: GsKey> {
    inner: GsDict<K, ()>,
}

impl<K: GsKey> Default for GsSet<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: GsKey> GsSet<K> {
    /// `[NSMutableSet set]`.
    pub fn new() -> Self {
        GsSet { inner: GsDict::new() }
    }
    /// `setWithCapacity:` / `initWithCapacity:`.
    pub fn with_capacity(n: usize) -> Self {
        GsSet { inner: GsDict::with_capacity(n) }
    }
    /// `addObject:` (an existing member is kept). Returns true when newly added.
    pub fn insert(&mut self, k: K) -> bool {
        if self.inner.contains_key(&k) {
            return false;
        }
        self.inner.insert(k, ());
        true
    }
    /// `removeObject:`.
    pub fn remove(&mut self, k: &K) -> bool {
        self.inner.remove(k).is_some()
    }
    /// `containsObject:`.
    pub fn contains(&self, k: &K) -> bool {
        self.inner.contains_key(k)
    }
    /// `count`.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    /// `count == 0`.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    /// Members in enumeration order.
    pub fn items_in_order(&self) -> Vec<K> {
        self.inner.keys_in_order()
    }
    /// Iterator over members in enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = K> {
        self.items_in_order().into_iter()
    }
    /// `NSSet setWithArray:` / `copy`: forward insertion, capacity n.
    pub fn from_array(a: &[K]) -> Self {
        let mut s = Self::with_capacity(a.len());
        for k in a {
            s.insert(k.clone());
        }
        s
    }
    /// `NSMutableSet setWithArray:` / `initWithArray:` / `addObjectsFromArray:` / `mutableCopy`: REVERSE order,
    /// capacity n.
    pub fn from_array_mutable(a: &[K]) -> Self {
        let mut s = Self::with_capacity(a.len());
        for k in a.iter().rev() {
            s.insert(k.clone());
        }
        s
    }
    /// Copy of another set: enumerate source, insert forward, capacity = count.
    pub fn copy_from_set(src: &Self) -> Self {
        GsSet { inner: GsDict::copy_from(&src.inner) }
    }
    /// Bucket count (tests).
    pub fn bucket_count(&self) -> usize {
        self.inner.bucket_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(n: usize) -> Vec<String> {
        (1..=n).map(|i| format!("k{i}")).collect()
    }

    #[test]
    fn golden_k1_k39() {
        let mut d = GsDict::new();
        for k in keys(39) {
            d.insert(k, ());
        }
        assert_eq!(d.bucket_count(), 35);
        let want = "k29 k17 k18 k19 k1 k2 k3 k4 k5 k6 k7 k8 k9 k30 k31 k32 k20 k33 k21 k34 k22 k10 k35 k23 k11 k36 k24 k12 k37 k25 k13 k38 k26 k14 k39 k27 k15 k28 k16";
        assert_eq!(d.keys_in_order().join(" "), want);
    }

    #[test]
    fn growth_sequence() {
        let mut d: GsDict<String, ()> = GsDict::new();
        assert_eq!(d.bucket_count(), 1);
        let mut seen = vec![];
        let mut last = 1;
        for i in 1..=4000usize {
            d.insert(format!("key{i}"), ());
            if d.bucket_count() != last {
                // report the n at which the new size was first seen
                seen.push((i, d.bucket_count()));
                last = d.bucket_count();
            }
        }
        // golden "after n inserts": n=1:1, 3:3, 5:5, 8:9, 13:13, 19:21,... => the size becomes B when inserting the (n)th key.
        let want = [
            (3, 3),
            (5, 5),
            (8, 9),
            (13, 13),
            (19, 21),
            (29, 35),
            (48, 55),
            (75, 89),
            (120, 145),
            (195, 233),
            (312, 377),
            (504, 611),
            (816, 987),
            (1317, 1597),
            (2131, 2585),
            (3448, 4181),
        ];
        assert_eq!(seen, want);
    }

    #[test]
    fn init_capacity() {
        for (c, b) in [
            (0, 1),
            (1, 1),
            (2, 3),
            (3, 3),
            (4, 5),
            (5, 5),
            (6, 5),
            (7, 9),
            (8, 9),
            (9, 9),
            (12, 13),
            (13, 13),
            (14, 13),
            (20, 21),
            (21, 21),
            (22, 21),
            (34, 35),
            (35, 35),
            (100, 89),
        ] {
            assert_eq!(GsDict::<String, ()>::with_capacity(c).bucket_count(), b, "cap {c}");
        }
    }

    #[test]
    fn connections() {
        let mut d = GsDict::new();
        for p in [57431, 45576, 38896, 34666] {
            d.insert(format!("16777343:{p}"), p);
        }
        assert_eq!(d.bucket_count(), 3);
        let want: Vec<String> = [34666, 38896, 45576, 57431].iter().map(|p| format!("16777343:{p}")).collect();
        assert_eq!(d.keys_in_order(), want);
        assert_eq!(nsstring::gs_hash("16777343:34666"), 30906889);
    }

    #[test]
    fn plist_vector() {
        let names = "name type count pos_x pos_y uniqueID saveID version flag extra more last";
        let mut d = GsDict::plist_parsed();
        for k in names.split(' ') {
            d.insert(k.to_string(), 0);
        }
        assert_eq!(d.bucket_count(), 9);
        assert_eq!(
            d.keys_in_order().join(" "),
            "saveID flag uniqueID more count last version pos_x extra pos_y type name"
        );
        let mut f = GsDict::new();
        for k in names.split(' ') {
            f.insert(k.to_string(), 0);
        }
        assert_eq!(f.keys_in_order(), d.keys_in_order());
    }

    #[test]
    fn replace_remove() {
        let mut d = GsDict::new();
        for k in keys(39) {
            d.insert(k, 1);
        }
        let before = d.keys_in_order();
        assert_eq!(d.insert("k5".to_string(), 2), Some(1));
        assert_eq!(d.keys_in_order(), before);
        assert_eq!(d.get(&"k5".to_string()), Some(&2));
        for k in keys(30) {
            d.remove(&k);
        }
        assert_eq!(d.bucket_count(), 35);
        assert_eq!(d.len(), 9);
        d.remove_all();
        assert_eq!((d.bucket_count(), d.len()), (35, 0));
        d.insert("a".into(), 1);
        assert_eq!(d.keys_in_order(), vec!["a".to_string()]);
    }

    #[test]
    fn copy_from_order() {
        let mut d = GsDict::new();
        for k in keys(39) {
            d.insert(k, 0);
        }
        let c = GsDict::copy_from(&d);
        assert_eq!(c.len(), 39);
        assert_eq!(c.bucket_count(), 35);
        // chains inside a bucket reverse relative to source
        let src = d.keys_in_order();
        let dst = c.keys_in_order();
        let mut a = src.clone();
        let mut b = dst.clone();
        a.sort();
        b.sort();
        assert_eq!(a, b);
    }

    #[test]
    fn set_paths() {
        let a: Vec<String> = keys(12);
        let f = GsSet::from_array(&a);
        let r = GsSet::from_array_mutable(&a);
        assert_eq!(f.len(), 12);
        assert_eq!(f.bucket_count(), r.bucket_count());
        let mut fi = f.items_in_order();
        let mut ri = r.items_in_order();
        fi.sort();
        ri.sort();
        assert_eq!(fi, ri);
        let mut s = GsSet::new();
        assert!(s.insert("x".to_string()));
        assert!(!s.insert("x".to_string()));
        assert!(s.contains(&"x".to_string()));
        assert!(s.remove(&"x".to_string()));
        assert!(s.is_empty());
    }

    #[test]
    fn model_check() {
        struct Model {
            buckets: usize,
            chains: Vec<Vec<String>>, // head first
            count: usize,
        }
        impl Model {
            fn resize(&mut self, t: usize) {
                let n = fib_odd(t);
                if n == self.buckets {
                    return;
                }
                let mut nc: Vec<Vec<String>> = vec![vec![]; n];
                for c in &self.chains {
                    for k in c {
                        nc[(nsstring::gs_hash(k) as u64 % n as u64) as usize].insert(0, k.clone());
                    }
                }
                self.chains = nc;
                self.buckets = n;
            }
            fn add(&mut self, k: &str) {
                if self.chains.iter().any(|c| c.iter().any(|x| x == k)) {
                    return;
                }
                if 3 * self.count >= 4 * self.buckets {
                    self.resize(3 * self.count / 4 + 1);
                }
                let b = (nsstring::gs_hash(k) as u64 % self.buckets as u64) as usize;
                self.chains[b].insert(0, k.to_string());
                self.count += 1;
            }
            fn del(&mut self, k: &str) {
                for c in &mut self.chains {
                    if let Some(p) = c.iter().position(|x| x == k) {
                        c.remove(p);
                        self.count -= 1;
                    }
                }
            }
        }
        let mut x = 987654321u64;
        let mut rnd = |m: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % m
        };
        for _ in 0..40 {
            let n = 1 + rnd(300) as usize;
            let cap = rnd(3) as usize * rnd(50) as usize;
            let mut d = GsDict::with_capacity(cap);
            let mut m = Model { buckets: 0, chains: vec![], count: 0 };
            m.resize(3 * cap / 4 + 1);
            for _ in 0..n * 2 {
                let k = format!("key{}", rnd(n as u64 + 5));
                if rnd(4) == 0 {
                    d.remove(&k);
                    m.del(&k);
                } else {
                    d.insert(k.clone(), 0);
                    m.add(&k);
                }
            }
            let want: Vec<String> = m.chains.iter().flatten().cloned().collect();
            assert_eq!(d.keys_in_order(), want);
            assert_eq!(d.bucket_count(), m.buckets);
        }
    }

    #[test]
    fn defaults_and_small_paths() {
        let mut d: GsDict<String, i32> = GsDict::default();
        assert!(d.get_mut(&"x".to_string()).is_none());
        d.insert("x".to_string(), 1);
        *d.get_mut(&"x".to_string()).unwrap() += 1;
        assert_eq!(d.get(&"x".to_string()), Some(&2));
        let n = d.bucket_count();
        d.resize(n);
        assert_eq!(d.bucket_count(), n);
        let mut s: GsSet<String> = GsSet::default();
        s.insert("a".to_string());
        s.insert("b".to_string());
        assert_eq!(s.iter().count(), 2);
        let c = GsSet::copy_from_set(&s);
        assert_eq!(c.len(), s.len());
    }
}
