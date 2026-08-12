/// Assert less than.
///
/// ## Example Usage
///
/// ```
/// # use http_core::prelude::*;
/// assert_le!(3, 4);
/// ```
#[macro_export]
macro_rules! assert_le {
    ($left:expr,$right:expr) => {
        assert!($left < $right, "{} < {}", $left, $right)
    };
    ($left:expr,$right:expr,$msg:literal) => {
        assert!($left < $right, concat!("{} < {}: ", $msg), $left, $right)
    };
}

/// Assert less than or equal.
///
/// ## Example Usage
///
/// ```
/// # use http_core::prelude::*;
/// assert_leq!(4, 4);
/// ```
#[macro_export]
macro_rules! assert_leq {
    ($left:expr,$right:expr) => {
        assert!($left <= $right, "{} <= {}", $left, $right)
    };
    ($left:expr,$right:expr,$msg:literal) => {
        assert!($left <= $right, concat!("{} <= {}: ", $msg), $left, $right)
    };
}
