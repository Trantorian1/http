//! Basic quality-of-life macros which are used throughout the project.

pub mod prelude {
    //! A “prelude” for crates using the `macro_util` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use macro_util::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use super::assert_byte_eq;
    pub use super::assert_gr;
    pub use super::assert_greq;
    pub use super::assert_le;
    pub use super::assert_leq;
    pub use super::assert_matches;
    pub use super::assert_next;
    pub use super::assert_utf8_eq;
    pub use super::mutbytes;
    pub use super::nonzero;
}

#[macro_export]
macro_rules! mutbytes {
    ($bytes:literal) => {
        &mut $bytes.map(|b| b)
    };
}

#[macro_export]
/// Asserts match or extracts an element on match.
///
/// # Examples
///
/// Checking an element for a match:
///
/// ```rust
/// # use macro_util::prelude::*;
/// let value = Some(1);
///
/// assert_matches!(value, Some(..));
/// ```
///
/// Extracting an element on match:
///
/// ```rust
/// # use macro_util::prelude::*;
/// let value = Some(1);
///
/// assert_matches!(value, Some(n) => {
///     assert_eq!(n, 1);
/// })
/// ```
macro_rules! assert_matches {
    ($elem:expr, $pat:pat => $tt:tt) => {
        match $elem {
            $pat => $tt,
            n => panic!("invalid variant: {n:?}"),
        }
    };
    ($elem:expr, $pat:pat) => {
        assert_matches!($elem, $pat => {})
    }
}

/// Asserts equality or match on the next element of an iterator.
///
/// # Examples
///
/// Comparing the next element for equality:
///
/// ```rust
/// # use macro_util::prelude::*;
/// let mut iter = [0, 1, 2].iter();
///
/// assert_next!(iter, Some(&0));
/// ```
///
/// Pattern matching the next element:
///
/// ```rust
/// # use macro_util::prelude::*;
/// let mut iter = [Ok(0), Err(1), Ok(2)].iter();
///
/// assert_next!(iter, Some(Ok(n)) => {
///     assert_eq!(n, &0);
/// })
/// ````
#[macro_export]
macro_rules! assert_next {
    ($values:expr, $pat:pat => $tt:tt) => {
        match $values.next() {
            $pat => $tt,
            n => panic!("invalid variant: {n:?}"),
        }
    };
    ($values:expr, $expr:expr) => {
        assert_eq!($values.next(), $expr);
    };
}

/// Creates a new [`NonZero`] integer.
///
/// # Examples
///
/// ```rust
/// # use macro_util::prelude::*;
/// assert_eq!(nonzero!(42), std::num::NonZero::new(42).unwrap());
/// ```
///
/// [`NonZero`]: std::num::NonZero
#[macro_export]
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
/// # use macro_util::prelude::*;
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
/// # use macro_util::prelude::*;
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
/// # use macro_util::prelude::*;
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
/// # use macro_util::prelude::*;
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

/// Compares two utf8 byte strings together.
///
/// # Examples
///
/// ```rust
/// # use macro_util::prelude::*;
/// assert_utf8_eq!(b"Trantorian", b"Trantorian");
/// ```
#[macro_export]
macro_rules! assert_utf8_eq {
    ($left:expr,$right:expr) => {{
        let left = std::str::from_utf8($left).expect("Invalid utf8");
        let right = std::str::from_utf8($right).expect("Invalid utf8");
        assert_eq!(left, right);
    }};
}

/// Compares two utf8 bytes together.
///
/// # Example
///
/// ```rust
/// use macro_util::*;
/// assert_byte_eq!(b'a', b'a');
/// ```
#[macro_export]
macro_rules! assert_byte_eq {
    ($left:expr,$right:expr) => {{
        let left = char::from_u32($left as u32).expect("Invalid utf8");
        let right = char::from_u32($right as u32).expect("Invalid utf8");
        assert_eq!(left, right);
    }};
}
