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

use macro_util::*;

use super::error;

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
/// const HEADERS: &str = "Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";
///
/// // While parsing is allocation-free your are free to use your own method
/// // of collecting the results.
/// let mut headers = std::collections::HashMap::<_, Vec<_>>::new();
///
/// // Parsing is implemented as an iterator over header field lines...
/// for res in line::HeaderLines::new(HEADERS.as_bytes()) {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderLines<'a> {
    input: &'a [u8],
}

impl<'a> HeaderLines<'a> {
    /// Initializes a new parser with the input bytes to be processed.
    #[must_use]
    pub fn new(input: &'a [u8]) -> Self {
        Self { input }
    }
}

impl<'a> Iterator for HeaderLines<'a> {
    type Item = Result<HeaderItem<'a>, error::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.input {
            // End of header section
            [b'\r', b'\n', ..] => None,

            _ => match memchr::memmem::find(self.input, crate::constants::CRLF) {
                // Invalid header line end
                None => {
                    self.input = b"\r\n";
                    Some(Err(error::Error::MissingCRLFDelimiter))
                },

                // Extract and parse header line
                Some(n) => {
                    let line = &self.input[..n];
                    self.input = &self.input[n + crate::constants::CRLF.len()..];
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
/// const HEADER: &str = "Content-Type: text/plain";
///
/// let mut header = line::HeaderItem::new(HEADER.as_bytes())?;
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub fn new(line: &'a [u8]) -> Result<Self, error::Error> {
        assert_eq!(
            memchr::memmem::rfind(line, crate::constants::CRLF),
            None,
            "HeaderItem assumes CRLF sanitization has already taken place"
        );

        if line.is_empty() {
            return Err(error::Error::EmptyLine);
        }

        let field_name_delimiter = match memchr::memchr(b':', line) {
            None => return Err(error::Error::MissingFieldDelimiter),
            Some(0) => return Err(error::Error::MissingFieldName),
            Some(n) => n,
        };

        let field_name = &line[..field_name_delimiter];

        if let Some(c) = validate::is_legal_header_name(field_name) {
            return Err(error::Error::InvalidFieldName(c));
        }

        let field_value = HeaderValues::new(&line[field_name_delimiter + 1..]);

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderValues<'a> {
    state: ParsingStack<'a, 4>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HeaderSegment<'a> {
    Line(&'a [u8]),
    List(&'a [u8], usize),
    Element(&'a [u8]),
    Parameter(&'a [u8]),
}

/// A single HTTP header field value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value<'a> {
    /// Single element field value, can be part of a list.
    ///
    /// # Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), http_headers::Error> {
    /// use http_headers::prelude::*;
    ///
    /// const HEADER: &str = "Content-Type: text/plain";
    ///
    /// let mut header = line::HeaderItem::new(HEADER.as_bytes())?;
    /// let field_value = header.values().next().unwrap()?;
    ///
    /// assert_eq!(field_value, line::Value::Element(b"text/plain"));
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
    /// const HEADER: &str = "Content-Type: multipart/form-data; boundary=7MA4YWxkTrZu0gW";
    ///
    /// let mut header = line::HeaderItem::new(HEADER.as_bytes())?;
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
    pub(crate) fn new(field_value: &'a [u8]) -> Self {
        let mut state = ParsingStack::new();
        state.push(HeaderSegment::Line(field_value));
        Self { state }
    }
}

impl<'a> Iterator for &mut HeaderValues<'a> {
    type Item = Result<Value<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.state.peek()? {
                HeaderSegment::Line(line) => {
                    if memchr::memchr(b',', line).is_some() {
                        let list = &line[..];

                        self.state.pop();
                        self.state.push(HeaderSegment::List(list, 0));
                    } else {
                        let element = ows::skip_ows_all(&line[..]);

                        self.state.pop();
                        self.state.push(HeaderSegment::Element(element));
                    }
                },

                HeaderSegment::List(list, size) => {
                    *list = ows::skip_ows_leading(list);

                    if list.is_empty() {
                        if *size == 0 {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::EmptyList));
                        }
                        self.state.pop();
                        return None;
                    }

                    // Field elements can be quoted and may contain ',' delimiters, in which case
                    // those delimiters are considered as escaped and don't contribute to field
                    // value splitting.
                    let element = if list[0] == b'"' {
                        let mut quote_next = None;

                        // Search for the next non-escaped quote delimiter
                        let offset = 1;
                        let quotes = memchr::memchr_iter(b'"', &list[offset..]);
                        for quote in quotes {
                            if list[offset - 1 + quote] != b'\\' {
                                quote_next = Some(quote);
                                break;
                            }
                        }

                        let closing_quote = match quote_next {
                            Some(n) => n,
                            None => {
                                self.state.clear();
                                return Some(Err(error::ErrorValue::MissingClosingQuote));
                            },
                        };

                        // Look for the next list field delimiter AFTER the quoted segment
                        let unquoted = offset + closing_quote + 1;
                        let (delimiter_next, skip) = match memchr::memchr(b',', &list[unquoted..]) {
                            Some(n) => (unquoted + n, unquoted + n + 1),
                            None => (list.len(), list.len()),
                        };

                        // We can't guarantee zero-copy and support mixed quoted and unquoted field
                        // values. Besides which that is a really stupid idea anyway.
                        let leftover = &list[unquoted..delimiter_next];
                        if !ows::skip_ows_leading(leftover).is_empty() {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::LeftoverBytes));
                        }

                        let element = &list[..=offset + closing_quote];
                        *list = &list[skip..];

                        element
                    } else {
                        let (delimiter_next, skip) = match memchr::memchr(b',', list) {
                            Some(n) => (n, n + 1),
                            None => (list.len(), list.len()),
                        };

                        let element = &list[..delimiter_next];
                        *list = &list[skip..];

                        element
                    };

                    // Element is non-empty
                    if !element.is_empty() {
                        // Element is not quoted empty
                        if element[0] != b'"' || element.len() > 2 {
                            *size += 1;
                            self.state.push(HeaderSegment::Element(element));
                        }
                    }
                },

                HeaderSegment::Element(element) => {
                    // Empty element
                    if element.is_empty() {
                        self.state.pop();
                        return None;
                    }

                    // Empty quoted element
                    if element[0] == b'"' && element.len() >= 2 && element[1] == b'"' {
                        self.state.pop();
                        return None;
                    }

                    // Quoted non-empty elements are returned as-is
                    // FIXME: check for illegal characters
                    if element[0] == b'"' && element[element.len() - 1] == b'"' {
                        let immutable = *element;
                        self.state.pop();
                        return Some(Ok(Value::Element(&immutable[1..immutable.len() - 1])));
                    }

                    // Unquoted elements may still contain field parameters
                    let mut parameters = memchr::memchr_iter(b';', element);
                    match parameters.next() {
                        // Element IS a field parameter
                        Some(0) => {
                            let delimiter_next = match parameters.next() {
                                Some(n) => n,
                                None => element.len(),
                            };

                            let parameter = ows::skip_ows_all(&element[1..delimiter_next]);

                            *element = &element[delimiter_next..];
                            if element.is_empty() {
                                self.state.pop();
                            }

                            if !parameter.is_empty() {
                                self.state.push(HeaderSegment::Parameter(parameter));
                            }
                        },

                        // There are some bytes left to consume before the next field parameter
                        Some(n) => {
                            let immutable = ows::skip_ows_trailing(&element[..n]);
                            *element = &element[n..];
                            return Some(Ok(Value::Element(immutable)));
                        },

                        // Element does not contain any field parameters and can be consumed in its
                        // entirety.
                        None => {
                            let immutable = *element;
                            self.state.pop();
                            return Some(Ok(Value::Element(immutable)));
                        },
                    }
                },

                HeaderSegment::Parameter(parameter) => {
                    match memchr::memchr(b'=', parameter) {
                        None => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterDelimiter));
                        },
                        Some(0) => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterKey));
                        },
                        Some(n) if n + 1 >= parameter.len() => {
                            self.state.clear();
                            return Some(Err(error::ErrorValue::MissingParameterValue));
                        },
                        Some(n) => {
                            let key = &parameter[..n];

                            // Field parameter values may be quoted, but field parameter keys cannot.
                            let val = if parameter[n + 1] == b'"' {
                                let mut quote_next = None;

                                // Search for the next non-escaped quote delimiter
                                let offset = n + 2;
                                let quotes = memchr::memchr_iter(b'"', &parameter[offset..]);
                                for quote in quotes {
                                    if parameter[n + 1 + quote] != b'\\' {
                                        quote_next = Some(quote);
                                        break;
                                    }
                                }

                                let closing_quote = match quote_next {
                                    Some(n) => n,
                                    None => {
                                        self.state.clear();
                                        return Some(Err(error::ErrorValue::MissingClosingQuote));
                                    },
                                };

                                if offset + closing_quote != parameter.len() - 1 {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::LeftoverBytes));
                                }

