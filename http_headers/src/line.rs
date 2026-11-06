use super::Error;

pub(crate) struct Lines<'a> {
    iter: memchr::memmem::FindIter<'a, 'a>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Self {
        let iter = memchr::memmem::find_iter(input, crate::constants::CRLF);
        Self { iter }
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = HeaderItem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
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
        while field_value_stop > 0 && crate::constants::OWS.contains(&line[field_value_stop - 1]) {
            field_value_stop -= 1;
        }

        let field_value_range = field_value_start..field_value_stop;
        if field_value_range.is_empty() {
            return Err(Error::MissingFieldValue);
        }

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
    use macro_util::prelude::*;

    use super::*;

    #[test]
    fn header_item_line_full() {
        const LINE: &str = "Content-Type:text/plain";

        let expected = Ok(HeaderItem {
            field_name: "Content-Type",
            field_value: "text/plain",
        });

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_line_trim_ows() {
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

        let expected = Err(Error::MissingFieldValue);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }

    #[test]
    fn header_item_err_missing_field_value_ows() {
        const LINE: &str = "Content-Type:  \t \t\t  \t";

        let expected = Err(Error::MissingFieldValue);

        assert_eq!(HeaderItem::new(LINE.as_bytes()), expected);
    }
}
