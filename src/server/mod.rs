//! HTTP/1.1 server implementation.
//!
//! [`Server`]s are grouped by instances, with each instance responsible for handling it's own
//! stack-allocated memory [`Buffer`]. This makes it possible to spawn multiple instances, for
//! example across multiple threads, in order to support concurrent request handling without any
//! form of locking or memory contention.

use crate::prelude::*;

/// HTTP/1.1 server instance, handles connection responses and ensures proper flushing between
/// requests.
pub struct Server<const S1: usize, const S2: usize> {
    global_request_buffer: BufferForReading<S1>,
    global_response_buffer: BufferForWriting<S2>,
}

impl<const S1: usize, const S2: usize> Server<S1, S2> {
    /// Sets the [`Buffer`]  to be used for handling [`Request`]s.
    pub fn with_global_request_buffer<const S1P: usize>(
        self,
        global_request_buffer: BufferForReading<S1P>,
    ) -> Server<S1P, S2> {
        Server {
            global_request_buffer,
            global_response_buffer: self.global_response_buffer,
        }
    }

    /// Sets the [`Buffer`] to be used for handling [`Response`]s.
    pub fn with_global_response_buffer<const S2P: usize>(
        self,
        global_response_buffer: BufferForWriting<S2P>,
    ) -> Server<S1, S2P> {
        Server {
            global_request_buffer: self.global_request_buffer,
            global_response_buffer,
        }
    }

    /// Processes a stream of bytes into a [`RequestHandle`] which can be used to send back a
    /// [`Response`].
    pub fn process<RW>(&mut self, stream: RW) -> RequestHandle<'_, S1, S2, RW>
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

impl Default for Server<{ 8 * size::KB }, { 64 * size::KB }> {
    fn default() -> Self {
        Self {
            global_request_buffer: Buffer::new(),
            global_response_buffer: Buffer::new(),
        }
    }
}

/// TPC input stream parser and response handler.
pub struct RequestHandle<'a, const S1: usize, const S2: usize, RW>
where
    RW: std::io::Read + std::io::Write,
{
    stream: RW,
    global_request_buffer: &'a mut BufferForReading<S1>,
    global_response_buffer: &'a mut BufferForWriting<S2>,
}

impl<'a, const S1: usize, const S2: usize, RW> RequestHandle<'a, S1, S2, RW>
where
    RW: std::io::Read + std::io::Write,
{
    /// Respond to a TCP [`Request`].
    ///
    /// ```rust
    /// # let mut stream = http1::testing::MockTCP::new(*b"GET / HTTP/1.1\r\n\r\n");
    /// # let mut server = http1::Server::default();
    /// server
    ///     .process(stream)
    ///     .respond(|request, response| match request.target {
    ///         b"/" => response
    ///             .with_status_code(http1::code::Status::Ok)
    ///             .respond(),
    ///         _ => response
    ///             .with_status_code(http1::code::Status::NotFound)
    ///             .respond(),
    ///     });
    /// ```
    pub fn respond(
        mut self,
        f: fn(RequestInfo<'_>, Response<'_, '_, S2, RW>) -> std::io::Result<()>,
    ) {
        let res = match Request::new(&mut self.stream, self.global_request_buffer).process() {
            Err(status) => Response::new(&mut self.stream, self.global_response_buffer)
                .with_status_code(status)
                .respond(),
            Ok(request) => {
                let response = Response::new(&mut self.stream, self.global_response_buffer);
                f(request, response)
            }
        };

        if let Err(err) = res {
            tracing::error!("Failed to send data back to TPC stream: {err}");
        }
    }
}
