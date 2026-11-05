//! Zero-copy, statically allocated HTTP header deserialization.
//!
//! This crate deserializes a raw [header section] into a user-defined struct through serde,
//! without any heap allocation:
//!
//! - The set of headers to extract is learned from the target struct itself (its field names,
//!   post-`rename`), so only those headers are ever touched.
//! - Repeated headers are combined into their [field list] form (`Foo, Bazz`) exactly as
//!   [RFC9110] requires; combined values are materialized into a caller-provided backing buffer.
//! - A header that occurs exactly once is served as a direct slice of the input bytes (true
//!   zero-copy); only combined values are copied into the backing buffer.
//!
//! # Backing buffer sizing
//!
//! The combined value of every desired field fits into a backing buffer whose length is at least
//! that of the raw header section, so sizing the buffer to the section length guarantees that
//! deserialization cannot fail with [`Error::OOM`].
//!
//! [header section]: https://www.rfc-editor.org/rfc/rfc9112#section-5
//! [field list]: https://www.rfc-editor.org/rfc/rfc9110#section-5.3
//! [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110

mod deserializer;
mod error;
mod line;

pub use error::*;

/// Deserializes a raw header section into a target struct.
///
/// The input is the raw bytes of the header section: it must start at the first header line and
/// include the terminating blank line (CRLF). Field names are matched [case-insensitively]
/// against the target struct's field names (post-`rename`). Headers which no field declares are
/// ignored; repeated headers are combined into their [field list] form.
///
/// # Errors
///
/// Returns [`Error::OOM`] if the combined values of the desired headers do not fit into the
/// backing buffer, a line-level error ([`Error::Eof`], [`Error::ExpectedColon`],
/// [`Error::InvalidToken`], [`Error::ObsFold`]) on malformed input, or a value-level error
/// ([`Error::InvalidUtf8`], [`Error::InvalidValue`], [`Error::MissingField`]) if a header value
/// cannot be interpreted as its target type.
///
/// # Examples
///
/// ```rust
/// use http_headers::Error;
/// use http_headers::from_bytes;
///
/// #[derive(serde::Deserialize)]
/// struct Headers<'a> {
///     #[serde(rename = "Content-Type")]
///     content_type: &'a str,
///     #[serde(rename = "X-Repeat")]
///     repeat: Option<&'a str>,
/// }
///
/// # fn main() -> Result<(), Error> {
/// let input = b"Content-Type: text/plain\r\nX-Repeat: Foo\r\nX-Repeat: Bazz\r\n\r\n";
/// let mut backing = [0u8; 64];
/// let headers = from_bytes::<Headers>(input, &mut backing)?;
///
/// assert_eq!(headers.content_type, "text/plain");
/// assert_eq!(headers.repeat, Some("Foo, Bazz"));
/// # Ok(())
/// # }
/// ```
///
/// [case-insensitive]: https://www.rfc-editor.org/rfc/rfc9110#section-5.1
/// [field list]: https://www.rfc-editor.org/rfc/rfc9110#section-5.3
pub fn from_bytes<'de, T>(input: &'de [u8], backing: &'de mut [u8]) -> Result<T, Error>
where
    T: serde::Deserialize<'de>,
{
    let de = deserializer::Deserializer { input, backing };
    T::deserialize(de)
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
    struct MyHeaders<'a> {
        #[serde(rename = "Content-Type")]
        content_type: &'a str,
        #[serde(rename = "Content-Length")]
        content_length: usize,
        /// Repeated on the wire -> must be joined into the backing buffer.
        #[serde(rename = "X-Repeat")]
        repeat: Option<&'a str>,
        /// Single occurrence on the wire -> must be a direct input borrow.
        #[serde(rename = "X-Single")]
        single: Option<&'a str>,
    }

    fn parse<'de>(input: &'de [u8], backing: &'de mut [u8]) -> Result<MyHeaders<'de>, Error> {
        from_bytes(input, backing)
    }

    #[test]
    fn headers_deserialize() {
        let input = b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        let expected = Ok(MyHeaders {
            content_type: "text/plain",
            content_length: 3,
            repeat: None,
            single: None,
        });

        assert_eq!(parse(input, &mut backing), expected);
    }

    #[test]
    fn repeated_headers_are_joined() {
        let input = b"\
            Content-Type: text/plain\r\n\
            Content-Length: 3\r\n\
            X-Repeat: Foo\r\n\
            X-Repeat: Bazz\r\n\
            \r\n\
        ";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        assert_eq!(headers.repeat, Some("Foo, Bazz"));
    }

    #[test]
    fn case_insensitive_names() {
        let input = b"content-type: text/plain\r\nCONTENT-LENGTH: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        assert_eq!(headers.content_type, "text/plain");
        assert_eq!(headers.content_length, 3);
    }

    #[test]
    fn ows_trimmed() {
        let input = b"Content-Type:   text/plain \t\r\nContent-Length:\t3  \r\n\r\n";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        assert_eq!(headers.content_type, "text/plain");
        assert_eq!(headers.content_length, 3);
    }

    #[test]
    fn unknown_headers_ignored() {
        let input = b"\
            Host: example.com\r\n\
            Content-Type: text/plain\r\n\
            Accept: */*\r\n\
            Content-Length: 3\r\n\
            X-Custom: whatever\r\n\
            \r\n\
        ";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        assert_eq!(headers.content_type, "text/plain");
    }

    #[test]
    fn missing_required_field_errors() {
        let input = b"Content-Type: text/plain\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(
            parse(input, &mut backing),
            Err(Error::MissingField("Content-Length"))
        );
    }

    #[test]
    fn empty_value() {
        let input = b"Content-Type:\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        assert_eq!(headers.content_type, "");
    }

    #[test]
    fn empty_section() {
        let input = b"\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(
            parse(input, &mut backing),
            Err(Error::MissingField("Content-Type"))
        );
    }

    #[test]
    fn unterminated_section_errors() {
        // No terminating blank line: the last line has no CRLF.
        let input = b"Content-Type: text/plain\r\nContent-Length: 3";
        let mut backing = [0u8; 64];

        assert_eq!(parse(input, &mut backing), Err(Error::Eof));
    }

    #[test]
    fn obs_fold_rejected() {
        let input = b"Content-Type: text/plain\r\n continued\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(parse(input, &mut backing), Err(Error::ObsFold));
    }

    #[test]
    fn invalid_token_name_errors() {
        let input = b"Con tent: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(parse(input, &mut backing), Err(Error::InvalidToken));
    }

    #[test]
    fn line_without_colon_errors() {
        let input = b"Content-Type: text/plain\r\nBrokenLine\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(parse(input, &mut backing), Err(Error::ExpectedColon));
    }

    #[test]
    fn backing_overflow_errors() {
        let input = b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 4];

        assert_eq!(parse(input, &mut backing), Err(Error::OOM));
    }

    #[test]
    fn int_overflow_errors() {
        let input = b"Content-Type: text/plain\r\nContent-Length: 99999999999999999999\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(parse(input, &mut backing), Err(Error::InvalidValue));
    }

    #[test]
    fn zero_copy_single_occurrence_points_into_input() {
        let input = b"\
            Content-Type: text/plain\r\n\
            Content-Length: 3\r\n\
            X-Repeat: Foo\r\n\
            X-Repeat: Bazz\r\n\
            X-Single: OnlyOnce\r\n\
            \r\n\
        ";
        let mut backing = [0u8; 64];

        let headers = parse(input, &mut backing).unwrap();

        // A single occurrence must be a direct slice of the input (true zero-copy).
        let single = headers.single.unwrap();
        assert_eq!(single, "OnlyOnce");
        let ptr = single.as_ptr() as usize;
        assert!(
            (input.as_ptr() as usize..input.as_ptr() as usize + input.len()).contains(&ptr),
            "single-occurrence value should borrow from the input"
        );

        // A joined value must live in the backing buffer.
        let joined = headers.repeat.unwrap();
        assert_eq!(joined, "Foo, Bazz");
        let jptr = joined.as_ptr() as usize;
        assert!(
            (backing.as_ptr() as usize..backing.as_ptr() as usize + backing.len()).contains(&jptr),
            "joined value should borrow from the backing buffer"
        );
    }

    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[allow(dead_code)]
    struct SignedHeaders<'a> {
        #[serde(rename = "X-Negative")]
        negative: i64,
        #[serde(rename = "Content-Length")]
        content_length: usize,
        #[serde(rename = "Content-Type")]
        content_type: &'a str,
    }

    #[test]
    fn signed_integers() {
        let input = b"X-Negative: -42\r\nContent-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut backing = [0u8; 64];

        let headers = from_bytes::<SignedHeaders>(input, &mut backing).unwrap();

        assert_eq!(headers.negative, -42);
    }

    #[test]
    fn i64_min() {
        let input =
            b"X-Negative: -9223372036854775808\r\nContent-Type: t\r\nContent-Length: 1\r\n\r\n";
        let mut backing = [0u8; 64];

        let headers = from_bytes::<SignedHeaders>(input, &mut backing).unwrap();

        assert_eq!(headers.negative, i64::MIN);
    }

    #[test]
    fn i64_overflow_errors() {
        let input =
            b"X-Negative: -9223372036854775809\r\nContent-Type: t\r\nContent-Length: 1\r\n\r\n";
        let mut backing = [0u8; 64];

        assert_eq!(
            from_bytes::<SignedHeaders>(input, &mut backing),
            Err(Error::InvalidValue)
        );
    }

    #[test]
    fn display_errors() {
        assert_eq!(
            Error::OOM.to_string(),
            "The combined values of the desired headers do not fit into the given backing buffer"
        );
        assert_eq!(
            Error::MissingField("Host").to_string(),
            "The field `Host` was not present in the header section"
        );
    }
}
