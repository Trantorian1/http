//! Header line parsing utilities.
//!
//! This is intended as a lower-level API with higher flexibility: generally speaking, you are
//! better off using the higher level [`serde`] parsers provided by this crate.
use super::error;
use super::stack;

/// Helper macro for generating an [`Element`] header field [`Value`]. Handles static string conversion into a
/// mutable reference.
///
/// # Example
///
/// ```rust
/// use http_headers::prelude::*;
///
/// let mut val = *b"text/plain";
/// assert_eq!(Value::Element(&mut val), element!(b"text/plain"));
/// ```
///
/// [`Element`]: Value::Element
#[macro_export]
macro_rules! element {
    ($s:literal) => {
        $crate::line::Value::Element(&mut $s.map(|b| b))
    };
}

/// Helper macro for generating a [`Parameter`] header field [`Value`]. Handles static string
/// conversion into a mutable reference.
///
/// # Example
///
/// ```rust
/// use http_headers::prelude::*;
///
/// let mut key = *b"q";
/// let mut val = *b"0.3";
///
/// assert_eq!(
///     Value::Parameter {
///         key: &mut key,
///         val: &mut val
///     },
///     parameter!(b"q", b"0.3")
/// );
/// ```
///
/// [`Parameter`]: Value::Parameter
#[macro_export]
macro_rules! parameter {
    ($k:literal, $v:literal) => {
        $crate::line::Value::Parameter {
            key: &mut $k.map(|b| b),
            val: &mut $v.map(|b| b),
        }
    };
}

/// Header section parser.
///
/// # Specs
///
/// See the HTTP field format as described in [section 5] of [RFC9110], HTTP Semantics.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), http_headers::Error> {
/// use http_headers::prelude::*;
///
/// let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
///
/// // While parsing is allocation-free your are free to use your own method
/// // of collecting the results.
/// let mut headers = std::collections::HashMap::<_, Vec<_>>::new();
///
/// // Parsing is implemented as an iterator over header field lines...
/// for res in Headers::new(&mut input) {
///     let header = res?;
///     let values = headers.entry(header.name).or_default();
///
///     // ...which themselves allow you to iterate over header field values
///     for res in header.values {
///         let value = res?;
///         values.push(value);
///     }
/// }
///
/// assert_eq!(
///     headers.get(b"content-type" as &[u8]),
///     Some(&vec![element!(b"text/plain")])
/// );
///
/// assert_eq!(
///     headers.get(b"content-length" as &[u8]),
///     Some(&vec![element!(b"3")])
/// );
/// # Ok(())
/// # }
/// ```
///
/// Parsing is implemented as an iterator over each header field line which allows it to remain
/// allocation-free and zero-copy, though some minimal mutation of the underlying buffer might still
/// take place for sanitization purposes, such as header field name lower casing and quote pair
/// decoding.
///
/// Keep in mind that the parser will hand back **raw network bytes** which may or may not be safe
/// for consumption into a [`str`] or other types. Use with caution and make sure to perform your
/// own validation logic on top!
///
/// [section 5]: https://www.rfc-editor.org/rfc/rfc9110.html#name-fields
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
#[derive(Debug, PartialEq, Eq)]
pub struct Headers<'a> {
    input: Option<&'a mut [u8]>,
}

impl<'a> Headers<'a> {
    /// Initializes a new parser with the input bytes to be processed.
    #[must_use]
    pub fn new(input: &'a mut [u8]) -> Self {
        Self { input: Some(input) }
    }
}

impl<'a> Iterator for Headers<'a> {
    type Item = Result<HeaderLine<'a>, error::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.input.take()? {
            // End of header section
            [b'\r', b'\n'] => None,

            input => match memchr::memmem::find(input, crate::constants::CRLF) {
                // Invalid header line end
                None => Some(Err(error::Error::MissingCRLFDelimiter)),

                // Extract and parse header line
                Some(n) => {
                    let (line, remaining) = input.split_at_mut(n);
                    self.input = Some(&mut remaining[crate::constants::CRLF.len()..]);
                    Some(HeaderLine::new(line))
                },
            },
        }
    }
}

