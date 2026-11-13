//! HTTP/1.1 zero-copy header parsing implementation based off [RFC9110].
//!
//! [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values

mod deserializer;
mod error;
mod stack;

pub mod constants;
pub mod line;

pub use error::*;

pub mod prelude {
    //! A “prelude” for crates using the `http_headers` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use http_headers::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use super::element;
    pub use super::error::*;
    pub use super::line;
    pub use super::line::HeaderLine;
    pub use super::line::Headers;
    pub use super::line::Value;
    pub use super::parameter;
}
