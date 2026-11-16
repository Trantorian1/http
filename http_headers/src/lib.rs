//! HTTP/1.1 zero-copy header parsing implementation based off [RFC9110].
//!
//! [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values

mod deserializer;
mod error;
mod stack;

pub mod constants;
pub mod line;

pub use error::*;

pub mod de {
    //! Serde deserialization of parsed header sections.
    //!
    //! Deserializes a whole header section (a run of [`crate::line::Headers`] lines) into a
    //! typed target or a map, on top of the zero-copy parser in [`crate::line`].

    pub use super::deserializer::Error;
    pub use super::deserializer::ErrorParsing;

    /// Deserializes a whole header section into a typed target.
    ///
    /// The section is presented as a map from lower-cased field name to field value: struct
    /// targets get identifier-matched field names, and each field value is deserialized from
    /// its line's values. Borrowed data in `T` must not outlive the input buffer backing
    /// `headers`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parser`] for malformed header lines and [`Error::ErrorParsing`] when
    /// the wire data does not fit the target type (shape, name, or content mismatches).
    pub fn from_headers<'a, T>(headers: crate::line::Headers<'a>) -> Result<T, Error>
    where
        T: serde::Deserialize<'a>,
    {
        let mut section = super::deserializer::SectionDeserializer::from_headers(headers);

        T::deserialize(&mut section)
    }
}

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
