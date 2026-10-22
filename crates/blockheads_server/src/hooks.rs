//! Seam traits without silent defaults.
//!
//! Gameplay modules reach other modules through hook traits (`DwOpsHooks`, `BlockheadHost`, ...). Those traits used
//! to give every method a default body that did nothing. The production glue (`app::integration`) implemented only
//! part of each trait, so the real server silently skipped behaviour while unit tests (with their own stand-ins)
//! passed.
//!
//! [`hook_trait!`] declares such a seam twice:
//! - `pub trait X`: the production trait. Every method whose body is only a placeholder is *required*, so an
//!   implementor that forgets one does not compile. Methods marked `#[provided]` keep their body (it is real,
//!   spec-derived behaviour, not a stand-in).
//! - `pub trait XStub`: the same methods, all with their bodies. Test doubles and the "nothing installed" null
//!   objects implement this one and override only what they need. A blanket impl makes every `XStub` an `X`.
//!
//! Production code must implement `X` directly, never `XStub`.

/// See the module docs. Grammar:
///
/// ```text
/// hook_trait! {
///     /// docs
///     pub trait Name, stub NameStub {
///         /// docs
///         fn inert(&self, a: A) -> R { placeholder }          // required in `Name`
///         #[provided]
///         fn real(&self, a: A) -> R { real body }             // default in `Name` too
///     }
/// }
/// ```
macro_rules! hook_trait {
    (
        $(#[$($tattr:tt)*])*
        $vis:vis trait $name:ident, stub $stub:ident {
            $(
                $(#[$($mattr:tt)*])*
                fn $m:ident(&$slf:tt $(, $arg:ident : $aty:ty)* $(,)?) $(-> $ret:ty)? $body:block
            )*
        }
    ) => {
        $(#[$($tattr)*])*
        $vis trait $name {
            $(
                $crate::hooks::hook_trait! {
                    @req [] $(#[$($mattr)*])* fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)? $body
                }
            )*
        }

        $(#[$($tattr)*])*
        ///
        /// Stand-in flavour of the production trait (see [`crate::hooks`]): every method has its placeholder
        /// body. Only for test doubles and the "nothing installed" null objects.
        $vis trait $stub {
            $(
                $crate::hooks::hook_trait! {
                    @stub [] $(#[$($mattr)*])* fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)? $body
                }
            )*
        }

        #[allow(clippy::too_many_arguments)]
        impl<T: $stub + ?Sized> $name for T {
            $(
                #[inline]
                fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)? {
                    <T as $stub>::$m($slf $(, $arg)*)
                }
            )*
        }
    };

    // ---- one method of the production trait: strip `#[provided]` (keeps the body), else required
    (@req [$($acc:tt)*] #[provided] $($rest:tt)*) => {
        $crate::hooks::hook_trait! { @req_provided [$($acc)*] $($rest)* }
    };
    (@req [$($acc:tt)*] #[$($a:tt)*] $($rest:tt)*) => {
        $crate::hooks::hook_trait! { @req [$($acc)* #[$($a)*]] $($rest)* }
    };
    (@req [$($acc:tt)*] fn $m:ident(&$slf:tt $(, $arg:ident : $aty:ty)*) $(-> $ret:ty)? $body:block) => {
        $($acc)*
        fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)?;
    };
    (@req_provided [$($acc:tt)*] #[$($a:tt)*] $($rest:tt)*) => {
        $crate::hooks::hook_trait! { @req_provided [$($acc)* #[$($a)*]] $($rest)* }
    };
    (@req_provided [$($acc:tt)*] fn $m:ident(&$slf:tt $(, $arg:ident : $aty:ty)*) $(-> $ret:ty)? $body:block) => {
        $($acc)*
        fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)? $body
    };

    // ---- one method of the stub trait: always the body
    (@stub [$($acc:tt)*] #[provided] $($rest:tt)*) => {
        $crate::hooks::hook_trait! { @stub [$($acc)*] $($rest)* }
    };
    (@stub [$($acc:tt)*] #[$($a:tt)*] $($rest:tt)*) => {
        $crate::hooks::hook_trait! { @stub [$($acc)* #[$($a)*]] $($rest)* }
    };
    (@stub [$($acc:tt)*] fn $m:ident(&$slf:tt $(, $arg:ident : $aty:ty)*) $(-> $ret:ty)? $body:block) => {
        $($acc)*
        fn $m(&$slf $(, $arg: $aty)*) $(-> $ret)? $body
    };
}
pub(crate) use hook_trait;

#[cfg(test)]
mod tests {
    hook_trait! {
        /// A demo seam.
        #[allow(unused_variables)]
        pub trait Demo, stub DemoStub {
            /// Placeholder.
            fn inert(&self, a: i32) -> i32 { 0 }
            #[provided]
            /// Real.
            fn real(&self, a: i32, b: i32,) -> i32 { a + b + self.inert(a) }
            fn unit(&self) {}
        }
    }

    struct Prod;
    impl Demo for Prod {
        fn inert(&self, a: i32) -> i32 {
            a * 10
        }
        fn unit(&self) {}
    }

    struct Fake;
    impl DemoStub for Fake {}

    struct FakeOverride;
    impl DemoStub for FakeOverride {
        fn real(&self, _a: i32, _b: i32) -> i32 {
            7
        }
    }

    fn call(d: &dyn Demo) -> (i32, i32) {
        d.unit();
        (d.inert(2), d.real(2, 3))
    }

    #[test]
    fn production_required_and_provided() {
        assert_eq!(call(&Prod), (20, 25));
        Prod.unit();
    }

    #[test]
    fn stub_keeps_placeholders_and_overrides() {
        assert_eq!(call(&Fake), (0, 5));
        assert_eq!(call(&FakeOverride), (0, 7));
    }
}