/// Single HTTP header field parser.
///
/// # Specs
///
/// See the HTTP field line format as described in [section 5.2], Field Lines and Combined Field
/// Values of [RFC9110], HTTP Semantics.
///
/// # Examples
///
/// ```rust
/// # fn main() -> Result<(), http_headers::Error> {
/// use http_headers::prelude::*;
///
/// let mut input = *b"Content-Type: text/plain";
///
/// let mut header = HeaderLine::new(&mut input)?;
/// let mut values = header.values;
///
/// assert_eq!(header.name, b"content-type");
/// assert_eq!(values.next(), Some(Ok(element!(b"text/plain"))));
/// # Ok(())
/// # }
/// ```
///
/// Similar caution should be exercised when using this low-level API as with [`Headers`].
///
/// [section 5.2]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-lines-and-combined-fi
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderLine<'a> {
    /// Header field name, **will always be in lower case**.
    ///
    /// This is considered case-insensitive according to [section 5.1] of [RFC9110].
    ///
    /// [section 5.1]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-names
    /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
    pub name: &'a mut [u8],

    /// Header field [`Value`]s.
    pub values: HeaderValues<'a>,
}

impl<'a> HeaderLine<'a> {
    /// Initializes a new header field parser.
    ///
    /// This method is usually only called by [`Headers`] during the parsing process but can
    /// also be manually invoked in the rare cases where it is necessary to parse only a _single_
    /// header field.
    ///
    /// # Panics
    ///
    /// If ever the input bytes end with a [`CRLF`]. This is guaranteed not to be the case when
    /// invoked by [`Headers`] during parsing and must also be upheld on manual calls.
    ///
    /// # Errors
    ///
    /// For malformed inputs, this method will return [`MissingFieldDelimiter`], [`MissingFieldName`]
    /// or [`InvalidFieldName`] depending on the error type.
    ///
    /// [`CRLF`]: crate::constants::CRLF
    /// [`MissingFieldDelimiter`]: error::Error::MissingFieldDelimiter
    /// [`MissingFieldName`]: error::Error::MissingFieldName
    /// [`InvalidFieldName`]: error::Error::InvalidFieldName
    pub fn new(line: &'a mut [u8]) -> Result<Self, error::Error> {
        if line.is_empty() {
            return Err(error::Error::EmptyLine);
        }

        let field_name_delimiter = match memchr::memchr(b':', line) {
            None => return Err(error::Error::MissingFieldDelimiter),
            Some(0) => return Err(error::Error::MissingFieldName),
            Some(n) => n,
        };

        let (field_name, remaining) = line.split_at_mut(field_name_delimiter);

        if let Some(c) = validate::is_legal_header_name(field_name) {
            return Err(error::Error::InvalidFieldName(c));
        }

        let field_value = HeaderValues::new(&mut remaining[1..]);
        field_name.make_ascii_lowercase();

        Ok(Self {
            name: field_name,
            values: field_value,
        })
    }
}

/// HTTP header field value parser.
///
/// # Specs
///
/// See the HTTP field value format as described in [section 5.5], Field Values of [RFC9110], HTTP
/// Semantics.
///
/// # Usage
///
/// This parser is not meant to be constructed manually and is instead only returned by
/// [`HeaderLine::values`].
///
/// [section 5.5]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderValues<'a> {
    state: stack::Stack<HeaderSegment<'a>, 4>,
}

#[derive(Debug, PartialEq, Eq)]
enum HeaderSegment<'a> {
    Line(&'a mut [u8]),
    List(&'a mut [u8], usize),
    Element(&'a mut [u8]),
    Parameter(&'a mut [u8]),
}

/// A single HTTP header field value.
#[derive(PartialEq, Eq)]
pub enum Value<'a> {
    /// Single element field value, can be part of a list.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: text/plain";
    ///
    /// let mut header = HeaderLine::new(&mut input)?;
    /// let mut values = header.values;
    ///
    /// assert_eq!(values.next(), Some(Ok(element!(b"text/plain"))));
    /// # Ok(())
    /// # }
    /// ```
    Element(&'a mut [u8]),

    /// Dictionary-like field value, can also be part of a list.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: multipart/form-data; boundary=7MA4YWxk";
    ///
    /// let mut header = HeaderLine::new(&mut input)?;
    /// let mut values = header.values;
    ///
    /// // Header parameters are still typically associated to a field value element
    /// assert_eq!(values.next(), Some(Ok(element!(b"multipart/form-data"))));
    ///
    /// // Header parameters come in key-value pairs, separated by a single '=' delimiter
    /// assert_eq!(
    ///     values.next(),
    ///     Some(Ok(parameter!(b"boundary", b"7MA4YWxk")))
    /// );
    /// # Ok(())
    /// # }
    /// ```
    Parameter {
        /// Parameter key, **will always be in lower case.**
        ///
        /// This is considered case-insensitive according to [section 5.6.6] of [RFC9110].
        ///
        /// [section 5.1]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
        /// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
        key: &'a mut [u8],
        /// Parameter value.
        val: &'a mut [u8],
    },
}

impl std::fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Element(val) => f
                .debug_tuple("Element")
                .field(&std::str::from_utf8(val).unwrap_or_default())
                .finish(),
            Self::Parameter { key, val } => f
                .debug_struct("Parameter")
                .field("key", &std::str::from_utf8(key).unwrap_or_default())
                .field("val", &std::str::from_utf8(val).unwrap_or_default())
                .finish(),
        }
    }
}

