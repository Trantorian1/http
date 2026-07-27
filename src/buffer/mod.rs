pub mod read;
pub mod write;

/// Stack buffer implementation which guards against invalid writes and ensures proper flushing.
pub struct Buffer<const SIZE: usize> {
    buffer: [u8; SIZE],
    index: usize,
}

impl<const SIZE: usize> Buffer<SIZE> {
    /// Stack-allocates a new [`Buffer`] of the given size.
    pub fn new() -> Self {
        Self {
            buffer: [0; SIZE],
            index: 0,
        }
    }

    /// Writes out a set of bytes to a [`Writer`], guaranteeing proper flushing. See [`write::BufWriter`] for
    /// a list of available writing methods.
    ///
    /// ```rust
    /// # const KB: usize = 1_000;
    /// # let mut buffer = http_server::Buffer::<{64 * KB}>::new();
    /// # let stream = Vec::<u8>::new();
    /// buffer.write_out(stream, |writer| {
    ///     writer.write(b"HTTP/1.1 200 OK\r\n")
    /// });
    /// ```
    ///
    /// [`Writer`]: std::io::Write
    pub fn write_out<W: std::io::Write>(
        &mut self,
        writer: W,
        apply_writes: impl FnOnce(&mut write::BufWriter<'_, SIZE, W>) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let mut view = write::BufWriter::new(self, writer);
        apply_writes(&mut view)?;
        view.flush()
    }

    /// Appends data to the [`Buffer`], returning any bytes which could not be written. If this
    /// happens the buffer will have to be manually flushed.
    fn append<'a>(&mut self, data: &'a [u8]) -> &'a [u8] {
        let index_start = self.index;
        let index_stop = self.index + data.len();

        if index_stop < SIZE {
            // Buffer is large enough to fit all of `data`, copy it in.

            self.buffer[index_start..index_stop].copy_from_slice(data);
            self.index += data.len();

            &data[0..0]
        } else {
            // Too little free space, `data` will have to be partitioned and flushed in parts.

            let available_space = SIZE - index_start;

            self.buffer[index_start..].copy_from_slice(&data[..available_space]);
            self.index = SIZE;

            &data[available_space..]
        }
    }

    // Clears the buffer. In theory we could omit resetting all the bytes to 0, but this way feels
    // more secure in case we mess up indexing further down the line.
    fn reset(&mut self) {
        self.buffer.fill(0);
        self.index = 0;
    }
}

impl<const SIZE: usize> Default for Buffer<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SIZE: usize> AsRef<[u8]> for Buffer<SIZE> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[..self.index]
    }
}
