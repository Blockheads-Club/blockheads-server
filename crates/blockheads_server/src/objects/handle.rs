//! Shared object handles (`ObjRef`/`ObjWeak`) and identity helpers.
//!
//! (retain semantics; RESOLVED refcount note in `updatePosition:`).

use std::any::Any;
use std::rc::{Rc, Weak};

use super::base::DynamicObject;
use super::class_info::ClassInfo;

/// Strong reference to a dynamic object (ObjC "retain").
pub type ObjRef = Rc<dyn DynamicObject>;
/// Non-retained (ObjC "assign") back pointer.
pub type ObjWeak = Weak<dyn DynamicObject>;
/// `CPCache*` ivar/parameter. Client-only: always `None` on the server.
pub type CacheRef = Option<Rc<dyn Any>>;

/// `[[X alloc] init...]` replacement: allocates through `Rc::new_cyclic` so the object knows its own weak handle.
/// `init` receives the self weak and must store it via `DynamicObjectBase::alloc_zeroed_with`.
pub fn alloc_obj<T: DynamicObject + 'static>(init: impl FnOnce(ObjWeak) -> T) -> Rc<T> {
    Rc::new_cyclic(|w: &Weak<T>| {
        let weak: ObjWeak = w.clone();
        init(weak)
    })
}

/// Pointer identity as an integer (ObjC `==`, `removeObject:` with no `isEqual:` override).
pub fn obj_id(o: &ObjRef) -> usize {
    Rc::as_ptr(o) as *const () as usize
}

/// Pointer identity of two handles.
pub fn ptr_eq(a: &ObjRef, b: &ObjRef) -> bool {
    obj_id(a) == obj_id(b)
}

/// Identity compare of a possibly-nil handle with a handle (`nil == x` is false).
pub fn opt_ptr_eq(a: &Option<ObjRef>, b: &ObjRef) -> bool {
    match a {
        Some(a) => ptr_eq(a, b),
        None => false,
    }
}

/// Identity of `&self` (as seen from inside a trait method) against `me`. Used in `debug_assert!`s.
pub fn ptr_eq_self(this: &dyn DynamicObject, me: &ObjRef) -> bool {
    (this as *const dyn DynamicObject as *const ()) as usize == obj_id(me)
}

/// Concrete-type access: `o.as_any().downcast_ref::<T>()`.
pub fn downcast<T: 'static>(o: &ObjRef) -> Option<&T> {
    o.as_any().downcast_ref::<T>()
}

/// `[o isKindOfClass:c]`.
pub fn is_kind_of_class(o: &ObjRef, c: &'static ClassInfo) -> bool {
    o.class_info().is_kind_of(c)
}

/// `[o isMemberOfClass:c]`.
pub fn is_member_of_class(o: &ObjRef, c: &'static ClassInfo) -> bool {
    o.class_info().is_member_of(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::base::testutil::TestObj;

    /// `ptr_eq_self` compares the receiver of a trait method with the handle it came from.
    #[test]
    fn identity_of_the_receiver() {
        let a = TestObj::new();
        let b = TestObj::new();
        let (ma, mb): (ObjRef, ObjRef) = (a.clone(), b.clone());
        assert!(ptr_eq_self(&*a, &ma));
        assert!(!ptr_eq_self(&*a, &mb));
        assert!(ptr_eq(&ma, &ma) && !ptr_eq(&ma, &mb));
    }
}
