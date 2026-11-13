//! Header line parsing utilities.
//!
//! This is intended as a lower-level API which is not meant for public consumption unless you know
//! what you are doing. Importantly, parsing at this level is implemented as zero-copy, which
//! prevents certain sanitization operations from taking place, such as [quoted-pair] escaping.
//!
//! Generally speaking, you are better off using the higher-level serde parsers provided by this
//! crate as they will handle any additional complexities automatically. I've kept this part
//! publicly available in case you find the rest of the API yields insufficient flexibility for
//! your specific use-case.
//!
//! [quoted-pair]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.4
use super::error;
use super::stack;

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
/// for res in line::HeaderLines::new(&mut input) {
///     let mut header = res?;
///     let values = headers.entry(header.name()).or_default();
///
///     // ...which themselves allow you to iterate over header field values
///     for res in header.values() {
///         let value = res?;
///         values.push(value);
///     }
/// }
///
/// assert_eq!(
///     headers.get(b"Content-Type" as &[u8]),
///     Some(&vec![line::Value::Element(b"text/plain")])
/// );
///
/// assert_eq!(
///     headers.get(b"Content-Length" as &[u8]),
///     Some(&vec![line::Value::Element(b"3")])
/// );
/// # Ok(())
/// # }
/// ```
///
/// Parsing is implemented as an iterator over each header field line which allows it to remain
/// allocation-free and zero-copy. This has the limitation that the underlying header segment bytes
/// cannot be modified, so certain sanitization operations, such as [quoted-pair] escaping or
/// cases-insensitive matching cannot take place at this stage.
///
/// Keep in mind that the parser will hand back **raw network bytes** which may or may not be safe
/// for consumption into a [`str`] or other types. Use with caution and make sure to perform your
/// own validation logic on top!
///
/// [section 5]: https://www.rfc-editor.org/rfc/rfc9110.html#name-fields
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
/// [quoted-pair]: https://www.rfc-editor.org/rfc/rfc9110.html#section-5.6.4
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderLines<'a> {
    input: Option<&'a mut [u8]>,
}

impl<'a> HeaderLines<'a> {
    /// Initializes a new parser with the input bytes to be processed.
    #[must_use]
    pub fn new(input: &'a mut [u8]) -> Self {
        Self { input: Some(input) }
    }
}

impl<'a> Iterator for HeaderLines<'a> {
    type Item = Result<HeaderItem<'a>, error::Error>;

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
                    Some(HeaderItem::new(line))
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
/// let mut header = line::HeaderItem::new(&mut input)?;
///
/// let field_name = header.name();
/// let field_value = header.values().next().unwrap()?;
///
/// assert_eq!(field_name, b"Content-Type");
/// assert_eq!(field_value, line::Value::Element(b"text/plain"));
/// # Ok(())
/// # }
/// ```
///
/// Similar caution should be exercised when using this low-level API as with [`HeaderLines`].
///
/// [section 5.2]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-lines-and-combined-fi
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
#[derive(Debug, PartialEq, Eq)]
pub struct HeaderItem<'a> {
    field_name: &'a [u8],
    field_value: HeaderValues<'a>,
}

impl<'a> HeaderItem<'a> {
    /// Initializes a new header field parser.
    ///
    /// This method is usually only called by [`HeaderLines`] during the parsing process but can
    /// also be manually invoked in the rare cases where it is necessary to parse only a _single_
    /// header field.
    ///
    /// # Panics
    ///
    /// If ever the input bytes end with a [`CRLF`]. This is guaranteed not to be the case when
    /// invoked by [`HeaderLines`] during parsing and must also be upheld on manual calls.
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

        Ok(Self {
            field_name,
            field_value,
        })
    }

    /// Returns the header's field name.
    #[must_use]
    pub fn name(&self) -> &'a [u8] {
        self.field_name
    }

    /// Returns an iterator over the header's [field value].
    ///
    /// [field value]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
    #[must_use]
    pub fn values(&mut self) -> &mut HeaderValues<'a> {
        &mut self.field_value
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
/// [`HeaderItem::values`].
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
#[derive(Debug, PartialEq, Eq)]
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
    /// let mut header = line::HeaderItem::new(&mut input)?;
    /// let mut values = header.values();
    ///
    /// assert_eq!(values.next(), Some(Ok(line::Value::Element(b"text/plain"))));
    /// # Ok(())
    /// # }
    /// ```
    Element(&'a [u8]),

    /// Dictionary-like field value, can also be part of a list.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// let mut input = *b"Content-Type: multipart/form-data; boundary=7MA4YWxkTrZu0gW";
    ///
    /// let mut header = line::HeaderItem::new(&mut input)?;
    /// let mut values = header.values();
    ///
    /// // Header parameters are still typically associated to a field value element
    /// assert_eq!(
    ///     values.next().unwrap()?,
    ///     line::Value::Element(b"multipart/form-data")
    /// );
    ///
    /// // Header parameters come in key-value pairs, separated by a single '=' delimiter
    /// assert_eq!(
    ///     values.next().unwrap()?,
    ///     line::Value::Parameter {
    ///         key: b"boundary",
    ///         val: b"7MA4YWxkTrZu0gW"
    ///     }
    /// );
    /// # Ok(())
    /// # }
    /// ```
    Parameter {
        /// Parameter key.
        key: &'a [u8],
        /// Parameter value.
        val: &'a [u8],
    },
}