                                // Value can once again be empty after unquoting
                                let val = &parameter[offset..offset + closing_quote];
                                if val.is_empty() {
                                    self.state.clear();
                                    return Some(Err(error::ErrorValue::MissingParameterValue));
                                }

                                val
                            } else {
                                &parameter[n + 1..]
                            };

                            self.state.pop();
                            return Some(Ok(Value::Parameter { key, val }));
                        },
                    };
                },
            }
        }
    }
}

struct ParsingStack<'a, const SIZE: usize> {
    backing: [std::mem::MaybeUninit<HeaderSegment<'a>>; SIZE],
    index: usize,
}

impl<'a, const SIZE: usize> ParsingStack<'a, SIZE> {
    fn new() -> Self {
        Self {
            backing: std::array::from_fn(|_| std::mem::MaybeUninit::uninit()),
            index: 0,
        }
    }

    fn push(&mut self, state: HeaderSegment<'a>) {
        assert_le!(
            self.index,
            SIZE,
            "Cannot push more than SIZE elements to a ParsingStack"
        );

        self.backing[self.index].write(state);
        self.index += 1;
    }

    fn pop(&mut self) {
        if self.index > 0 {
            self.index -= 1;
        }
    }

    fn peek(&mut self) -> Option<&mut HeaderSegment<'a>> {
        if self.index > 0 {
            // SAFETY: index is kept in sync with stack push/pop operations and therefore only
            // points to initialized memory.
            unsafe { Some(self.backing[self.index - 1].assume_init_mut()) }
        } else {
            None
        }
    }

    fn clear(&mut self) {
        self.index = 0;
    }
}

