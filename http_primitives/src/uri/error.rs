#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    MissingHost,
    ReservedCharacter(u8),
    EmptyQueryParameter,
    InvalidQuery,
    MissingQueryKey,
    MissingQueryValue,
}
