// TODO: include line and character information as part of error reporting. `Error` could also
// reference parsed bytes but that would require adding an extra lifetime argument to it, which will
// be quite cumbersome for end users but might be worth it for the added clarity on errors such as
// `InvalidStructName`.

/// Header segment parsing errors.
///
/// Grammar is defined in [Section 5] of [RFC9110].
///
/// [section 5.6.1]: https://www.rfc-editor.org/rfc/rfc9110.html#name-fields
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// # Struct name does not match input bytes
    ///
    /// A `struct` can only be deserialized from a header field element which shares the same name
    /// as it.
    InvalidStructName(&'static str),

    /// # Enum name does not match input bytes
    ///
    /// An `enum` can only be deserialized from a header field element which shares the same name as
    /// it.
    InvalidEnumName(&'static str),

    /// Missing ':' field delimiter in a header line.
    MissingFieldDelimiter,

    /// Missing field name in a header line.
    MissingFieldName,

    /// Missing [`CRLF`] delimiter at the end of a header line.
    ///
    /// [`CRLF`]: crate::constants::CRLF
    MissingCRLFDelimiter,

    /// Unsupported byte in header field name.
    InvalidFieldName(char),

    /// # Tried to parse an empty header field value list.
    ///
    /// Unlike single field elements, lists cannot be empty. Even if they can contain _some_ empty
    /// elements, they must at least contain _one_ non-empty element to be considered valid.
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
    /// # Specs
    ///
    /// Header field quotation semantics are defined in [section 5.6.4] of [RFC9110], HTTP Semantics.
    ///
    /// [section 5.6.4]: https://www.rfc-editor.org/rfc/rfc9110.html#name-quoted-strings
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    LeftoverBytes,

    /// # A header field value parameter is missing its '=' delimiter.
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
    InvalidFieldValue(char),

    /// # Invalid byte in a header field parameter.
    ///
    /// Parsed bytes do not correspond to the legal set allowed in the specs.
    ///
    ///
    /// Validation rules only differ between parameter keys and values in that the latter can be
    /// quoted. Despite this, this error type is used for both key and value format errors and does
    /// not distinguish between either.
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
    InvalidFieldParameter(char),

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
    InvalidEscapeSequence(char),

    /// # Arithmetic conversion error
    ///
    /// This can happen as a result of parsing an integer which is too large for its containing type.
    Arithmetic(lexical_core::Error),

    /// # Invalid utf-8 encoding
    ///
    /// While utf-8 encoding is not enforced at the parsing step, deserializing to string types will
    /// still require the input be valid code points.
    Utf8Error(std::str::Utf8Error),

    /// [`serde`]-specific errors.
    Custom(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStructName(name) => {
                write!(f, "Invalid struct name, expected \"{name}\"")
            },
            Self::InvalidEnumName(name) => {
                write!(f, "Invalid enum name, expected \"{name}\"")
            },
            Self::MissingFieldDelimiter => {
                write!(f, "Missing ':' field delimiter in header line")
            },
            Self::MissingFieldName => {
                write!(f, "Missing field name in header line")
            },
            Self::MissingCRLFDelimiter => {
                write!(f, "Missing CRLF delimiter at the end of a header line")
            },
            Self::InvalidFieldName(c) => {
                write!(f, "Invalid byte '{c}' in header field name")
            },
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
            Self::Arithmetic(err) => {
                write!(f, "{err}")
            },
            Self::Utf8Error(err) => {
                write!(f, "{err}")
            },
            Self::Custom(err) => {
                write!(f, "{err}")
            },
        }
    }
}

impl serde::de::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl std::error::Error for Error {}
