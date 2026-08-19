//! Stack-based [`ByteStream`]s.

use crate::prelude::*;

/// Stack-based ring buffer which implements [`Read`] and [`Write`]. Can be used as a mock
/// [`TcpStream`].
///
/// ## Example Usage
///
/// ```
/// # use http_primitives::prelude::*;
/// # use std::io::Read as _;
/// # use std::io::Write as _;
/// let mut stream_buffer = [0; 16];
/// let mut stream = ByteStream::new(&mut stream_buffer);
///
/// let message = b"Hello, World";
/// stream.write(message).unwrap();
///
/// let mut read_buffer = [0; 16];
/// let bytes = stream.read(&mut read_buffer).unwrap();
/// assert_eq!(&read_buffer[..bytes], message);
/// ```
///
/// [`Read`]: std::io::Read
// [`Write`]: std::io::Write
/// [`TcpStream`]: std::net::TcpStream
pub struct ByteStream<'data> {
    buffer: &'data mut [u8],
    start: usize,
    size: usize,
}

impl<'data> ByteStream<'data> {
    /// Creates a new [`ByteStream`]. Will panic if `buffer` is empty.
    pub fn new(buffer: &'data mut [u8]) -> Self {
        // Zero-away the buffer to guard against misuse
        buffer.fill(0);

        Self::any(buffer, 0, 0)
    }

    /// Creates a new [`ByteStream`], using `buffer` as initialized memory. This can be useful in
    /// testing for example.
    ///
    /// Will panic if `buffer` is empty.
    ///
    /// ```rust
    /// # use std::io::Read as _;
    /// # use http_primitives::prelude::*;
    /// let mut array: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    /// let mut stream = ByteStream::pre_populate(&mut array);
    ///
    /// let mut read_buffer = [0; 8];
    /// let bytes = stream.read(&mut read_buffer).unwrap();
    ///
    /// assert_eq!(&read_buffer[..bytes], &[0, 1, 2, 3, 4, 5, 6, 7]);
    /// ```
    pub fn pre_populate(buffer: &'data mut [u8]) -> Self {
        Self::any(buffer, 0, buffer.len())
    }

    fn any(buffer: &'data mut [u8], start: usize, size: usize) -> Self {
        assert!(!buffer.is_empty());
        assert_le!(start, buffer.len());
        assert_leq!(start, buffer.len());

        Self {
            buffer,
            start,
            size,
        }
    }

    /// Total available memory the stream has access to.
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the length of unread data currently in the byte stream.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns [`true`] if the byte stream contains no unread data.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the amount of space in the stream which can still be written to.
    pub fn space_left(&self) -> usize {
        self.capacity() - self.size
    }

    /// Returns an iterator over the stream.
    ///
    /// The iterator yields all items from start to end, consuming them as it goes.
    pub fn iter(&mut self) -> Iter<'_, 'data> {
        Iter::new(self)
    }
}

