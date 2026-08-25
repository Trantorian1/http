//! Zero-copy HTTP/1.1 request parsers.

use http_primitives::prelude::*;

use crate::prelude::*;

/// Parses the **method portion** of an HTTP request.
///
/// # Errors
///
/// Returns [`Status::NotImplemented`] if the HTTP request does not match any supported methods.
pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..methods::GET.len()] {
        [b'G', b'E', b'T', ..] => Ok(Some(unsafe {
            // SAFETY: GET is not empty
            std::num::NonZero::new_unchecked(methods::GET.len())
        })),
        [b'H', b'E', b'A', b'D', ..] => Ok(Some(unsafe {
            // SAFETY: HEAD is not empty
            std::num::NonZero::new_unchecked(methods::HEAD.len())
        })),
        [b'P', b'O', b'S', b'T', ..] => Ok(Some(unsafe {
            // SAFETY: POST is not empty
            std::num::NonZero::new_unchecked(methods::POST.len())
        })),
        [b'P', b'U', b'T', ..] => Ok(Some(unsafe {
            // SAFETY: PUT is not empty
            std::num::NonZero::new_unchecked(methods::PUT.len())
        })),
        [b'D', b'E', b'L', b'E', b'T', b'E', ..] => Ok(Some(unsafe {
            // SAFETY: DELETE is not empty
            std::num::NonZero::new_unchecked(methods::DELETE.len())
        })),
        [b'C', b'O', b'N', b'N', b'E', b'C', b'T', ..] => Ok(Some(unsafe {
            // SAFETY: CONNECT is not empty
            std::num::NonZero::new_unchecked(methods::CONNECT.len())
        })),
        [b'O', b'P', b'T', b'I', b'O', b'N', b'S', ..] => Ok(Some(unsafe {
            // SAFETY: OPTIONS is not empty
            std::num::NonZero::new_unchecked(methods::OPTIONS.len())
        })),
        [b'T', b'R', b'A', b'C', b'E', ..] => Ok(Some(unsafe {
            // SAFETY: TRACE is not empty
            std::num::NonZero::new_unchecked(methods::TRACE.len())
        })),
        _ if data.len() >= methods::CONNECT.len() => Err(Status::BadRequest),
        _ => Ok(None),
    }
}

/// Parses a **single space** in an HTTP request
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..SP.len()] {
        // SAFETY: SP is not empty
        SP => Ok(Some(unsafe { std::num::NonZero::new_unchecked(SP.len()) })),
        _ => Ok(None),
    }
}

/// Parses the **target** of an HTTP request.
///
/// # Errors
///
/// Returns [`Status::BadRequest`] if the HTTP request does not contain a target.
pub fn target(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match memchr::memmem::find(data, SP) {
        // SAFETY: this branch only runs if stop > 0
        Some(stop) if stop > 0 => Ok(Some(unsafe { std::num::NonZero::new_unchecked(stop) })),
        Some(_) => Err(Status::BadRequest),
        None => Ok(None),
    }
}

/// Parses the **protocol version** in use by an HTTP request.
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn protocol(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..PROTOCOL.len()] {
        // SAFETY: PROTOCOL is not empty
        PROTOCOL => Ok(Some(unsafe {
            std::num::NonZero::new_unchecked(PROTOCOL.len())
        })),
        _ => Ok(None),
    }
}

/// Parses a **carriage return line feed** (line end) in an HTTP request.
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    match &data[..CRLF.len()] {
        // SAFETY: CRLF is not empty
        CRLF => Ok(Some(unsafe {
            std::num::NonZero::new_unchecked(CRLF.len())
        })),
        _ => Ok(None),
    }
}
