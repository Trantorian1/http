use crate::prelude::*;

pub struct ReadIn;

/// Misuse-resistant byte stream parser, allows the user to perform zero-copy parsing of incoming
/// data streams. Because of how memory is statically allocated, byte streams are limited in size to
/// the capacity of the [`Buffer`] used to parse them. Trying to parse a stream when the underlying
/// buffer is too small will result in a [`ContentTooLarge`] error.
///
/// [`ContentTooLarge`]: code::Status::ContentTooLarge
pub struct BufReader<'a, 'b, const SIZE: usize, R: std::io::Read> {
    buffer: &'a mut Buffer<SIZE, ReadIn>,
    reader: &'b mut R,
}

impl<const SIZE: usize> Buffer<SIZE, ReadIn> {
    /// Parse in a byte stream. See [`BufReader`] for a list of available methods.
    ///
    /// ```rust
    /// # let mut buffer = http1::BufferForReading::<{64 * http1::size::KB}>::new();
    /// # let mut stream = std::collections::VecDeque::from(*b"GET / HTTP/1.1\r\n\r\n");
    /// let (method, target) = buffer.read_in(&mut stream, |reader| {
    ///     let method = reader.read(http1::request::parsers::method)?;
    ///
    ///     reader.read(http1::request::parsers::sp)?;
    ///
    ///     let target = reader.read(http1::request::parsers::target)?;
    ///
    ///     reader.read(http1::request::parsers::sp)?;
    ///     reader.read(http1::request::parsers::protocol)?;
    ///     reader.read(http1::request::parsers::crlf)?;
    ///
    ///     Ok((method, target))
    /// }).unwrap();
    /// ```
    ///
    /// [`Reader`]: std::io::Read
    pub fn read_in<'a, 'b, R: std::io::Read, T>(
        &'a mut self,
        reader: &'b mut R,
        apply_reads: impl FnOnce(&mut BufReader<'a, 'b, SIZE, R>) -> Result<T, code::Status>,
    ) -> Result<T, code::Status> {
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

        assert!(start < stop, "{start} < {stop}");
        assert!(stop <= SIZE, "{stop} < {SIZE}");
        start..stop
    }

    /// Read in a byte stream and updates the viewing window accordingly.
    ///
    /// Returns the number of bytes which have been read. A return value of 0 indicates either that
    /// the byte stream is empty or that the buffer is full.
    fn append_from<R: std::io::Read>(&mut self, stream: &mut R) -> std::io::Result<usize> {
        let bytes_read = stream.read(&mut self.buffer[self.window.end..])?;
        self.window.end += bytes_read;

        Ok(bytes_read)
    }
}

impl<'a, 'b, const SIZE: usize, R: std::io::Read> BufReader<'a, 'b, SIZE, R> {
    pub(crate) fn new(view: &'a mut Buffer<SIZE, ReadIn>, reader: &'b mut R) -> Self {
        Self {
            buffer: view,
            reader,
        }
    }

    /// Keeps trying to parse a byte stream with the provided parser.
    ///
    /// ```rust
    /// # let mut buffer = http1::BufferForReading::<{64 * http1::size::KB}>::new();
    /// # let mut stream = std::collections::VecDeque::from(*b"GET / HTTP/1.1\r\n\r\n");
    /// # let _ = buffer.read_in(&mut stream, |reader| {
    /// let method = reader.read(http1::request::parsers::method)?;
    /// # Ok(method)
    /// # }).unwrap();
    /// ```
    pub fn read(
        &mut self,
        parse: fn(&[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status>,
    ) -> Result<std::ops::Range<usize>, code::Status> {
        loop {
            if let Some(index) = parse(self.buffer.as_ref())? {
                break Ok(self.buffer.process(index));
            }

            let new_bytes = self
                .buffer
                .append_from(&mut self.reader)
                .map_err(code::Status::internal)?;

            if self.buffer.len() == SIZE {
                return Err(code::Status::ContentTooLarge);
            } else if new_bytes == 0 {
                return Err(code::Status::RequestTimetout);
            }
        }
    }

    fn flush(&mut self) {
        self.buffer.window.start = 0;
    }
}
