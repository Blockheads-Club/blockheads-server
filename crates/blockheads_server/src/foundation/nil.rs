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

    #[test]
    fn defaults() {
        assert_eq!(nil_or_default(None::<i32>, |x| x + 1), 0);
        assert_eq!(nil_or_default(Some(1), |x| x + 1), 2);
        assert!(!nil_or_default(None::<&str>, |s| s.is_empty()));
        assert_eq!(nil_or(None::<i32>, 7, |x| x), 7);
        assert_eq!(nil_count::<u8>(None), 0);
        assert_eq!(nil_count(Some(&vec![1, 2])), 2);
        assert_eq!(NIL_FLOAT, 0.0);
    }

    #[test]
    fn nil_helpers() {
        assert_eq!(nil_or(Some(2), 9, |v| v * 2), 4);
        assert_eq!(nil_or(None::<i32>, 9, |v| v * 2), 9);
        assert_eq!(nil_or_default(Some(2), |v| v + 1), 3);
        assert_eq!(nil_or_default(None::<i32>, |v| v + 1), 0);
        assert_eq!(nil_count(Some(&vec![1, 2])), 2);
        assert_eq!(nil_count::<i32>(None), 0);
        assert_eq!(NIL_FLOAT, 0.0);
    }
}
