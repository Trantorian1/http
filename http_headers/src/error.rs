#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    EmptyLine,

    MissingFieldDelimiter,
    MissingFieldName,
    MissingCRLFDelimiter,

    InvalidFieldName,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorValue {
    EmptyList,
}

impl std::fmt::Display for ErrorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}

impl std::error::Error for ErrorValue {}
