//! Shared imports. owned by skeleton.
//! Deliberately tiny: layer-0 cannot name types that later modules define. Modules import
//! what they need by full path (`crate::foundation::...`). Add re-exports here only via the module table.

pub use std::cell::{Cell, RefCell};
pub use std::collections::HashMap;
pub use std::rc::Rc;
