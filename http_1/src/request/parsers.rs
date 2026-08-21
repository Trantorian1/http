//! Zero-copy HTTP/1.1 request parsers.

use http_primitives::prelude::*;

use crate::prelude::*;

const GET: &[u8] = b"GET";

/// Parses the **method portion** of an HTTP request.
///
/// # Errors
///
/// Returns [`Status::NotImplemented`] if the HTTP request does not match any supported methods.
pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    const _: () = assert!(!GET.is_empty());

    if data.len() < GET.len() {
        return Ok(None);
    }

    match &data[..GET.len()] {
        // SAFETY: The above assertions guarantees that GET can never have a length of 0
        GET => Ok(Some(unsafe { std::num::NonZero::new_unchecked(GET.len()) })),
        _ => Err(Status::NotImplemented),
    }
}

/// Parses a **single space** in an HTTP request
///
/// # Errors
///
/// This function never errors, but this guarantee might change in future versions as more edge
/// cases are handled.
pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, Status> {
    const _: () = assert!(!SP.is_empty());

    match &data[..SP.len()] {
        // SAFETY: The above assertions guarantees that SP can never have a length of 0
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
    const _: () = assert!(!PROTOCOL.is_empty());

    match &data[..PROTOCOL.len()] {
        // SAFETY: The above assertions guarantees that PROTOCOL can never have a length of 0
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
    const _: () = assert!(!CRLF.is_empty());

    match &data[..CRLF.len()] {
        // SAFETY: The above assertions guarantees that CRLF can never have a length of 0
        CRLF => Ok(Some(unsafe {
            std::num::NonZero::new_unchecked(CRLF.len())
        })),
        _ => Ok(None),
    }
}
