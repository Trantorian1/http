use crate::prelude::*;

use super::ByteStream;

use std::io::Read as _;
use std::io::Write as _;

use super::fixtures::*;

mod test {
    use super::*;

    #[test]
    fn stream_init() {
        let mut stream_buffer = [0; MAX_SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        assert_eq!(stream.len(), 0, "An empty stream must be empty");
        assert_eq!(stream.start, 0, "An empty stream must start at index 0");

        let mut read_buffer = [0; MAX_SIZE];
        let bytes = stream
            .read(&mut read_buffer)
            .expect("Reading a byte stream must always succeed");

        assert_eq!(bytes, 0, "An empty stream must not contain any data");
        assert_eq!(
            read_buffer, [0; MAX_SIZE],
            "Reading empty stream has no side effect"
        );
    }

    #[test]
    fn stream_read_write() {
        let mut stream_buffer = [0; MAX_SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        let message = b"Hello, World";
        assert_leq!(message.len(), MAX_SIZE, "Message must fit in the stream");

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, message.len(), "All message bytes must be written");
        assert_eq!(stream.len(), message.len(), "Stream length must update");
        assert_eq!(stream.start, 0, "The stream's start index must not change");

        let mut read_buffer = [0; MAX_SIZE];
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
    fn stream_read_empty_buffer(mut array: [u8; MAX_SIZE], #[from(array)] oracle: [u8; MAX_SIZE]) {
        let mut stream = ByteStream::any(&mut array, 0, MAX_SIZE);
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
        let mut stream_buffer = [0; MAX_SIZE];
        let mut stream = ByteStream::new(&mut stream_buffer);

        let message = b"Lorem ipsum dolor si amet";
        assert_gr!(
            message.len(),
            MAX_SIZE,
            "Message must NOT fit in the stream"
        );

        let bytes = stream
            .write(message)
            .expect("Writing to a byte stream of sufficient size must succeed");

        assert_eq!(bytes, MAX_SIZE);

        let mut buffer = [0; MAX_SIZE];

        let bytes = stream
            .read(&mut buffer)
            .expect("Reading a byte stream must always succeed");

        assert_eq!(bytes, MAX_SIZE, "Byte stream should have been full");

        assert_eq!(
            buffer,
            &message[..MAX_SIZE],
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

use super::invariants::*;

#[cfg(test)]
mod validate {
    use super::*;

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
    #[rstest::rstest]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    fn stream_harness(
        generate_stream: impl bolero::generator::ValueGenerator<Output = (usize, usize, usize)>,
    ) {
        bolero::check!()
            .with_generator(generate_stream)
            .and_then(|(n_read, n_write, capacity)| {
                (
                    n_read,
                    n_write,
                    capacity,
                    // Initial stream start index
                    bolero::produce::<usize>().with().bounds(..capacity),
                    // Initial stream size
                    bolero::produce::<usize>().with().bounds(..=capacity),
                )
            })
            .cloned()
            .for_each(|(n_read, n_write, capacity, start, size)| {
                stream_invariant_problem(n_read, n_write, capacity, start, size);
            })
    }
}
