//! The [`Url`] [standard] limits which code points can be used in each URL segment. This is the
//! case for code points which are used as section delimiters and for any non-ASCII code points.
//!
//! # Percent encoding
//!
//! For some segments, it is still possible to include these code points as special
//! _percent-encoded_ bytes. To percent-encode a byte, return a string consisting of U+0025 (%),
//! followed by two ASCII upper hex digits representing that byte.
//!
//! | Byte         | Percent-encoded |
//! |--------------|-----------------|
//! | U+0020 SPACE | %20             |
//! | U+002F (/)   | %2F             |
//! | U+003F (?)   | %3F             |
//! | U+0023 (#)   | %23             |
//! | U+0026 (&)   | %26             |
//! | U+003D (=)   | %3D             |
//! | U+0025 (%)   | %25             |
//! | U+002B (+)   | %2B             |
//!
//! # Percent-encode sets
//!
//! For any URL segment, the set of code points which _must_ be percent encoded to appear in that
//! segment is referred to as a percent-[`EncodeSet`].
//!
//! [`Url`]: crate::Url

mod decode;
mod encode;
mod sets;

pub use decode::*;
pub use encode::*;
pub use sets::*;
