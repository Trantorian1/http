use crate::prelude::*;

use super::ByteStream;

impl<'data> std::io::Read for ByteStream<'data> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(self.read_impl(buf))
    }
}

impl<'data> ByteStream<'data> {
    //
    // -- Mutations
    //
    #[cfg_attr(all(test, kani), kani::modifies(&self.start, &self.size, buf))]
    //
    // -- Pre-conditions
    //
    #[cfg_attr(all(test, kani), kani::requires(self.invariants()))]
    //
    // -- Post-conditions
    //
    // Start index must wrap around the buffer.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|_| self.start == (old(self.start) + old(self.size).min(buf.len())) % self.capacity()))]
    //
    // Bytes are consumed as they are read.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|_| self.size == old(self.size) - old(self.size).min(buf.len())))]
    //
    // Results are coherent with the data being read and can never error out.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|read|
        *read == old(self.size).min(buf.len())
            && self.size == old(self.size) - *read
    ))]
    ///
    /// If this seems confusing to you, check out the `kani` docs on [function contracts]
    ///
    /// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
    fn read_impl(&mut self, buf: &mut [u8]) -> usize {
        assert_le!(self.start, self.capacity());
        assert_leq!(self.size, self.capacity());

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        if stop <= self.capacity() {
            // Stream data stops before the end of the buffer, no need for wrap-around logic.
            let space_after_start = self.size.min(bytes);

            // Single-copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            self.start = (self.start + space_after_start) % self.capacity();
            self.size -= space_after_start;
            assert_leq!(self.size, self.capacity());

            space_after_start
        } else {
            // Stream data goes past the end of the buffer, we need to handle ring wrap-around.
            let space_after_start = (self.capacity() - self.start).min(bytes);

            // First copy, retrieve all data before the end of the buffer.
            buf[..space_after_start]
                .copy_from_slice(&self.buffer[start..start + space_after_start]);

            let space_before_stop = (stop - self.capacity()).min(bytes - space_after_start);

            // Second copy, wrap around to the start of the buffer and copy data from there.
            buf[space_after_start..space_after_start + space_before_stop]
                .copy_from_slice(&self.buffer[..space_before_stop]);

            self.start = (self.start + space_after_start + space_before_stop) % self.capacity();
            self.size -= space_after_start + space_before_stop;
            assert_leq!(self.size, self.capacity());

            space_after_start + space_before_stop
        }
    }
}

#[cfg(all(test, kani))]
use super::fixtures::*;

#[cfg(all(test, kani))]
use super::invariants::*;

/// See [function contracts].
///
/// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
#[cfg(all(test, kani))]
mod contracts {
    use super::*;

    /// Contract validation tests MUST be run with `kani`.
    ///
    /// ```bash
    /// cargo bolero test -p http_primitives mem::stream::read::contracts::check_contract_read --engine kani
    /// ```
    #[rstest::rstest]
    #[kani::proof_for_contract(ByteStream::read_impl)]
    #[kani::unwind(17)]
    fn check_contract_read(
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
