use http_primitives::prelude::*;

use crate::prelude::*;

/// See [RFC9112], message format.
///
/// > _"An HTTP/1.1 message consists of a start-line followed by a CRLF and a sequence of octets in
/// > a format similar to the Internet Message Format [RFC5322]: zero or more header field lines
/// > (collectively referred to as the "headers" or the "header section"), an empty line indicating
/// > the end of the header section, and an optional message body._
/// >
/// > ```txt
/// > HTTP-message   = start-line CRLF
/// >                  *( field-line CRLF )
/// >                  CRLF
/// >                  [ message-body ]
/// > ```
/// >
/// > _A message can be either a request from client to server or a response from server to client.
/// > Syntactically, the two types of messages differ only in the start-line, which is either a
/// > request-line (for requests) or a status-line (for responses), and in the algorithm for
/// > determining the length of the message body (Section 6)."_
///
/// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#section-2.1
/// [RFC5322]: https://datatracker.ietf.org/doc/html/rfc5322
pub struct Response<'buf, 'data, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    stream: &'buf mut W,

    // content
    content_type: &'static [u8],
    content: &'buf [u8],

    status: Status,
}

impl<'buf, 'data, W> Response<'buf, 'data, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    /// Initializes a new [`Response`]
    pub fn new(buffer: &'buf mut BufferForWriting<'data>, stream: &'buf mut W) -> Self {
        Self {
            stream,
            status: Status::default(),

            content_type: b"",
            content: b"",

            buffer,
        }
    }

    /// Sets the response status code.
    #[must_use]
    pub fn with_status_code(mut self, status: Status) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn with_content(mut self, content_type: &'static [u8], content: &'buf [u8]) -> Self {
        self.content_type = content_type;
        self.content = content;
        self
    }

    /// Sends out the response back to the connected HTTP client.
    ///
    /// # Errors
    ///
    /// Errors in case writing to the underlying stream fails.
    pub fn send(self) -> std::io::Result<()> {
        if let Status::InternalServerError(err) = &self.status {
            tracing::error!(err);
        }

        self.buffer.write_out(self.stream, |writer| {
            writer.write(PROTOCOL)?;
            writer.write(SP)?;
            writer.write(self.status.code())?;
            writer.write(SP)?;
            writer.write(self.status.reason())?;
            writer.write(CRLF)?;

            // content
            if !self.content_type.is_empty() && !self.content.is_empty() {
                writer.write(headers::CONTENT_TYPE)?;
                writer.write(self.content_type)?;
                writer.write(CRLF)?;

                let mut itoa = itoa::Buffer::new();
                let content_length = itoa.format(self.content.len());

                writer.write(headers::CONTENT_LENGTH)?;
                writer.write(content_length.as_bytes())?;
                writer.write(CRLF)?;

                writer.write(CRLF)?;

                writer.write(self.content)?;
            } else {
                writer.write(CRLF)?;
            }

            Ok(())
        })
    }
}

// impl<'buf, 'data, W> Drop for Response<'buf, 'data, '_, W>
// where
//     'data: 'buf,
//     W: std::io::Write,
// {
//     fn drop(&mut self) {
//         // Clears the buffer once we are done processing the request, that way data cannot be
//         // read by future calls if ever we mess up indexing in our `Buffer` implementation.
//         self.buffer.clear();
//     }
// }

impl<'buf, 'data, W> std::fmt::Debug for Response<'buf, 'data, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("buffer", &self.buffer)
            .field("status", &self.status)
            .finish()
    }
}
