/// General header line format errors.
///
/// This includes errors pertaining to the field name as well as the field value. More specific
/// value errors are encapsulated in the [`ErrorValue`] type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Tried to parse a header with an empty field.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"";
    ///
    /// assert_eq!(line::HeaderLine::new(&mut input), Err(Error::EmptyLine));
    /// # Ok(())
    /// # }
    /// ```
    EmptyLine,

    /// Missing ':' field delimiter in a header line.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type text/plain\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// assert_eq!(headers.next(), Some(Err(Error::MissingFieldDelimiter)));
    /// # Ok(())
    /// # }
    /// ```
    MissingFieldDelimiter,

    /// Missing field name in a header line.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b": text/plain\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// assert_eq!(headers.next(), Some(Err(Error::MissingFieldName)));
    /// # Ok(())
    /// # }
    /// ```
    MissingFieldName,

    /// Missing [`CRLF`] delimiter at the end of a header line.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: text/plain";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// assert_eq!(headers.next(), Some(Err(Error::MissingCRLFDelimiter)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`CRLF`]: crate::constants::CRLF
    MissingCRLFDelimiter,

    /// Unsupported byte in header field name.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content=Type: text/plain\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// assert_eq!(headers.next(), Some(Err(Error::InvalidFieldName(b'='))));
    /// # Ok(())
    /// # }
    /// ```
    InvalidFieldName(u8),

    /// Field value parsing error.
    Value(ErrorValue),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::EmptyLine => {
                write!(f, "Tried to parse a header with an empty field")
            },
            Error::MissingFieldDelimiter => {
                write!(f, "Missing ':' field delimiter in header line")
            },
            Error::MissingFieldName => {
                write!(f, "Missing field name in header line")
            },
            Error::MissingCRLFDelimiter => {
                write!(f, "Missing CRLF delimiter at the end of a header line")
            },
            Error::InvalidFieldName(c) => {
                write!(f, "Invalid byte '{c}' in header field name")
            },
            Error::Value(err) => write!(f, "{err}"),
        }
    }
}

impl From<ErrorValue> for Error {
    fn from(err: ErrorValue) -> Self {
        Self::Value(err)
    }
}

impl std::error::Error for Error {}

