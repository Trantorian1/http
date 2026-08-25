//! Zero-copy HTTP/1.1 request parsers.

use http_primitives::prelude::*;

use crate::prelude::*;

/// Parses the **method portion** of an HTTP request.
///
/// # Errors
///
/// Returns [`Status::NotImplemented`] if the HTTP request does not match any supported methods.
pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if data.len() < methods::CONNECT.len() {
        return Ok(None);
    }

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
        _ => Err(Status::BadRequest),
    }
}

/// Parses a **single space** in an HTTP request
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if data.len() < SP.len() {
        return Ok(None);
    }

    match &data[..SP.len()] {
        // SAFETY: SP is not empty
        SP => Ok(Some(unsafe { std::num::NonZero::new_unchecked(SP.len()) })),
        _ => Err(Status::BadRequest),
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
    if data.len() < PROTOCOL.len() {
        return Ok(None);
    }

    match &data[..PROTOCOL.len()] {
        // SAFETY: PROTOCOL is not empty
        PROTOCOL => Ok(Some(unsafe {
            std::num::NonZero::new_unchecked(PROTOCOL.len())
        })),
        _ => Err(Status::BadRequest),
    }
}

pub fn status(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if data.len() < 3 {
        return Ok(None);
    }

    // FIXME: look up the set of legal values for http status codes and parse that instead. This is
    // a quick hack but is overly restrictive against custom status codes.
    match &data[..3] {
        b"200" | b"400" | b"404" | b"408" | b"413" | b"500" | b"501" => {
            // SAFETY: 3 > 0 dumbass
            Ok(Some(unsafe { std::num::NonZero::new_unchecked(3) }))
        },
        _ => Err(Status::NotImplemented),
    }
}

/// Parses a **carriage return line feed** (line end) in an HTTP request.
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    if data.len() < CRLF.len() {
        return Ok(None);
    }

    match &data[..CRLF.len()] {
        // SAFETY: CRLF is not empty
        CRLF => Ok(Some(unsafe {
            std::num::NonZero::new_unchecked(CRLF.len())
        })),
        _ => Err(Status::BadRequest),
    }
}
