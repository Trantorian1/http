//! Stack-based sliding [`Buffer`]s.

mod read;
mod write;

pub use read::BufReader;
pub use write::BufWriter;

/// A buffer which can **ONLY** be used for [reading].
///
/// [reading]: BufReader
pub type BufferForReading<'data> = Buffer<'data, read::ReadIn>;

/// A buffer which can **ONLY** be used for [writing].
///
/// [writing]: BufWriter
pub type BufferForWriting<'data> = Buffer<'data, write::WriteOut>;

/// Stack-allocated sliding view buffer which guards against invalid writes and ensures proper flushing.
pub struct Buffer<'data, Mode> {
    buffer: &'data mut [u8],
    window: std::ops::Range<usize>,

    _phantom: std::marker::PhantomData<Mode>,
}

impl<'data, Mode> Buffer<'data, Mode> {
    /// Stack-allocates a new [`Buffer`] of the given `'data`.
    ///
    /// ```rust
    /// # use http_core::prelude::*;
    /// let mut read_backing = [0; 8 * KB];
    /// let read_buffer = BufferForReading::pre_populate(&mut read_backing);
    ///
    /// let mut write_backing = [0; 64 * KB];
    /// let write_buffer = BufferForWriting::pre_populate(&mut write_backing);
    /// ```
    ///
    /// [`BufferForReading`] can only be used to read from byte streams, while [`BufferForWriting`]
    /// can only be used to write back to those streams. This prevents the user from re-using the
    /// same buffer for reading and writing and potentially jumbling data.
    pub fn new(buffer: &'data mut [u8]) -> Self {
        assert!(!buffer.is_empty());

        buffer.fill(0);

        Self {
            buffer,
            window: 0..0,

            _phantom: std::marker::PhantomData,
        }
    }

    /// Stack allocates a new [`Buffer`], keeping the initialized memory. This can be useful for
    /// testing for example.
    ///
    /// ```rust
    /// # use http_core::prelude::*;
    /// let mut array: [u8; 8] = std::array::from_fn(|i| i as u8);
    /// let buffer = BufferForReading::pre_populate(&mut array);
    ///
    /// assert_eq!(buffer.as_ref(), &[0, 1, 2, 3, 4, 5, 6, 7]);
    /// ```
    pub fn pre_populate(buffer: &'data mut [u8]) -> Self {
        assert!(!buffer.is_empty());

        Self {
            window: 0..buffer.len(),
            buffer,

            _phantom: std::marker::PhantomData,
        }
    }

    /// Total available memory the buffer has access to.
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the length of the buffer. This is different from a buffer's full size and only
    /// counts data which has already been written to it.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// Returns true if the buffer has reached capacity an can no longer be written to. Calling
    /// [`clear`] will reset this.
    ///
    /// [`clear`]: Self::clear
    pub fn is_full(&self) -> bool {
        match self.len().cmp(&self.capacity()) {
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => true,
            std::cmp::Ordering::Greater => panic!("Invariant violated: invalid length"),
        }
    }

    /// Zeros-out the contents of a buffer.
    pub fn clear(&mut self) {
        self.buffer.fill(0);
        self.window = 0..0;
    }
}

impl<'data, Mode> AsRef<[u8]> for Buffer<'data, Mode> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.window.clone()]
    }
}

impl<'data, Mode> std::ops::Index<std::ops::Range<usize>> for Buffer<'data, Mode> {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data, Mode> std::ops::Index<std::ops::RangeInclusive<usize>> for Buffer<'data, Mode> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeInclusive<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data, Mode> std::ops::Index<std::ops::RangeFrom<usize>> for Buffer<'data, Mode> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data, Mode> std::ops::Index<std::ops::RangeTo<usize>> for Buffer<'data, Mode> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data, Mode> std::ops::Index<std::ops::RangeFull> for Buffer<'data, Mode> {
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

    #[rstest::rstest]
    fn index_range_base(mut array: [u8; SIZE]) {
        let oracle = array;
        let buffer = BufferForReading::pre_populate(&mut array);

        pretty_assertions::assert_eq!(buffer[1..10], oracle[1..10]);
    }

    #[rstest::rstest]
    fn index_range_inclusive(mut array: [u8; SIZE]) {
        let oracle = array;
        let buffer = BufferForReading::pre_populate(&mut array);

        pretty_assertions::assert_eq!(buffer[1..=10], oracle[1..=10]);
    }

    #[rstest::rstest]
    fn index_range_from(mut array: [u8; SIZE]) {
        let oracle = array;
        let buffer = BufferForReading::pre_populate(&mut array);

        pretty_assertions::assert_eq!(buffer[1..], oracle[1..]);
    }

    #[rstest::rstest]
    fn index_range_to(mut array: [u8; SIZE]) {
        let oracle = array;
        let buffer = BufferForReading::pre_populate(&mut array);

        pretty_assertions::assert_eq!(buffer[..SIZE], oracle[..SIZE]);
    }

    #[rstest::rstest]
    fn index_range_full(mut array: [u8; SIZE]) {
        let oracle = array;
        let buffer = BufferForReading::pre_populate(&mut array);

        pretty_assertions::assert_eq!(buffer[..], oracle[..])
    }
}
