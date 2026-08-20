// It probably doesn't make sense to increase this too much as then we would just be polluting
// the problem space with garbage data which likely does not contain any new edge cases. The
// most interesting targets probably lie around small array sizes anyway.
pub const MAX_SIZE: usize = 16;
const _: () = assert!(MAX_SIZE > 0);

#[rstest::fixture]
pub fn array() -> [u8; MAX_SIZE] {
    std::array::from_fn(|i| i as u8)
}

/// Number of bytes to read.
#[rstest::fixture]
pub fn generate_n_read() -> impl bolero::generator::ValueGenerator<Output = usize> {
    bolero::produce::<usize>().with().bounds(..MAX_SIZE)
}

/// Number of bytes to write.
#[rstest::fixture]
pub fn generate_n_write() -> impl bolero::generator::ValueGenerator<Output = usize> {
    bolero::produce::<usize>().with().bounds(..MAX_SIZE)
}

/// Stream capacity, cannot be 0.
#[rstest::fixture]
pub fn generate_capacity() -> impl bolero::generator::ValueGenerator<Output = usize> {
    bolero::produce::<usize>().with().bounds(1..MAX_SIZE)
}

#[rstest::fixture]
pub fn generate_stream(
    generate_n_read: impl bolero::generator::ValueGenerator<Output = usize>,
    generate_n_write: impl bolero::generator::ValueGenerator<Output = usize>,
    generate_capacity: impl bolero::generator::ValueGenerator<Output = usize>,
) -> impl bolero::generator::ValueGenerator<Output = (usize, usize, usize)> {
    (generate_n_read, generate_n_write, generate_capacity)
}
