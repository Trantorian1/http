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
/// # Examples
///
/// ```rust
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

/// Compares two byte strings together.
///
/// # Examples
///
/// ```rust
/// # use http_primitives::prelude::*;
/// assert_streq!(b"Trantorian", b"Trantorian");
/// ```
#[macro_export]
macro_rules! assert_str_eq {
    ($left:expr,$right:expr) => {{
        let left = std::str::from_utf8($left).expect("Invalid utf8");
        let right = std::str::from_utf8($right).expect("Invalid utf8");
        assert_eq!(left, right);
    }};
}

#[macro_export]
macro_rules! assert_char_eq {
    ($left:expr,$right:expr) => {{
        let left = char::from_u32($left as u32).expect("Invalid utf8");
        let right = char::from_u32($right as u32).expect("Invalid utf8");
        assert_eq!(left, right);
    }};
}