impl<'a> HeaderValues<'a> {
    pub(crate) fn new(field_value: &'a mut [u8]) -> Self {
        let mut state = stack::Stack::new();
        state.push(HeaderSegment::Line(field_value));
        Self { state }
    }
}

impl<'a> Iterator for HeaderValues<'a> {
    type Item = Result<Value<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state.pop()? {
                HeaderSegment::Line(line) => {
                    if memchr::memchr(b',', line).is_some() {
                        let list = ows::skip_ows_leading(line);
                        self.state.push(HeaderSegment::List(list, 0));
                    } else {
                        let element = ows::skip_ows_all(line);

                        // The element state itself does not perform any quote checks due to how
                        // specific to each parsing state those are. Here for example we don't have
                        // to worry about list separators, which is not the case in the list state.
                        if !element.is_empty() && element[0] == b'"' {
                            let Some(closing_quote) = quote::closing_quote(element, 1) else {
                                self.state.clear();
                                return Some(Err(error::ErrorValue::MissingClosingQuote));
                            };

                            let leftover = ows::skip_ows_leading(&mut element[closing_quote + 1..]);
                            if !leftover.is_empty() {
                                self.state.clear();
                                return Some(Err(error::ErrorValue::LeftoverBytes));
                            }
                        }

                        self.state.push(HeaderSegment::Element(element));
                    }
                },

                HeaderSegment::List(mut list, mut size) => {
                    // Field elements can be quoted and may contain ',' delimiters, in which case
                    // those delimiters are considered as escaped and don't contribute to field
                    // value splitting.
                    let element = if list[0] == b'"' {
                        let Some(closing_quote) = quote::closing_quote(list, 1) else {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingClosingQuote));
                        };

                        // Look for the next list field delimiter AFTER the quoted segment
                        let unquoted = closing_quote + 1;

                        // CAUTION: `delimiter_next` is indexed relative to `list`, however `skip`
                        // is indexed relative to `&list[unquoted..]`
                        let (delimiter_next, skip) = match memchr::memchr(b',', &list[unquoted..]) {
                            Some(n) => (unquoted + n, n + 1),
                            None => (list.len(), 0),
                        };

                        // We can't guarantee zero-copy and support mixed quoted and unquoted field
                        // values. Besides which that is a really stupid idea anyway.
                        let leftover = ows::skip_ows_leading(&mut list[unquoted..delimiter_next]);
                        if !leftover.is_empty() {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::LeftoverBytes));
                        }

                        let (element, remaining) = list.split_at_mut(unquoted);
                        list = &mut remaining[skip..];

                        element
                    } else {
                        // CAUTION: `delimiter_next` is indexed relative to `list`, however `skip`
                        // is indexed relative to `&list[delimiter_next..]`
                        let (delimiter_next, skip) = match memchr::memchr(b',', list) {
                            Some(n) => (n, 1),
                            None => (list.len(), 0),
                        };

                        let (element, remaining) = list.split_at_mut(delimiter_next);
                        list = &mut remaining[skip..];

                        element
                    };

                    list = ows::skip_ows_leading(list);

                    if !element.is_empty() && element[0] != b'"' || element.len() > 2 {
                        // Element is non-empty nor quoted empty
                        size += 1;

                        if !list.is_empty() {
                            self.state.push(HeaderSegment::List(list, size));
                        }

                        self.state.push(HeaderSegment::Element(element));
                    } else if list.is_empty() {
                        // No more bytes left to consume, check if list was non-empty
                        if size == 0 {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::EmptyList));
                        }
                    } else {
                        // Still some bytes left, put the list state back on top of the stack
                        self.state.push(HeaderSegment::List(list, size));
                    }
                },

                HeaderSegment::Element(element) => {
                    // Empty element
                    if element.is_empty() {
                        return None;
                    }

                    // Empty quoted element
                    if element[0] == b'"' && element.len() >= 2 && element[1] == b'"' {
                        return None;
                    }

                    let value = if element[0] == b'"' && element[element.len() - 1] == b'"' {
                        let len = element.len();
                        let value = &mut element[1..len - 1];

                        if let Some(c) = validate::is_legal_header_quote(value) {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::InvalidEscapeSequence(c)));
                        }

                        let unpaired = quote::decode_quoted_pairs(value);
                        &mut value[..unpaired]
                    } else {
                        // Unquoted elements may still contain field parameters
                        let mut parameters = memchr::memchr_iter(b';', element);
                        match parameters.next() {
                            // Element IS a field parameter
                            Some(0) => {
                                let delimiter_next = match parameters.next() {
                                    Some(n) => n,
                                    None => element.len(),
                                };

                                let (mut param, remaining) = element.split_at_mut(delimiter_next);
                                param = ows::skip_ows_all(&mut param[1..]);

                                if !remaining.is_empty() {
                                    self.state.push(HeaderSegment::Element(remaining));
                                }

                                if !param.is_empty() {
                                    self.state.push(HeaderSegment::Parameter(param));
                                }

                                continue;
                            },

                            // There are some bytes left to consume before the next field parameter
                            Some(n) => {
                                let (value, remaining) = element.split_at_mut(n);

                                if !remaining.is_empty() {
                                    self.state.push(HeaderSegment::Element(remaining));
                                }

                                ows::skip_ows_trailing(value)
                            },

                            // Element does not contain any field parameters and can be consumed in
                            // its entirety.
                            None => element,
                        }
                    };

                    if let Some(c) = validate::is_legal_header_element(value) {
                        self.state.clear();
                        return Some(Err(error::ErrorValue::InvalidFieldValue(c)));
                    }

                    return Some(Ok(Value::Element(value)));
                },

                HeaderSegment::Parameter(param) => {
                    match memchr::memchr(b'=', param) {
                        None => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterDelimiter));
                        },
                        Some(0) => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterKey));
                        },
                        Some(n) if n + 1 >= param.len() => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterValue));
                        },
                        Some(n) => {
                            let (key, remaining) = param.split_at_mut(n);
                            key.make_ascii_lowercase();

                            if let Some(c) = validate::is_legal_header_parameter(key) {
                                self.state.clear();
                                return Some(Err(error::ErrorValue::InvalidFieldParameter(c)));
                            }

                            // Field parameter values may be quoted, but field parameter keys cannot.
                            let val = if remaining[1] == b'"' {
                                let Some(closing_quote) = quote::closing_quote(remaining, 2) else {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::MissingClosingQuote));
                                };

                                if closing_quote != remaining.len() - 1 {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::LeftoverBytes));
                                }

                                // Value can once again be empty after unquoting
                                let val = &mut remaining[2..closing_quote];
                                if val.is_empty() {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::MissingParameterValue));
                                }

                                if let Some(err) = validate::is_legal_header_parameter_quote(val) {
                                    self.state.clear();
                                    return Some(Err(err));
                                }

                                let unpaired = quote::decode_quoted_pairs(val);
                                &mut val[..unpaired]
                            } else {
                                let val = &mut remaining[1..];

                                if let Some(c) = validate::is_legal_header_parameter(val) {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::InvalidFieldParameter(c)));
                                }

                                val
                            };

                            return Some(Ok(Value::Parameter { key, val }));
                        },
                    };
                },
            }
        }
    }
}

