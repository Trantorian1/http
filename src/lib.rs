pub mod buffer;
pub mod code;
pub mod request;
pub mod response;

pub use buffer::Buffer;
pub use request::Request;
pub use response::Response;

// HTTP protocol version
pub const PROTOCOL: &[u8] = b"HTTP/1.1";

// Carriage Return + Line Feed
pub const CRLF: &[u8] = b"\r\n";

// Single space
pub const SP: &[u8] = b" ";
