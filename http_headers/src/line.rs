//! Header line parsing utilities.
//!
//! This is intended as a lower-level API with higher flexibility: generally speaking, you are
//! better off using the higher level [`serde`] parsers provided by this crate.
//!
//! # Examples
//!
//! The following showcases a manual parsing implementation which accumulates headers into a
//! [`HashMap`]-like collection.
//!
//! ```rust
//! # use std::collections::HashMap;
//! # use std::collections::hash_map::Entry;
//! #
//! # use http_headers::prelude::*;
//! #
//! // This is the main entry-point to our parser, handling header parsing and de-duplication.
//! #[derive(Debug, Default, Clone, PartialEq, Eq)]
//! struct HeaderSet<'a>(HashMap<ByteStr<'a>, Vec<HeaderValue<'a>>>);
//!
//! // The library doesn't provide any out-of-the box primitive for storing header elements and
//! // their parameters, as doing so would either require allocating or enforcing hard bounds on
//! // the number of header parameters per element. Instead, we leave it up to the user to
//! // implement their own storage systems with whatever allocation strategy they desire.
//! #[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
//! struct HeaderValue<'a> {
//!     element: Option<Element<'a>>,
//!     parameters: Vec<Parameter<'a>>,
//! }
//!
//! impl<'a> HeaderValue<'a> {
//!     fn new(element: Option<Element<'a>>) -> Self {
//!         Self {
//!             element,
//!             parameters: Vec::new(),
//!         }
//!     }
//! }
//!
//! impl<'a> HeaderSet<'a> {
//!     // Retrieves information about a header which has already been parsed.
//!     fn get(&self, header_name: impl Into<ByteStr<'a>>) -> Option<&Vec<HeaderValue<'a>>> {
//!         self.0.get(&header_name.into())
//!     }
//!
//!     // `http_headers` provides iterators which yield structured information about a header
//!     // segment. This method is responsible for parsing that structure and appending it to our
//!     // header set.
//!     fn insert(&mut self, mut header: HeaderLine<'a>) -> Result<(), Error> {
//!         match header.values.next().transpose()? {
//!             // A header line can contain only a single value
//!             Some(Count::One(value)) => {
//!                 self.process_value(header.name, value)?;
//!             },
//!
//!             // A header line can also contain a list of many different values
//!             Some(Count::Many(mut list)) => {
//!                 while let Some(value) = list.next().transpose()? {
//!                     self.process_value(header.name, value)?;
//!                 }
//!             },
//!
//!             // A header line can also be empty!
//!             None => {
//!                 self.0.entry(header.name).or_default();
//!             },
//!         }
//!
//!         Ok(())
//!     }
//!
//!     // The `Value` enum is the main primitive for parsing header fields. This method is
//!     // responsible for extracting its contents.
//!     fn process_value(
//!         &mut self,
//!         header_name: ByteStr<'a>,
//!         value: Value<'a>,
//!     ) -> Result<(), Error> {
//!         match value {
//!             // A value can be a single element
//!             Value::Element(element, mut parameters) => {
//!                 let mut header_value = HeaderValue::new(Some(element));
//!
//!                 // An element may be followed by zero or more parameters
//!                 while let Some(parameter) = parameters.next().transpose()? {
//!                     header_value.parameters.push(parameter);
//!                 }
//!
//!                 // remember that a header can have multiple values, which each need to be
//!                 // accumulated into a shared collection.
//!                 self.0.entry(header_name).or_default().push(header_value);
//!             },
//!
//!             // An value can also be zero or more parameters, with no leading element
//!             Value::Parameters(mut parameters) => {
//!                 let mut header_value = HeaderValue::new(None);
//!
//!                 while let Some(parameter) = parameters.next().transpose()? {
//!                     header_value.parameters.push(parameter);
//!                 }
//!
//!                 self.0.entry(header_name).or_default().push(header_value)
//!             },
//!         }
//!
//!         Ok(())
//!     }
//! }
//!
//! fn main() -> Result<(), Error> {
//!     let mut header_lines = *b"\
//!         Content-Type: multipart/form-data; boundary=7MA4YWxk; charset=utf-8\r\n\
//!         \r\n\
//!     ";
//!
//!     let mut headers = HeaderSet::default();
//!     let mut iter = Headers::new(&mut header_lines);
//!
//!     // Parsing is implemented as an iterator over header field lines
//!     while let Some(mut header) = iter.next().transpose()? {
//!         // Our `HeaderSet` takes care of parsing the rest of the header structure.
//!         headers.insert(header)?;
//!     }
//!
//!     // Now that each header has been successfully processed,
//!     // we can access each of their individual values.
//!
//!     assert_eq!(
//!         // Header names get lower-cased!
//!         headers.get(b"content-type"),
//!         // Each header contains...
//!         Some(&vec![
//!             // A list of header values...
//!             HeaderValue {
//!                 // With an optional element...
//!                 element: Some(Element::new(b"multipart/form-data")),
//!                 parameters: vec![
//!                     // Followed by zero or more parameters
//!                     Parameter::new(b"boundary", b"7MA4YWxk"),
//!                     Parameter::new(b"charset", b"utf-8"),
//!                 ]
//!             }
//!         ])
//!     );
//!
//!     // Remember that all of the above could be empty,
//!     // and that would still be valid header line!
//!
//!     Ok(())
//! }
//! ```
//!
//! [`HashMap`]: std::collections::HashMap
use super::error;

