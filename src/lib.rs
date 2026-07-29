//! A simple HTTP/1.1 server implementation, based off Codecrafter's [build your own HTTP server].
//!
//! See [RFC9112] for an overview of the specs.
//!
//! [build your own HTTP server]: https://app.codecrafters.io/courses/http-server/overview
//! [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112

pub mod buffer;
pub mod code;
pub mod request;
pub mod response;
pub mod server;
pub mod size;

pub mod testing;

pub(crate) mod prelude {
    pub use super::*;

    pub mod code {
        pub use crate::code::*;
    }

    pub mod size {
        pub use crate::size::*;
    }
}

pub use buffer::Buffer;
pub use buffer::BufferForReading;
pub use buffer::BufferForWriting;

pub use request::Request;
pub use request::RequestInfo;

pub use response::Response;
pub use server::Server;

/// HTTP protocol version
pub const PROTOCOL: &[u8] = b"HTTP/1.1";

/// Carriage Return + Line Feed
pub const CRLF: &[u8] = b"\r\n";

/// Single space
pub const SP: &[u8] = b" ";
