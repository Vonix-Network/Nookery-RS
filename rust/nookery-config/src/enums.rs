//! Configuration enums mirroring `pc/include/acgc/settings.hpp`.
//!
//! Every enum has a canonical string form (the value stored in the settings
//! file) and a positional numeric form. The numeric form is *not* authoritative:
//! upstream decodes it through `borealis::config::Codec<T>`, whose mapping is
//! supplied by the runtime. [`from_code`](DiscVerificationState::from_code)
//! therefore exists only so an explicit adapter can be written and tested; see
//! [`crate::migration::EnumCodec`].

/// `backend.discVerification`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiscVerificationState {
    Unknown,
    Success,
    HashMismatch,
}

/// `video.theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterfaceTheme {
    Dynamic,
    Day,
    Night,
}

/// `gameplay.travelMoveOutChance`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TravelMoveOutChance {
    Ten,
    Fifty,
    Default,
}

macro_rules! config_enum {
    (
        $ty:ty,
        default = $default:ident,
        variants = [$(($variant:ident, $text:literal, $code:literal)),+ $(,)?]
    ) => {
        impl $ty {
            /// All variants in declaration (= numeric code) order.
            pub const ALL: [Self; config_enum!(@count $($variant),+)] = [$(Self::$variant),+];

            /// Canonical settings-file string.
            pub const fn as_config_str(&self) -> &'static str {
                match *self {
                    $(Self::$variant => $text),+
                }
            }

            /// Inverse of [`Self::as_config_str`]; unknown strings are rejected.
            pub fn from_config_str(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Positional numeric code used by the runtime enum codec.
            ///
            /// Adapter boundary: upstream's codec owns this mapping. Do not treat
            /// it as a stable on-disk format.
            pub const fn to_code(self) -> i64 {
                match self {
                    $(Self::$variant => $code),+
                }
            }

            /// Inverse of [`Self::to_code`].
            pub const fn from_code(value: i64) -> Option<Self> {
                match value {
                    $($code => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl Default for $ty {
            fn default() -> Self {
                Self::$default
            }
        }

        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.as_config_str())
            }
        }
    };
    (@count $($variant:ident),+) => {
        <[()]>::len(&[$(config_enum!(@unit $variant)),+])
    };
    (@unit $variant:ident) => { () };
}

config_enum!(
    DiscVerificationState,
    default = Unknown,
    variants = [
        (Unknown, "unknown", 0),
        (Success, "success", 1),
        (HashMismatch, "hash_mismatch", 2)
    ]
);

config_enum!(
    InterfaceTheme,
    default = Dynamic,
    variants = [
        (Dynamic, "dynamic", 0),
        (Day, "day", 1),
        (Night, "night", 2)
    ]
);

config_enum!(
    TravelMoveOutChance,
    default = Default,
    variants = [(Ten, "10", 0), (Fifty, "50", 1), (Default, "default", 2)]
);
