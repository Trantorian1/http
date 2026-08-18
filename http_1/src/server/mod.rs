//! HTTP/1.1 server implementation.
//!
//! [`Server`]s are grouped by instances, with each instance responsible for handling it's own
//! stack-allocated memory [`Buffer`]. This makes it possible to spawn multiple instances, for
//! example across multiple threads, in order to support concurrent request handling without any
//! form of locking or memory contention.

use crate::prelude::*;
use http_core::prelude::*;

/// HTTP/1.1 server instance, handles connection responses and ensures proper flushing between
/// requests.
pub struct Server<'data> {
    global_request_buffer: BufferForReading<'data>,
    global_response_buffer: BufferForWriting<'data>,
}

impl<'data> Server<'data> {
    pub fn new(
        global_request_buffer: &'data mut [u8],
        global_response_buffer: &'data mut [u8],
    ) -> Self {
        Self {
            global_request_buffer: BufferForReading::new(global_request_buffer),
            global_response_buffer: BufferForWriting::new(global_response_buffer),
        }
    }

    /// Processes a stream of bytes into a [`RequestHandle`] which can be used to send back a
    /// [`Response`].
    pub fn process<RW>(&mut self, stream: RW) -> RequestHandle<'_, 'data, RW>
    where
        RW: std::io::Read + std::io::Write,
    {
        RequestHandle {
            stream,
            global_request_buffer: &mut self.global_request_buffer,
            global_response_buffer: &mut self.global_response_buffer,
        }
    }
}

/// TPC input stream parser and response handler.
pub struct RequestHandle<'buf, 'data, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    stream: RW,
    global_request_buffer: &'buf mut BufferForReading<'data>,
    global_response_buffer: &'buf mut BufferForWriting<'data>,
}

impl<'buf, 'data, RW> RequestHandle<'buf, 'data, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    /// Respond to a TCP [`Request`].
    ///
    /// ```rust
    /// # use http_core::prelude::*;
    /// # use http_1::prelude::*;
    /// #
    /// # use std::io::Write as _;
    /// #
    /// # let mut stream_buffer = [0; 8 * KB];
    /// # let mut stream = ByteStream::new(&mut stream_buffer);
    /// # stream.write(b"GET / HTTP/1.1\r\n\r\n").unwrap();
    /// #
    /// # let mut global_request_buffer = [0; 8 * KB];
    /// # let mut global_response_buffer = [0; 64 * KB];
    /// # let mut server = Server::new(&mut global_request_buffer, &mut global_response_buffer);
    /// server
    ///     .process(stream)
    ///     .respond(|request, response| match request.target {
    ///         b"/" => response
    ///             .with_status_code(Status::Ok)
    ///             .send(),
    ///         _ => response
    ///             .with_status_code(Status::NotFound)
    ///             .send(),
    ///     });
    /// ```
    pub fn respond(
        mut self,
        f: fn(RequestInfo<'buf>, Response<'buf, 'data, '_, RW>) -> std::io::Result<()>,
    ) {
        let res = match Request::new(self.global_request_buffer, &mut self.stream).process() {
            Err(status) => Response::new(self.global_response_buffer, &mut self.stream)
                .with_status_code(status)
                .send(),
            Ok(request) => {
                let response = Response::new(self.global_response_buffer, &mut self.stream);
                f(request, response)
            }
        };

        if let Err(err) = res {
            tracing::error!("Failed to send data back to TPC stream: {err}");
        }
    }
}
