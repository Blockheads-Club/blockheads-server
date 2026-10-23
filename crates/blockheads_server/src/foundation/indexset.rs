//! `NSIndexSet` / `NSMutableIndexSet` plus the `SerializationMethods` category.
//!

/// Magic first word of the serialised form.
pub const SERIALIZATION_MAGIC: u32 = 0x0017_1e8f;

/// Sorted, disjoint, non-adjacent `(start, len)` runs.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct IndexSet {
    runs: Vec<(u64, u64)>,
}

impl IndexSet {
    /// Empty set.
    pub fn new() -> Self {
        IndexSet { runs: Vec::new() }
    }

    /// `addIndex:`.
    pub fn add_index(&mut self, i: u64) {
        self.add_indexes_in_range(i, 1);
    }

    /// `addIndexesInRange:` (len 0 is a no-op; ranges merge).
    pub fn add_indexes_in_range(&mut self, loc: u64, len: u64) {
        if len == 0 {
            return;
        }
        let mut start = loc;
        let mut end = loc.saturating_add(len); // exclusive
        let mut out: Vec<(u64, u64)> = Vec::with_capacity(self.runs.len() + 1);
        let mut placed = false;
        for &(s, l) in &self.runs {
            let e = s + l;
            if e < start {
                out.push((s, l));
            } else if s > end {
                if !placed {
                    out.push((start, end - start));
                    placed = true;
                }
                out.push((s, l));
            } else {
                start = start.min(s);
                end = end.max(e);
            }
        }
        if !placed {
            out.push((start, end - start));
        }
        self.runs = out;
    }

    /// `addIndexes:`.
    pub fn add_indexes(&mut self, other: &IndexSet) {
        for &(s, l) in &other.runs {
            self.add_indexes_in_range(s, l);
        }
    }

    /// `removeIndex:`.
    pub fn remove_index(&mut self, i: u64) {
        self.remove_indexes_in_range(i, 1);
    }

    /// `removeIndexesInRange:`.
    pub fn remove_indexes_in_range(&mut self, loc: u64, len: u64) {
        if len == 0 {
            return;
        }
        let rs = loc;
        let re = loc.saturating_add(len);
        let mut out = Vec::with_capacity(self.runs.len() + 1);
        for &(s, l) in &self.runs {
            let e = s + l;
            if e <= rs || s >= re {
                out.push((s, l));
                continue;
            }
            if s < rs {
                out.push((s, rs - s));
            }
            if e > re {
                out.push((re, e - re));
            }
        }
        self.runs = out;
    }

    /// `removeAllIndexes`.
    pub fn remove_all_indexes(&mut self) {
        self.runs.clear();
    }

    /// `containsIndex:`.
    pub fn contains_index(&self, i: u64) -> bool {
        match self.runs.binary_search_by(|&(s, _)| s.cmp(&i)) {
            Ok(_) => true,
            Err(0) => false,
            Err(p) => {
                let (s, l) = self.runs[p - 1];
                i < s + l
            }
        }
    }

    /// `count`.
    pub fn count(&self) -> usize {
        self.runs.iter().map(|&(_, l)| l as usize).sum()
    }

    /// True when no index is present.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// `firstIndex` (NSNotFound = None).
    pub fn first_index(&self) -> Option<u64> {
        self.runs.first().map(|r| r.0)
    }

    /// `lastIndex` (NSNotFound = None).
    pub fn last_index(&self) -> Option<u64> {
        self.runs.last().map(|r| r.0 + r.1 - 1)
    }

    /// `indexGreaterThanIndex:`.
    pub fn index_greater_than_index(&self, i: u64) -> Option<u64> {
        let n = i.checked_add(1)?;
        for &(s, l) in &self.runs {
            if n < s {
                return Some(s);
            }
            if n < s + l {
                return Some(n);
            }
        }
        None
    }

    /// `indexLessThanIndex:`.
    pub fn index_less_than_index(&self, i: u64) -> Option<u64> {
        for &(s, l) in self.runs.iter().rev() {
            if s < i {
                return Some((s + l - 1).min(i - 1));
            }
        }
        None
    }