/// Header section parser.
///
/// # Specs
///
/// See the HTTP field format as described in [section 5] of [RFC9110], HTTP Semantics.
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
///
/// assert_eq!(
///     values.next(),
///     Some(Ok(Count::One(Value::Element(
///         Element::new(b"text/plain"),
///         HeaderParameters::empty()
///     ))))
/// );
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
    pub name: ByteStr<'a>,

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
            name: ByteStr::new(field_name),
            values: field_value,
        })
    }
}

/// Counts the occurrences of [`Value`]s in a header line.
#[derive(Debug, PartialEq, Eq)]
pub enum Count<'a> {
    /// Single-[`Value`] occurrence.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_headers::prelude::*;
    /// # use std::assert_matches;
    /// let mut input = *b"1";
    /// let mut values = HeaderValues::new(&mut input);
    ///
    /// assert_matches!(values.next(), Some(Ok(Count::One(..))));
    /// ```
    One(Value<'a>),

    /// Multiple-[`Value`] occurrence, indicative of a list.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_headers::prelude::*;
    /// # use std::assert_matches;
    /// let mut input = *b"1, 2, 3";
    /// let mut values = HeaderValues::new(&mut input);
    ///
    /// assert_matches!(values.next(), Some(Ok(Count::Many(..))));
    /// ```
    Many(HeaderList<'a>),
}

/// Iterator over header [`Value`]s in a [`HeaderLine`].
#[derive(PartialEq, Eq)]
pub struct HeaderValues<'a> {
    bytes: Option<&'a mut [u8]>,
}

impl<'a> HeaderValues<'a> {
    /// Initializes a new header [`Value`] iterator with the provided content.
    #[must_use]
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes: Some(ows::skip_ows_leading(bytes)),
        }
    }

    /// Creates an empty header [`Value`] iterator.
    #[must_use]
    pub fn empty() -> Self {
        Self { bytes: None }
    }
}

impl<'a> Iterator for HeaderValues<'a> {
    type Item = Result<Count<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.bytes.take()?;

        if bytes.is_empty() {
            return None;
        }

        let mut from = 0;

        // List delimiters only count as separators if they are unquoted
        while let Some(mut n_list) = memchr::memchr(b',', &bytes[from..]) {
            n_list += from;

            // We need to ensure list delimiters are not contained withing quotes
            if let Some(n_quote_open) = quote::next_quote_valid(bytes, from) {
                // Which means we also need to make sure those quotes are properly terminated
                if let Some(n_quote_close) = quote::next_quote_valid(bytes, n_quote_open + 1)
                    && n_quote_open < n_list
                    && n_quote_close + 1 > n_list
                {
                    from = n_quote_close + 1;
                    continue;
                }
            }

            return Some(Ok(Count::Many(HeaderList::new(bytes))));
        }

        if bytes.is_empty() {
            // Empty elements shouldn't yield any further parsing steps
            None
        } else if bytes.len() < 3 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
            // Quoted empty elements should be treated the same as regular empty elements
            None
        } else {
            match Value::new(ows::skip_ows_all(bytes)) {
                Ok(value) => Some(Ok(Count::One(value))),
                Err(err) => Some(Err(err)),
            }
        }
    }
}

