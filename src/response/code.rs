#[derive(Default)]
/// HTTP status code.
pub enum Status {
    #[default]
    Ok,
    NotFound,
}

impl Status {
    /// See [RFC9112], status line.
    ///
    /// > _"The first line of a response message is the status-line, consisting of [..] the status
    /// > code."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-status-line
    pub fn code(&self) -> &'static [u8] {
        match self {
            Self::Ok => b"200 ",
            Self::NotFound => b"404 ",
        }
    }

    /// See [RFC9112], status line.
    ///
    /// > _"The first line of a response message is the status-line [...] ending with an OPTIONAL
    /// > textual phrase describing the status code."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-status-line
    pub fn reason(&self) -> &'static [u8] {
        match self {
            Status::Ok => b"OK",
            Status::NotFound => b"Not Found",
        }
    }
}
