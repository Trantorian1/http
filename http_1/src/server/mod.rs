//! HTTP/1.1 server implementation.
//!
//! [`Server`]s are implemented as stateless message handlers, with each instance responsible for
//! it's own stack-allocated memory [`Buffer`]. This makes it easy to spawn instances across
//! multiple threads in order to support concurrent request handling without any form of locking or
//! memory contention.

use http_primitives::prelude::*;

use crate::prelude::*;

/// HTTP/1.1 server instance, handles connection responses and ensures proper flushing between
/// requests.
///
/// Note that servers only handle message responses. Request must instead be fed manually from a
/// separate [`TcpListener`] using [`process`].
///
/// [`TcpListener`]: std::net::TcpListener
/// [`process`]: Self::process
#[derive(Debug)]
pub struct Server<'data> {
    global_request_buffer: BufferForReading<'data>,
    global_response_buffer: BufferForWriting<'data>,
}

impl<'data> Server<'data> {
    /// Creates a new server instance.
    ///
    /// [`TcpListener`]: std::net::TcpListener
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
    pub fn respond(
        mut self,
        f: fn(RequestInfo<'buf, 'data>, Response<'buf, 'data, '_, RW>) -> std::io::Result<()>,
    ) {
        let res = match Request::new(self.global_request_buffer, &mut self.stream).process() {
            Err(status) => Response::new(self.global_response_buffer, &mut self.stream)
                .with_status_code(status)
                .send(),
            Ok(request) => {
                let response = Response::new(self.global_response_buffer, &mut self.stream);
                f(request, response)
            },
        };

        if let Err(err) = res {
            tracing::error!("Failed to send data back to TPC stream: {err}");
        }
    }
}

impl<'buf, 'data, RW> std::fmt::Debug for RequestHandle<'buf, 'data, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestHandle")
            .field("global_request_buffer", &self.global_request_buffer)
            .field("global_response_buffer", &self.global_response_buffer)
            .finish()
    }
}
