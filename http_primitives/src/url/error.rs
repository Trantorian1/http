/// [`Url`] parsing errors.
///
/// [`Url`]: crate::Url
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// [`URL`] with a scheme specifier (`://`) but not scheme preceding it.
    ///
    /// # Example
    ///
    /// ```text
    /// ://example.com
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingScheme,

    /// [`URL`] with a port but no host, or a scheme but no host.
    ///
    /// # Example
    ///
    /// ```text
    /// http:://
    ///
    /// or
    ///
    /// http:///
    ///
    /// or
    ///
    /// http:///
    ///
    /// or
    ///
    /// :80
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingHost,

    /// [`Url`] with a fragment specifier (`#`) but no fragment after it.
    ///
    /// # Example
    ///
    /// ```text
    /// example.com#
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingFragment,

    /// [`URL`] containing a character which is not part of the allowed set for a given fragment.
    ///
    /// # Example
    ///
    /// ```text
    /// example!com
    /// ```
    ///
    /// [`URL`]: super::Url
    ReservedCharacter(u8),

    /// [`Query`] containing an empty parameter.
    ///
    /// # Example
    ///
    /// ```text
    /// ?&b=1
    /// ```
    ///
    /// [`Query`]: super::Query
    EmptyQueryParameter,

    /// [`Query`] parameter which does not adhere to `x-www-form-urlencoded`
    ///
    /// # Example
    ///
    /// ```text
    /// ?abc
    /// ```
    ///
    /// [`Query`]: super::Query
    InvalidQuery,

    /// [`Query`] parameter without a key.
    ///
    /// # Example
    ///
    /// ```text
    /// ?=value
    /// ```
    ///
    /// [`Query`]: super::Query
    MissingQueryKey,

    /// [`Query`] parameter without a value.
    ///
    /// # Example
    ///
    /// ```text
    /// ?key=
    /// ```
    ///
    /// [`Query`]: super::Query
    MissingQueryValue,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::MissingScheme => f.write_str("Empty scheme before scheme delimiter"),
            Error::MissingHost => f.write_str("Empty host in URI with non-empty port or scheme"),
            Error::MissingFragment => f.write_str("Empty fragment after fragment delimiter"),
            Error::ReservedCharacter(c) => write!(f, "Use of reserved character {c}"),
            Error::EmptyQueryParameter => f.write_str("Empty query parameter"),
            Error::InvalidQuery => f.write_str("Query parameter does not adhere to key-value form"),
            Error::MissingQueryKey => f.write_str("Query parameter without a key"),
            Error::MissingQueryValue => f.write_str("Query parameter without a value"),
        }
    }
}

impl std::error::Error for Error {}
