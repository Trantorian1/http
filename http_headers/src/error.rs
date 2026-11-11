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
    /// const HEADER: &str = "";
    ///
    /// assert_eq!(
    ///     line::HeaderItem::new(HEADER.as_bytes()),
    ///     Err(Error::EmptyLine)
    /// );
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
    /// const HEADERS: &str = "Content-Type text/plain\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
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
    /// const HEADERS: &str = ": text/plain\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
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
    /// const HEADERS: &str = "Content-Type: text/plain";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
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
    /// const HEADERS: &str = "Content=Type: text/plain\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
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

/// Field-specific header format errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorValue {
    /// Tried to parse an empty header field value list.
    ///
    /// # Specs
    ///
    /// Header field value lists are defined in [section 5.6.1] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Example-List: ,  ,, ,\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::EmptyList)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.1]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.1
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    EmptyList,

    /// Header field value is missing a closing quote.
    ///
    /// # Specs
    ///
    /// Header field quotation semantics are defined in [section 5.6.4] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Content-Type: \"multipart/form-data\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingClosingQuote)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingClosingQuote,

    /// Header field value has leftover bytes after its closing quote delimiter.
    ///
    /// # Specs
    ///
    /// Header field quotation semantics are defined in [section 5.6.4] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Content-Type: \"multipart\"/form-data\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::LeftoverBytes)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    LeftoverBytes,

    /// A header field value parameter is missing its '=' delimiter.
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Content-Type: ;boundary7MA4YWxkTrZu0gW\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Err(ErrorValue::MissingParameterDelimiter))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterDelimiter,

    /// A header field value parameter is missing its key.
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Content-Type: ;=7MA4YWxkTrZu0gW\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingParameterKey)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterKey,

    /// A header field value parameter is missing its value.
    ///
    /// # Specs
    ///
    /// Header field parameters are defined in [section 5.6.6] of [RFC9110], HTTP Semantics.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADERS: &str = "Content-Type: ;boundary=\r\n\r\n";
    ///
    /// let mut headers = line::HeaderLines::new(HEADERS.as_bytes());
    /// let mut header = headers.next().unwrap()?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Err(ErrorValue::MissingParameterValue)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [section 5.6.6]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.6
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    MissingParameterValue,

    InvalidFieldElement(u8),

    InvalidFieldParameter(u8),

    InvalidQuotedPair(u8),
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
            Self::InvalidFieldElement(c) => {
                write!(f, "Invalid byte '{c}' in header field element")
            },
            Self::InvalidFieldParameter(c) => {
                write!(f, "Invalid byte '{c}' in header field parameter")
            },
            Self::InvalidQuotedPair(c) => {
                write!(f, "Invalid escape '{c}' in header field value quote")
            },
        }
    }
}

impl std::error::Error for ErrorValue {}
