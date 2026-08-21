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

    #[rstest::rstest]
    fn stream_debug_no_wrap_around(
        mut array: [u8; MAX_SIZE],
        #[from(array)] oracle: [u8; MAX_SIZE],
    ) {
        let stream = ByteStream::any(&mut array, 0, MAX_SIZE);
        assert_eq!(format!("{:?}", stream), format!("{:?}", oracle))
    }

    #[rstest::rstest]
    fn stream_debug_with_wrap_around(
        mut array: [u8; MAX_SIZE],
        #[from(array)] mut oracle: [u8; MAX_SIZE],
    ) {
        assert_gr!(MAX_SIZE, 2);

        let stream = ByteStream::any(&mut array, 2, MAX_SIZE);
        oracle.rotate_right(MAX_SIZE - 2);

        assert_eq!(format!("{:?}", stream), format!("{:?}", oracle))
    }

    #[test]
    #[should_panic]
    fn stream_with_capacity_zero_should_panic() {
        let mut stream_buffer = [0; 0];
        let _stream = ByteStream::new(&mut stream_buffer);
    }
}

#[cfg(all(test, kani))]
mod stubs {
    /// Rotates `buffer` to the left by `mid` elements, so that the element at index `mid` becomes the
    /// first element. Equivalent to [`<[u8]>::rotate_left`], using the three-reversal algorithm.
    ///
    /// ## Why not [`<[u8]>::rotate_left`]?
    ///
    /// `core` picks between three rotation algorithms at runtime, based on `min(mid, len - mid)`. For
    /// concrete, small byte slices it always lands on the branch which copies through a stack buffer
    /// and is loop-free.
    ///
    /// Under `kani` however, the slice length is symbolic, so that dispatch cannot be resolved and
    /// CBMC has to explore the cyclic-permutation branch as well (`core::slice::rotate::ptr_rotate_gcd`).
    /// That branch is a doubly-nested loop over raw pointers whose trip counts are a function of
    /// `gcd(len, len - mid)`; every unwinding re-enters the loop on a fresh path, and the resulting
    /// path explosion never terminates in practice.
    ///
    /// The three-reversal algorithm has the same semantics but a flat loop shape which `kani`
    /// unwinds in `len / 2` iterations.
    pub fn rotate_left<T>(buffer: &mut [T], mid: usize) {
        reverse(&mut buffer[..mid]);
        reverse(&mut buffer[mid..]);
        reverse(buffer);
    }

    /// Reverses `buffer` in place.
    ///
    /// Deliberately written as a flat, index-based loop: this makes it feasible for `kani` to
    /// validate the use of this method without causing a path explosion.
    pub fn reverse<T>(buffer: &mut [T]) {
        let mut i = 0;
        let mut j = buffer.len();

        while i + 1 < j {
            j -= 1;
            buffer.swap(i, j);
            i += 1;
        }
    }
}

#[cfg(test)]
use super::invariants::*;

#[cfg(test)]
mod validate {
    use super::*;

    #[cfg(all(test, kani))]
    use stubs::*;

    /// ## Libfuzzer
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_harness
    /// ```
    ///
    /// ## AFL
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_harness --engine afl --sanitizer NONE
    /// ```
    ///
    /// ## Kani
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_harness --engine kani
    /// ```
    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    fn stream_harness() {
        bolero::check!()
            .with_generator(generate_stream::default())
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

    /// ## Libfuzzer
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_make_contiguous
    /// ```
    ///
    /// ## AFL
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_make_contiguous --engine afl --sanitizer NONE
    /// ```
    ///
    /// ## Kani
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::tests::validate::stream_make_contiguous --engine kani
    /// ```
    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    #[cfg_attr(kani, kani::stub(<[u8]>::rotate_left, stubs::rotate_left))]
    fn stream_make_contiguous() {
        bolero::check!()
            .with_generator(generate_capacity::default())
            .and_then(|capacity| {
                (
                    capacity,
                    // Initial stream start index
                    bolero::produce::<usize>().with().bounds(..capacity),
                    // Initial stream size
                    bolero::produce::<usize>().with().bounds(..=capacity),
                )
            })
            .cloned()
            .for_each(|(capacity, start, size)| {
                make_contiguous_invariant_problem(capacity, start, size);
            })
    }
}
