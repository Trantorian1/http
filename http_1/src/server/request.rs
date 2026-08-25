//! Stack-allocated HTTP request helpers.
//!
//! All requests are written to a fixed-size [`Buffer`]. Keep in mind that parsing **WILL** fail in
//! case the buffer is too small to process the entire request, in which case the server **SHOULD**
//! return [`413`].
//!
//! [`Buffer`]: Buffer
//! [`413`]: Status::ContentTooLarge

use http_primitives::prelude::*;

use crate::prelude::*;

/// See [RFC9112], request line.
///
/// > _"A request-line begins with a method token, followed by a single space (SP), the
/// > request-target, and another single space (SP), and ends with the protocol version."_
/// >
/// > ```text
/// > request-line   = method SP request-target SP HTTP-version
/// > ```
/// >
/// > _"Although the request-line grammar rule requires that each of the component elements be
/// > separated by a single SP octet, recipients **MAY** instead parse on whitespace-delimited word
/// > boundaries and, aside from the CRLF terminator, treat any form of whitespace as the SP
/// > separator while ignoring preceding or trailing whitespace; such whitespace includes one or
/// > more of the following octets: SP, HTAB, VT (%x0B), FF (%x0C), or bare CR. However, lenient
/// > parsing can result in request smuggling security vulnerabilities if there are multiple
/// > recipients of the message and each has its own unique interpretation of robustness (see
/// > [Section 11.2])."_
///
/// Note that since the request implements buffer-based parsing it will fail if the content of the
/// request cannot fit in the target buffer, in which case the server will respond with a
/// [`ContentTooLarge`] status code.
///
/// # Usage
///
/// This struct is not meant to be consumed as is: to read the contents a byte stream request you
/// must first [`process`] it, after which you will be able to read the resulting [`RequestInfo`].
///
/// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-request-line
/// [Section 11.2]: https://datatracker.ietf.org/doc/html/rfc9112#request.smuggling
/// [`ContentTooLarge`]: Status::ContentTooLarge
/// [`process`]: Self::process
pub struct Request<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    buffer: &'buf mut BufferForReading<'data>,
    stream: &'reader mut R,
}

impl<'buf, 'data, 'reader, R> Request<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    /// Initializes a new [`Request`].
    pub fn new(buffer: &'buf mut BufferForReading<'data>, stream: &'reader mut R) -> Self {
        Self { buffer, stream }
    }

    /// Parses a byte stream into an HTTP/1.1 request, validating format along the way.
    ///
    /// # Errors
    ///
    /// Returns an error [`Status`] code if request parsing fails.
    pub fn process(self) -> Result<RequestInfo<'buf, 'data>, Status> {
        let (method, target) = self.buffer.read_in(self.stream, |reader| {
            let method = reader.read(parsers::method)?;

            reader.read(parsers::sp)?;

            let target = reader.read(parsers::target)?;

            reader.read(parsers::sp)?;
            reader.read(parsers::protocol)?;
            reader.read(parsers::crlf)?;

            // while let Some(header) = reader.read(header)? {
            //     // process header
            // }

            Ok((method, target))
        })?;

        Ok(RequestInfo {
            buffer: self.buffer,
            method,
            target,
        })
    }
}

impl<'buf, 'data, R> std::fmt::Debug for Request<'buf, 'data, '_, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("buffer", &self.buffer)
            .finish()
    }
}

/// Zero-copy slice of the parsed [`Request`] data, allows to index into specific parts of the
/// request.
///
/// # Drop
///
/// This `struct` will [`clear`] the backing buffer on drop to ensure future requests cannot read
/// past data.
///
/// [`clear`]: http_primitives::Buffer::clear
pub struct RequestInfo<'buf, 'data>
where
    'data: 'buf,
{
    buffer: &'buf mut BufferForReading<'data>,

    /// See [RFC9112], method.
    ///
    /// > _"The method token indicates the request method to be performed on the target resource.
    /// > The request method is case-sensitive."_
    /// >
    /// > ```text
    /// > method         = token
    /// > ```
    /// >
    /// > _"The request methods defined by this specification can be found in [Section 9] of
    /// > [\[HTTP\]], along with information regarding the HTTP method registry and considerations for
    /// > defining new methods."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-method
    /// [Section 9]: https://www.rfc-editor.org/rfc/rfc9110#section-9
    /// [\[HTTP\]]: https://datatracker.ietf.org/doc/html/rfc9110
    method: std::ops::Range<usize>,

    /// See [RFC9112], request target.
    ///
    /// > _"The request-target identifies the target resource upon which to apply the request. The
    /// > client derives a request-target from its desired target URI. There are four distinct
    /// > formats for the request-target, depending on both the method being requested and whether
    /// > the request is to a proxy."_
    /// >
    /// > ```text
    /// > request-target = origin-form
    /// >                  / absolute-form
    /// >                  / authority-form
    /// >                  / asterisk-form
    /// > ```
    /// >
    /// > _"No whitespace is allowed in the request-target. Unfortunately, some user agents fail to
    /// > properly encode or exclude whitespace found in hypertext references, resulting in those
    /// > disallowed characters being sent as the request-target in a malformed request-line."_
    /// >
    /// > _"Recipients of an invalid request-line **SHOULD** respond with either a [`400`]
    /// > (Bad Request) error or a 301 (Moved Permanently) redirect with the request-target properly
    /// > encoded. A recipient **SHOULD NOT** attempt to autocorrect and then process the request
    /// > without a redirect, since the invalid request-line might be deliberately crafted to bypass
    /// > security filters along the request chain."_
    /// >
    /// > _"A server **MUST** respond with a [`400`] (Bad Request) status code to any HTTP/1.1
    /// > request message that lacks a Host header field and to any request message that contains
    /// > more than one Host header field line or a Host header field with an invalid field value."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#section-3.2
    /// [`400`]: Status::BadRequest
    target: std::ops::Range<usize>,
}

impl<'buf, 'data> RequestInfo<'buf, 'data>
where
    'data: 'buf,
{
    #[must_use]
    /// Returns the request [method].
    ///
    /// [method]: crate::methods
    pub fn method(&self) -> &[u8] {
        &self.buffer[self.method.clone()]
    }

    #[must_use]
    /// Returns the target of the request.
    pub fn target(&self) -> &[u8] {
        &self.buffer[self.target.clone()]
    }
}

impl<'buf, 'data> Drop for RequestInfo<'buf, 'data>
where
    'data: 'buf,
{
    fn drop(&mut self) {
        // Clears the buffer once we are done processing the request, that way data cannot be
        // read by future calls if ever we mess up indexing in our `Buffer` implementation.
        self.buffer.clear();
    }
}

impl<'buf, 'data> std::fmt::Debug for RequestInfo<'buf, 'data>
where
    'data: 'buf,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestInfo")
            .field(
                "method",
                &std::str::from_utf8(self.method()).unwrap_or("non-utf8 method"),
            )
            .field(
                "target",
                &std::str::from_utf8(self.target()).unwrap_or("non-utf8 target"),
            )
            .finish()
    }
}
