//! Shared newtype macros for string-backed identifier wrappers.

/// Generates a `String`-backed newtype with standard conversions.
///
/// Generates: `Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq,
/// PartialOrd, Serialize`, `#[serde(transparent)]`, `as_str()`, `Display`,
/// `From<String>`, `From<&str>`, `AsRef<str>`, `PartialEq<str>`,
/// `PartialEq<&str>`, `PartialEq<$name> for str`.
///
/// Pass `json` to also derive `schemars::JsonSchema`:
/// ```ignore
/// string_newtype!(MyType, "doc string", json);
/// ```
macro_rules! string_newtype {
    ($name:ident, $doc:expr) => {
        string_newtype!($name, $doc,);
    };
    ($name:ident, $doc:expr, json) => {
        #[doc = $doc]
        #[derive(
            Clone,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            JsonSchema,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        #[serde(transparent)]
        pub(crate) struct $name(pub(crate) String);

        impl $name {
            /// Returns the inner string slice.
            #[inline]
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<$name> for str {
            #[inline]
            fn eq(&self, other: &$name) -> bool {
                self == other.0.as_str()
            }
        }
    };
    ($name:ident, $doc:expr, $($extra:ident),* $(,)?) => {
        #[doc = $doc]
        #[derive(
            Clone,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
            $($extra,)*
        )]
        #[serde(transparent)]
        pub(crate) struct $name(pub(crate) String);

        impl $name {
            /// Returns the inner string slice.
            #[inline]
            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            #[inline]
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            #[inline]
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            #[inline]
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            #[inline]
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl PartialEq<str> for $name {
            #[inline]
            fn eq(&self, other: &str) -> bool {
                self.0 == other
            }
        }

        impl PartialEq<&str> for $name {
            #[inline]
            fn eq(&self, other: &&str) -> bool {
                self.0 == *other
            }
        }

        impl PartialEq<$name> for str {
            #[inline]
            fn eq(&self, other: &$name) -> bool {
                self == other.0.as_str()
            }
        }
    };
}