impl<'data> std::io::Read for ByteStream<'data> {
    //
    // -- Mutations
    //
    #[cfg_attr(kani, kani::modifies(&self.start, &self.size, buf))]
    //
    // -- Pre-conditions
    //
    #[cfg_attr(kani, kani::requires(contracts::common_preconditions(self)))]
    //
    // -- Post-conditions
    //
    // Start index must wrap around the buffer.
    //
    #[cfg_attr(kani, kani::ensures(|_| self.start == (old(self.start) + old(self.size).min(buf.len())) % self.capacity()))]
    //
    // Bytes are consumed as they are read.
    //
    #[cfg_attr(kani, kani::ensures(|_| self.size == old(self.size) - old(self.size).min(buf.len())))]
    //
    // Results are coherent with the data being read and can never error out.
    //
    #[cfg_attr(kani, kani::ensures(|result| match result  {
        Ok(read) => *read == old(self.size).min(buf.len())
            && self.size == old(self.size) - *read,
        Err(_) => false
    }))]
    ///
    /// If this seems confusing to you, check out the `kani` docs on [function contracts]
    ///
    /// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        assert_leq!(self.start, self.capacity());
        assert_leq!(self.size, self.capacity());

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        if stop <= self.capacity() {
            // Stream data stops before the end of the buffer, no need for wrap-around logic.
            let space_after_start = self.size.min(bytes);

            // Single-copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            self.start = (self.start + space_after_start) % self.capacity();
            self.size -= space_after_start;
            assert_leq!(self.size, self.capacity());

            Ok(space_after_start)
        } else {
            // Stream data goes past the end of the buffer, we need to handle ring wrap-around.
            let space_after_start = (self.capacity() - self.start).min(bytes);

            // First copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            let space_before_stop = (stop - self.capacity()).min(bytes - space_after_start);

            // Second copy, wrap around to the start of the buffer and copy data from there.
            buf[space_after_start..space_after_start + space_before_stop]
                .copy_from_slice(&self.buffer[..space_before_stop]);

            self.start = (self.start + space_after_start + space_before_stop) % self.capacity();
            self.size -= space_after_start + space_before_stop;
            assert_leq!(self.size, self.capacity());

            Ok(space_after_start + space_before_stop)
        }
    }
}

