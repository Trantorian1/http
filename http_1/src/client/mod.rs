use http_primitives::prelude::*;

mod request;
mod response;

pub use request::*;
pub use response::*;

pub struct Client<'data> {
    local_request_buffer: BufferForWriting<'data>,
    local_response_buffer: BufferForReading<'data>,
}

impl<'data> Client<'data> {
    pub fn new(
        local_request_buffer: &'data mut [u8],
        local_response_buffer: &'data mut [u8],
    ) -> Self {
        Self {
            local_request_buffer: BufferForWriting::new(local_request_buffer),
            local_response_buffer: BufferForReading::new(local_response_buffer),
        }
    }

    pub fn request<'buf, 'writer, W: std::io::Write>(
        &'buf mut self,
        stream: &'writer mut W,
    ) -> RequestHandle<'buf, 'data, 'writer, W> {
        RequestHandle::new(&mut self.local_request_buffer, stream)
    }

    pub fn response<'buf, 'reader, R: std::io::Read>(
        &'buf mut self,
        stream: &'reader mut R,
    ) -> ResponseHandle<'buf, 'data, 'reader, R> {
        ResponseHandle::new(&mut self.local_response_buffer, stream)
    }
}
