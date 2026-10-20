//! `Flurry`: analytics stub class.
//!

use crate::plist::value::PlistDict;

/// `Flurry` (class methods only).
pub struct Flurry;

impl Flurry {
    /// `+[Flurry logEvent:withParameters:]`: empty body, no side effects.
    pub fn log_event_with_parameters(_event: &str, _params: Option<&PlistDict>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// : the stub exists and does nothing.
    #[test]
    fn stub_is_callable() {
        Flurry::log_event_with_parameters("evt", None);
    }
}
