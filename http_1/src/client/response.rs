use http_primitives::prelude::*;

use crate::prelude::*;

pub struct ResponseHandle<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    buffer: &'buf mut BufferForReading<'data>,
    stream: &'reader mut R,
}

impl<'buf, 'data, 'reader, R> ResponseHandle<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    pub fn new(buffer: &'buf mut BufferForReading<'data>, stream: &'reader mut R) -> Self {
        Self { buffer, stream }
    }

    pub fn process(self) -> Result<ResponseInfo<'buf, 'data>, Status> {
        let (protocol, status) = self.buffer.read_in(self.stream, |reader| {
            let protocol = reader.read(parsers::protocol)?;

            reader.read(parsers::sp)?;

            let status = reader.read(parsers::status)?;

            // TODO: add status message parsing

            Ok((protocol, status))
        })?;

        Ok(ResponseInfo {
            buffer: self.buffer,
            protocol,
            status,
        })
    }
}

pub struct ResponseInfo<'buf, 'data>
where
    'data: 'buf,
{
    buffer: &'buf mut BufferForReading<'data>,

    protocol: std::ops::Range<usize>,
    status: std::ops::Range<usize>,
}

impl<'buf, 'data> ResponseInfo<'buf, 'data>
where
    'data: 'buf,
{
    #[must_use]
    pub fn protocol(&self) -> &[u8] {
        &self.buffer[self.protocol.clone()]
    }

    #[must_use]
    pub fn status(&self) -> &[u8] {
        &self.buffer[self.status.clone()]
    }
}

impl<'buf, 'data> Drop for ResponseInfo<'buf, 'data>
where
    'data: 'buf,
{
    fn drop(&mut self) {
        self.buffer.clear();
    }
}
