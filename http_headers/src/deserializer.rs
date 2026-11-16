//! Serde deserialization layer for header field values.
//!
//! This module provides a [`serde::Deserializer`] implementation which maps header field values
//! onto Serde's data model on top of the zero-copy parser in [`crate::line`].

use serde::de;

use super::line::HeaderValues;
use super::line::Value;

/// Error type for the serde deserialization layer.
///
/// This is deliberately separate from [`crate::error::Error`] so that consumers of the low-level
/// parser in [`crate::line`] never have to handle deserialization-specific variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A parser error surfaced while driving the header value iterator.
    Parser(crate::error::Error),

    /// A failure specific to the deserialization process itself.
    ErrorParsing(ErrorParsing),
}

/// Deserialization-specific failures which are not raised by the low-level parser.
#[derive(Clone, Debug, PartialEq, Eq)]
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

impl From<crate::error::ErrorValue> for Error {
    fn from(err: crate::error::ErrorValue) -> Self {
        Self::Parser(crate::error::Error::from(err))
    }
}

impl From<ErrorParsing> for Error {
    fn from(err: ErrorParsing) -> Self {
        Self::ErrorParsing(err)
    }
}

/// Serde deserializer for a single header field value.
///
/// Wraps the zero-copy [`HeaderValues`] iterator together with at most one buffered (lookahead)
/// [`Value`] and an item-scope flag. Consumption is strictly forward-only: once a value has been
/// yielded, its buffer region is frozen and must never be mutated again.
#[derive(Debug)]
pub(crate) struct Deserializer<'a> {
    /// The underlying field-value iterator driving the parser in [`super::line`].
    values: HeaderValues<'a>,

    /// At most one value pulled ahead for shape classification and item grouping.
    lookahead: Option<Value<'a>>,

    /// Whether the current position is inside a list item (set by the sequence access).
    in_item_scope: bool,
}

impl<'a> Deserializer<'a> {
    /// Initializes a new deserializer over the field values of a parsed header line.
    #[must_use]
    pub(crate) fn from_header_values(values: HeaderValues<'a>) -> Self {
        Self {
            values,
            lookahead: None,
            in_item_scope: false,
        }
    }

    /// Pulls the next value out of the lookahead buffer or, if that is empty, the underlying
    /// iterator.
    fn next_value(&mut self) -> Result<Option<Value<'a>>, Error> {
        if let Some(value) = self.lookahead.take() {
            return Ok(Some(value));
        }

        Ok(self.values.next().transpose()?)
    }
}

/// Coerces an owned element of the backing buffer into a shared view.
///
/// Forward-only invariant: taking the mutable slice by value is what enforces this — once a
/// region has been coerced and yielded, no mutable handle to it remains, so it can never be
/// mutated again for the lifetime of the buffer.
fn shared_bytes(element: &mut [u8]) -> &[u8] {
    element
}

/// Coerces an owned element of the backing buffer into a shared string view, validating UTF-8.
///
/// See [`shared_bytes`] for the forward-only invariant this coercion relies on.
fn shared_str(element: &mut [u8]) -> Result<&str, Error> {
    std::str::from_utf8(element).map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::element;
    use crate::error;
    use crate::line;

    #[test]
    fn parser_error_conversion() {
        let err = Error::from(error::Error::MissingFieldDelimiter);

        assert!(matches!(
            err,
            Error::Parser(error::Error::MissingFieldDelimiter)
        ));
    }

    #[test]
    fn error_value_conversion() {
        let err = Error::from(error::ErrorValue::EmptyList);

        assert_eq!(
            err,
            Error::Parser(error::Error::Value(error::ErrorValue::EmptyList))
        );
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

    #[test]
    fn deserializer_construction() {
        let mut input_a = *b"Content-Type: text/plain";
        let mut input_b = *b"Accept: */*";

        let header_a = line::HeaderLine::new(&mut input_a).unwrap();
        let header_b = line::HeaderLine::new(&mut input_b).unwrap();

        let de_a = Deserializer::from_header_values(header_a.values);
        let de_b = Deserializer::from_header_values(header_b.values);

        assert_ne!(format!("{de_a:?}"), format!("{de_b:?}"));
    }

    #[test]
    fn next_value_pulls_in_order() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        assert_eq!(de.next_value(), Ok(Some(element!(b"b"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn next_value_lookahead_takes_priority() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        // Pull both values so the underlying iterator is exhausted...
        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        let pulled = de.next_value().unwrap();
        assert_eq!(pulled, Some(element!(b"b")));

        // ...then buffer the last one in the lookahead. It must be served before any further pull.
        de.lookahead = pulled;
        assert_eq!(de.next_value(), Ok(Some(element!(b"b"))));

        // The lookahead pull did not advance the iterator: it is still exhausted.
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn shared_bytes_borrows_element() {
        let mut input = *b"Content-Length: 3";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert_eq!(shared_bytes(element), b"3".as_slice());
    }

    #[test]
    fn shared_str_borrows_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert_eq!(shared_str(element), Ok("text/plain"));
    }

    #[test]
    fn shared_str_invalid_utf8() {
        // `obs-text` (0x80-FF) is legal per the parser, so non-UTF-8 elements do reach this layer.
        let mut input = *b"Example-X: te\xffxt";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert!(matches!(
            shared_str(element),
            Err(Error::ErrorParsing(ErrorParsing::InvalidUtf8))
        ));
    }
}
