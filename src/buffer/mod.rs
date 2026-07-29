//! Stack-based statically allocated sliding [`Buffer`]s.

mod read;
mod write;

pub use read::BufReader;
pub use write::BufWriter;

pub type BufferForReading<const SIZE: usize> = Buffer<SIZE, read::ReadIn>;
pub type BufferForWriting<const SIZE: usize> = Buffer<SIZE, write::WriteOut>;

/// Sliding view stack buffer which guards against invalid writes and ensures proper flushing.
pub struct Buffer<const SIZE: usize, RW> {
    buffer: [u8; SIZE],
    window: std::ops::Range<usize>,

    _phantom: std::marker::PhantomData<RW>,
}

impl<const SIZE: usize, RW> Buffer<SIZE, RW> {
    /// Stack-allocates a new [`Buffer`] of the given `SIZE`.
    ///
    /// ```rust
    /// let buffer = http1::BufferForReading::{ 8 * http1::size::KB }::new();
    /// ```
    ///
    /// [`BufferForReading`] can only be used to read from byte streams, while [`BufferForWriting`]
    /// can only be used to write back to those streams. This prevents the user from re-using the
    /// same buffer for reading and writing and potentially jumbling data.
    pub fn new() -> Self {
        assert!(SIZE > 0);

        Self {
            buffer: [0; SIZE],
            window: 0..0,

            _phantom: std::marker::PhantomData,
        }
    }

    /// Returns the length of the buffer. This is different from a buffer's full size and only
    /// counts data which has already been written to it.
    fn len(&self) -> usize {
        self.window.len()
    }

    fn clear(&mut self) {
        self.buffer.fill(0);
        self.window = 0..0;
    }
}

impl<const SIZE: usize, RW> Default for Buffer<SIZE, RW> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SIZE: usize, RW> From<[u8; SIZE]> for Buffer<SIZE, RW> {
    fn from(buffer: [u8; SIZE]) -> Self {
        Buffer {
            buffer,
            window: 0..SIZE,

            _phantom: std::marker::PhantomData,
        }
    }
}

impl<const SIZE: usize, RW> AsRef<[u8]> for Buffer<SIZE, RW> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.window.clone()]
    }
}

impl<const SIZE: usize, RW> std::ops::Index<std::ops::Range<usize>> for Buffer<SIZE, RW> {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<const SIZE: usize, RW> std::ops::Index<std::ops::RangeInclusive<usize>> for Buffer<SIZE, RW> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeInclusive<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<const SIZE: usize, RW> std::ops::Index<std::ops::RangeFrom<usize>> for Buffer<SIZE, RW> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<const SIZE: usize, RW> std::ops::Index<std::ops::RangeTo<usize>> for Buffer<SIZE, RW> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<const SIZE: usize, RW> std::ops::Index<std::ops::RangeFull> for Buffer<SIZE, RW> {
    type Output = [u8];

    fn index(&self, _range: std::ops::RangeFull) -> &Self::Output {
        self.as_ref()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const SIZE: usize = 64;
    const _: () = assert!(SIZE >= 16);

    #[rstest::fixture]
    fn array() -> [u8; SIZE] {
        std::array::from_fn(|i| i as u8)
    }

    #[rstest::fixture]
    fn buffer(array: [u8; SIZE]) -> Buffer<SIZE, write::WriteOut> {
        assert!(!array.is_empty());
        Buffer::from(array)
    }

    #[rstest::rstest]
    fn index_range_base(buffer: Buffer<SIZE, write::WriteOut>, array: [u8; SIZE]) {
        pretty_assertions::assert_eq!(buffer[1..10], array[1..10]);
    }

    #[rstest::rstest]
    fn index_range_inclusive(buffer: Buffer<SIZE, write::WriteOut>, array: [u8; SIZE]) {
        pretty_assertions::assert_eq!(buffer[1..=10], array[1..=10]);
    }

    #[rstest::rstest]
    fn index_range_from(buffer: Buffer<SIZE, write::WriteOut>, array: [u8; SIZE]) {
        pretty_assertions::assert_eq!(buffer[1..], array[1..]);
    }

    #[rstest::rstest]
    fn index_range_to(buffer: Buffer<SIZE, write::WriteOut>, array: [u8; SIZE]) {
        pretty_assertions::assert_eq!(buffer[..SIZE], array[..SIZE]);
    }

    #[rstest::rstest]
    fn index_range_full(buffer: Buffer<SIZE, write::WriteOut>, array: [u8; SIZE]) {
        pretty_assertions::assert_eq!(buffer[..], array[..])
    }
}
