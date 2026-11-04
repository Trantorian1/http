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
    global_buffer_request: BufferForReading<'data>,
    global_buffer_response: BufferForWriting<'data>,
    global_buffer_url: &'data mut [u8],
}

impl<'data> Server<'data> {
    /// Creates a new server instance.
    pub fn new(
        global_buffer_request: &'data mut [u8],
        global_buffer_response: &'data mut [u8],
        global_buffer_url: &'data mut [u8],
    ) -> Self {
        Self {
            global_buffer_request: BufferForReading::new(global_buffer_request),
            global_buffer_response: BufferForWriting::new(global_buffer_response),
            global_buffer_url,
        }
    }

    /// Processes a stream of bytes into a [`RequestHandle`] which can be used to send back an HTTP
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
            global_buffer_request: &mut self.global_buffer_request,
            global_buffer_response: &mut self.global_buffer_response,
            global_buffer_url: self.global_buffer_url,
        }
    }
}

/// TPC input stream parser and response handler.
///
/// # Drop
///
/// This `struct` will [`clear`] all global buffers on drop to ensure future requests cannot read
/// past data.
///
/// [`clear`]: http_primitives::Buffer::clear
/// past data.
pub struct RequestHandle<'buf, 'data, 'stream, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    stream: &'stream mut RW,
    global_buffer_request: &'buf mut BufferForReading<'data>,
    global_buffer_response: &'buf mut BufferForWriting<'data>,
    global_buffer_url: &'buf mut [u8],
}

impl<'buf, 'data, RW> RequestHandle<'buf, 'data, '_, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    /// Respond to a TCP [`Request`].
    pub fn respond(self, f: fn(RequestInfo<'_>, Response<'_, 'data, RW>) -> std::io::Result<()>) {
        let request = Request::new(
            self.global_buffer_request,
            self.global_buffer_url,
            self.stream,
        );

        let res = match request.process() {
            Err(status) => Response::new(self.global_buffer_response, self.stream)
                .with_status_code(status)
                .send(),
            Ok(info) => {
                let response = Response::new(self.global_buffer_response, self.stream);
                f(info, response)
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
        self.global_buffer_request.clear();
        self.global_buffer_response.clear();
        self.global_buffer_url.fill(0);
    }
}

impl<'buf, 'data, RW> std::fmt::Debug for RequestHandle<'buf, 'data, '_, RW>
where
    'data: 'buf,
    RW: std::io::Read + std::io::Write,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestHandle")
            .field("global_buffer_request", &self.global_buffer_request)
            .field("global_buffer_response", &self.global_buffer_response)
            .field("global_buffer_url", &self.global_buffer_url)
            .finish()
    }
}
