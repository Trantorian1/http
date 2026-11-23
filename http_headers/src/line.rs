use super::bytestr::ByteStr;
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
pub(crate) struct Headers<'de> {
    input: Option<ByteStr<'de>>,
}

impl<'de> Headers<'de> {
    /// Initializes a new parser with the input bytes to be processed.
    #[must_use]
    pub(crate) fn new(input: impl Into<ByteStr<'de>>) -> Self {
        Self {
            input: Some(input.into()),
        }
    }

    pub(crate) fn next_key(&mut self) -> Result<Option<ByteStr<'de>>, error::ErrorParse> {
        match self.input.take() {
            // End of header section
            None | Some(ByteStr([b'\r', b'\n'])) => Ok(None),

            Some(ByteStr(input)) => {
                match memchr::memmem::find(input, crate::constants::CRLF) {
                    // Invalid header line end
                    None => Err(error::ErrorParse::MissingCRLFDelimiter),

                    // Extract and parse header line
                    Some(pos_crlf) => match memchr::memchr(b':', &input[..pos_crlf]) {
                        // Missing field delimiter (':')
                        None => {
                            self.input = None;
                            Err(error::ErrorParse::MissingFieldDelimiter)
                        },

                        // Field name
                        Some(pos_value) => {
                            let (field_name, remaining) = input.split_at_mut(pos_value);

                            if field_name.is_empty() {
                                return Err(error::ErrorParse::MissingFieldName);
                            }

                            if let Some(c) = validate::is_legal_header_name(field_name) {
                                return Err(error::ErrorParse::InvalidFieldName(c));
                            }

                            self.input = Some(ByteStr(remaining));
                            Ok(Some(ByteStr(field_name)))
                        },
                    },
                }
            },
        }
    }

    pub(crate) fn next_val(&mut self) -> HeaderList<'de> {
        assert_ne!(self.input, None);

        let Some(ByteStr(input)) = self.input.take() else {
            unreachable!();
        };

        assert_ne!(input, &[]);
        assert_eq!(char::from(input[0]), ':');

        let Some(pos_crlf) = memchr::memmem::find(input, crate::constants::CRLF) else {
            unreachable!();
        };

        let (field_value, remaining) = input.split_at_mut(pos_crlf);

        self.input = Some(ByteStr(&mut remaining[crate::constants::CRLF.len()..]));
        HeaderList::new(&mut field_value[1..])
    }
}

/// Header field value list parser.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HeaderList<'de> {
    bytes: Option<ByteStr<'de>>,
    entries: usize,
    is_empty: bool,
}

impl<'de> HeaderList<'de> {
    /// Initializes a new header [`Value`] list iterator with the provided content.
    #[must_use]
    pub(crate) fn new(bytes: &'de mut [u8]) -> Self {
        Self {
            bytes: Some(ows::skip_ows_leading(bytes).into()),
            entries: 0,
            is_empty: true,
        }
    }

    pub(crate) fn is_seq(&self) -> bool {
        if let Some(ByteStr(bytes)) = &self.bytes {
            if quote::next_unquoted(b',', bytes).is_some() {
                return true;
            }
        }

        false
    }

