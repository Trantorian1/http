//! Zero-copy HTTP/1.1 request parsers.

use crate::prelude::*;
use http_primitives::prelude::*;

const GET: &[u8] = b"GET";

/// Parses the **method portion** of an HTTP request.
pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if data.len() < GET.len() {
        return Ok(None);
    }

    match &data[..GET.len()] {
        GET => Ok(Some(std::num::NonZero::new(GET.len()).unwrap())),
        _ => Err(Status::NotImplemented),
    }
}

/// Parses a **single space** in an HTTP request
pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..SP.len()] {
        SP => Ok(Some(std::num::NonZero::new(SP.len()).unwrap())),
        _ => Ok(None),
    }
}

/// Parses the **target** of an HTTP request.
pub fn target(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if let Some(stop) = memchr::memmem::find(data, SP) {
        Ok(Some(std::num::NonZero::new(stop).unwrap()))
    } else {
        Ok(None)
    }
}

/// Parses the **protocol version** in use by an HTTP request.
pub fn protocol(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..PROTOCOL.len()] {
        PROTOCOL => Ok(Some(std::num::NonZero::new(PROTOCOL.len()).unwrap())),
        _ => Ok(None),
    }
}

/// Parses a **carriage return line feed** (line end) in an HTTP request.
pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..CRLF.len()] {
        CRLF => Ok(Some(std::num::NonZero::new(CRLF.len()).unwrap())),
        _ => Ok(None),
    }
}