impl std::fmt::Debug for HeaderValues<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderValues")
            .field("bytes", &utf_8::option_string(&self.bytes))
            .finish()
    }
}

/// Header [`Value`] list iterator.
#[derive(PartialEq, Eq)]
pub struct HeaderList<'a> {
    bytes: Option<&'a mut [u8]>,
    size: usize,
}

impl<'a> HeaderList<'a> {
    /// Initializes a new header [`Value`] list iterator with the provided content.
    #[must_use]
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes: Some(ows::skip_ows_leading(bytes)),
            size: 0,
        }
    }

    /// Crates a new empty header [`Value`] list iterator.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            bytes: None,
            size: 0,
        }
    }
}

impl<'a> Iterator for HeaderList<'a> {
    type Item = Result<Value<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        // Need to handle empty list items
        loop {
            let list = self.bytes.take()?;

            // Empty list
            if list.is_empty() {
                return if self.size == 0 {
                    Some(Err(error::ErrorValue::EmptyList))
                } else {
                    None
                };
            }

            // Field elements can be quoted and may contain ',' delimiters, in which case
            // those delimiters are considered as escaped and don't contribute to field
            // value splitting.
            let element = if list[0] == b'"' {
                let Some(closing_quote) = quote::next_quote_valid(list, 1) else {
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
                    return Some(Err(error::ErrorValue::LeftoverBytes));
                }

                let (element, remaining) = list.split_at_mut(unquoted);
                self.bytes = Some(ows::skip_ows_leading(&mut remaining[skip..]));

                ows::skip_ows_all(element)
            } else {
                // CAUTION: `delimiter_next` is indexed relative to `list`, however `skip`
                // is indexed relative to `&list[delimiter_next..]`
                let (delimiter_next, skip) = match memchr::memchr(b',', list) {
                    Some(n) => (n, 1),
                    None => (list.len(), 0),
                };

                let (element, remaining) = list.split_at_mut(delimiter_next);
                self.bytes = Some(ows::skip_ows_leading(&mut remaining[skip..]));

                ows::skip_ows_all(element)
            };

            if !element.is_empty() && element[0] != b'"' || element.len() > 2 {
                // Element is non-empty nor quoted empty
                self.size += 1;

                return Some(Value::new(element));
            }
        }
    }
}

impl std::fmt::Debug for HeaderList<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderList")
            .field("bytes", &utf_8::option_string(&self.bytes))
            .field("size", &self.size)
            .finish()
    }
}

/// Header [`Parameter`] iterator.
#[derive(PartialEq, Eq)]
pub struct HeaderParameters<'a> {
    bytes: Option<&'a mut [u8]>,
}

impl<'a> HeaderParameters<'a> {
    /// Initializes a new header [`Parameter`] iterator with the provided content.
    #[must_use]
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self {
            bytes: Some(ows::skip_ows_leading(bytes)),
        }
    }

    /// Creates a new empty header [`Parameter`] iterator.
    #[must_use]
    pub fn empty() -> Self {
        Self { bytes: None }
    }
}

impl<'a> Iterator for HeaderParameters<'a> {
    type Item = Result<Parameter<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let parameters = self.bytes.take()?;

            if parameters.is_empty() {
                return None;
            }

            let (delimiter_next, skip) = match memchr::memchr(b';', parameters) {
                Some(n) => (n, 1),
                None => (parameters.len(), 0),
            };

            let (parameter, remaining) = parameters.split_at_mut(delimiter_next);
            let param = ows::skip_ows_all(parameter);

            self.bytes = Some(&mut remaining[skip..]);

