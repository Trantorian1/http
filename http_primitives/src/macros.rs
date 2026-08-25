#[macro_export]
/// Creates a new [`NonZero`] integer.
///
/// # Examples
///
/// ```rust
/// # use http_primitives::prelude::*;
/// assert_eq!(nonzero!(42), std::num::NonZero::new(42).unwrap());
/// ```
///
/// [`NonZero`]: std::num::NonZero
macro_rules! nonzero {
    ($n:expr) => {
        std::num::NonZero::new($n).expect("literal should be non-zero")
    };
}

/// Assert less than.
///
/// # Examples
///
/// ```rust
/// # use http_primitives::prelude::*;
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
/// # Examples
///
/// ```rust
/// # use http_primitives::prelude::*;
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

/// Assert greater than.
///
/// # Examples
///
/// ```rust
/// # use http_primitives::prelude::*;
/// assert_gr!(4, 3);
/// ```
#[macro_export]
macro_rules! assert_gr {
    ($left:expr,$right:expr) => {
        assert!($left > $right, "{} > {}", $left, $right)
    };
    ($left:expr,$right:expr,$msg:literal) => {
        assert!($left > $right, concat!("{} > {}: ", $msg), $left, $right)
    };
}

/// Assert greater than or equal.
///
/// ## Example Usage
///
/// ```
/// # use http_primitives::prelude::*;
/// assert_greq!(4, 4);
/// ```
#[macro_export]
macro_rules! assert_greq {
    ($left:expr,$right:expr) => {
        assert!($left >= $right, "{} >= {}", $left, $right)
    };
    ($left:expr,$right:expr,$msg:literal) => {
        assert!($left >= $right, concat!("{} >= {}: ", $msg), $left, $right)
    };
}
