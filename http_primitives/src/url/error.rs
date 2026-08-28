#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    MissingScheme,
    MissingHost,
    MissingQuery,
    MissingFragment,
    ReservedCharacter(u8),
    EmptyQueryParameter,
    InvalidQuery,
    MissingQueryKey,
    MissingQueryValue,
}
