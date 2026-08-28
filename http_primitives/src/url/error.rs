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

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::MissingScheme => f.write_str("Empty scheme before scheme delimiter"),
            Error::MissingHost => f.write_str("Empty host in URI with non-empty port or scheme"),
            Error::MissingQuery => f.write_str("Empty query after query delimiter"),
            Error::MissingFragment => f.write_str("Empty fragment after fragment delimiter"),
            Error::ReservedCharacter(c) => write!(f, "Use of reserved character {c}"),
            Error::EmptyQueryParameter => f.write_str("Empty query parameter"),
            Error::InvalidQuery => f.write_str("Query parameter does not adhere to key-value form"),
            Error::MissingQueryKey => f.write_str("Query parameter without a key"),
            Error::MissingQueryValue => f.write_str("Query parameter without a value"),
        }
    }
}

impl std::error::Error for Error {}
