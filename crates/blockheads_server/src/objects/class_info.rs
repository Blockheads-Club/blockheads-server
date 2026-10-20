//! Runtime class descriptors for `isKindOfClass:` / `isMemberOfClass:` / `conformsToProtocol:`.
//!
//!  (`classForDynamicObjectType` names).

use std::cell::RefCell;
use std::collections::HashMap;

/// One Objective-C class: name, superclass link and adopted protocol names.
pub struct ClassInfo {
    pub name: &'static str,
    pub superclass: Option<&'static ClassInfo>,
    /// Names of adopted protocols (own only; superclasses are walked by `conforms_to`).
    pub protocols: &'static [&'static str],
}

impl ClassInfo {
    /// `[self isSubclassOfClass:other]` (walk the chain by pointer identity).
    pub fn is_kind_of(&'static self, other: &'static ClassInfo) -> bool {
        let mut cur: Option<&'static ClassInfo> = Some(self);
        while let Some(c) = cur {
            if std::ptr::eq(c, other) {
                return true;
            }
            cur = c.superclass;
        }
        false
    }

    /// `[obj isMemberOfClass:other]` (exact class).
    pub fn is_member_of(&'static self, other: &'static ClassInfo) -> bool {
        std::ptr::eq(self, other)
    }

    /// `[cls conformsToProtocol:]`, walking superclasses.
    pub fn conforms_to(&self, protocol: &str) -> bool {
        if self.protocols.iter().any(|p| *p == protocol) {
            return true;
        }
        match self.superclass {
            Some(s) => s.conforms_to(protocol),
            None => false,
        }
    }
}

/// `DynamicObject: NSObject` (NSObject is not modelled).
pub static DYNAMIC_OBJECT_CLASS: ClassInfo = ClassInfo { name: "DynamicObject", superclass: None, protocols: &[] };

/// `InteractionObject: DynamicObject <TapChoiceDynamicObject>`.
pub static INTERACTION_OBJECT_CLASS: ClassInfo = ClassInfo {
    name: "InteractionObject",
    superclass: Some(&DYNAMIC_OBJECT_CLASS),
    protocols: &["TapChoiceDynamicObject"],
};

thread_local! {
    static REGISTRY: RefCell<HashMap<&'static str, &'static ClassInfo>> = RefCell::new(HashMap::new());
}

/// `objc_lookup_class(name)`: only classes registered through [`register_class_info`] are known (the two base
/// classes are always known).
pub fn class_info_by_name(name: &str) -> Option<&'static ClassInfo> {
    match name {
        "DynamicObject" => return Some(&DYNAMIC_OBJECT_CLASS),
        "InteractionObject" => return Some(&INTERACTION_OBJECT_CLASS),
        _ => {}
    }
    REGISTRY.with(|r| r.borrow().get(name).copied())
}

/// Called by `object_registry` at startup for every concrete class.
pub fn register_class_info(ci: &'static ClassInfo) {
    REGISTRY.with(|r| {
        r.borrow_mut().insert(ci.name, ci);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    static NPC: ClassInfo = ClassInfo {
        name: "NPC",
        superclass: Some(&DYNAMIC_OBJECT_CLASS),
        protocols: &["Harmable", "TapChoiceDynamicObject"],
    };
    static DODO: ClassInfo = ClassInfo { name: "Dodo", superclass: Some(&NPC), protocols: &[] };
    static CHEST: ClassInfo = ClassInfo { name: "Chest", superclass: Some(&INTERACTION_OBJECT_CLASS), protocols: &[] };

    #[test]
    fn chains() {
        assert!(DODO.is_kind_of(&NPC));
        assert!(DODO.is_kind_of(&DYNAMIC_OBJECT_CLASS));
        assert!(!DODO.is_kind_of(&INTERACTION_OBJECT_CLASS));
        assert!(!DODO.is_member_of(&NPC));
        assert!(DODO.is_member_of(&DODO));
        assert!(CHEST.is_kind_of(&INTERACTION_OBJECT_CLASS));
        assert!(CHEST.conforms_to("TapChoiceDynamicObject"));
        assert!(DODO.conforms_to("TapChoiceDynamicObject"));
        assert!(!DYNAMIC_OBJECT_CLASS.conforms_to("TapChoiceDynamicObject"));
    }

    #[test]
    fn lookup() {
        assert!(class_info_by_name("Chest").is_none());
        register_class_info(&CHEST);
        assert!(std::ptr::eq(class_info_by_name("Chest").unwrap(), &CHEST));
        assert!(class_info_by_name("InteractionObject").is_some());
    }
}
