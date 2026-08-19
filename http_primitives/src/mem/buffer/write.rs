use crate::prelude::*;

pub struct WriteOut;

/// Misuse-resistant [`Buffer`] mutator. Allows the user to specify which parts of the HTTP message
/// to push to a given [`Writer`] while handling flushing and other buffering operations.
///
/// [`Writer`]: std::io::Write
pub struct BufWriter<'buf, 'data, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    writer: &'writer mut W,
}

impl<'data> Buffer<'data, WriteOut> {
    /// Writes out a set of bytes to a [`Writer`], guaranteeing proper flushing. See [`BufWriter`]
    /// for a list of available writing methods.
    ///
    /// ```rust
    /// # use http_primitives::prelude::*;
    /// # let mut backing = [0; 64 * KB];
    /// # let mut buffer = BufferForWriting::new(&mut backing);
    /// # let mut stream = Vec::<u8>::new();
    /// buffer.write_out(&mut stream, |writer| {
    ///     writer.write(b"HTTP/1.1 200 OK\r\n")
    /// });
    /// ```
    ///
    /// [`Writer`]: std::io::Write
    pub fn write_out<'buf, 'writer, W: std::io::Write>(
        &'buf mut self,
        writer: &'writer mut W,
        apply_writes: impl FnOnce(&mut BufWriter<'buf, 'data, 'writer, W>) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        // Make sure we are not re-using data from previous requests/responses.
        self.clear();

        let mut buf_writer = BufWriter::new(self, writer);
        apply_writes(&mut buf_writer)?;
        buf_writer.flush()
    }

    /// Appends data to the [`Buffer`], returning any bytes which could not be written. If this
    /// happens the buffer will have to be manually flushed.
    fn append_to<'b>(&mut self, data: &'b [u8]) -> &'b [u8] {
        let index_start = self.window.end;
        let index_stop = self.window.end + data.len();
        let capacity = self.capacity();

        if index_stop < capacity {
            // Buffer is large enough to fit all of `data`, copy it in.

            self.buffer[index_start..index_stop].copy_from_slice(data);
            self.window.end += data.len();

            &data[0..0]
        } else {
            // Too little free space, `data` will have to be partitioned and flushed in parts.

            let available_space = capacity - index_start;

            self.buffer[index_start..].copy_from_slice(&data[..available_space]);
            self.window.end = capacity;

            &data[available_space..]
        }
    }
}

impl<'buf, 'data, 'writer, W> BufWriter<'buf, 'data, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    /// New [`BufWriter`]s cannot be created by the user: they are only exposed by [`write_out`] as
    /// a safe writing interface.
    ///
    /// [`write_out`]: Buffer::write_out
    pub(crate) fn new(buffer: &'buf mut BufferForWriting<'data>, writer: &'writer mut W) -> Self {
        Self { buffer, writer }
    }

    /// Writes new data to a [`Buffer`], handling flushing and buffering.
    pub fn write(&mut self, mut data: &[u8]) -> std::io::Result<()> {
        assert!(!data.is_empty());

        loop {
            data = self.buffer.append_to(data);

            if !data.is_empty() {
                self.writer.write_all(self.buffer.as_ref())?;
                self.buffer.clear();
            } else {
                break Ok(());
            }
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.write_all(self.buffer.as_ref())
    }
}
