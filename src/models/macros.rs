//! Internal helper macros for model definitions.

/// Declares a string enum deserialized from API responses, with
/// `#[non_exhaustive]` and an `Unknown` fallback (`#[serde(other)]`) that
/// cannot be serialized.
macro_rules! wire_enum {
    ($(#[$meta:meta])* $name:ident, $case:literal, { $($(#[$vmeta:meta])* $variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = $case)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$vmeta])* $variant,)+
            /// A value not known to this SDK version.
            #[serde(other, skip_serializing)]
            Unknown,
        }
    };
}