impl<const SIZE: usize> Clone for ParsingStack<'_, SIZE> {
    fn clone(&self) -> Self {
        Self {
            backing: std::array::from_fn(|i| {
                if i < self.index {
                    // SAFETY: index is kept in sync with stack push/pop operations and therefore
                    // only points to initialized memory. Where index - 1 points to the last written
                    // slot, i < index guarantees that i points to initialized memory.
                    std::mem::MaybeUninit::new(unsafe { self.backing[i].assume_init_ref().clone() })
                } else {
                    std::mem::MaybeUninit::uninit()
                }
            }),
            index: self.index,
        }
    }
}

impl<const SIZE: usize> std::fmt::Debug for ParsingStack<'_, SIZE> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsingStack")
            .field("backing", &&self.backing[..self.index])
            .field("index", &self.index)
            .finish()
    }
}

impl<const SIZE: usize> PartialEq for ParsingStack<'_, SIZE> {
    fn eq(&self, other: &Self) -> bool {
        if self.index == other.index {
            for i in 0..self.index {
                // SAFETY: index is kept in sync with stack push/pop operations and therefore only
                // points to initialized memory. Where index - 1 points to the last written slot,
                // i < index guarantees that i points to initialized memory.
                unsafe {
                    let left = self.backing[i].assume_init_ref();
                    let right = other.backing[i].assume_init_ref();

                    if left != right {
                        return false;
                    }
                };
            }
            true
        } else {
            false
        }
    }
}

impl<const SIZE: usize> Eq for ParsingStack<'_, SIZE> {}

mod ows {
    #[inline]
    pub(super) fn skip_ows_all(bytes: &[u8]) -> &[u8] {
        skip_ows_leading(skip_ows_trailing(bytes))
    }

    #[inline]
    pub(super) fn skip_ows_leading(bytes: &[u8]) -> &[u8] {
        let mut start = 0;
        while start < bytes.len() && crate::constants::OWS.contains(&bytes[start]) {
            start += 1;
        }

        &bytes[start..]
    }

    #[inline]
    pub(super) fn skip_ows_trailing(bytes: &[u8]) -> &[u8] {
        let mut stop = bytes.len();
        while stop > 0 && crate::constants::OWS.contains(&bytes[stop - 1]) {
            stop -= 1;
        }

        &bytes[..stop]
    }
}

