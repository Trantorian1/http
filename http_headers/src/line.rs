use super::error;

pub(crate) struct HeaderLines<'a> {
    input: &'a [u8],
}

impl<'a> HeaderLines<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeaderItem<'a> {
    field_name: &'a [u8],
    field_value: HeaderValues<'a>,
}

impl<'a> HeaderItem<'a> {
    fn new(line: &'a [u8]) -> Result<Self, error::Error> {
        if line.is_empty() {
            return Err(error::Error::EmptyLine);
        }

        let field_name_delimiter = match memchr::memchr(b':', line) {
            None => return Err(error::Error::MissingFieldDelimiter),
            Some(0) => return Err(error::Error::MissingFieldName),
            Some(n) => n,
        };

        let field_name = &line[..field_name_delimiter];

        if !validate::is_legal_header_name(field_name) {
            return Err(error::Error::InvalidFieldName);
        }

        let field_value = HeaderValues::new(&line[field_name_delimiter + 1..]);

        Ok(Self {
            field_name,
            field_value,
        })
    }

    fn name(&self) -> &[u8] {
        self.field_name
    }

    fn values(&mut self) -> &mut HeaderValues<'a> {
        &mut self.field_value
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeaderValues<'a> {
    field_value: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Value<'a> {
    Element(&'a [u8]),
    Parameter(ValueParameter<'a>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValueParameter<'a> {
    key: &'a [u8],
    val: &'a [u8],
}

impl<'a> HeaderValues<'a> {
    pub(crate) fn new(field_value: &'a [u8]) -> Self {
        Self { field_value }
    }
}

impl<'a> Iterator for &mut HeaderValues<'a> {
    type Item = Result<Value<'a>, error::ErrorValue>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.field_value = ows::skip_ows_leading(self.field_value);
            let mut element = ows::skip_ows_trailing(self.field_value);
            let mut next_element = self.field_value.len();

            // Field value lists
            if let Some(list_delimiter) = memchr::memchr(b',', element) {
                next_element = list_delimiter;

                // Skip empty list items
                element = ows::skip_ows_trailing(&self.field_value[..next_element]);
                while element.is_empty() && next_element < self.field_value.len() {
                    self.field_value = &self.field_value[next_element + 1..];
                    self.field_value = ows::skip_ows_leading(self.field_value);

                    next_element = match memchr::memchr(b',', self.field_value) {
                        Some(n) => n,
                        None => self.field_value.len(),
                    };

                    element = ows::skip_ows_trailing(&self.field_value[..next_element]);
                }

                if element.is_empty() {
                    return Some(Err(error::ErrorValue::EmptyList));
                }
            }

            // Parameters
            let mut parameters = memchr::memchr_iter(b';', element);
            match parameters.next() {
                Some(0) => {
                    let mut next_parameter = match parameters.next() {
                        Some(n) => n,
                        None => element.len(),
                    };

                    // Skip empty parameters
                    element = ows::skip_ows_all(&element[1..next_parameter]);
                    while element.is_empty() && next_parameter < element.len() {
                        self.field_value = &self.field_value[next_parameter + 1..];
                        self.field_value = ows::skip_ows_leading(self.field_value);

                        next_parameter = match memchr::memchr(b';', element) {
                            Some(n) => n,
                            None => element.len(),
                        };

                        element = ows::skip_ows_trailing(&self.field_value[..next_parameter]);
                    }

                    // Consume parameter so it is not processed again in the next loop iteration
                    self.field_value = &self.field_value[next_parameter..];

                    if element.is_empty() {
                        // Empty list item, move on to next list item (if there is any)
                        continue;
                    }

                    match memchr::memchr(b'=', element) {
                        None => {
                            return Some(Err(error::ErrorValue::MissingParameterDelimiter));
                        },
                        Some(0) => {
                            return Some(Err(error::ErrorValue::MissingParameterKey));
                        },
                        Some(n) if n + 1 >= element.len() => {
                            return Some(Err(error::ErrorValue::MissingParameterValue));
                        },
                        Some(n) => {
                            // TODO: check for illegal chars
                            let key = &element[..n];
                            let val = &element[n + 1..];

                            return Some(Ok(Value::Parameter(ValueParameter { key, val })));
                        },
                    };
                },
                Some(next_parameter) => {
                    // Consume the current element up to the parameter so that it is not processed
                    // again in the next loop iteration
                    self.field_value = &self.field_value[next_parameter..];

                    let element = &element[..next_parameter];
                    return Some(Ok(Value::Element(ows::skip_ows_trailing(element))));
                },
                None => {
                    // Consume the current element so that is is not processed again in the next
                    // loop iteration.
                    let next = (next_element + 1).min(self.field_value.len());
                    self.field_value = &self.field_value[next..];

                    if element.is_empty() {
                        return None;
                    } else {
                        return Some(Ok(Value::Element(element)));
                    }
                },
            }
        }
    }
}

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

    /// tchar          = "!" / "#" / "$" / "%" / "&" / "'" / "*"
    ///                / "+" / "-" / "." / "^" / "_" / "`" / "|" / "~"
    ///                / DIGIT / ALPHA
    ///                ; any VCHAR, except delimiters
    /// ```
    /// 
    /// See [RFC9110] for more information.
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#section-5.6.2
    pub(crate) fn is_legal_header_name(bytes: &[u8]) -> bool {
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
                return false;
            }
        }

        true
    }
}

#[cfg(test)]
mod test {
    use macro_util::prelude::*;

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
        // TODO: add support for quotation
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
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_2, b"boundary");
        assert_utf8_eq!(val1_2, b"7MA4YWxkTrZu0gW");

        let (key1_3, val1_3) = match v1.next() {
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
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
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
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
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
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
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
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
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_8, b"format");
        assert_utf8_eq!(val1_8, b"fixed");

        let (key1_9, val1_9) = match v1.next() {
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
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

        let (key1_8, val1_8) = match v1.next() {
            Some(Ok(Value::Parameter(ValueParameter { key, val }))) => (key, val),
            Some(Ok(Value::Element(bytes))) => {
                let element = std::str::from_utf8(bytes).unwrap();
                panic!("{element}");
            },
            other => panic!("{other:?}"),
        };

        assert_utf8_eq!(key1_8, b"q");
        assert_utf8_eq!(val1_8, b"0.5");

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
        const LINE: &str = "Content=Type: text/plain\r\n";

        let expected = Err(error::Error::InvalidFieldName);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }
}
