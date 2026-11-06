#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Custom(String),
    InvalidUtf8(std::str::Utf8Error),
    EmptyLine,
    MissingFieldDelimiter,
    MissingFieldName,
    MissingFieldValue,
    MissingCRLFDelimiter,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self::Custom(msg.to_string())
    }
}

impl std::error::Error for Error {}
