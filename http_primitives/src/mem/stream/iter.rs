use crate::prelude::*;

use super::ByteStream;

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
        assert_leq!(self.stream.start, self.stream.capacity());
        assert_leq!(self.stream.size, self.stream.capacity());

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
use super::fixtures::*;

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
                let mut backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
                let bytes_prev = backing;

                let mut stream = ByteStream::any(&mut backing[..capacity], start, size);
                let mut iter = stream.iter();

                for i in 0..size {
                    assert_eq!(iter.next(), Some(bytes_prev[(i + start) % capacity]));
                }

                assert!(stream.is_empty());
            });
    }
}