mod ows {
    #[inline]
    pub(super) fn skip_ows_all(bytes: &mut [u8]) -> &mut [u8] {
        skip_ows_trailing(skip_ows_leading(bytes))
    }

    #[inline]
    pub(super) fn skip_ows_leading(bytes: &mut [u8]) -> &mut [u8] {
        let mut start = 0;
        while start < bytes.len() && crate::constants::OWS.contains(&bytes[start]) {
            start += 1;
        }

        &mut bytes[start..]
    }

    #[inline]
    pub(super) fn skip_ows_trailing(bytes: &mut [u8]) -> &mut [u8] {
        let mut stop = bytes.len();
        while stop > 0 && crate::constants::OWS.contains(&bytes[stop - 1]) {
            stop -= 1;
        }

        &mut bytes[..stop]
    }
}

mod quote {
    pub(super) fn closing_quote(bytes: &[u8], offset: usize) -> Option<usize> {
        let quotes = memchr::memchr_iter(b'"', &bytes[offset..]);
        for quote in quotes {
            if bytes[offset - 1 + quote] != b'\\' {
                return Some(quote + offset);
            }
        }

        None
    }

    pub(super) fn decode_quoted_pairs(bytes: &mut [u8]) -> usize {
        let mut from = 0;
        let mut len = bytes.len();

        while let Some(n) = memchr::memchr(b'\\', &bytes[from..]) {
            bytes.copy_within(from + n + 1..len, from + n);
            from += n + 1;
            len -= 1;
        }

        len
    }
}