mod validate {
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
    pub(crate) fn is_legal_header_name(bytes: &[u8]) -> Option<u8> {
        for c in bytes {
            if !matches!(
                c,
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
                | b'A'..=b'Z',
            ) {
                return Some(*c);
            }
        }

        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn header_line_full() {
        const LINES: &str = "Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_err_missing_crlf_delimiter() {
        const LINES: &str = "Content-Type: text/plain";

        let mut lines = HeaderLines::new(LINES.as_bytes());
        assert_eq!(lines.next(), Some(Err(error::Error::MissingCRLFDelimiter)));
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_list() {
        const LINES: &str = "Example-Dates: Sat 04 May 1996, Wed 14 Sep 2005\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Example-List: ,\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_multiple() {
        const LINES: &str = "Example-List: ,   ,\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_quoted() {
        const LINES: &str = "Example-List: \"\",  \"\"\t, \"\"   \r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-List");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::EmptyList)));
        assert_eq!(v1.next(), None);
    }

    #[test]
    fn header_line_parameters_simple() {
        // TODO: add support for case-insensitivity
        const LINES: &str =
            "Content-Type: multipart/form-data; boundary=7MA4YWxkTrZu0gW; charset=utf-8\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Accept:    text/*;q=0.3, text/plain;q=0.7;; \t  ;, text/plain;   \
                             format=flowed,text/plain;   \tformat=fixed;q=0.4, */*  ;    \
                             q=0.5\t\t\t\t\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Content-Type: ;boundary7MA4YWxkTrZu0gW\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Content-Type: ;=7MA4YWxkTrZu0gW\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Content-Type");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::MissingParameterKey)));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_value() {
        const LINES: &str = "Content-Type: ;boundary=\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Example-Dates: \"Sat, 04 May 1996\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Example-Dates: \"\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_list_simple() {
        const LINES: &str = "Example-Dates: \"Sat, 04 May 1996\", \"Wed, 14 Sep 2005\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Example-Dates: \"Sat, 04 May 1996\", \"\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str =
            "Content-Type: multipart/form-data; boundary=\"7MA4YWxkTrZu0gW\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
    fn header_line_quoted_err_missing_quote_delimiter_element() {
        const LINES: &str = "Example-Dates: \"Sat, 04 May 1996\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::MissingClosingQuote)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_missing_quote_delimiter_parameter() {
        const LINES: &str = "Content-Type: multipart/form-data; boundary=\"7MA4YWxkTrZu0gW\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Example-Dates: \"Sat, 04 May\" 1996\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

        let mut l1 = lines.next().unwrap().unwrap();
        assert_utf8_eq!(l1.name(), b"Example-Dates");

        let mut v1 = l1.values();
        assert_eq!(v1.next(), Some(Err(error::ErrorValue::LeftoverBytes)));

        assert_eq!(v1.next(), None);
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_parameter() {
        const LINES: &str =
            "Content-Type: multipart/form-data; boundary=\"7MA4YWxkTr\"Zu0gW\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINES: &str = "Content-Type: multipart/form-data; boundary=\"\"\r\n\r\n";

        let mut lines = HeaderLines::new(LINES.as_bytes());

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
        const LINE: &str = "Content-Type:text/plain";

        let mut header = HeaderItem::new(LINE.as_bytes()).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), Some(Ok(Value::Element(b"text/plain"))));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_trim_ows() {
        const LINE: &str = "Content-Type: \t text/plain\t\t  \t";

        let mut header = HeaderItem::new(LINE.as_bytes()).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), Some(Ok(Value::Element(b"text/plain"))));
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_empty_field_value_no_ows() {
        const LINE: &str = "Content-Type:";

        let mut header = HeaderItem::new(LINE.as_bytes()).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_empty_field_value_ows() {
        const LINE: &str = "Content-Type:  \t \t\t  \t";

        let mut header = HeaderItem::new(LINE.as_bytes()).unwrap();

        assert_utf8_eq!(header.name(), b"Content-Type");

        let mut values = header.values();
        assert_eq!(values.next(), None);
    }

    #[test]
    fn header_item_err_empty_line() {
        const LINE: &str = "";

        let expected = Err(error::Error::EmptyLine);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_delimiter() {
        const LINE: &str = "Content-Type text/plain";

        let expected = Err(error::Error::MissingFieldDelimiter);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_name() {
        const LINE: &str = ": text/plain";

        let expected = Err(error::Error::MissingFieldName);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_invalid_field_name() {
        const LINE: &str = "Content=Type: text/plain";

        let expected = Err(error::Error::InvalidFieldName(b'='));

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    #[should_panic(expected = "HeaderItem assumes CRLF sanitization has already taken place")]
    fn header_item_panic_trailing_crlf() {
        const LINE: &str = "Content=Type: text/plain\r\n";

        let expected = Err(error::Error::InvalidFieldName(b'='));

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    #[should_panic(expected = "Cannot push more than SIZE elements to a ParsingStack")]
    fn parsing_stack_push_over_size() {
        let mut stack = ParsingStack::<1>::new();
        stack.push(HeaderSegment::Line(b""));
        stack.push(HeaderSegment::Line(b""));
    }
}