    pub(crate) fn next_val(
        &mut self,
    ) -> Result<Option<(ByteStr<'de>, HeaderParameters<'de, '_>)>, error::ErrorValue> {
        let bytes = match self.bytes.take() {
            // Empty list checks
            Some(ByteStr([])) | None => {
                return if self.is_empty && self.entries > 0 {
                    Err(error::ErrorValue::EmptyList)
                } else {
                    Ok(None)
                };
            },

            Some(ByteStr(bytes)) => bytes,
        };

        let (element, parameters, remaining) = match bytes[0] {
            // Quoted Element
            b'"' => {
                let Some(pos_quote) = quote::next_quote_valid(bytes, 1) else {
                    return Err(error::ErrorValue::MissingClosingQuote);
                };

                let pos_unquoted = pos_quote + 1;
                let (pos_segment, skip_segment) =
                    match quote::next_unquoted(b',', &bytes[pos_unquoted..]) {
                        Some(n) => (pos_unquoted + n, 1),
                        None => (bytes.len(), 0),
                    };

                let (pos_parameters, skip_parameters) =
                    match quote::next_unquoted(b';', &bytes[..pos_segment]) {
                        Some(n) => (n, 1),
                        None => (pos_segment, 0),
                    };

                let leftover = ows::skip_ows_leading(&mut bytes[pos_unquoted..pos_parameters]);
                if !leftover.is_empty() {
                    return Err(error::ErrorValue::LeftoverBytes);
                }

                let (segment, mut remaining) = bytes.split_at_mut(pos_segment);
                let (mut element, mut parameters) = segment.split_at_mut(pos_parameters);

                element = &mut element[1..pos_quote];
                parameters = &mut parameters[skip_parameters..];
                remaining = ows::skip_ows_leading(&mut remaining[skip_segment..]);

                if let Some(c) = validate::is_legal_header_quote(element) {
                    return Err(error::ErrorValue::InvalidEscapeSequence(c));
                }

                let n = quote::decode_quoted_pairs(element);
                element = &mut element[..n];

                (element, parameters, remaining)
            },
            _ => {
                let (pos_segment, skip_segment) = match quote::next_unquoted(b',', bytes) {
                    Some(n) => (n, 1),
                    None => (bytes.len(), 0),
                };

                let (segment, mut remaining) = bytes.split_at_mut(pos_segment);

                let (pos_parameters, skip_parameters) = match quote::next_unquoted(b';', segment) {
                    Some(n) => (n, 1),
                    None => (segment.len(), 0),
                };

                let (mut element, mut parameters) = segment.split_at_mut(pos_parameters);

                element = ows::skip_ows_all(element);
                parameters = &mut parameters[skip_parameters..];
                remaining = ows::skip_ows_leading(&mut remaining[skip_segment..]);

                (element, parameters, remaining)
            },
        };

        if !element.is_empty() {
            self.is_empty = false;

            if let Some(c) = validate::is_legal_header_element(element) {
                return Err(error::ErrorValue::InvalidFieldValue(c));
            }
        }

        self.bytes = Some(ByteStr(remaining));

        Ok(Some((
            ByteStr(element),
            HeaderParameters::new(parameters, &mut self.is_empty),
        )))
    }
}

/// Header parameter parser.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HeaderParameters<'de, 'a> {
    pub(crate) bytes: Option<ByteStr<'de>>,
    pub(crate) is_empty: &'a mut bool,
}

impl<'de, 'a> HeaderParameters<'de, 'a> {
    #[must_use]
    pub(crate) fn new(bytes: &'de mut [u8], is_empty: &'a mut bool) -> Self {
        Self {
            bytes: Some(ows::skip_ows_leading(bytes).into()),
            is_empty,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        if let Some(ByteStr(bytes)) = &self.bytes {
            if quote::next_unquoted(b';', bytes).is_some() {
                return false;
            }
        }

        true
    }

    pub(crate) fn next_key(&mut self) -> Result<Option<ByteStr<'de>>, error::ErrorValue> {
        loop {
            let bytes = match self.bytes.take() {
                None | Some(ByteStr([])) => return Ok(None),
                Some(ByteStr(bytes)) => bytes,
            };

            let (pos_parameter, skip) = match quote::next_unquoted(b';', bytes) {
                Some(n) => (n, 1),
                None => (bytes.len(), 0),
            };

            // empty parameters are skipped as otherwise parsing would flag them as invalid
            if ows::skip_ows_leading(&mut bytes[..pos_parameter]).is_empty() {
                self.bytes = Some(ByteStr(&mut bytes[skip..]));
                continue;
            }

            let pos_delimiter = match quote::next_unquoted(b'=', bytes) {
                Some(n) => n,
                None => return Err(error::ErrorValue::MissingParameterDelimiter),
            };

            let (key, remaining) = bytes.split_at_mut(pos_delimiter);

            if key.is_empty() {
                return Err(error::ErrorValue::MissingParameterKey);
            }

            if let Some(c) = validate::is_legal_header_parameter(key) {
                return Err(error::ErrorValue::InvalidFieldParameter(c));
            }

            self.bytes = Some(ByteStr(&mut remaining[1..]));
            return Ok(Some(ByteStr(key)));
        }
    }