/// Header field validation logic.
mod validate {
    use super::error;

    macro_rules! vchar {
        () => {
            0x21..=0x7E
        };
    }

    macro_rules! obs_text {
        () => {
            0x80..=0xFF
        };
    }

    macro_rules! field_vchar {
        () => {
            vchar!() | obs_text!()
        };
    }

    macro_rules! quoted_pair {
        () => {
            b'\t' | b' ' | vchar!() | obs_text!()
        };
    }

    macro_rules! tchar {
        () => {
            b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
            | b'0'..=b'9'
            | b'a'..=b'z'
            | b'A'..=b'Z'
        };
    }

    /// HTTP header field names can be made up of any visible ASCII character (VCHAR), except for
    /// delimiters (DQUOTE and "(),/:;<=>?@[\]{}").
    ///
    /// # ABNF
    ///
    /// ```text
    /// field-name     = token
    ///
    /// token          = 1*tchar
    ///
    /// tchar          = "!" / "#" / "$" / "%" / "&" / "'" / "*"
    ///                / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
    ///                / DIGIT / ALPHA
    ///                ; any VCHAR, except delimiters
    /// ```
    ///
    /// See [RFC9110] for more information.
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#section-5.6.2
    pub(super) fn is_legal_header_name(bytes: &[u8]) -> Option<u8> {
        for c in bytes {
            if !matches!(c, tchar!(),) {
                return Some(*c);
            }
        }

        None
    }

    /// HTTP header single field value elements cannot start nor end with any OWS, even when quoted,
    /// and their inner bodies can be any combination of OWS and VCHAR.
    ///
    /// # ABNF
    ///
    /// ```text
    /// field-value    = *field-content
    /// field-content  = field-vchar
    ///                  [ 1*( SP / HTAB / field-vchar ) field-vchar ]
    /// field-vchar    = VCHAR / obs-text
    /// obs-text       = %x80-FF
    /// ```
    pub(super) fn is_legal_header_element(bytes: &[u8]) -> Option<u8> {
        assert_ne!(bytes, &[]);

        if !matches!(bytes[0], field_vchar!()) {
            return Some(bytes[0]);
        }

        if bytes.len() > 1 {
            for c in &bytes[1..bytes.len() - 1] {
                #[allow(clippy::unnested_or_patterns)]
                if !matches!(c, field_vchar!() | b' ' | b'\t') {
                    return Some(*c);
                }
            }
        }

        if !matches!(bytes[bytes.len() - 1], field_vchar!()) {
            return Some(bytes[bytes.len() - 1]);
        }

        None
    }

    /// Checks for invalid quoted pairs.
    ///
    /// Since quotes are then checked against a field value element, we don't check the validity of
    /// each individual character here since that will be done by [`is_legal_header_element`].
    pub(super) fn is_legal_header_quote(bytes: &[u8]) -> Option<u8> {
        let mut iter = bytes.iter();
        while let Some(c) = iter.next() {
            if c == &b'\\' {
                match iter.next() {
                    Some(quoted_pair!()) => (),
                    Some(next) => return Some(*next),
                    None => unreachable!(
                        "empty quoted pairs are not possible as the parser does not count \\\" as \
                         a quote delimiter"
                    ),
                }
            }
        }

        None
    }

