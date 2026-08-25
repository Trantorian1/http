use super::ByteStream;
use crate::prelude::*;

impl std::io::Write for ByteStream<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.write_impl(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl ByteStream<'_> {
    //
    // -- Mutations
    //
    #[cfg_attr(all(test, kani), kani::modifies(&self.start, &self.size, self.backing))]
    //
    // -- Pre-conditions
    //
    #[cfg_attr(all(test, kani), kani::requires(self.invariants()))]
    //
    // -- Post-conditions
    //
    // Start index cannot be mutated by stream writes, only stream reads.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|_| self.start == old(self.start)))]
    //
    // Stream size grows with the number of bytes written.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|_| self.size == old(self.size) + old(self.space_left()).min(buf.len())))]
    //
    // Results are coherent with the data being written and can never error out.
    //
    #[cfg_attr(all(test, kani), kani::ensures(|written|
        *written == old(self.space_left()).min(buf.len())
            && self.size == old(self.size) + *written
    ))]
    ///
    /// If this seems confusing to you, check out the `kani` docs on [function contracts]
    ///
    /// [function contracts]: https://model-checking.github.io/kani/crates/doc/kani/contracts/index.html
    fn write_impl(&mut self, buf: &[u8]) -> usize {
        assert_le!(self.start, self.capacity());
        assert_leq!(self.size, self.capacity());

        let start = self.start;
        let stop = self.start + self.size;
        let bytes = buf.len();

        if stop <= self.capacity() {
            // The data currently in the buffer is contiguous, we might need to wrap around in order
            // to write more bytes. Here, were start by appending to the end of the stream.
            let space_after_stop = (self.capacity() - stop).min(bytes);
            self.backing[stop..stop + space_after_stop].copy_from_slice(&buf[..space_after_stop]);

            // Next, we try and write whatever bytes remain at the start of the stream.
            let space_before_start = start.min(bytes - space_after_stop);
            self.backing[..space_before_start]
                .copy_from_slice(&buf[space_after_stop..space_after_stop + space_before_start]);

            self.size += space_after_stop + space_before_start;

            space_after_stop + space_before_start
        } else {
            // The data currently in the buffer is NOT contiguous and wraps around. This actually
            // makes our life easier, as we only need a single write to cover the area of memory
            // which we have left.
            let stop_wrapped = stop - self.capacity();
            let space_before_start = (start - stop_wrapped).min(bytes);

            // Write to the middle of the buffer, taking existing data wrap-around into consideration.
            self.backing[stop_wrapped..stop_wrapped + space_before_start]
                .copy_from_slice(&buf[..space_before_start]);

            self.size += space_before_start;

            space_before_start
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
    /// cargo bolero test -p http_primitives mem::stream::write::contracts::check_contract_write --engine kani
    /// ```
    #[rstest::rstest]
    #[kani::proof_for_contract(ByteStream::write_impl)]
    #[kani::unwind(17)]
    fn check_contract_write(
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
                stream_invariant_problem(n_read, n_write, nonzero!(capacity), start, size);
            })
    }
}
