//! Custom allocation primitives.

#![cfg_attr(feature = "nightly", feature(allocator_api))]

pub mod arena;

pub mod prelude {
    //! A “prelude” for crates using the `http_alloc` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use http_alloc::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.
}