            if !param.is_empty() {
                match memchr::memchr(b'=', param) {
                    None => {
                        return Some(Err(error::ErrorValue::MissingParameterDelimiter));
                    },
                    Some(0) => {
                        return Some(Err(error::ErrorValue::MissingParameterKey));
                    },
                    Some(n) if n + 1 >= param.len() => {
                        return Some(Err(error::ErrorValue::MissingParameterValue));
                    },
                    Some(n) => {
                        let (key, mut val) = param.split_at_mut(n);
                        key.make_ascii_lowercase();

                        if let Some(c) = validate::is_legal_header_parameter(key) {
                            return Some(Err(error::ErrorValue::InvalidFieldParameter(c)));
                        }

                        // Field parameter values may be quoted, but field parameter keys cannot.
                        if val[1] == b'"' {
                            let Some(closing_quote) = quote::next_quote_valid(val, 2) else {
                                return Some(Err(error::ErrorValue::MissingClosingQuote));
                            };

                            if closing_quote != val.len() - 1 {
                                return Some(Err(error::ErrorValue::LeftoverBytes));
                            }

                            // Value can once again be empty after unquoting
                            val = &mut val[2..closing_quote];
                            if val.is_empty() {
                                return Some(Err(error::ErrorValue::MissingParameterValue));
                            }

                            if let Some(err) = validate::is_legal_header_parameter_quote(val) {
                                return Some(Err(err));
                            }

                            let unpaired = quote::decode_quoted_pairs(val);
                            val = &mut val[..unpaired];
                        } else {
                            val = &mut val[1..];

                            if let Some(c) = validate::is_legal_header_parameter(val) {
                                return Some(Err(error::ErrorValue::InvalidFieldParameter(c)));
                            }
                        }

                        return Some(Ok(Parameter::new(key, val)));
                    },
                }
            }
        }
    }
}

impl std::fmt::Debug for HeaderParameters<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderParameters")
            .field("bytes", &utf_8::option_string(&self.bytes))
            .finish()
    }
}

/// Header [field values].
///
/// [field values]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
#[derive(Debug, PartialEq, Eq)]
pub enum Value<'a> {
    /// Single element field value.
    Element(Element<'a>, HeaderParameters<'a>),

    /// Zero or more header field value [parameters].
    ///
    /// [parameters]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
    Parameters(HeaderParameters<'a>),
}

impl<'a> Value<'a> {
    fn new(element: &'a mut [u8]) -> Result<Self, error::ErrorValue> {
        assert_ne!(element, &[]);

        let (val, parameters) = if element[0] == b'"' && element[element.len() - 1] == b'"' {
            assert!(element.len() > 2);

            // Quoted element, make sure quotes are valid and return the value between them

            let len = element.len();
            let value = &mut element[1..len - 1];

            if let Some(c) = validate::is_legal_header_quote(value) {
                return Err(error::ErrorValue::InvalidEscapeSequence(c));
            }

            let unpaired = quote::decode_quoted_pairs(value);
            let val = &mut value[..unpaired];
            let parameters = HeaderParameters::empty();

            (val, parameters)
        } else {
            // Unquoted elements may still contain field parameters

            match memchr::memchr(b';', element) {
                // Element IS a field parameter
                Some(0) => {
                    let param = &mut element[1..];
                    return Ok(Value::Parameters(HeaderParameters::new(param)));
                },

                // There are some bytes left to consume before the next field parameter
                Some(n) => {
                    let (value, remaining) = element.split_at_mut(n);
                    let val = ows::skip_ows_trailing(value);
                    let param = &mut remaining[1..];

                    (val, HeaderParameters::new(param))
                },

                // Element does not contain any field parameters and can be consumed in
                // its entirety.
                None => (element, HeaderParameters::empty()),
            }
        };

        // Quote validation, including missing closing quotes and leftover bytes. This is only
        // performed on the element and not on any potential parameter bytes in order to adhere to
        // the lazy parsing conventions employed by other iterators.
        if let Some(n_quote_open) = quote::next_quote_valid(val, 0) {
            if let Some(n_quote_close) = quote::next_quote_valid(val, n_quote_open + 1) {
                if n_quote_close + 1 != val.len() {
                    return Err(error::ErrorValue::LeftoverBytes);
                }
            } else {
                return Err(error::ErrorValue::MissingClosingQuote);
            }
        }

        // Checks element for any illegal code points
        if let Some(c) = validate::is_legal_header_element(val) {
            return Err(error::ErrorValue::InvalidFieldValue(c));
        }

        Ok(Value::Element(Element::new(val), parameters))
    }
}

/// A single header [field value].
///
/// [field value]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Element<'a> {
    /// Header [field value].
    ///
    /// [field value]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
    pub val: ByteStr<'a>,
}

