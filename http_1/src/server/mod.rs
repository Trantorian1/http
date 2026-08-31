//! HTTP/1.1 server implementation.
//!
//! [`Server`]s are implemented as stateless message handlers, with each instance responsible for
//! it's own stack-allocated memory [`Buffer`]. This makes it easy to spawn instances across
//! multiple threads in order to support concurrent request handling without any form of locking or
//! memory contention.

use http_primitives::prelude::*;

mod request;
mod response;

pub use request::*;
pub use response::*;

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
    pub fn process<'buf, 'stream, RW>(
        &'buf mut self,
        stream: &'stream mut RW,
    ) -> RequestHandle<'buf, 'data, 'stream, RW>
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
///
/// # Drop
///
/// This `struct` will [`clear`] both the global request buffer and global response buffer on drop
/// to ensure future requests cannot read past data.
///
/// [`clear`]: http_primitives::Buffer::clear
/// past data.
pub struct RequestHandle<'buf, 'data, 'stream, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    stream: &'stream mut RW,
    global_request_buffer: &'buf mut BufferForReading<'data>,
    global_response_buffer: &'buf mut BufferForWriting<'data>,
}

impl<'buf, 'data, RW> RequestHandle<'buf, 'data, '_, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    /// Respond to a TCP [`Request`].
    pub fn respond(self, f: fn(RequestInfo<'_>, Response<'_, 'data, RW>) -> std::io::Result<()>) {
        let res = match Request::new(self.global_request_buffer, self.stream).process() {
            Err(status) => Response::new(self.global_response_buffer, self.stream)
                .with_status_code(status)
                .send(),
            Ok(request) => {
                let response = Response::new(self.global_response_buffer, self.stream);
                f(request, response)
            },
        };

        if let Err(err) = res {
            tracing::error!("Failed to send data back to TPC stream: {err}");
        }
    }
}

impl<'buf, 'data, RW> Drop for RequestHandle<'buf, 'data, '_, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    fn drop(&mut self) {
        self.global_request_buffer.clear();
        self.global_response_buffer.clear();
    }
}

impl<'buf, 'data, RW> std::fmt::Debug for RequestHandle<'buf, 'data, '_, RW>
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