    pub(crate) fn next_val(&mut self) -> Result<ByteStr<'de>, error::ErrorValue> {
        assert_ne!(self.bytes, None);

        let Some(ByteStr(bytes)) = self.bytes.take() else {
            unreachable!();
        };

        let (pos_parameter, skip) = match quote::next_unquoted(b';', bytes) {
            Some(n) => (n, 1),
            None => (bytes.len(), 0),
        };

        let (mut value, remaining) = bytes.split_at_mut(pos_parameter);
        value = validate::is_legal_header_parameter_value(value)?;

        *self.is_empty = false;

        self.bytes = Some(ByteStr(ows::skip_ows_leading(&mut remaining[skip..])));
        Ok(ByteStr(value))
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
    // TODO: make this return a start index instead
    pub(super) fn skip_ows_leading_immut(bytes: &[u8]) -> &[u8] {
        let mut start = 0;
        while start < bytes.len() && crate::constants::OWS.contains(&bytes[start]) {
            start += 1;
        }

        &bytes[start..]
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

    pub(super) fn next_unquoted(c: u8, bytes: &[u8]) -> Option<usize> {
        let mut from = 0;

        while let Some(pos_delimiter) = memchr::memchr(c, &bytes[from..]) {
            if let Some(pos_quote_open) = next_quote_valid(bytes, from)
                && let Some(pos_quote_close) = next_quote_valid(bytes, pos_quote_open + 1)
                && pos_quote_open < pos_delimiter
                && pos_delimiter < pos_quote_close
            {
                from = pos_quote_close + 1;
                continue;
            }

            return Some(pos_delimiter);
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
    use super::ows;
    use super::quote;

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
    pub(super) fn is_legal_header_name(bytes: &[u8]) -> Option<char> {
        for c in bytes {
            if !matches!(c, tchar!(),) {
                return Some(char::from(*c));
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
    pub(super) fn is_legal_header_element(bytes: &[u8]) -> Option<char> {
        if bytes.is_empty() {
            return None;
        }

        if !matches!(bytes[0], field_vchar!()) {
            return Some(char::from(bytes[0]));
        }

        if bytes.len() > 1 {
            for c in &bytes[1..bytes.len() - 1] {
                #[allow(clippy::unnested_or_patterns)]
                if !matches!(c, field_vchar!() | b' ' | b'\t') {
                    return Some(char::from(*c));
                }
            }
        }

        if !matches!(bytes[bytes.len() - 1], field_vchar!()) {
            return Some(char::from(bytes[bytes.len() - 1]));
        }

        None
    }

    /// Checks for invalid quoted pairs.
    ///
    /// Since quotes are then checked against a field value element, we don't check the validity of
    /// each individual character here since that will be done by [`is_legal_header_element`].
    pub(super) fn is_legal_header_quote(bytes: &[u8]) -> Option<char> {
        let mut iter = bytes.iter();
        while let Some(c) = iter.next() {
            if c == &b'\\' {
                match iter.next() {
                    Some(quoted_pair!()) => (),
                    Some(next) => return Some(char::from(*next)),
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
    pub(super) fn is_legal_header_parameter(bytes: &[u8]) -> Option<char> {
        for c in bytes {
            if !matches!(c, tchar!()) {
                return Some(char::from(*c));
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
                    Some(next) => {
                        return Some(error::ErrorValue::InvalidEscapeSequence(char::from(*next)));
                    },
                    None => unreachable!(
                        "empty quoted pairs are not possible as the parser does not count \\\" as \
                         a quote delimiter"
                    ),
                }
            }

            if !matches!(c, tchar!()) {
                return Some(error::ErrorValue::InvalidFieldParameter(char::from(*c)));
            }
        }

        None
    }

    pub(super) fn is_legal_header_parameter_value(
        mut val: &mut [u8],
    ) -> Result<&mut [u8], error::ErrorValue> {
        val = ows::skip_ows_trailing(val);

        if val.is_empty() {
            return Err(error::ErrorValue::MissingParameterValue);
        }

        if val[0] == b'"' {
            let Some(closing_quote) = quote::next_quote_valid(val, 1) else {
                return Err(error::ErrorValue::MissingClosingQuote);
            };

            if closing_quote != val.len() - 1 {
                return Err(error::ErrorValue::LeftoverBytes);
            }

            // Value can once again be empty after unquoting
            val = &mut val[1..closing_quote];
            if val.is_empty() {
                return Err(error::ErrorValue::MissingParameterValue);
            }

            if let Some(err) = is_legal_header_parameter_quote(val) {
                return Err(err);
            }

            let unpaired = quote::decode_quoted_pairs(val);
            val = &mut val[..unpaired];
        } else {
            if let Some(c) = is_legal_header_parameter(val) {
                return Err(error::ErrorValue::InvalidFieldParameter(char::from(c)));
            }
        }

        Ok(val)
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

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"text/plain");
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Length"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"3");
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_empty() {
        let mut input = *b"\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_missing_crlf_delimiter() {
        let mut input = *b"Content-Type: text/plain";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Err(error::ErrorParse::MissingCRLFDelimiter)
        );
        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_missing_field_delimiter() {
        let mut input = *b"Content-Type text/plain\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Err(error::ErrorParse::MissingFieldDelimiter)
        );
        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_missing_field_name() {
        let mut input = *b": text/plain\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(lines.next_key(), Err(error::ErrorParse::MissingFieldName));
        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_invalid_field_name() {
        let mut input = *b"Content=Type: text/plain\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Err(error::ErrorParse::InvalidFieldName('='))
        );
        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_invalid_field_value_simple() {
        let mut input = *b"Content-Type: text\0plain\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type")))),
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_eq!(line.next_val(), Err(error::ErrorValue::InvalidFieldValue('\0')));
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_invalid_field_value_quoted() {
        let mut input = *b"Content-Type: \" text/plain\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type")))),
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_eq!(line.next_val(), Err(error::ErrorValue::InvalidFieldValue(' ')));
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_list() {
        let mut input = *b"Example-Dates: Sat 04 May 1996, Wed 14 Sep 2005\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-Dates"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Sat 04 May 1996");
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Wed 14 Sep 2005");
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_empty_list_delimiter_single() {
        let mut input = *b"Example-List: ,\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-List"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_empty_list_delimiter_multiple() {
        let mut input = *b"Example-List: ,   , \r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-List"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_err_empty_list_delimiter_quoted() {
        let mut input = *b"Example-List: \"\",  \"\"\t, \"\"   \r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-List"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_simple() {
        let mut input =
            *b"Content-Type: multipart/form-data; boundary=7MA4YWxk; charset=utf-8\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"multipart/form-data");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"7MA4YWxk"))));

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"charset")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"utf-8"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_fuck_me() {
        let mut input = *b"\
            Accept:    text/*;q=0.3\
                , text/plain;q=0.7;; \t  ;\
                , text/plain;   format=flowed\
                ,text/plain;   \tformat=fixed;q=0.4, */*  ;    q=0.5\t\t\t\t\r\n\
            \r\n\
        ";

        let mut lines = Headers::new(&mut input);

        assert_eq!(lines.next_key(), Ok(Some(ByteStr(mutbytes!(b"Accept")))));

        assert_matches!(lines.next_val(), mut list => {
            assert_matches!(list.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"text/*");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"q")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"0.3"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(list.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"text/plain");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"q")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"0.7"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(list.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"text/plain");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"format")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"flowed"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(list.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"text/plain");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"format")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"fixed"))));

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"q")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"0.4"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(list.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"*/*");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"q")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"0.5"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(list.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_delimiter() {
        let mut input = *b"Content-Type: ;boundary7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Err(error::ErrorValue::MissingParameterDelimiter));
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_key() {
        let mut input = *b"Content-Type: ;=7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Err(error::ErrorValue::MissingParameterKey));
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_err_missing_parameter_value() {
        let mut input = *b"Content-Type: ;boundary=\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::MissingParameterValue));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_err_invalid_parameter_key() {
        let mut input = *b"Content-Type: ;bo\0undary=7ma4ywxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Err(error::ErrorValue::InvalidFieldParameter('\0')));
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_parameters_err_invalid_parameter_value() {
        let mut input = *b"Content-Type: ;boundary=7ma\04ywxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::InvalidFieldParameter('\0')));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_element_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-Dates"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Sat, 04 May 1996");
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_element_empty() {
        let mut input = *b"Example-Dates: \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-Dates"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_list_simple() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"Wed, 14 Sep 2005\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-Dates"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Sat, 04 May 1996");
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Wed, 14 Sep 2005");
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_list_empty_element() {
        let mut input = *b"Example-Dates: \"Sat, 04 May 1996\", \"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Example-Dates"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"Sat, 04 May 1996");
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);
                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"multipart/form-data");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"7MA4YWxk"))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_pair_element() {
        let mut input = *b"Content-Type: \"\\\"text/plain\\\"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"\"text/plain\"");
                assert_eq!(parameters.next_key(), Ok(None));
            });
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_pair_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"\\\"\\7MA4Y\\Wx\\kTrZu0gW\\\"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Ok(ByteStr(mutbytes!(b"\"7MA4YWxkTrZu0gW\""))));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_element() {
        let mut input = *b"Content-Type: \"multipart/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_eq!(line.next_val(), Err(error::ErrorValue::MissingClosingQuote));
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_missing_closing_quote_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxk\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"multipart/form-data");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::MissingClosingQuote));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_element() {
        let mut input = *b"Content-Type: \"multipart\"/form-data\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_eq!(line.next_val(), Err(error::ErrorValue::LeftoverBytes));
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_leftover_bytes_parameter() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"7MA4YWxkTr\"Zu0gW\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"multipart/form-data");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::LeftoverBytes));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_missing_parameter_value() {
        let mut input = *b"Content-Type: multipart/form-data; boundary=\"\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, b"multipart/form-data");

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::MissingParameterValue));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_invalid_pair_element() {
        let mut input = *b"Content-Type: \"text\\\0/plain\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_eq!(line.next_val(), Err(error::ErrorValue::InvalidEscapeSequence('\0')));
            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }

    #[test]
    fn header_line_quoted_err_invalid_pair_parameter() {
        let mut input = *b"Content-Type: ; boundary=\"7MA4Y\\\0WxkTrZu0gW\"\r\n\r\n";
        let mut lines = Headers::new(&mut input);

        assert_eq!(
            lines.next_key(),
            Ok(Some(ByteStr(mutbytes!(b"Content-Type"))))
        );

        assert_matches!(lines.next_val(), mut line => {
            assert_matches!(line.next_val(), Ok(Some((element, mut parameters))) => {
                assert_eq!(element, []);

                assert_eq!(parameters.next_key(), Ok(Some(ByteStr(mutbytes!(b"boundary")))));
                assert_eq!(parameters.next_val(), Err(error::ErrorValue::InvalidEscapeSequence('\0')));

                assert_eq!(parameters.next_key(), Ok(None));
            });

            assert_eq!(line.next_val(), Ok(None));
        });

        assert_eq!(lines.next_key(), Ok(None));
    }
}
