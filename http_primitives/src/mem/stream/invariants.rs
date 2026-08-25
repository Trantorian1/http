use std::io::Read as _;
use std::io::Write as _;

use super::ByteStream;
use super::fixtures::*;

impl ByteStream<'_> {
    /// General [`ByteStream`] pre-conditions, shared between [`Read`] and [`Write`] function
    /// contracts.
    ///
    /// [`ByteStream`]: ByteStream
    /// [`Read`]: std::io::Read
    /// [`Write`]: std::io::Write
    #[must_use]
    pub fn invariants(&self) -> bool {
        !self.backing.is_empty()  // Stream buffer cannot have size 0
            && self.start < self.capacity() // Start index must be less than stream capacity
            && self.size <= self.capacity() // Stream size cannot exceed stream capacity
    }
}

/// Simulates stream read-write operations under the following conditions:
///
/// - Partial reads.
/// - Partial writes.
/// - Varying stream capacity.
/// - Varying stream size.
/// - Wrapped and contiguous data.
pub(crate) fn stream_invariant_problem(
    n_read: usize,                    // number of bytes read
    n_write: usize,                   // number of bytes written
    capacity: std::num::NonZeroUsize, // stream capacity
    start: usize,                     // stream start index, causes wrap-around
    size: usize, // initial stream size, data in the backing buffer which is kept
) {
    // Initial stream data. The number of bytes kept is informed by `size`. The rest will be
    // overwritten during subsequent writes.
    let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
    let mut read_buffer = [0; MAX_SIZE];

    // Bytes to be written to the stream. The actual number of writes is determined by
    // `n_write` and by the initial stream `size`.
    let bytes_new: [u8; MAX_SIZE] = std::array::from_fn(|i| (i + MAX_SIZE) as u8);
    let bytes_prev = backing;

    // The system under test
    let mut stream = ByteStream::any(&mut backing[..capacity.get()], start, size);

    // Invariant test 1:
    //
    // We write UP TO `n_write` bytes to the stream. The actual number of bytes we manage to
    // write will depend on the stream capacity as well as it's start size. If there is not
    // enough space left to write `n_write` bytes, as many bytes as possible should still be
    // written to the stream.
    let written = stream.write(&bytes_new[..n_write]).unwrap();
    assert_eq!(written, n_write.min(capacity.get() - size));

    // Invariant test 2:
    //
    // We read UP TO `n_read` bytes from the stream. The actual number of bytes we manage to read
    // will depend on the initial size of the stream as well as the number of bytes which were
    // previously written. If there is not enough space in `read_buffer` to read all of the
    // stream's data, as many bytes as possible should still be read.
    let read = stream.read(&mut read_buffer[..n_read]).unwrap();
    let processed = (written + size).min(n_read);

    assert_eq!(read, processed);
    assert_eq!(stream.len(), written + size - processed);

    // Invariant test 3:
    //
    // Bytes which were initially present in the stream should not have been overwritten if they
    // could be read.
    for i in 0..size.min(n_read) {
        assert_eq!(read_buffer[i], bytes_prev[(i + start) % capacity]);
    }

    // Invariant test 4:
    //
    // Bytes which were later written to the stream should also be present in `read_buffer` if
    // they could be read.
    for i in 0..written.min(n_read.saturating_sub(size)) {
        assert_eq!(read_buffer[size + i], bytes_new[i]);
    }
}

pub(crate) fn stream_iter_invariant_problems(
    capacity: std::num::NonZeroUsize,
    start: usize,
    size: usize,
) {
    let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
    let bytes_prev = backing;

    let stream = ByteStream::any(&mut backing[..capacity.get()], start, size);
    let mut iter = stream.iter();

    for i in 0..size {
        assert_eq!(iter.next(), Some(bytes_prev[(i + start) % capacity]));
    }

    assert_eq!(stream.len(), size);
    assert_eq!(stream.start, start);
}

pub(crate) fn make_contiguous_invariant_problem(
    capacity: std::num::NonZeroUsize,
    start: usize,
    size: usize,
) {
    let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
    let backing_copy = backing;
    let oracle = &backing_copy[..capacity.get()];

    let mut stream = ByteStream::any(&mut backing[..capacity.get()], start, size);

    let contiguous = stream.make_contiguous();
    assert_eq!(contiguous.len(), size);

    if start + size <= capacity.get() {
        assert_eq!(contiguous[..size], oracle[start..start + size]);
    } else {
        let space_after_start = capacity.get() - start;
        assert_eq!(
            contiguous[..space_after_start],
            oracle[start..start + space_after_start]
        );

        let space_before_stop = start + size - capacity.get();
        assert_eq!(contiguous[space_after_start..], oracle[..space_before_stop]);
    }
}
