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

        let field_name_delimiter =
            memchr::memchr(b':', line).ok_or(error::Error::MissingFieldDelimiter)?;

        if field_name_delimiter == 0 {
            return Err(error::Error::MissingFieldName);
        }

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
        self.field_value = ows::skip_leading_ows(self.field_value);

        // Field value lists
        if let Some(mut list_delimiter) = memchr::memchr(b',', self.field_value) {
            let mut list_element = ows::skip_trailing_ows(&self.field_value[..list_delimiter]);

            while list_element.is_empty() && list_delimiter < self.field_value.len() {
                self.field_value = &self.field_value[list_delimiter + 1..];

                list_delimiter = match memchr::memchr(b',', self.field_value) {
                    Some(n) => n,
                    None => self.field_value.len(),
                };

                list_element = &self.field_value[..list_delimiter];
                list_element = ows::skip_leading_ows(&list_element);
                list_element = ows::skip_trailing_ows(&list_element);
            }

            self.field_value = &self.field_value[list_delimiter..];

            if list_element.is_empty() {
                return Some(Err(error::ErrorValue::EmptyList));
            } else {
                return Some(Ok(Value::Element(list_element)));
            }
        }

        // Single element
        let element = ows::skip_trailing_ows(self.field_value);
        self.field_value = &[];

        if element.is_empty() {
            None
        } else {
            Some(Ok(Value::Element(element)))
        }
    }
}

mod ows {
    #[inline]
    pub(super) fn skip_leading_ows(bytes: &[u8]) -> &[u8] {
        let mut start = 0;
        while start < bytes.len() && crate::constants::OWS.contains(&bytes[start]) {
            start += 1;
        }

        &bytes[start..]
    }

    #[inline]
    pub(super) fn skip_trailing_ows(bytes: &[u8]) -> &[u8] {
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
