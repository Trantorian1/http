//! HTTP/1.1 zerocopy client for querying local or remote [`Server`]s.
//!
//! [`Server`]: crate::Server

use http_primitives::prelude::*;

mod request;
mod response;

pub use request::*;
pub use response::*;

/// HTTP/1.1 client instance. Sends out requests via an established connection [stream].
///
/// [stream]: std::net::TcpStream
#[derive(Debug)]
pub struct Client<'data> {
    local_request_buffer: BufferForWriting<'data>,
    local_response_buffer: BufferForReading<'data>,
}

impl<'data> Client<'data> {
    /// Creates a new client instance.
    pub fn new(
        local_request_buffer: &'data mut [u8],
        local_response_buffer: &'data mut [u8],
    ) -> Self {
        Self {
            local_request_buffer: BufferForWriting::new(local_request_buffer),
            local_response_buffer: BufferForReading::new(local_response_buffer),
        }
    }

    /// Initializes a new request builder.
    ///
    /// Client requests will be written to the provided bytes stream.
    pub fn request<'buf, 'writer, W: std::io::Write>(
        &'buf mut self,
        stream: &'writer mut W,
    ) -> Request<'buf, 'data, 'writer, W> {
        Request::new(&mut self.local_request_buffer, stream)
    }

    /// Initializes a new byte stream decoder which can be used to parse [`Server`] responses.
    ///
    /// [`Server`]: crate::Server
    pub fn response<'buf, 'reader, R: std::io::Read>(
        &'buf mut self,
        stream: &'reader mut R,
    ) -> Response<'buf, 'data, 'reader, R> {
        Response::new(&mut self.local_response_buffer, stream)
    }
}