    /// Ascending iteration (`enumerateIndexesUsingBlock:`).
    pub fn iter(&self) -> impl Iterator<Item = u64> + '_ {
        self.runs.iter().flat_map(|&(s, l)| s..s + l)
    }

    /// The `(start, len)` runs.
    pub fn ranges(&self) -> &[(u64, u64)] {
        &self.runs
    }

    /// Serialised words without the faithful-crash behaviour (little-endian u32 bytes).
    pub fn serialized_data_raw(&self) -> Vec<u8> {
        let mut v: Vec<u32> = vec![SERIALIZATION_MAGIC];
        let mut found = false;
        let mut range_count: u32 = 0;
        let mut last: u32 = 0;
        for &(s, l) in &self.runs {
            let idx32 = s as u32;
            if !found {
                v.push(idx32);
                found = true;
                range_count = l as u32;
            } else if last.wrapping_add(1) == idx32 {
                range_count = range_count.wrapping_add(l as u32);
            } else {
                v.push(range_count);
                v.push(idx32);
                range_count = l as u32;
            }
            last = (s + l - 1) as u32;
        }
        if found {
            v.push(range_count);
        }
        v.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// `-serializedData`. A non-empty set aborts like the original ( GNUstep block type encoding
    /// crash) when the `faithful-crashes` feature is on.
    pub fn serialized_data(&self) -> Vec<u8> {
        #[cfg(feature = "faithful-crashes")]
        if !self.is_empty() {
            crate::foundation::exception::gnustep_block_abort();
        }
        self.serialized_data_raw()
    }

    /// `+indexSetWithData:`.
    pub fn from_serialized_data(data: Option<&[u8]>) -> IndexSet {
        let mut set = IndexSet::new();
        let d = match data {
            Some(d) => d,
            None => return set,
        };
        if d.len() > 4 && u32::from_le_bytes([d[0], d[1], d[2], d[3]]) == SERIALIZATION_MAGIC {
            let words = d.len() / 4;
            let n = (words - 1) / 2;
            let w = |k: usize| u32::from_le_bytes([d[4 * k], d[4 * k + 1], d[4 * k + 2], d[4 * k + 3]]) as u64;
            for i in 0..n {
                set.add_indexes_in_range(w(2 * i + 1), w(2 * i + 2));
            }
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(b: &[u8]) -> Vec<u32> {
        b.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }

    #[test]
    fn add_remove_merge() {
        let mut s = IndexSet::new();
        s.add_indexes_in_range(5, 3);
        s.add_index(10);
        s.add_index(8); // adjacent merge
        assert_eq!(s.ranges(), &[(5, 4), (10, 1)]);
        s.add_index(9);
        assert_eq!(s.ranges(), &[(5, 6)]);
        s.add_indexes_in_range(0, 0);
        s.remove_index(7);
        assert_eq!(s.ranges(), &[(5, 2), (8, 3)]);
        assert!(s.contains_index(5) && !s.contains_index(7) && s.contains_index(10) && !s.contains_index(11));
        assert_eq!(s.count(), 5);
        assert_eq!((s.first_index(), s.last_index()), (Some(5), Some(10)));
        assert_eq!(s.index_greater_than_index(6), Some(8));
        assert_eq!(s.index_greater_than_index(10), None);
        assert_eq!(s.index_less_than_index(8), Some(6));
        assert_eq!(s.index_less_than_index(5), None);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![5, 6, 8, 9, 10]);
        let mut t = IndexSet::new();
        t.add_indexes_in_range(0, 3);
        t.add_indexes(&s);
        assert_eq!(t.ranges(), &[(0, 3), (5, 2), (8, 3)]);
        t.remove_indexes_in_range(1, 8);
        assert_eq!(t.ranges(), &[(0, 1), (9, 2)]);
        t.remove_all_indexes();
        assert!(t.is_empty() && t.first_index().is_none());
    }

    #[test]
    fn serialisation() {
        assert_eq!(IndexSet::new().serialized_data_raw().len(), 4);
        let mut s = IndexSet::new();
        for i in [1, 2, 3, 7] {
            s.add_index(i);
        }
        assert_eq!(words(&s.serialized_data_raw()), vec![0x171e8f, 1, 3, 7, 1]);
        let back = IndexSet::from_serialized_data(Some(&s.serialized_data_raw()));
        assert_eq!(back, s);
    }

    #[test]
    fn decoding() {
        assert!(IndexSet::from_serialized_data(None).is_empty());
        assert!(IndexSet::from_serialized_data(Some(&[1, 2, 3])).is_empty());
        let bad: Vec<u8> = [0xdeadu32, 1, 2].iter().flat_map(|w| w.to_le_bytes()).collect();
        assert!(IndexSet::from_serialized_data(Some(&bad)).is_empty());
        let ov: Vec<u8> = [0x171e8fu32, 1, 5, 3, 5, 20, 0].iter().flat_map(|w| w.to_le_bytes()).collect();
        let s = IndexSet::from_serialized_data(Some(&ov));
        assert_eq!(s.ranges(), &[(1, 7)]); // zero-length pair adds nothing
        let mut big = IndexSet::new();
        big.add_index(0x1_0000_0005);
        assert_eq!(words(&big.serialized_data_raw()), vec![0x171e8f, 5, 1]);
    }

    #[test]
    fn range_edges_and_serialisation_merging() {
        let mut s = IndexSet::new();
        s.add_indexes_in_range(5, 3);
        s.remove_indexes_in_range(6, 0);
        s.add_indexes_in_range(20, 2);
        s.remove_indexes_in_range(10, 5);
        assert_eq!(s.ranges(), &[(5, 3), (20, 2)]);
        assert!(!s.contains_index(2) && s.contains_index(6));
        s.remove_indexes_in_range(20, 2);
        assert_eq!(s.ranges(), &[(5, 3)]);
        assert_eq!(s.index_greater_than_index(5), Some(6));
        assert_eq!(s.index_greater_than_index(7), None);
        assert_eq!(s.index_greater_than_index(0), Some(5));
        // adjacent runs that were stored separately merge in the serialised word list
        let mut t = IndexSet::new();
        t.add_index(1);
        t.add_index(3);
        t.add_index(2);
        assert_eq!(IndexSet::from_serialized_data(Some(&t.serialized_data_raw())).ranges(), t.ranges());
        // 32-bit wrap of the stored words: runs one apart modulo 2^32 are written as one run
        let mut w = IndexSet::new();
        w.add_index(1);
        w.add_index((1 << 32) + 2);
        assert_eq!(w.serialized_data_raw().len(), 3 * 4);
        assert_eq!(IndexSet::new().serialized_data(), IndexSet::new().serialized_data_raw());
    }
}
