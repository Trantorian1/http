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
    /// Returns an HTTP error [`Status`] code if the provided [`BufReader`] fails to parse the byte
    /// stream which is passed to it.
    ///
    /// See [`read`] for a more complete list of read-related errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_primitives::prelude::*;
    /// #
    /// # let mut backing_buffer = [0; 8 * KB];
    /// # let mut buffer = BufferForReading::new(&mut backing_buffer);
    /// #
    /// # let mut backing_stream = *b"GET / HTTP/1.1\r\n\r\n";
    /// # let mut stream = ByteStream::pre_populate(&mut backing_stream);
    /// #
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
    /// let method = buffer
    ///     .read_in(&mut stream, |reader| reader.read(parser))
    ///     .unwrap();
    /// ```
    ///
    /// [`read`]: BufReader::read
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
    /// Returns [`RequestTimetout`] if no bytes are left to consume and parsing still fails.
    ///
    /// Returns an [`InternalServerError`] wrapping an [`io::Error`] if the underlying reader fails
    /// to read new bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_primitives::prelude::*;
    /// #
    /// # let mut backing_buffer = [0; 8 * KB];
    /// # let mut buffer = BufferForReading::new(&mut backing_buffer);
    /// #
    /// # let mut backing_stream = *b"GET / HTTP/1.1\r\n\r\n";
    /// # let mut stream = ByteStream::pre_populate(&mut backing_stream);
    /// #
    /// # let _ = buffer.read_in(&mut stream, |reader| {
    /// #
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
    /// [`RequestTimetout`]: Status::RequestTimetout
    /// [`InternalServerError`]: Status::InternalServerError
    /// [`io::Error`]: std::io::Error
    pub fn read(
        &mut self,
        parse: impl Fn(&[u8]) -> Result<Option<std::num::NonZeroUsize>, Status>,
    ) -> Result<std::ops::Range<usize>, Status> {
        loop {
            if let Some(index) = parse(self.buffer.as_ref())? {
                break Ok(self.buffer.process(index));
            }

            if self.buffer.is_full() {
                return Err(Status::ContentTooLarge);
            }

            let new_bytes = self
                .buffer
                .append_from(&mut self.reader)
                .map_err(Status::internal)?;

            if new_bytes == 0 {
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

#[cfg(test)]
use super::invariants::*;

#[cfg(test)]
mod validate {
    use super::*;

    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    // #[cfg_attr(kani, kani::stub_verified(ByteStream::read_impl))]
    // #[cfg_attr(kani, kani::stub_verified(ByteStream::write_impl))]
    fn buf_read_harness() {
        let generator = (
            // Number of bytes to read
            bolero::produce::<usize>().with().bounds(..MAX_SIZE),
            // Read buffer capacity, cannot be 0
            bolero::produce::<usize>().with().bounds(1..MAX_SIZE),
        );

        bolero::check!()
            .with_generator(generator)
            .and_then(|(n_read, capacity)| {
                (
                    n_read,
                    capacity,
                    bolero::produce::<usize>().with().bounds(1..capacity),
                )
            })
            .cloned()
            .for_each(|(n_read, capacity, chunk)| {
                buffer_read_invariant_problem(n_read, nonzero!(capacity), nonzero!(chunk));
            });
    }
}