    /// Header field value parameter keys and values can be composed of any combination of `tchar`s
    /// (that is to say, any visible ASCII character except for delimiters), as long as they are non-empty.
    pub(super) fn is_legal_header_parameter(bytes: &[u8]) -> Option<u8> {
        assert_ne!(bytes, []);

        for c in bytes {
            if !matches!(c, tchar!()) {
                return Some(*c);
            }
        }

        None
    }

    /// '\' is not a legal `tchar`, so we have to fuse field parameter checks and quote pair checks
    /// into a single function so they don't interfere with each other, otherwise this would cause a
    /// parsing error as `\` is not normally a valid character in a header parameter.
    pub(super) fn is_legal_header_parameter_quote(bytes: &[u8]) -> Option<error::ErrorValue> {
        let mut iter = bytes.iter();
        while let Some(c) = iter.next() {
            if c == &b'\\' {
                match iter.next() {
                    Some(quoted_pair!()) => continue,
                    Some(next) => return Some(error::ErrorValue::InvalidEscapeSequence(*next)),
                    None => unreachable!(
                        "empty quoted pairs are not possible as the parser does not count \\\" as \
                         a quote delimiter"
                    ),
                }
            }

            if !matches!(c, tchar!()) {
                return Some(error::ErrorValue::InvalidFieldParameter(*c));
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use macro_util::*;

    use super::*;

    macro_rules! assert_next {
        ($values:expr, $expr:expr) => {
            assert_eq!($values.next(), $expr);
        };
    }

    #[test]
    fn quoted_pair_decode() {
        let mut input = *b"\\\"el\\ement\\\"";
        let n = quote::decode_quoted_pairs(&mut input);

        assert_eq!(n, input.len() - 3);
        assert_utf8_eq!(&input[..n], b"\"element\"");
    }

    #[test]
    fn header_line_full() {
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_utf8_eq!(l1.name, b"content-type");
        assert_next!(l1.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(l1.values, None);

        let mut l2 = lines.next().unwrap().unwrap();

        assert_utf8_eq!(l2.name, b"content-length");
        assert_next!(l2.values, Some(Ok(element!(b"3"))));
        assert_next!(l2.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_empty() {
        let mut input = *b"\r\n";
        let mut lines = Headers::new(&mut input);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_err_missing_crlf_delimiter() {
        let mut input = *b"Content-Type: text/plain";
        let mut lines = Headers::new(&mut input);

        assert_next!(lines, Some(Err(error::Error::MissingCRLFDelimiter)));
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_list() {
        let mut input = *b"Example-Dates: Sat 04 May 1996, Wed 14 Sep 2005\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_utf8_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(element!(b"Sat 04 May 1996"))));
        assert_next!(l1.values, Some(Ok(element!(b"Wed 14 Sep 2005"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_single() {
        let mut input = *b"Example-List: ,\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_utf8_eq!(l1.name, b"example-list");
        assert_next!(l1.values, Some(Err(error::ErrorValue::EmptyList)));
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_multiple() {
        let mut input = *b"Example-List: ,   ,\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-list");

        assert_next!(l1.values, Some(Err(error::ErrorValue::EmptyList)));
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_quoted() {
        let mut input = *b"Example-List: \"\",  \"\"\t, \"\"   \r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-list");

        assert_next!(l1.values, Some(Err(error::ErrorValue::EmptyList)));
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_parameters_simple() {
        let mut input =
            *b"Content-Type: multipart/form-data; boundary=7MA4YWxk; charset=utf-8\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(element!(b"multipart/form-data"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"boundary", b"7MA4YWxk"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"charset", b"utf-8"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_fuck_me() {
        let mut input = *b"Accept:    text/*;q=0.3, text/plain;Q=0.7;; \t  ;, text/plain;   \
                             fOrMat=flowed,text/plain;   \tformat=fixed;q=0.4, */*  ;    \
                             q=0.5\t\t\t\t\r\n\r\n";

        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"accept");

        assert_next!(l1.values, Some(Ok(element!(b"text/*"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"q", b"0.3"))));

        assert_next!(l1.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"q", b"0.7"))));

        assert_next!(l1.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"format", b"flowed"))));

        assert_next!(l1.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"format", b"fixed"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"q", b"0.4"))));

        assert_next!(l1.values, Some(Ok(element!(b"*/*"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"q", b"0.5"))));

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_delimiter() {
        let mut input = *b"Content-Type: ;boundary7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(
            l1.values,
            Some(Err(error::ErrorValue::MissingParameterDelimiter))
        );
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_key() {
        let mut input = *b"Content-Type: ;=7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Err(error::ErrorValue::MissingParameterKey)));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_value() {
        let mut input = *b"Content-Type: ;boundary=\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(
            l1.values,
            Some(Err(error::ErrorValue::MissingParameterValue))
        );
    }

    #[test]
    fn header_line_quoted_element_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(element!(b"Sat, 04 May 1996"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_element_empty() {
        let mut input = *b"Example-Dates: \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_list_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"Wed, 14 Sep 2005\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(element!(b"Sat, 04 May 1996"))));
        assert_next!(l1.values, Some(Ok(element!(b"Wed, 14 Sep 2005"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_list_empty_element() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(element!(b"Sat, 04 May 1996"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(element!(b"multipart/form-data"))));
        assert_next!(l1.values, Some(Ok(parameter!(b"boundary", b"7MA4YWxk"))));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_element() {
        let mut input = *b"Content-Type: \"multipart/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Err(error::ErrorValue::MissingClosingQuote)));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(element!(b"multipart/form-data"))));
        assert_next!(l1.values, Some(Err(error::ErrorValue::MissingClosingQuote)));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_element() {
        let mut input = *b"Content-Type: \"multipart\"/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Err(error::ErrorValue::LeftoverBytes)));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTr\"Zu0gW\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(element!(b"multipart/form-data"))));
        assert_next!(l1.values, Some(Err(error::ErrorValue::LeftoverBytes)));
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_parameter_value() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(element!(b"multipart/form-data"))));
        assert_next!(
            l1.values,
            Some(Err(error::ErrorValue::MissingParameterValue))
        );
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_item_full() {
        let mut input = *b"Content-Type:text/plain";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_utf8_eq!(header.name, b"content-type");

        assert_next!(header.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_trim_ows() {
        let mut input = *b"Content-Type: \t text/plain\t\t  \t";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_utf8_eq!(header.name, b"content-type");

        assert_next!(header.values, Some(Ok(element!(b"text/plain"))));
        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_empty_field_value_no_ows() {
        let mut input = *b"Content-Type:";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_utf8_eq!(header.name, b"content-type");
        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_empty_field_value_ows() {
        let mut input = *b"Content-Type:  \t \t\t  \t";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_utf8_eq!(header.name, b"content-type");
        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_err_empty_line() {
        let mut input = *b"";

        let expected = Err(error::Error::EmptyLine);

        assert_eq!(HeaderLine::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_missing_field_delimiter() {
        let mut input = *b"Content-Type text/plain";

        let expected = Err(error::Error::MissingFieldDelimiter);

        assert_eq!(HeaderLine::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_missing_field_name() {
        let mut input = *b": text/plain";

        let expected = Err(error::Error::MissingFieldName);

        assert_eq!(HeaderLine::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_invalid_field_name() {
        let mut input = *b"Content=Type: text/plain";

        let expected = Err(error::Error::InvalidFieldName(b'='));

        assert_eq!(HeaderLine::new(&mut input), expected);
    }

    #[test]
    fn header_value_err_invalid_field_value_any() {
        let mut input = *b"Content-Type: text\0plain";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidFieldValue(b'\0')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_value_crlf() {
        let mut input = *b"Content-Type: text/plain\r\n";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidFieldValue(b'\r')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_value_start() {
        let mut input = *b"Content-Type: \" text/plain\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_eq!(
            header.values.next(),
            Some(Err(error::ErrorValue::InvalidFieldValue(b' ')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_key() {
        let mut input = *b"Content-Type: ; bo\\undary=7MA4YWxk";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_val() {
        let mut input = *b"Content-Type: ; boundary=7MA4Y\\WxkTrZu0gW";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_quoted_pair_element() {
        let mut input = *b"Content-Type: \"\\\"text/plain\\\"\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(header.values, Some(Ok(element!(b"\"text/plain\""))));
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_quoted_pair_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"\\\"\\7MA4Y\\Wx\\kTrZu0gW\\\"\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Ok(parameter!(b"boundary", b"\"7MA4YWxkTrZu0gW\"")))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_quote_element() {
        let mut input = *b"Content-Type: \"text\\\0/plain\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidEscapeSequence(b'\0')))
        );
        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_quote_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"7MA4Y\\\0WxkTrZu0gW\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidEscapeSequence(b'\0')))
        );
        assert_next!(header.values, None);
    }
}
