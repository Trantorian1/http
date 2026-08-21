use crate::prelude::*;

#[derive(Debug)]
pub struct ReadIn;

/// Misuse-resistant byte stream parser, allowing for zero-copy parsing of incoming data streams.
/// Because of how memory is statically allocated, byte streams are limited in size to the capacity
/// of the [`Buffer`] used to parse them. Trying to parse a stream when the underlying buffer is too
/// small will result in a [`ContentTooLarge`] error.
///
/// [`ContentTooLarge`]: Status::ContentTooLarge
pub struct BufReader<'buf, 'data, 'reader, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    buffer: &'buf mut BufferForReading<'data>,
    reader: &'reader mut R,
}

impl<'data> Buffer<'data, ReadIn> {
    /// Parse in a byte stream. See [`BufReader`] for a list of available methods.
    ///
    /// # Errors
    ///
    /// This method only fails if the underlying reader returns an [`io::Error`] while reading.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_primitives::prelude::*;
    /// # let mut backing = [0; 8 * KB];
    /// # let mut buffer = BufferForReading::new(&mut backing);
    /// # let mut stream = std::collections::VecDeque::from(*b"GET / HTTP/1.1\r\n\r\n");
    /// // Parses in an HTTP/1.1 GET method
    /// fn parser(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    ///     const GET: &[u8] = b"GET";
    ///
    ///     if data.len() < GET.len() {
    ///         return Ok(None);
    ///     }
    ///
    ///     match &data[..GET.len()] {
    ///         GET => Ok(Some(std::num::NonZero::new(GET.len()).unwrap())),
    ///         _ => Err(Status::NotImplemented),
    ///     }
    /// }
    ///
    /// let method = buffer.read_in(&mut stream, |reader| {
    ///     reader.read(parser)
    /// }).unwrap();
    /// ```
    ///
    /// [`Reader`]: std::io::Read
    /// [`io::Error`]: std::io::Error
    pub fn read_in<'buf, 'reader, R: std::io::Read, T>(
        &'buf mut self,
        reader: &'reader mut R,
        apply_reads: impl FnOnce(&mut BufReader<'buf, 'data, 'reader, R>) -> Result<T, Status>,
    ) -> Result<T, Status>
    where
        'data: 'buf,
    {
        // Make sure we are not re-using data from previous requests/responses.
        self.clear();

        let mut buf_reader = BufReader::new(self, reader);
        let reads = apply_reads(&mut buf_reader);

        buf_reader.flush();

        reads
    }

    /// Slide the start of the viewing window forwards by `n` bytes.
    fn process(&mut self, n: std::num::NonZeroUsize) -> std::ops::Range<usize> {
        let stop = self.window.start + n.get();
        let start = std::mem::replace(&mut self.window.start, stop);

        assert_le!(start, stop);
        assert_leq!(stop, self.capacity());
        start..stop
    }

    /// Read in a byte stream and updates the viewing window accordingly.
    ///
    /// Returns the number of bytes which have been read. A return value of 0 indicates either that
    /// the byte stream is empty or that the buffer is full.
    fn append_from<R: std::io::Read>(&mut self, stream: &mut R) -> std::io::Result<usize> {
        let bytes_read = stream.read(&mut self.backing[self.window.end..])?;
        self.window.end += bytes_read;

        Ok(bytes_read)
    }
}

impl<'buf, 'data, 'reader, R: std::io::Read> BufReader<'buf, 'data, 'reader, R> {
    pub(crate) fn new(buffer: &'buf mut BufferForReading<'data>, reader: &'reader mut R) -> Self {
        Self { buffer, reader }
    }

    /// Keeps trying to parse a byte stream with the provided parser.
    ///
    /// # Errors
    ///
    /// Returns [`ContentTooLarge`] if there is not enough space left in the buffer to write all
    /// parsed bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_primitives::prelude::*;
    /// # let mut backing = [0; 8 * KB];
    /// # let mut buffer = BufferForReading::new(&mut backing);
    /// # let mut stream = std::collections::VecDeque::from(*b"GET / HTTP/1.1\r\n\r\n");
    /// # let _ = buffer.read_in(&mut stream, |reader| {
    /// // Parses in an HTTP/1.1 GET method
    /// fn parser(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    ///     const GET: &[u8] = b"GET";
    ///
    ///     if data.len() < GET.len() {
    ///         return Ok(None);
    ///     }
    ///
    ///     match &data[..GET.len()] {
    ///         GET => Ok(Some(std::num::NonZero::new(GET.len()).unwrap())),
    ///         _ => Err(Status::NotImplemented),
    ///     }
    /// }
    ///
    /// let method = reader.read(parser)?;
    /// # Ok(method)
    /// # }).unwrap();
    /// ```
    ///
    /// [`ContentTooLarge`]: Status::ContentTooLarge
    pub fn read(
        &mut self,
        parse: fn(&[u8]) -> Result<Option<std::num::NonZeroUsize>, Status>,
    ) -> Result<std::ops::Range<usize>, Status> {
        loop {
            if let Some(index) = parse(self.buffer.as_ref())? {
                break Ok(self.buffer.process(index));
            }

            let new_bytes = self
                .buffer
                .append_from(&mut self.reader)
                .map_err(Status::internal)?;

            if self.buffer.is_full() {
                return Err(Status::ContentTooLarge);
            } else if new_bytes == 0 {
                return Err(Status::RequestTimetout);
            }
        }
    }

    fn flush(&mut self) {
        self.buffer.window.start = 0;
    }
}

impl<'buf, 'data, R> std::fmt::Debug for BufReader<'buf, 'data, '_, R>
where
    'data: 'buf,
    R: std::io::Read,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufReader")
            .field("buffer", &self.buffer)
            .finish()
    }
}
