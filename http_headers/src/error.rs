//! Error types for header deserialization.

/// Fatal errors that occur during header deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The combined values of the desired headers do not fit into the given backing buffer.
    OOM,

    /// The header section is unterminated: input bytes remain without a CRLF line terminator.
    Eof,

    /// A header line does not contain a colon separating the field name from the field value.
    ExpectedColon,

    /// A header field name contains characters that are not valid [token] characters.
    ///
    /// [token]: https://www.rfc-editor.org/rfc/rfc9110#name-tokens
    InvalidToken,

    /// A header line begins with whitespace: obsolete line folding ([obs-fold]) is rejected.
    ///
    /// [obs-fold]: https://www.rfc-editor.org/rfc/rfc9112#section-5.2
    ObsFold,

    /// The target struct declares more headers than this crate supports.
    TooManyFields,

    /// A header value is not valid UTF-8, but the target type requires a string.
    InvalidUtf8,

    /// A header value could not be interpreted as the type requested by the target field.
    InvalidValue,

    /// The requested deserialization form is not supported by this format.
    Unsupported,

    /// A field required by the target struct was not present in the header section.
    MissingField(&'static str),

    /// A custom error was produced through [`serde::de::Error::custom`].
    ///
    /// The message is intentionally discarded to keep this type allocation-free.
    Custom,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::OOM => write!(
                f,
                "The combined values of the desired headers do not fit into the given backing \
                 buffer"
            ),
            Error::Eof => write!(f, "The header section is unterminated"),
            Error::ExpectedColon => {
                write!(
                    f,
                    "A header line does not contain a colon separating name from value"
                )
            },
            Error::InvalidToken => {
                write!(f, "A header field name contains invalid token characters")
            },
            Error::ObsFold => write!(
                f,
                "A header line begins with whitespace (obsolete line folding)"
            ),
            Error::TooManyFields => write!(f, "The target struct declares too many headers"),
            Error::InvalidUtf8 => write!(f, "A header value is not valid UTF-8"),
            Error::InvalidValue => write!(
                f,
                "A header value could not be interpreted as the requested type"
            ),
            Error::Unsupported => write!(f, "The requested deserialization form is not supported"),
            Error::MissingField(field) => write!(
                f,
                "The field `{field}` was not present in the header section"
            ),
            Error::Custom => write!(f, "A custom error occurred"),
        }
    }
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(_msg: T) -> Self {
        // The message is discarded to keep `Error` allocation-free.
        Self::Custom
    }

    fn missing_field(field: &'static str) -> Self {
        Self::MissingField(field)
    }

    fn invalid_type(_unexp: serde::de::Unexpected, _exp: &dyn serde::de::Expected) -> Self {
        Self::Unsupported
    }
}
