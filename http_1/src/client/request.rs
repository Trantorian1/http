use http_primitives::prelude::*;

use crate::prelude::*;

/// A [`Client`] request to be sent out to an HTTP/1.1 [Server].
pub struct Request<'buf, 'data, 'stream, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    stream: &'stream mut W,
}

impl<'buf, 'data, 'writer, W> Request<'buf, 'data, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    /// Creates a new request builder.
    pub fn new(buffer: &'buf mut BufferForWriting<'data>, stream: &'writer mut W) -> Self {
        Self { buffer, stream }
    }

    /// Creates a new [`GET`] [`RequestLine`].
    ///
    /// [`GET`]: methods::GET
    #[must_use]
    pub fn get<'inner>(self) -> RequestLine<'buf, 'data, 'inner, 'writer, W> {
        RequestLine::get(self.buffer, self.stream)
    }
}

impl<'buf, 'data, W> std::fmt::Debug for Request<'buf, 'data, '_, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestHandle")
            .field("buffer", &self.buffer)
            .finish()
    }
}

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
/// # Drop
///
/// This struct will [`clear`] the backing buffer on drop to ensure future [`Request`]s cannot read
/// past data.
///
/// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-request-line
/// [Section 11.2]: https://datatracker.ietf.org/doc/html/rfc9112#request.smuggling
/// [`clear`]: http_primitives::Buffer::clear
pub struct RequestLine<'buf, 'data, 'payload, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    stream: &'writer mut W,

    method: &'payload [u8],
    target: &'payload [u8],
}

impl<'buf, 'data, 'payload, 'writer, W> RequestLine<'buf, 'data, 'payload, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    /// Crates a new [`GET`] request line.
    ///
    /// [`GET`]: methods::GET
    pub fn get(buffer: &'buf mut BufferForWriting<'data>, stream: &'writer mut W) -> Self {
        Self {
            buffer,
            stream,

            method: methods::GET,
            target: b"/",
        }
    }

    /// Sets the target of the request.
    ///
    /// By default the target is set to '/'.
    #[must_use]
    pub fn target(mut self, target: &'payload [u8]) -> Self {
        self.target = target;
        self
    }

    /// Sends out the request.
    ///
    /// Keep in mind that sending requests is not automatic: method requests will not be sent out
    /// unless you call this function!
    ///
    /// # Errors
    ///
    /// Errors in case writing to the underlying stream fails.
    pub fn send(self) -> std::io::Result<()> {
        self.buffer.write_out(self.stream, |writer| {
            writer.write(self.method)?;
            writer.write(SP)?;
            writer.write(self.target)?;
            writer.write(SP)?;
            writer.write(PROTOCOL)?;
            writer.write(CRLF)?;
            writer.write(CRLF)?;
            Ok(())
        })
    }
}

impl<'buf, 'data, W> Drop for RequestLine<'buf, 'data, '_, '_, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    fn drop(&mut self) {
        self.buffer.clear();
    }
}

impl<'buf, 'data, W> std::fmt::Debug for RequestLine<'buf, 'data, '_, '_, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("buffer", &self.buffer)
            .field("method", &self.method)
            .field("target", &self.target)
            .finish()
    }
}
