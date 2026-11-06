use super::Error;

pub(crate) struct HeaderLines<'a> {
    input: &'a [u8],
}

impl<'a> HeaderLines<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        Self { input }
    }
}

impl<'a> Iterator for HeaderLines<'a> {
    type Item = Result<HeaderItem<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.input {
            // End of header section
            [b'\r', b'\n', ..] => None,

            _ => match memchr::memmem::find(self.input, crate::constants::CRLF) {
                // Invalid header line end
                None => Some(Err(Error::MissingCRLFDelimiter)),

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
    field_name: &'a str,
    field_value: &'a str,
}

impl<'a> HeaderItem<'a> {
    fn new(line: &'a [u8]) -> Result<Self, Error> {
        if line.is_empty() {
            return Err(Error::EmptyLine);
        }

        let field_name_delimiter =
            memchr::memchr(b':', line).ok_or(Error::MissingFieldDelimiter)?;

        if field_name_delimiter == 0 {
            return Err(Error::MissingFieldName);
        }

        let field_name_raw = &line[..field_name_delimiter];

        let mut field_value_start = field_name_delimiter + 1;
        while field_value_start < line.len()
            && crate::constants::OWS.contains(&line[field_value_start])
        {
            field_value_start += 1;
        }

        let mut field_value_stop = line.len();
        while field_value_stop > field_value_start
            && crate::constants::OWS.contains(&line[field_value_stop - 1])
        {
            field_value_stop -= 1;
        }

        // Header values may be empty
        let field_value_range = field_value_start..field_value_stop;
        let field_value_raw = &line[field_value_range];

        let field_name = std::str::from_utf8(field_name_raw).map_err(Error::InvalidUtf8)?;
        let field_value = std::str::from_utf8(field_value_raw).map_err(Error::InvalidUtf8)?;

        Ok(Self {
            field_name,
            field_value,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn header_line_full() {
        const LINES: &str = "Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let expected = [
            Some(Ok(HeaderItem {
                field_name: "Content-Type",
                field_value: "text/plain",
            })),
            Some(Ok(HeaderItem {
                field_name: "Content-Length",
                field_value: "3",
            })),
            None,
        ]
        .into_iter();

        let mut lines = HeaderLines::new(LINES.as_bytes());

        for item in expected {
            assert_eq!(lines.next(), item);
        }
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
        assert_eq!(lines.next(), Some(Err(Error::MissingCRLFDelimiter)));
    }

    #[test]
    fn header_item_full() {
        const LINE: &str = "Content-Type:text/plain";

        let expected = Ok(HeaderItem {
            field_name: "Content-Type",
            field_value: "text/plain",
        });

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_trim_ows() {
        const LINE: &str = "Content-Type: \t text/plain\t\t  \t";

        let expected = Ok(HeaderItem {
            field_name: "Content-Type",
            field_value: "text/plain",
        });

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_empty_line() {
        const LINE: &str = "";

        let expected = Err(Error::EmptyLine);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_delimiter() {
        const LINE: &str = "Content-Type text/plain";

        let expected = Err(Error::MissingFieldDelimiter);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_name() {
        const LINE: &str = ": text/plain";

        let expected = Err(Error::MissingFieldName);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_value_no_ows() {
        const LINE: &str = "Content-Type:";

        let expected = Ok(HeaderItem {
            field_name: "Content-Type",
            field_value: "",
        });

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_value_ows() {
        const LINE: &str = "Content-Type:  \t \t\t  \t";

        let expected = Ok(HeaderItem {
            field_name: "Content-Type",
            field_value: "",
        });

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }
}
