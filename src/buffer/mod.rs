pub mod read;
pub mod write;

pub use read::ReadIn;
pub use write::WriteOut;

/// Sliding view stack buffer which guards against invalid writes and ensures proper flushing.
pub struct Buffer<const SIZE: usize, RW> {
    buffer: [u8; SIZE],
    window: std::ops::Range<usize>,

    _phantom: std::marker::PhantomData<RW>,
}

impl<const SIZE: usize, RW> Buffer<SIZE, RW> {
    /// Stack-allocates a new [`Buffer`] of the given size.
    pub fn new() -> Self {
        assert!(SIZE > 0);

        Self {
            buffer: [0; SIZE],
            window: 0..0,

            _phantom: std::marker::PhantomData,
        }
    }

    fn len(&self) -> usize {
        self.window.len()
    }

    // Clears the buffer. In theory we could omit resetting all the bytes to 0, but this way feels
    // more secure in case we mess up indexing further down the line.
    pub fn clear(&mut self) {
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
