//! Serde deserialization layer for header field values.
//!
//! This module provides a [`serde::Deserializer`] implementation which maps header field values
//! onto Serde's data model on top of the zero-copy parser in [`crate::line`].

use serde::de;

/// Error type for the serde deserialization layer.
///
/// This is deliberately separate from [`crate::error::Error`] so that consumers of the low-level
/// parser in [`crate::line`] never have to handle deserialization-specific variants.
#[derive(Debug)]
pub(crate) enum Error {
    /// A parser error surfaced while driving the header value iterator.
    Parser(crate::error::Error),

    /// A failure specific to the deserialization process itself.
    ErrorParsing(ErrorParsing),
}

/// Deserialization-specific failures which are not raised by the low-level parser.
#[derive(Debug)]
pub(crate) enum ErrorParsing {
    /// A params-only field value was encountered where an element is required.
    MissingElement,

    /// An element did not normalize-match the expected name of a struct or variant.
    NameMismatch {
        /// The name expected by the target type.
        expected: &'static str,
        /// The element bytes as found on the wire.
        found: Vec<u8>,
    },

    /// A scope which must be a string was not valid UTF-8.
    InvalidUtf8,

    /// The value shape cannot satisfy the requested deserialization hint.
    UnsupportedShape(&'static str),

    /// The requested deserialization hint is not supported (bool and floats in v1).
    UnsupportedHint(&'static str),

    /// A numeric hint was given a non-numeric element.
    InvalidInteger {
        /// The element bytes as found on the wire.
        found: Vec<u8>,
    },

    /// A failure reported by Serde itself (unknown variant, invalid value, ...).
    Custom(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parser(err) => err.fmt(f),
            Self::ErrorParsing(err) => err.fmt(f),
        }
    }
}

impl std::fmt::Display for ErrorParsing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingElement => write!(f, "Header field value is missing an element"),
            Self::NameMismatch { expected, found } => write!(
                f,
                "Header field element '{}' does not match expected name '{}'",
                String::from_utf8_lossy(found),
                expected
            ),
            Self::InvalidUtf8 => write!(f, "Header field value is not valid UTF-8"),
            Self::UnsupportedShape(reason) => {
                write!(f, "Unsupported header field value shape: {reason}")
            },
            Self::UnsupportedHint(hint) => {
                write!(
                    f,
                    "Deserializing a header field value as '{hint}' is not supported"
                )
            },
            Self::InvalidInteger { found } => write!(
                f,
                "Invalid integer in header field value: '{}'",
                String::from_utf8_lossy(found)
            ),
            Self::Custom(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for ErrorParsing {}

impl de::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::ErrorParsing(ErrorParsing::Custom(msg.to_string()))
    }
}

impl From<crate::error::Error> for Error {
    fn from(err: crate::error::Error) -> Self {
        Self::Parser(err)
    }
}

impl From<ErrorParsing> for Error {
    fn from(err: ErrorParsing) -> Self {
        Self::ErrorParsing(err)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::error;

    #[test]
    fn parser_error_conversion() {
        let err = Error::from(error::Error::MissingFieldDelimiter);

        assert!(matches!(
            err,
            Error::Parser(error::Error::MissingFieldDelimiter)
        ));
    }

    #[test]
    fn error_parsing_conversion() {
        let err = Error::from(ErrorParsing::MissingElement);

        assert!(matches!(
            err,
            Error::ErrorParsing(ErrorParsing::MissingElement)
        ));
    }

    #[test]
    fn display_parser_error() {
        let err = Error::from(error::Error::MissingFieldDelimiter);

        assert_eq!(
            err.to_string(),
            "Missing ':' field delimiter in header line"
        );
    }

    #[test]
    fn display_name_mismatch() {
        let err = Error::ErrorParsing(ErrorParsing::NameMismatch {
            expected: "TextPlain",
            found: b"text/html".to_vec(),
        });

        assert_eq!(
            err.to_string(),
            "Header field element 'text/html' does not match expected name 'TextPlain'"
        );
    }

    #[test]
    fn display_invalid_utf8() {
        let err = Error::ErrorParsing(ErrorParsing::InvalidUtf8);

        assert_eq!(err.to_string(), "Header field value is not valid UTF-8");
    }

    #[test]
    fn display_unsupported_shape() {
        let err = Error::ErrorParsing(ErrorParsing::UnsupportedShape(
            "expected single bare element",
        ));

        assert_eq!(
            err.to_string(),
            "Unsupported header field value shape: expected single bare element"
        );
    }

    #[test]
    fn display_unsupported_hint() {
        let err = Error::ErrorParsing(ErrorParsing::UnsupportedHint("bool"));

        assert_eq!(
            err.to_string(),
            "Deserializing a header field value as 'bool' is not supported"
        );
    }

    #[test]
    fn display_invalid_integer() {
        let err = Error::ErrorParsing(ErrorParsing::InvalidInteger {
            found: b"abc".to_vec(),
        });

        assert_eq!(
            err.to_string(),
            "Invalid integer in header field value: 'abc'"
        );
    }

    #[test]
    fn serde_custom_error() {
        let err = <Error as de::Error>::custom("unknown variant `foo`");

        assert!(matches!(err, Error::ErrorParsing(ErrorParsing::Custom(_))));
        assert_eq!(err.to_string(), "unknown variant `foo`");
    }
}
