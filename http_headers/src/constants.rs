//! Special byte segments referred to in parsing.

/// Carriage-Return Line-Feed. Separates each line in a header segment.
pub const CRLF: &[u8] = b"\r\n";

/// Optional Whitespace
///
/// # Specs
///
/// See [section 5.6.3], Whitespace of [RFC9110], HTTP Semantics.
///
/// [Section 5.6.3]: https://www.rfc-editor.org/rfc/rfc9110.html#name-whitespace
/// [RFC9110]: https://www.rfc-editor.org/rfc/rfc9110.html
pub const OWS: &[u8] = b" \t";