/// Header format errors which are specific to field values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorValue {
    /// # Tried to parse an empty header field value list.
    ///
    /// Unlike single field elements, lists cannot be empty. Even if they can contain _some_ empty
    /// elements, they must at least contain _one_ non-empty element to be considered valid.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Example-List: ,  ,, ,\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::EmptyList)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field value lists are defined in [section 5.6.1] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.1]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.1
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    EmptyList,

    /// # Header field value is missing a closing quote.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: \"multipart/form-data\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingClosingQuote)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field quotation semantics are defined in [section 5.6.4] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingClosingQuote,

    /// # Header field value has leftover bytes after its closing quote delimiter.
    ///
    /// Similar to how quotation escape sequences are handled, mixed quoted byte sequences greatly
    /// complicate the parsing process and make it much harder to avoid copies during
    /// deserialization. They are therefore not supported in the current design.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: \"multipart\"/form-data\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::LeftoverBytes)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field quotation semantics are defined in [section 5.6.4] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    LeftoverBytes,

    /// # A header field value parameter is missing its '=' delimiter.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: ;boundary7MA4YWxkTrZu0gW\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::MissingParameterDelimiter))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterDelimiter,

    /// # A header field value parameter is missing its key.
    ///
    /// Unlike single field elements, parameters pairs cannot have an empty key or value.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: ;=7MA4YWxkTrZu0gW\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingParameterKey)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterKey,

    /// # A header field value parameter is missing its value.
    ///
    /// Unlike single field elements, parameters pairs cannot have an empty key or value.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: ;boundary=\r\n\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingParameterValue)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterValue,

    /// # Invalid byte in a header field value.
    ///
    /// Parsed bytes do not correspond to the legal set allowed in the specs.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: text\0plain\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::InvalidFieldValue(b'\0')))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// The grammar for header field values is defined in [section 5.5] of [RFC9110], HTTP Semantics.
    ///
    /// ```text
    /// field-value    = *field-content
    /// field-content  = field-vchar [ 1*( SP / HTAB / field-vchar ) field-vchar ]
    /// field-vchar    = VCHAR / obs-text
    /// obs-text       = %x80-FF
    /// ```
    ///
    /// [section 5.5]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    InvalidFieldValue(u8),

    /// # Invalid byte in a header field parameter.
    ///
    /// Parsed bytes do not correspond to the legal set allowed in the specs.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// // "\" cannot be used in parameter keys.
    /// let mut input = *b"Content-Type: ; bo\\undary=7MA4YWxkTrZu0gW\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::InvalidFieldParameter(b'\\')))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Validation rules only differ between parameter keys and values in that the latter can be
    /// quoted. Despite this, this error type is used for both key and value format errors and does
    /// not distinguish between either.
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// // This time the parameter *value* is invalid.
    /// let mut input = *b"Content-Type: ; boundary=7MA4Y\\WxkTrZu0gW\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::InvalidFieldParameter(b'\\')))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// The grammar for header field parameters is defined in [section 5.6.6] of [RFC9110], HTTP
    /// Semantics.
    ///
    /// ```text
    /// parameters      = *( OWS ";" OWS [ parameter ] )
    /// parameter       = parameter-name "=" parameter-value
    /// parameter-name  = token
    /// parameter-value = ( token / quoted-string )
    ///
    /// token           = 1*tchar
    /// tchar           = "!" / "#" / "$" / "%" / "&" / "'" / "*"
    ///                 / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
    ///                 / DIGIT / ALPHA
    ///
    /// quoted-string   = DQUOTE *( qdtext / quoted-pair ) DQUOTE
    /// qdtext          = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
    /// quoted-pair     = "\" ( HTAB / SP / VCHAR / obs-text )
    ///
    /// OWS             = *( SP / HTAB )
    /// ```
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    InvalidFieldParameter(u8),

    /// # Invalid escape sequence inside a header field value quote.
    ///
    /// Escape bytes in HTTP headers do not work the same way as escape bytes in ASCII, in that you
    /// aren't storing a hex representation of the next byte to be inserted. Instead, the next byte
    /// after a `\` character is inserted verbatim, so `\"` translates to `"`. This means that an
    /// escape sequence is considered invalid if the byte it would insert is not legal in the
    /// context of the field in which it is being used.
    ///
    /// _This also happens to be one of the main limitations to using the low-level parsing API:
    /// since special care is taken to avoid copying data when parsing header bytes, such escape
    /// sequences cannot be replaced at this stage in the parsing process. A second cleanup step has
    /// to then be manually performed in order to properly replace them._
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: \"text\\\0/plain\"\r\n";
    ///
    /// let mut headers = line::Headers::new(&mut input);
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::InvalidEscapeSequence(b'\0')))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Specs
    ///
    /// The grammar for header quoted strings is defined in [section 5.6.4] of [RFC9110], HTTP
    /// Semantics.
    ///
    /// ```text
    /// quoted-string   = DQUOTE *( qdtext / quoted-pair ) DQUOTE
    /// qdtext          = HTAB / SP / %x21 / %x23-5B / %x5D-7E / obs-text
    /// quoted-pair     = "\" ( HTAB / SP / VCHAR / obs-text )
    /// ```
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    InvalidEscapeSequence(u8),
}

impl std::fmt::Display for ErrorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyList => {
                write!(f, "Header field value lists cannot be empty")
            },
            Self::MissingClosingQuote => {
                write!(f, "Header field value is missing a closing quote")
            },
            Self::LeftoverBytes => {
                write!(f, "Header field value contains leftover bytes afetr quote")
            },
            Self::MissingParameterDelimiter => {
                write!(f, "Header field value parameter missing its '=' delimiter")
            },
            Self::MissingParameterKey => {
                write!(f, "Header field value parameter is missing a key")
            },
            Self::MissingParameterValue => {
                write!(f, "Header field value parameter is missing a value")
            },
            Self::InvalidFieldValue(c) => {
                write!(f, "Invalid byte '{c}' in header field element")
            },
            Self::InvalidFieldParameter(c) => {
                write!(f, "Invalid byte '{c}' in header field parameter")
            },
            Self::InvalidEscapeSequence(c) => {
                write!(f, "Invalid escape '{c}' in header field value quote")
            },
        }
    }
}

impl std::error::Error for ErrorValue {}
