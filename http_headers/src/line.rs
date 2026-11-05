//! Header line scanning primitives.
//!
//! See [RFC9112], header fields, for the wire grammar:
//!
//! > ```text
//! > header-field   = field-name ":" OWS field-value OWS
//! > ```
//!
//! [RFC9112]: https://www.rfc-editor.org/rfc/rfc9112#section-5

use crate::error::Error;

/// A single parsed header line.
pub(super) struct Line<'a> {
    /// The field name, excluding the terminating colon.
    pub(super) name: &'a [u8],

    /// The field value, with leading and trailing [OWS] removed.
    ///
    /// [OWS]: https://www.rfc-editor.org/rfc/rfc9110#name-whitespace
    pub(super) value: &'a [u8],
}

/// A single space (U+0020).
const SP: u8 = b' ';

/// A horizontal tab (U+0009).
const HTAB: u8 = b'\t';

/// Carriage return line feed.
const CRLF: &[u8] = b"\r\n";

/// Iterates over the header lines in a raw header section.
///
/// The input is expected to start at the first header line and include the terminating blank
/// line. Iteration stops after that blank line; any bytes following it are ignored. A non-empty
/// trailing remainder without a CRLF terminator yields [`Error::Eof`].
pub(super) fn iter_lines(input: &[u8]) -> LineIter<'_> {
    LineIter {
        rest: input,
        done: false,
    }
}

pub(super) struct LineIter<'a> {
    rest: &'a [u8],
    done: bool,
}

impl<'a> Iterator for LineIter<'a> {
    type Item = Result<Line<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.rest.is_empty() {
            return None;
        }

        let Some(pos) = memchr::memmem::find(self.rest, CRLF) else {
            // Bytes remain but no CRLF terminator: the header section is unterminated.
            self.done = true;
            return Some(Err(Error::Eof));
        };

        let line = &self.rest[..pos];
        self.rest = &self.rest[pos + CRLF.len()..];

        if line.is_empty() {
            // Blank line: end of the header section.
            self.done = true;
            return None;
        }

        Some(parse_line(line))
    }
}

/// Parses a single raw header line, rejecting [obs-fold] continuations and validating the field
/// name as a [token].
///
/// # Errors
///
/// Returns [`Error::ObsFold`] if the line begins with whitespace, [`Error::ExpectedColon`] if no
/// colon is present, or [`Error::InvalidToken`] if the field name contains non-token characters.
///
/// [obs-fold]: https://www.rfc-editor.org/rfc/rfc9112#section-5.2
/// [token]: https://www.rfc-editor.org/rfc/rfc9110#name-tokens
fn parse_line(line: &[u8]) -> Result<Line<'_>, Error> {
    // Recipients MUST NOT generate obs-fold; we reject it outright (RFC9112, section 5.2).
    if line.first().is_some_and(|c| is_ow(*c)) {
        return Err(Error::ObsFold);
    }

    let Some(colon) = line.iter().position(|&c| c == b':') else {
        return Err(Error::ExpectedColon);
    };

    let name = &line[..colon];
    if name.is_empty() || !name.iter().all(|&c| is_token_char(c)) {
        return Err(Error::InvalidToken);
    }

    let mut value = &line[colon + 1..];
    while let Some(trimmed) = strip_prefix_ows(value) {
        value = trimmed;
    }
    while let Some(trimmed) = strip_suffix_ows(value) {
        value = trimmed;
    }

    Ok(Line { name, value })
}

/// Checks if a byte is a valid [token] character.
///
/// [token]: https://www.rfc-editor.org/rfc/rfc9110#name-tokens
#[inline]
fn is_token_char(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'!' | b'#'
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
        )
}

/// Checks if a byte is [OWS] (SP or HTAB).
///
/// [OWS]: https://www.rfc-editor.org/rfc/rfc9110#name-whitespace
#[inline]
fn is_ow(c: u8) -> bool {
    matches!(c, SP | HTAB)
}

/// Strips leading [OWS] from a field value.
///
/// [OWS]: https://www.rfc-editor.org/rfc/rfc9110#name-whitespace
#[inline]
fn strip_prefix_ows(bytes: &[u8]) -> Option<&[u8]> {
    let n = bytes.iter().take_while(|c| is_ow(**c)).count();
    (n > 0).then(|| &bytes[n..])
}

/// Strips trailing [OWS] from a field value.
///
/// [OWS]: https://www.rfc-editor.org/rfc/rfc9110#name-whitespace
#[inline]
fn strip_suffix_ows(bytes: &[u8]) -> Option<&[u8]> {
    let n = bytes.iter().rev().take_while(|c| is_ow(**c)).count();
    (n > 0).then(|| &bytes[..bytes.len() - n])
}

/// Case-insensitively matches a wire field name against the desired names.
///
/// Field names are [case-insensitive], so matching compares ASCII lowercase forms byte by byte,
/// exiting on the first mismatch.
///
/// # Returns
///
/// The index of the first matching desired name, if any.
///
/// [case-insensitive]: https://www.rfc-editor.org/rfc/rfc9110#section-5.1
#[inline]
pub(super) fn match_name(name: &[u8], desired: &[&str]) -> Option<usize> {
    for (i, desired_name) in desired.iter().enumerate() {
        if name.eq_ignore_ascii_case(desired_name.as_bytes()) {
            return Some(i);
        }
    }

    None
}
