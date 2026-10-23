//! Nil-messaging helpers: a message sent to `nil` returns 0 / NO / nil.
//!

/// Message to nil returning a default (`0`, `NO`, `nil`).
pub fn nil_or_default<T, R: Default>(o: Option<T>, f: impl FnOnce(T) -> R) -> R {
    match o {
        Some(v) => f(v),
        None => R::default(),
    }
}

/// Message to nil returning an explicit default.
pub fn nil_or<T, R>(o: Option<T>, default: R, f: impl FnOnce(T) -> R) -> R {
    match o {
        Some(v) => f(v),
        None => default,
    }
}

/// `[nil count]` is 0.
pub fn nil_count<T>(o: Option<&Vec<T>>) -> usize {
    o.map_or(0, |v| v.len())
}

/// A nil message returning float yields 0.0; mark call sites with `// NIL-FLOAT`.
pub const NIL_FLOAT: f32 = 0.0;

#[cfg(test)]
mod tests {
    use super::*;

    // Each generic helper is called once from a single site with both a present and a missing value, so one
    // instantiation covers both arms.
    #[test]
    fn defaults() {
        for (o, want) in [(None, 0), (Some(1), 2)] {
            assert_eq!(nil_or_default(o, |x: i32| x + 1), want);
        }
        for (o, want) in [(None, 7), (Some(1), 1)] {
            assert_eq!(nil_or(o, 7, |x: i32| x), want);
        }
        let v = vec![1, 2];
        for (o, want) in [(None, 0), (Some(&v), 2)] {
            assert_eq!(nil_count(o), want);
        }
        assert_eq!(NIL_FLOAT, 0.0);
    }
}