impl<'a> Element<'a> {
    /// Initializes a new header [field value] with the provided content.
    ///
    /// [field value]: https://www.rfc-editor.org/rfc/rfc9110.html#name-field-values
    #[must_use]
    pub fn new(val: impl Into<ByteStr<'a>>) -> Self {
        Self { val: val.into() }
    }
}

impl std::fmt::Debug for Element<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Element").field("val", &self.val).finish()
    }
}

/// A single header [field parameter].
///
/// [field parameter]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Parameter<'a> {
    /// Header [field parameter] key.
    ///
    /// [field parameter]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
    pub key: ByteStr<'a>,

    /// Header [field parameter] value.
    ///
    /// [field parameter]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
    pub val: ByteStr<'a>,
}

impl<'a> Parameter<'a> {
    /// Initializes a new header [field parameter] with the provided key/value pair.
    ///
    /// [field parameter]: https://www.rfc-editor.org/rfc/rfc9110.html#name-parameters
    #[must_use]
    pub fn new(key: impl Into<ByteStr<'a>>, val: impl Into<ByteStr<'a>>) -> Self {
        Self {
            key: key.into(),
            val: val.into(),
        }
    }
}

impl std::fmt::Debug for Parameter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parameter")
            .field("key", &self.key)
            .field("val", &self.val)
            .finish()
    }
}

/// Binary string backed by a [`u8`] slice, used to hold raw network bytes for further processing
/// without re-allocation.
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteStr<'a>(&'a [u8]);

impl<'a> ByteStr<'a> {
    /// Initializes a new binary string with the provided content.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// Consumes the binary string and retrieve the backing [`u8`] byte array.
    #[must_use]
    pub fn into_inner(self) -> &'a [u8] {
        self.0
    }
}

impl AsRef<[u8]> for ByteStr<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl<'a> From<&'a [u8]> for ByteStr<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::new(bytes)
    }
}

impl<'a> From<&'a mut [u8]> for ByteStr<'a> {
    fn from(bytes: &'a mut [u8]) -> Self {
        Self::new(bytes)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for ByteStr<'a> {
    fn from(bytes: &'a [u8; N]) -> Self {
        Self::new(bytes)
    }
}

impl<'a, const N: usize> From<&'a mut [u8; N]> for ByteStr<'a> {
    fn from(bytes: &'a mut [u8; N]) -> Self {
        Self::new(bytes)
    }
}

impl<'a> From<ByteStr<'a>> for &'a [u8] {
    fn from(bytestr: ByteStr<'a>) -> Self {
        bytestr.0
    }
}

impl PartialEq<&[u8]> for ByteStr<'_> {
    fn eq(&self, other: &&[u8]) -> bool {
        &self.0 == other
    }
}

impl PartialEq<&mut [u8]> for ByteStr<'_> {
    fn eq(&self, other: &&mut [u8]) -> bool {
        &self.0 == other
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for ByteStr<'_> {
    fn eq(&self, other: &&[u8; N]) -> bool {
        &self.0 == other
    }
}

impl<const N: usize> PartialEq<&mut [u8; N]> for ByteStr<'_> {
    fn eq(&self, other: &&mut [u8; N]) -> bool {
        &self.0 == other
    }
}

impl std::fmt::Debug for ByteStr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ByteStr")
            .field(&std::str::from_utf8(self.0).unwrap_or_default())
            .finish()
    }
}

