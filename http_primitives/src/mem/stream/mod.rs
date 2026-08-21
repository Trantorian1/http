//! Stack-based [`ByteStream`]s.

use crate::prelude::*;

mod iter;
mod read;
mod write;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fixtures;

#[cfg(test)]
mod invariants;

pub use iter::Iter;

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
/// [`Write`]: std::io::Write
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

    /// Re-arrange the internal storage of this stream so that it is one contiguous slice, which is
    /// then returned.
    ///
    /// This method does not allocate and does not change the order of the inserted elements.
    pub fn make_contiguous(&mut self) -> &[u8] {
        assert_le!(self.start, self.capacity());
        assert_leq!(self.size, self.capacity());

        let start = self.start;
        let stop = self.start + self.size;

        if stop <= self.capacity() {
            // No buffer wrap-around.
            //
            //      ┌─start
            //      ▼
            // [ _, 0, 1, 2, 3 ]
            //               ▲
            //           stop┘
            //
            // We just need to move array elements to the start of the buffer.
            //
            //   ┌─start
            //   ▼
            // [ 0, 1, 2, 3, _ ]
            //            ▲
            //       stop─┘
            self.buffer.copy_within(start.., 0);
        } else {
            // Buffer wrap-around.
            //
            //      start─┐
            //            ▼
            // [ 2, 3, 4, 0, 1 ]
            //         ▲
            //         └─stop
            //
            // We need to re-order each section of the buffer. This can be achieved easily with the
            // use of `slice.rotate_right`.
            //
            //   ┌─start
            //   ▼
            // [ 0, 1, 2, 3, 4 ]
            //               ▲
            //          stop─┘
            self.buffer.rotate_left(start);
        }

        self.start = 0;
        &self.buffer[..self.size]
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
}

impl<'data> std::fmt::Debug for ByteStream<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
