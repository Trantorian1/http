//! HTTP status codes, see [RFC9110].
//!
//! > _"The status code of a response is a three-digit integer code that describes the result of the
//! > request and the semantics of the response, including whether the request was successful and
//! > what content is enclosed (if any). All valid status codes are within the range of 100 to 599,
//! > inclusive."_
//!
//! [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-status-codes

/// HTTP status code.
#[repr(u16)]
#[derive(Debug, Default)]
pub enum Status {
    #[default]
    /// See [RFC9110], 200 Ok
    ///
    /// > _"The 200 (OK) status code indicates that the request has succeeded."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-200-ok
    Ok = 200,

    /// See [RFC9110], 400 Bad Request
    ///
    /// > _"The 400 (Bad Request) status code indicates that the server cannot or will not process
    /// > the request due to something that is perceived to be a client error (e.g., malformed
    /// > request syntax, invalid request message framing, or deceptive request routing)."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-400-bad-request
    BadRequest = 400,

    /// See [RFC9110], 404 Not Found
    ///
    /// > _"The 404 (Not Found) status code indicates that the origin server did not find a current
    /// > representation for the [target resource] or is not willing to disclose that one exists. A
    /// > 404 status code does not indicate whether this lack of representation is temporary or
    /// > permanent; the 410 (Gone) status code is preferred over 404 if the origin server knows,
    /// > presumably through some configurable means, that the condition is likely to be
    /// > permanent."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-404-not-found
    /// [target resource]: https://www.rfc-editor.org/info/rfc9110/#target.resource
    NotFound = 404,

    /// See [RFC9110], 408 Request Timeout
    ///
    /// > _"The 408 (Request Timeout) status code indicates that the server did not receive a
    /// > complete request message within the time that it was prepared to wait."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-408-request-timeout
    RequestTimetout = 408,

    /// See [RFC9110], 413 Content Too Large
    ///
    /// > _"The 413 (Content Too Large) status code indicates that the server is refusing to process
    /// > a request because the request content is larger than the server is willing or able to
    /// > process. The server **MAY** terminate the request, if the protocol version in use allows
    /// > it; otherwise, the server **MAY** close the connection."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-413-content-too-large
    ContentTooLarge = 413,

    /// See [RFC9110], Internal Server Error
    ///
    /// > _"The 500 (Internal Server Error) status code indicates that the server encountered an
    /// > unexpected condition that prevented it from fulfilling the request."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-500-internal-server-error
    InternalServerError(Box<dyn std::error::Error>) = 500,

    /// See [RFC9110], Not Implemented
    ///
    /// > _"The 501 (Not Implemented) status code indicates that the server does not support the
    /// > functionality required to fulfill the request. This is the appropriate response when the
    /// > server does not recognize the request method and is not capable of supporting it for any
    /// > resource."_
    ///
    /// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-501-not-implemented
    NotImplemented = 501,
}

impl Status {
    /// Builds a new [`InternalServerError`].
    ///
    /// [`InternalServerError`]: Self::InternalServerError
    pub fn internal(error: impl std::error::Error + 'static) -> Self {
        // FIXME: this is the only part of the crate which allocates at runtime. It would be nice if
        // we could either define some common error interface or find some other way to handle
        // generic error encapsulation without allocating. By using a pointer perhaps?
        Self::InternalServerError(error.into())
    }

    /// See [RFC9112], status line.
    ///
    /// > _"The first line of a response message is the status-line, consisting of [..] **the status
    /// > code**."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-status-line
    pub fn code(&self) -> &'static [u8] {
        match self {
            Self::Ok => b"200 ",

            Self::BadRequest => b"400 ",
            Self::NotFound => b"404 ",
            Self::RequestTimetout => b"408 ",
            Self::ContentTooLarge => b"413 ",

            Self::InternalServerError(_) => b"500 ",
            Self::NotImplemented => b"501 ",
        }
    }

    /// See [RFC9112], status line.
    ///
    /// > _"The first line of a response message is the status-line [...] **ending with an OPTIONAL
    /// > textual phrase describing the status code**."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-status-line
    pub fn reason(&self) -> &'static [u8] {
        match self {
            Status::Ok => b"OK",

            Status::BadRequest => b"Bad Request",
            Status::NotFound => b"Not Found",
            Status::RequestTimetout => b"Request Timeout",
            Status::ContentTooLarge => b"Content Too Large",

            Status::InternalServerError(_) => b"Internal Server Error",
            Status::NotImplemented => b"Not Implemented",
        }
    }
}
