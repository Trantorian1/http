use crate::prelude::*;

use super::ByteStream;

impl<'data> ByteStream<'data> {
    /// Returns an iterator over the stream.
    pub fn iter(&self) -> Iter<'_, 'data> {
        Iter::new(self)
    }
}

/// An iterator over the elements of a `ByteStream`.
///
/// This `struct` is created by the [`iter`] method on a [`ByteStream`]. See its documentation for more.
///
/// [`iter`]: ByteStream::iter
pub struct Iter<'stream, 'data>
where
    'data: 'stream,
{
    stream: &'stream ByteStream<'data>,
    start: usize,
    index: usize,
}

impl<'stream, 'data> Iter<'stream, 'data> {
    fn new(stream: &'stream ByteStream<'data>) -> Self {
        Self {
            start: stream.start,
            index: 0,

            stream,
        }
    }
}

impl<'stream, 'data> std::fmt::Debug for Iter<'stream, 'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Iter").field(self.stream).finish()
    }
}

impl<'stream, 'data> Iterator for Iter<'stream, 'data> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        assert_leq!(self.stream.start, self.stream.capacity());
        assert_leq!(self.stream.size, self.stream.capacity());

        if self.index < self.stream.size {
            let stop = self.start + self.index;
            let index = if stop >= self.stream.capacity() {
                stop - self.stream.capacity()
            } else {
                stop
            };

            let item = self.stream.buffer[index];
            self.index += 1;

            Some(item)
        } else {
            None
        }
    }
}

#[cfg(test)]
use super::fixtures::*;

#[cfg(test)]
use super::invariants::*;

#[cfg(test)]
mod validate {
    use super::*;

    /// ## Libfuzzer
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::iter::validate::stream_iter
    /// ```
    ///
    /// ## AFL
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::iter::validate::stream_iter --engine afl --sanitizer NONE
    /// ```
    ///
    /// ## Kani
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::iter::validate::stream_iter --engine kani
    /// ```
    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(17))]
    fn stream_iter() {
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
                stream_iter_invariant_problems(capacity, start, size);
            });
    }
}
