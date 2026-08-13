use crate::prelude::*;

/// Stack-based single-threaded ring buffer which implements [`Read`] and [`Write`]. Can be used as
/// a mock [`TcpStream`].
///
/// ## Example Usage
///
/// ```
/// # use http_core::prelude::*;
/// # use std::io::Read as _;
/// # use std::io::Write as _;
/// let mut stream = ByteStream::<16>::new();
/// let mut buffer = [0; 16];
///
/// let message = b"Hello, World";
/// stream.write(message).unwrap();
///
/// let bytes = stream.read(&mut buffer).unwrap();
/// assert_eq!(&buffer[..bytes], message);
/// ```
///
/// [`Read`]: std::io::Read
/// [`Write`]: std::io::Write
/// [`TcpStream`]: std::net::TcpStream
pub struct ByteStream<const SIZE: usize> {
    buffer: [u8; SIZE],
    start: usize,
    size: usize,
}

impl<const SIZE: usize> ByteStream<SIZE> {
    /// Creates a new by stream. Will panic if `SIZE` is equal to 0.
    pub fn new() -> Self {
        assert!(SIZE > 0);

        Self {
            buffer: [0; SIZE],
            start: 0,
            size: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        SIZE
    }

    /// Returns the length of unread data currently in the byte stream.
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns [`true`] if the byte stream contains no unread data.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<const SIZE: usize> Default for ByteStream<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SIZE: usize> std::io::Read for ByteStream<SIZE> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        assert_leq!(self.start, SIZE);
        assert_leq!(self.size, SIZE);

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        if stop <= SIZE {
            // Stream data stops before the end of the buffer, no need for wrap-around logic.
            let space_after_start = stop.min(bytes);

            // Single-copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            self.start += space_after_start;
            self.size -= space_after_start;

            Ok(space_after_start)
        } else {
            // Stream data goes past the end of the buffer, we need to handle ring wrap-around.
            let space_after_start = (SIZE - self.start).min(bytes);

            // First copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            let space_before_stop = (stop - SIZE).min(bytes - space_after_start);

            // Second copy, wrap around to the start of the buffer and copy data from there.
            buf[space_after_start..space_after_start + space_before_stop]
                .copy_from_slice(&self.buffer[..space_before_stop]);

            self.start = (self.start + space_after_start + space_before_stop) % SIZE;
            self.size -= space_after_start - space_before_stop;

            Ok(space_after_start + space_before_stop)
        }
    }
}

impl<const SIZE: usize> std::io::Write for ByteStream<SIZE> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        assert_leq!(self.start, SIZE);
        assert_leq!(self.size, SIZE);

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        // We always try and append data first. This is a no-op in case there is no space left or a
        // wrap-around is needed.
        let space_after_stop = (SIZE - stop).min(bytes);

        self.buffer[stop..stop + space_after_stop].copy_from_slice(&buf[..space_after_stop]);

        if stop <= SIZE {
            // No wrap-around needed.
            self.size += space_after_stop;

            Ok(space_after_stop)
        } else {
            // Wrap-around needed.
            let space_before_start = (start).min(bytes - space_after_stop);

            // Append the rest of the data to the start of the buffer.
            self.buffer[..space_before_start]
                .copy_from_slice(&buf[space_after_stop..space_after_stop + space_before_start]);

            self.size += space_after_stop + space_before_start;

            Ok(space_after_stop - space_before_start)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    use std::io::Read as _;
    use std::io::Write as _;

    const SIZE: usize = 16;

    #[rstest::fixture]
    fn stream() -> ByteStream<SIZE> {
        ByteStream::new()
    }

    #[rstest::rstest]
    fn stream_init(mut stream: ByteStream<SIZE>) {
        let mut buffer = [0; SIZE];

        assert_eq!(stream.len(), 0, "An empty stream must be empty");
        assert_eq!(stream.start, 0, "An empty stream must start at index 0");

        let bytes = stream
            .read(&mut buffer)
            .expect("Reading a byte stream must always succeed");

        assert_eq!(bytes, 0, "An empty stream must not contain any data");
        assert_eq!(buffer, [0; SIZE], "Reading empty stream has no side effect");
    }

    #[rstest::rstest]
    fn stream_read_write(mut stream: ByteStream<SIZE>) {
        let mut buffer = [0; SIZE];

        let message = b"Hello, World";
        assert_le!(message.len(), SIZE, "Message must fit in the stream");

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be written");
        assert_eq!(stream.len(), message.len(), "Stream length must update");
        assert_eq!(stream.start, 0, "The stream's start index must not change");

        let bytes = stream
            .read(&mut buffer)
            .expect("Read on a data stream should always succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be read");
        assert_eq!(&buffer[..bytes], message, "Buffer should contain message");
        assert_eq!(stream.len(), 0, "Stream reads must consume the data read");
        assert_eq!(stream.start, message.len(), "Stream reads update start idx");
    }

    #[rstest::rstest]
    fn stream_write_message_too_big(mut stream: ByteStream<SIZE>) {
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
        let _stream = ByteStream::<0>::new();
    }
}
