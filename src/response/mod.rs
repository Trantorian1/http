//! Stack-allocated HTTP response helpers.
//!
//! All responses are written to a fixed-size [`Buffer`] and are only flushed as capacity is
//! reached.
//!
//! [`Buffer`]: buffer::Buffer

pub mod buffer;
pub mod code;

pub use buffer::Buffer;
pub use code::Status;

// HTTP protocol version
const PROTOCOL: &[u8] = b"HTTP/1.1 ";

// Carriage Return + Line Feed
const CRLF: &[u8] = b"\r\n";

/// See [RFC9112], message format.
///
/// >  _"An HTTP/1.1 message consists of a start-line followed by a CRLF and a sequence of octets in
/// >  a format similar to the Internet Message Format [RFC5322]: zero or more header field lines
/// >  (collectively referred to as the "headers" or the "header section"), an empty line indicating
/// >  the end of the header section, and an optional message body._
/// >    
/// >  ```txt
/// >  HTTP-message   = start-line CRLF
/// >                   *( field-line CRLF )
/// >                   CRLF
/// >                   [ message-body ]
/// >  ```
/// >    
/// >  _A message can be either a request from client to server or a response from server to client.
/// >  Syntactically, the two types of messages differ only in the start-line, which is either a
/// >  request-line (for requests) or a status-line (for responses), and in the algorithm for
/// >  determining the length of the message body (Section 6)."_
///
/// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#section-2.1
/// [RFC5322]: https://datatracker.ietf.org/doc/html/rfc5322
pub struct Response<'a, const SIZE: usize> {
    stream: std::net::TcpStream,
    status: code::Status,
    buffer: &'a mut buffer::Buffer<SIZE>,
}

impl<'a, const SIZE: usize> Response<'a, SIZE> {
    pub fn new(stream: std::net::TcpStream, buffer: &'a mut buffer::Buffer<SIZE>) -> Self {
        Self {
            stream,
            status: code::Status::default(),
            buffer,
        }
    }

    /// Sets the [`Response`] status code.
    pub fn with_status_code(mut self, status: code::Status) -> Self {
        self.status = status;
        self
    }

    /// Sends out the [`Response`] to the connected HTTP client.
    pub fn respond(self) -> std::io::Result<()> {
        self.buffer.write_out(self.stream, |writer| {
            writer.write(PROTOCOL)?;
            writer.write(self.status.code())?;
            writer.write(self.status.reason())?;
            writer.write(CRLF)?;
            writer.write(CRLF)?;
            Ok(())
        })
    }
}
