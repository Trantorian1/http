//! A simple HTTP/1.1 server, with a focus on zero-copy deserialization and predictable behavior via
//! stack-based allocations and a deterministic implementation.
//!
//! Based off Codecrafter's [build your own HTTP server].
//!
//! See [RFC9112] for an overview of the specs.
//!
//! [build your own HTTP server]: https://app.codecrafters.io/courses/http-server/overview
//! [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112

pub mod request;
pub mod response;
pub mod server;

pub mod prelude {
    //! A “prelude” for crates using the `http_1` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use http_1::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use super::*;

    pub use super::request::Request;
    pub use super::request::RequestInfo;

    pub use super::response::Response;
    pub use super::server::Server;
}

pub use prelude::*;

/// HTTP protocol version
pub const PROTOCOL: &[u8] = b"HTTP/1.1";

/// Carriage Return + Line Feed
pub const CRLF: &[u8] = b"\r\n";

/// Single space
pub const SP: &[u8] = b" ";