impl<'data> std::io::Write for ByteStream<'data> {
    //
    // -- Mutations
    //
    #[cfg_attr(kani, kani::modifies(&self.start, &self.size, self.buffer))]
    //
    // -- Pre-conditions
    //
    #[cfg_attr(kani, kani::requires(contracts::common_preconditions(self)))]
    //
    // -- Post-conditions
    //
    // Start index cannot be mutated by stream writes, only stream reads.
    //
    #[cfg_attr(kani, kani::ensures(|_| self.start == old(self.start)))]
    //
    // Stream size grows with the number of bytes written.
    //
    #[cfg_attr(kani, kani::ensures(|_| self.size == old(self.size) + old(self.space_left()).min(buf.len())))]
    //
    // Results are coherent with the data being written and can never error out.
    //
    #[cfg_attr(kani, kani::ensures(|result| match result {
        Ok(written) => *written == old(self.space_left()).min(buf.len())
            && self.size == old(self.size) + *written,
        Err(_) => false
    }))]
    ///
    /// If this seems confusing to you, check out the `kani` docs on [function contracts]
    ///
    /// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        assert_leq!(self.start, self.capacity());
        assert_leq!(self.size, self.capacity());

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        if stop <= self.capacity() {
            // The data currently in the buffer is contiguous, we might need to wrap around in order
            // to write more bytes. Here, were start by appending to the end of the stream.
            let space_after_stop = (self.capacity() - stop).min(bytes);
            self.buffer[stop..stop + space_after_stop].copy_from_slice(&buf[..space_after_stop]);

            // Next, we try and write whatever bytes remain at the start of the stream.
            let space_before_start = start.min(bytes - space_after_stop);
            self.buffer[..space_before_start]
                .copy_from_slice(&buf[space_after_stop..space_after_stop + space_before_start]);

            self.size += space_after_stop + space_before_start;

            Ok(space_after_stop + space_before_start)
        } else {
            // The data currently in the buffer is NOT contiguous and wraps around. This actually
            // makes our life easier, as we only need a single write to cover the area of memory
            // which we have left.
            let stop = stop - self.capacity();
            let space_before_start = (start - stop).min(bytes);

            // Write to the middle of the buffer, taking existing data wrap-around into consideration.
            self.buffer[stop..stop + space_before_start]
                .copy_from_slice(&buf[..space_before_start]);

            self.size += space_before_start;

            Ok(space_before_start)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct Iter<'stream, 'data>
where
    'data: 'stream,
{
    stream: &'stream mut ByteStream<'data>,
}

impl<'stream, 'data> Iter<'stream, 'data> {
    pub fn new(stream: &'stream mut ByteStream<'data>) -> Self {
        Self { stream }
    }
}

impl<'stream, 'data> Iterator for Iter<'stream, 'data> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stream.size > 0 {
            let item = self.stream.buffer[self.stream.start];

            self.stream.start = (self.stream.start + 1) % self.stream.capacity();
            self.stream.size -= 1;

            Some(item)
        } else {
            None
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    use std::io::Read as _;
    use std::io::Write as _;

    const SIZE: usize = 16;

    #[rstest::fixture]
    fn array() -> [u8; SIZE] {
        std::array::from_fn(|i| i as u8)
    }

    #[test]
    fn stream_init() {
        let mut stream_buffer = [0; SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        assert_eq!(stream.len(), 0, "An empty stream must be empty");
        assert_eq!(stream.start, 0, "An empty stream must start at index 0");

        let mut read_buffer = [0; SIZE];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Reading a byte stream must always succeed");

        assert_eq!(bytes, 0, "An empty stream must not contain any data");
        assert_eq!(
            read_buffer, [0; SIZE],
            "Reading empty stream has no side effect"
        );
    }

    #[test]
    fn stream_read_write() {
        let mut stream_buffer = [0; SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        let message = b"Hello, World";
        assert_leq!(message.len(), SIZE, "Message must fit in the stream");

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be written");
        assert_eq!(stream.len(), message.len(), "Stream length must update");
        assert_eq!(stream.start, 0, "The stream's start index must not change");

        let mut read_buffer = [0; SIZE];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Read on a byte stream should always succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be read");
        assert_eq!(
            &read_buffer[..bytes],
            message,
            "Buffer should contain message"
        );

        assert_eq!(stream.len(), 0, "Stream reads must consume the data read");
        assert_eq!(stream.start, message.len(), "Stream reads update start idx");
    }

    #[test]
    fn stream_read_write_offset() {
        let mut stream_buffer = [0; 2];
        let mut stream = ByteStream::any(&mut stream_buffer, 1, 0);

        let message = b"hi";
        assert_leq!(message.len(), 2, "Message must fit in the stream");

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be written");
        assert_eq!(stream.len(), message.len(), "Stream length must update");
        assert_eq!(stream.start, 1, "The stream's start index must not change");

        let mut read_buffer = [0; 2];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Read on a byte stream should always succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be read");
        assert_eq!(
            &read_buffer[..bytes],
            message,
            "Buffer should contain message"
        );

        assert_eq!(stream.len(), 0, "Stream reads must consume the data read");
        assert_eq!(stream.start, 1, "Stream reads update start idx");
    }

    #[test]
    fn stream_read_empty() {
        let mut stream_buffer = [0; 2];
        let mut stream = ByteStream::any(&mut stream_buffer, 0, 0);
        assert!(stream.is_empty(), "Stream must be empty");

        let mut read_buffer = [0; 2];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Read on byte stream should always succeed");

        assert_eq!(bytes, 0, "No bytes should have been read off empty stream");
        assert!(stream.is_empty(), "Stream should still be emtpy");
        assert_eq!(stream.start, 0, "Reading emtpy stream musn't mutate start");
    }

    #[test]
    fn stream_read_empty_with_offset() {
        let mut stream_buffer = [0; 2];
        let mut stream = ByteStream::any(&mut stream_buffer, 1, 0);
        assert!(stream.is_empty(), "Stream must be empty");
        assert_eq!(stream.start, 1, "Stream offset must be set");

        let mut read_buffer = [0; 2];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Read on byte stream should always succeed");

        assert_eq!(bytes, 0, "No bytes should have been read off empty stream");
        assert!(stream.is_empty(), "Stream should still be emtpy");
        assert_eq!(stream.start, 1, "Reading emtpy stream musn't mutate start");
    }

    #[rstest::rstest]
    fn stream_read_empty_buffer(mut array: [u8; SIZE], #[from(array)] oracle: [u8; SIZE]) {
        let mut stream = ByteStream::any(&mut array, 0, SIZE);
        assert_eq!(stream.len(), oracle.len(), "Stream is not empty");

        let mut read_buffer = [0; 0];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Read on byte stream should always succeed");

        assert_eq!(bytes, 0, "No bytes can be read with an empty read buffer");
        assert_eq!(stream.len(), oracle.len(), "Stream must not have been read");
        assert_eq!(stream.start, 0, "Empty buffer read musn't mutate start");
    }

    #[test]
    fn stream_write_message_too_big() {
        let mut stream_buffer = [0; SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        let message = b"Lorem ipsum dolor si amet";
        assert_gr!(message.len(), SIZE, "Message must NOT fit in the stream");

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, SIZE);

        let mut buffer = [0; SIZE];

        let bytes = stream
            .read(&mut buffer)
            .expect("Reading a byte stream must always succeed");

        assert_eq!(bytes, SIZE, "Byte stream should have been full");

        assert_eq!(
            buffer,
            &message[..SIZE],
            "Part of the message should still have been written"
        );
    }

    #[test]
    #[should_panic]
    fn stream_with_capacity_zero_should_panic() {
        let mut stream_buffer = [0; 0];
        let _stream = ByteStream::new(&mut stream_buffer);
    }
}

#[cfg(test)]
mod validate {
    use std::io::Read as _;
    use std::io::Write as _;

    use super::*;

    // It probably doesn't make sense to increase this too much as then we would just be polluting
    // the problem space with garbage data which likely does not contain any new edge cases. The
    // most interesting targets probably lie around small array sizes anyway.
    const MAX_SIZE: usize = 16;
    const _: () = assert!(MAX_SIZE > 0);

    /// ## Libfuzzer
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_harness
    /// ```
    ///
    /// ## AFL
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_harness --engine afl --sanitizer NONE
    /// ```
    ///
    /// ## Kani
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_harness --engine kani
    /// ```
    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    fn stream_harness() {
        let generator = (
            // Number of bytes to write
            bolero::produce::<usize>().with().bounds(..MAX_SIZE),
            // Stream capacity, cannot be 0
            bolero::produce::<usize>().with().bounds(1..MAX_SIZE),
        );

        bolero::check!()
            .with_generator(generator)
            .and_then(|(n, capacity)| {
                (
                    n,
                    capacity,
                    // Initial stream start index
                    bolero::produce::<usize>().with().bounds(..capacity),
                    // Initial stream size
                    bolero::produce::<usize>().with().bounds(..capacity),
                )
            })
            .cloned()
            .for_each(|(n, capacity, start, size)| {
                let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
                let mut read_buffer = [0; MAX_SIZE];

                let bytes_prev = backing;
                let bytes_new: [u8; MAX_SIZE] = std::array::from_fn(|i| (i + MAX_SIZE) as u8);

                let mut stream = ByteStream::any(&mut backing[..capacity], start, size);

                let written = stream.write(&bytes_new[..n]).unwrap();
                assert_eq!(written, n.min(capacity - size));

                let read = stream.read(&mut read_buffer).unwrap();

                assert_eq!(read, written + size);
                assert!(stream.is_empty());

                // Make sure that previous data has not been overwritten
                for i in 0..size {
                    assert_eq!(read_buffer[i], bytes_prev[(i + start) % capacity]);
                }

                // Check that new data has been written correctly
                for i in 0..written {
                    assert_eq!(read_buffer[size + i], bytes_new[i]);
                }
            })
    }

    /// ## Libfuzzer
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_iter
    /// ```
    ///
    /// ## AFL
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_iter --engine afl --sanitizer NONE
    /// ```
    ///
    /// ## Kani
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::validate::stream_iter --engine kani
    /// ```
    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    fn stream_iter() {
        let generator = (
            // Number of bytes to write
            bolero::produce::<usize>().with().bounds(..MAX_SIZE),
            // Stream capacity, cannot be 0
            bolero::produce::<usize>().with().bounds(1..MAX_SIZE),
        );

        bolero::check!()
            .with_generator(generator)
            .and_then(|(n, capacity)| {
                (
                    n,
                    capacity,
                    // Initial stream start index
                    bolero::produce::<usize>().with().bounds(..capacity),
                    // Initial stream size
                    bolero::produce::<usize>().with().bounds(..capacity),
                )
            })
            .cloned()
            .for_each(|(n, capacity, start, size)| {
                let mut backing = [0; MAX_SIZE];
                let bytes_new: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
                let bytes_prev = backing;

                let mut stream = ByteStream::any(&mut backing[..capacity], start, size);

                let written = stream.write(&bytes_new[..n]).unwrap();
                assert_eq!(written, n.min(capacity - size));

                let mut iter = stream.iter();

                // Make sure that previous data has not been overwritten
                for i in 0..size {
                    assert_eq!(iter.next(), Some(bytes_prev[(i + start) % capacity]));
                }

                // Check that new data has been written correctly
                for i in 0..written {
                    assert_eq!(iter.next(), Some(bytes_new[i]));
                }

                assert!(stream.is_empty());
            });
    }
}

#[cfg(kani)]
/// See [function contracts].
///
/// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
mod contracts {
    use super::*;

    use std::io::Read as _;
    use std::io::Write as _;

    const MAX_SIZE: usize = 16;
    const _: () = assert!(MAX_SIZE > 0);

    /// General [`ByteStream`] pre-conditions, shared between [`Read`] and [`Write`] function
    /// contracts.
    ///
    /// [`ByteStream`]: ByteStream
    /// [`Read`]: std::io::Read
    /// [`Write`]: std::io::Write
    pub fn common_preconditions<'stream, 'data>(stream: &'stream mut ByteStream<'data>) -> bool {
        !stream.buffer.is_empty()  // Stream buffer cannot have size 0
            && stream.start < stream.capacity() // Start index must be less than stream capacity
            && stream.size <= stream.capacity() // Stream size cannot exceed stream capacity
    }

    /// Contract validation tests MUST be run with `kani`.
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::contracts::check_read_contract --engine kani
    /// ```
    #[kani::proof_for_contract(<ByteStream as std::io::Read>::read)]
    #[kani::unwind(17)]
    fn check_read_contract() {
        let capacity = kani::any();
        let start = kani::any();
        let size = kani::any();

        kani::assume(capacity > 0 && capacity < MAX_SIZE);
        kani::assume(start < capacity);
        kani::assume(size < capacity);

        let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
        let mut read_buffer = [0; MAX_SIZE];
        let bytes_prev = backing;

        let mut stream = ByteStream::any(&mut backing[..capacity], start, size);

        let read = stream.read(&mut read_buffer).unwrap();

        assert_eq!(read, size);
        assert!(stream.is_empty());

        for i in 0..read {
            assert_eq!(read_buffer[i], bytes_prev[(i + start) % capacity]);
        }
    }

    /// Contract validation tests MUST be run with `kani`.
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::contracts::check_write_contract --engine kani
    /// ```
    #[kani::proof_for_contract(<ByteStream as std::io::Write>::write)]
    #[kani::unwind(17)]
    fn check_write_contract() {
        let n = kani::any();
        let capacity = kani::any();
        let start = kani::any();
        let size = kani::any();

        kani::assume(n < MAX_SIZE);
        kani::assume(capacity > 0 && capacity < MAX_SIZE);
        kani::assume(start < capacity);
        kani::assume(size < capacity);

        let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
        let mut read_buffer = [0; MAX_SIZE];

        let bytes_prev = backing;
        let bytes_new: [u8; MAX_SIZE] = std::array::from_fn(|i| (i + MAX_SIZE) as u8);

        let mut stream = ByteStream::any(&mut backing[..capacity], start, size);

        let written = stream.write(&bytes_new[..n]).unwrap();
        assert_eq!(written, n.min(capacity - size));

        let read = stream.read(&mut read_buffer).unwrap();

        assert_eq!(read, written + size);
        assert!(stream.is_empty());

        // Make sure that previous data has not been overwritten
        for i in 0..size {
            assert_eq!(read_buffer[i], bytes_prev[(i + start) % capacity]);
        }

        // Check that new data has been written correctly
        for i in 0..written {
            assert_eq!(read_buffer[size + i], bytes_new[i]);
        }
    }
}