impl<'a> HeaderValues<'a> {
    pub(crate) fn new(field_value: &'a mut [u8]) -> Self {
        let mut state = stack::Stack::new();
        state.push(HeaderSegment::Line(field_value));
        Self { state }
    }
}

impl<'a> Iterator for &mut HeaderValues<'a> {
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

                    let immutable = if element[0] == b'"' && element[element.len() - 1] == b'"' {
                        // Quoted non-empty elements are returned as-is
                        let immutable = &element[1..element.len() - 1];

                        if let Some(c) = validate::is_legal_header_quote(immutable) {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::InvalidEscapeSequence(c)));
                        }

                        immutable
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

                    if let Some(c) = validate::is_legal_header_element(immutable) {
                        self.state.clear();
                        return Some(Err(error::ErrorValue::InvalidFieldValue(c)));
                    }

                    return Some(Ok(Value::Element(immutable)));
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
                            let key = &param[..n];

                            if let Some(c) = validate::is_legal_header_parameter(key) {
                                self.state.clear();
                                return Some(Err(error::ErrorValue::InvalidFieldParameter(c)));
                            }

                            // Field parameter values may be quoted, but field parameter keys cannot.
                            let val = if param[n + 1] == b'"' {
                                let Some(closing_quote) = quote::closing_quote(param, n + 2) else {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::MissingClosingQuote));
                                };

                                if closing_quote != param.len() - 1 {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::LeftoverBytes));
                                }

                                // Value can once again be empty after unquoting
                                let val = &param[n + 2..closing_quote];
                                if val.is_empty() {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::MissingParameterValue));
                                }

                                if let Some(err) = validate::is_legal_header_parameter_quote(val) {
                                    self.state.clear();
                                    return Some(Err(err));
                                }

                                val
                            } else {
                                let val = &param[n + 1..];

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
                    Some(quoted_pair!()) => (),
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

    #[test]
    fn header_line_full() {
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1, b"text/plain");
        assert_eq!(v1.next(), None);

        let mut l2 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l2.name(), b"Content-Length");

        let mut v2 = l2.values();
        let Some(Ok(Value::Element(value2))) = v2.next() else {
            panic!();
        };

        assert_utf8_eq!(value2, b"3");
        assert_eq!(v2.next(), None);

        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_empty() {
        let mut input = *b"\r\n";
        let mut lines = HeaderLines::new(&mut input);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_err_missing_crlf_delimiter() {
        let mut input = *b"Content-Type: text/plain";
        let mut lines = HeaderLines::new(&mut input);
        assert_eq!(lines.next(), Some(Err(error::Error::MissingCRLFDelimiter)));
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_list() {
        let mut input = *b"Example-Dates: Sat 04 May 1996, Wed 14 Sep 2005\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"Sat 04 May 1996");

        let Some(Ok(Value::Element(value1_2))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_2, b"Wed 14 Sep 2005");

        assert_eq!(v1.next(), None);

        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_single() {
        let mut input = *b"Example-List: ,\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_multiple() {
        let mut input = *b"Example-List: ,   ,\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_quoted() {
        let mut input = *b"Example-List: \"\",  \"\"\t, \"\"   \r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_parameters_simple() {
        let mut input =
            *b"Content-Type: multipart/form-data; boundary=7MA4YWxkTrZu0gW; charset=utf-8\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"multipart/form-data");

        let (key1_2, val1_2) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_2, b"boundary");
        assert_utf8_eq!(val1_2, b"7MA4YWxkTrZu0gW");

        let (key1_3, val1_3) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_3, b"charset");
        assert_utf8_eq!(val1_3, b"utf-8");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_parameters_fuck_me() {
        let mut input = *b"Accept:    text/*;q=0.3, text/plain;q=0.7;; \t  ;, text/plain;   \
                             format=flowed,text/plain;   \tformat=fixed;q=0.4, */*  ;    \
                             q=0.5\t\t\t\t\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Accept");

        let mut v1 = l1.values();

        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"text/*");

        let (key1_2, val1_2) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_2, b"q");
        assert_utf8_eq!(val1_2, b"0.3");

        let Some(Ok(Value::Element(value1_3))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_3, b"text/plain");

        let (key1_4, val1_4) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_4, b"q");
        assert_utf8_eq!(val1_4, b"0.7");

        let Some(Ok(Value::Element(value1_5))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_5, b"text/plain");

        let (key1_6, val1_6) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_6, b"format");
        assert_utf8_eq!(val1_6, b"flowed");

        let Some(Ok(Value::Element(value1_7))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_7, b"text/plain");

        let (key1_8, val1_8) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_8, b"format");
        assert_utf8_eq!(val1_8, b"fixed");

        let (key1_9, val1_9) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_9, b"q");
        assert_utf8_eq!(val1_9, b"0.4");

        let Some(Ok(Value::Element(value1_10))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_10, b"*/*");

        let (key1_11, val1_11) = match v1.next() {
            Some(Ok(Value::Parameter { key, val })) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_11, b"q");
        assert_utf8_eq!(val1_11, b"0.5");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_delimiter() {
        let mut input = *b"Content-Type: ;boundary7MA4YWxkTrZu0gW\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(
            v1.next(),
            Some(Err(error::ErrorValue::MissingParameterDelimiter))
        );
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_key() {
        let mut input = *b"Content-Type: ;=7MA4YWxkTrZu0gW\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::MissingParameterKey)));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_value() {
        let mut input = *b"Content-Type: ;boundary=\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(
            v1.next(),
            Some(Err(error::ErrorValue::MissingParameterValue))
        );
    }

    #[test]
    fn header_line_quoted_element_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"Sat, 04 May 1996");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_element_empty() {
        let mut input = *b"Example-Dates: \"\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_list_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"Wed, 14 Sep 2005\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"Sat, 04 May 1996");

        let Some(Ok(Value::Element(value1_2))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_2, b"Wed, 14 Sep 2005");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_list_empty_element() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"Sat, 04 May 1996");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTrZu0gW\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"multipart/form-data");

        let Some(Ok(Value::Parameter {
            key: key1_2,
            val: val1_2,
        })) = v1.next()
        else {
            panic!();
        };

        assert_utf8_eq!(key1_2, b"boundary");
        assert_utf8_eq!(val1_2, b"7MA4YWxkTrZu0gW");

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_element() {
        let mut input = *b"Content-Type: \"multipart/form-data\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::MissingClosingQuote)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTrZu0gW\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"multipart/form-data");

        assert_eq!(v1.next(), Some(Err(error::ErrorValue::MissingClosingQuote)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_element() {
        let mut input = *b"Content-Type: \"multipart\"/form-data\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::LeftoverBytes)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTr\"Zu0gW\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"multipart/form-data");

        assert_eq!(v1.next(), Some(Err(error::ErrorValue::LeftoverBytes)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_missing_parameter_value() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"\"\r\n\r\n";
        let mut lines = HeaderLines::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        let Some(Ok(Value::Element(value1_1))) = v1.next() else {
            panic!();
        };

        assert_utf8_eq!(value1_1, b"multipart/form-data");

        assert_eq!(
            v1.next(),
            Some(Err(error::ErrorValue::MissingParameterValue))
        );

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_item_full() {
        let mut input = *b"Content-Type:text/plain";
        let mut header = HeaderItem::new(&mut input).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), Some(Ok(Value::Element(b"text/plain"))));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_trim_ows() {
        let mut input = *b"Content-Type: \t text/plain\t\t  \t";
        let mut header = HeaderItem::new(&mut input).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), Some(Ok(Value::Element(b"text/plain"))));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_empty_field_value_no_ows() {
        let mut input = *b"Content-Type:";
        let mut header = HeaderItem::new(&mut input).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_empty_field_value_ows() {
        let mut input = *b"Content-Type:  \t \t\t  \t";
        let mut header = HeaderItem::new(&mut input).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_err_empty_line() {
        let mut input = *b"";

        let expected = Err(error::Error::EmptyLine);

        assert_eq!(HeaderItem::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_missing_field_delimiter() {
        let mut input = *b"Content-Type text/plain";

        let expected = Err(error::Error::MissingFieldDelimiter);

        assert_eq!(HeaderItem::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_missing_field_name() {
        let mut input = *b": text/plain";

        let expected = Err(error::Error::MissingFieldName);

        assert_eq!(HeaderItem::new(&mut input), expected);
    }

    #[test]
    fn header_item_err_invalid_field_name() {
        let mut input = *b"Content=Type: text/plain";

        let expected = Err(error::Error::InvalidFieldName(b'='));

        assert_eq!(HeaderItem::new(&mut input), expected);
    }

    #[test]
    fn header_value_err_invalid_field_value_any() {
        let mut input = *b"Content-Type: text\0plain";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidFieldValue(b'\0')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_value_crlf() {
        let mut input = *b"Content-Type: text/plain\r\n";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidFieldValue(b'\r')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_value_start() {
        let mut input = *b"Content-Type: \" text/plain\"";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidFieldValue(b' ')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_key() {
        let mut input = *b"Content-Type: ; bo\\undary=7MA4YWxkTrZu0gW";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_val() {
        let mut input = *b"Content-Type: ; boundary=7MA4Y\\WxkTrZu0gW";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_field_element_quote_pair() {
        let mut input = *b"Content-Type: \"\\\"text/plain\\\"\"";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Ok(Value::Element(b"\\\"text/plain\\\"")))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_quote_element() {
        let mut input = *b"Content-Type: \"text\\\0/plain\"";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidEscapeSequence(b'\0')))
        );
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_value_err_invalid_field_quote_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"7MA4Y\\\0WxkTrZu0gW\"";
        let mut header = HeaderItem::new(&mut input).unwrap();
        let mut values = header.values();

        assert_eq!(
            values.next(),
            Some(Err(error::ErrorValue::InvalidEscapeSequence(b'\0')))
        );
        assert_eq!(values.next(), None);
    }
}