impl std::fmt::Display for ByteStr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(self.0).unwrap_or_default())
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
    pub(super) fn next_quote_valid(bytes: &[u8], offset: usize) -> Option<usize> {
        let quotes = memchr::memchr_iter(b'"', &bytes[offset..]);
        for quote in quotes {
            if quote == 0 || bytes[offset + quote - 1] != b'\\' {
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

mod utf_8 {
    pub(super) fn option_string<'a>(bytes: &'a Option<&'a mut [u8]>) -> &'a str {
        bytes
            .as_deref()
            .map_or(Ok(""), std::str::from_utf8)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod test {
    use macro_util::prelude::*;

    use super::*;

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

        // -- Line 1

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"text/plain"));
        });

        assert_next!(l1.values, None);

        // -- Line 2

        let mut l2 = lines.next().unwrap().unwrap();
        assert_eq!(l2.name, b"content-length");

        assert_next!(l2.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"3"));
        });

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

        assert_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Ok(Value::Element(element, ..))) => {
                assert_eq!(element, Element::new(b"Sat 04 May 1996"));
            });

            assert_next!(list, Some(Ok(Value::Element(element, ..))) => {
                assert_eq!(element, Element::new(b"Wed 14 Sep 2005"));
            });

            assert_next!(list, None);
        });
        assert_next!(l1.values, None);

        assert_next!(lines, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_single() {
        let mut input = *b"Example-List: ,\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_eq!(l1.name, b"example-list");
        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Err(error::ErrorValue::EmptyList)));
            assert_next!(list, None);
        });
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_multiple() {
        let mut input = *b"Example-List: ,   ,\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_eq!(l1.name, b"example-list");
        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Err(error::ErrorValue::EmptyList)));
            assert_next!(list, None);
        });
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_err_empty_list_delimiter_quoted() {
        let mut input = *b"Example-List: \"\",  \"\"\t, \"\"   \r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();

        assert_eq!(l1.name, b"example-list");
        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Err(error::ErrorValue::EmptyList)));
            assert_next!(list, None);
        });
        assert_next!(l1.values, None);
    }

    #[test]
    fn header_line_parameters_simple() {
        let mut input =
            *b"Content-Type: multipart/form-data; boundary=7MA4YWxk; charset=utf-8\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, mut parameters)))) => {
            assert_eq!(element, Element::new(b"multipart/form-data"));
            assert_next!(parameters, Some(Ok(Parameter::new(b"boundary", b"7MA4YWxk"))));
            assert_next!(parameters, Some(Ok(Parameter::new(b"charset", b"utf-8"))));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_fuck_me() {
        let mut input = *b"\
            Accept:    text/*;q=0.3\
                , text/plain;Q=0.7;; \t  ;\
                , text/plain;   fOrMat=flowed\
                ,text/plain;   \tformat=fixed;q=0.4, */*  ;    q=0.5\t\t\t\t\r\n\
            \r\n\
        ";

        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"accept");

        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Ok(Value::Element(element, mut parameters))) => {
                assert_eq!(element, Element::new(b"text/*"));
                assert_next!(parameters, Some(Ok(Parameter::new(b"q", b"0.3"))));
                assert_next!(parameters, None);
            });

            assert_next!(list, Some(Ok(Value::Element(element, mut parameters))) => {
                assert_eq!(element, Element::new(b"text/plain"));
                assert_next!(parameters, Some(Ok(Parameter::new(b"q", b"0.7"))));
                assert_next!(parameters, None);
            });

            assert_next!(list, Some(Ok(Value::Element(element, mut parameters))) => {
                assert_eq!(element, Element::new(b"text/plain"));
                assert_next!(parameters, Some(Ok(Parameter::new(b"format", b"flowed"))));
                assert_next!(parameters, None);
            });

            assert_next!(list, Some(Ok(Value::Element(element, mut parameters))) => {
                assert_eq!(element, Element::new(b"text/plain"));
                assert_next!(parameters, Some(Ok(Parameter::new(b"format", b"fixed"))));
                assert_next!(parameters, Some(Ok(Parameter::new(b"q", b"0.4"))));
                assert_next!(parameters, None);
            });

            assert_next!(list, Some(Ok(Value::Element(element, mut parameters))) => {
                assert_eq!(element, Element::new(b"*/*"));
                assert_next!(parameters, Some(Ok(Parameter::new(b"q", b"0.5"))));
                assert_next!(parameters, None);
            });

            assert_next!(list, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_delimiter() {
        let mut input = *b"Content-Type: ;boundary7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::MissingParameterDelimiter)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_key() {
        let mut input = *b"Content-Type: ;=7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::MissingParameterKey)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_value() {
        let mut input = *b"Content-Type: ;boundary=\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::MissingParameterValue)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_element_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"Sat, 04 May 1996"));
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_element_empty() {
        let mut input = *b"Example-Dates: \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_list_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"Wed, 14 Sep 2005\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Ok(Value::Element(element, ..))) => {
                assert_eq!(element, Element::new(b"Sat, 04 May 1996"));
            });
            assert_next!(list, Some(Ok(Value::Element(element, ..))) => {
                assert_eq!(element, Element::new(b"Wed, 14 Sep 2005"));
            });
            assert_next!(list, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_list_empty_element() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"example-dates");

        assert_next!(l1.values, Some(Ok(Count::Many(mut list))) => {
            assert_next!(list, Some(Ok(Value::Element(element, ..))) => {
                assert_eq!(element, Element::new(b"Sat, 04 May 1996"));
            });
            assert_next!(list, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, mut parameters)))) => {
            assert_eq!(element, Element::new(b"multipart/form-data"));
            assert_next!(parameters, Some(Ok(Parameter::new(b"boundary", b"7MA4YWxk"))));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_element() {
        let mut input = *b"Content-Type: \"multipart/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Err(error::ErrorValue::MissingClosingQuote)));

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, mut parameters)))) => {
            assert_eq!(element, Element::new(b"multipart/form-data"));
            assert_next!(parameters, Some(Err(error::ErrorValue::MissingClosingQuote)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_element() {
        let mut input = *b"Content-Type: \"multipart\"/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Err(error::ErrorValue::LeftoverBytes)));

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTr\"Zu0gW\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, mut parameters)))) => {
            assert_eq!(element, Element::new(b"multipart/form-data"));
            assert_next!(parameters, Some(Err(error::ErrorValue::LeftoverBytes)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_line_quoted_err_missing_parameter_value() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        let mut l1 = lines.next().unwrap().unwrap();
        assert_eq!(l1.name, b"content-type");

        assert_next!(l1.values, Some(Ok(Count::One(Value::Element(element, mut parameters)))) => {
            assert_eq!(element, Element::new(b"multipart/form-data"));
            assert_next!(parameters, Some(Err(error::ErrorValue::MissingParameterValue)));
            assert_next!(parameters, None);
        });

        assert_next!(l1.values, None);
        assert_next!(lines, None);
    }

    #[test]
    fn header_item_full() {
        let mut input = *b"Content-Type:text/plain";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_eq!(header.name, b"content-type");

        assert_next!(header.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"text/plain"));
        });

        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_trim_ows() {
        let mut input = *b"Content-Type: \t text/plain\t\t  \t";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_eq!(header.name, b"content-type");

        assert_next!(header.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"text/plain"));
        });

        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_empty_field_value_no_ows() {
        let mut input = *b"Content-Type:";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_eq!(header.name, b"content-type");
        assert_next!(header.values, None);
    }

    #[test]
    fn header_item_empty_field_value_ows() {
        let mut input = *b"Content-Type:  \t \t\t  \t";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_eq!(header.name, b"content-type");
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

        assert_next!(
            header.values,
            Some(Err(error::ErrorValue::InvalidFieldValue(b' ')))
        );

        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_key() {
        let mut input = *b"Content-Type: ; bo\\undary=7MA4YWxk";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(header.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\'))));
            assert_next!(parameters, None);
        });

        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_err_invalid_field_parameter_val() {
        let mut input = *b"Content-Type: ; boundary=7MA4Y\\WxkTrZu0gW";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(header.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::InvalidFieldParameter(b'\\'))));
            assert_next!(parameters, None);
        });

        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_quoted_pair_element() {
        let mut input = *b"Content-Type: \"\\\"text/plain\\\"\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(header.values, Some(Ok(Count::One(Value::Element(element, ..)))) => {
            assert_eq!(element, Element::new(b"\"text/plain\""));
        });

        assert_next!(header.values, None);
    }

    #[test]
    fn header_value_quoted_pair_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"\\\"\\7MA4Y\\Wx\\kTrZu0gW\\\"\"";
        let mut header = HeaderLine::new(&mut input).unwrap();

        assert_next!(header.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Ok(Parameter::new(b"boundary", b"\"7MA4YWxkTrZu0gW\""))));
            assert_next!(parameters, None);
        });

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

        assert_next!(header.values, Some(Ok(Count::One(Value::Parameters(mut parameters)))) => {
            assert_next!(parameters, Some(Err(error::ErrorValue::InvalidEscapeSequence(b'\0'))));
            assert_next!(parameters, None);
        });

        assert_next!(header.values, None);
    }
}
