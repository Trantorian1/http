use http_primitives::prelude::*;

use crate::prelude::*;

/// Parses HTTP/1.1 [Server] response messages.
pub struct Response<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    buffer: &'buf mut BufferForReading<'data>,
    stream: &'reader mut R,
}

impl<'buf, 'data, 'reader, R> Response<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    /// Creates a new response handler.
    pub fn new(buffer: &'buf mut BufferForReading<'data>, stream: &'reader mut R) -> Self {
        Self { buffer, stream }
    }

    /// Tries to parse the associated HTTP/1.1 stream into a [`ResponseInfo`] `struct`.
    ///
    /// # Errors
    ///
    /// Returns an error [`Status`] code if parsing the [`Server`]'s response fails.
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

impl<'buf, 'data, R> std::fmt::Debug for Response<'buf, 'data, '_, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseHandle")
            .field("buffer", &self.buffer)
            .finish()
    }
}

/// Zero-copy response fields.
///
/// # Drop
///
/// This `struct` will [`clear`] its backing buffer on drop to ensure future responses cannot read
/// past data.
///
/// [`clear`]: http_primitives::Buffer::clear
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
    /// Returns the HTTP protocol in use.
    pub fn protocol(&self) -> &[u8] {
        &self.buffer[self.protocol.clone()]
    }

    #[must_use]
    /// Returns the response [Status] code.
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

impl<'buf, 'data> std::fmt::Debug for ResponseInfo<'buf, 'data>
where
    'data: 'buf,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseInfo")
            .field(
                "protocol",
                &std::str::from_utf8(self.protocol()).unwrap_or_default(),
            )
            .field(
                "status",
                &std::str::from_utf8(self.status()).unwrap_or_default(),
            )
            .finish()
    }
}
