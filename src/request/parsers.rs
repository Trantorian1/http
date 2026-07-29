use crate::prelude::*;

const GET: &[u8] = b"GET";

pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status> {
    if data.len() < GET.len() {
        return Ok(None);
    }

    match &data[..GET.len()] {
        GET => Ok(Some(std::num::NonZero::new(GET.len()).unwrap())),
        _ => Err(code::Status::NotImplemented),
    }
}

pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status> {
    match &data[..SP.len()] {
        SP => Ok(Some(std::num::NonZero::new(SP.len()).unwrap())),
        _ => Ok(None),
    }
}

pub fn target(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status> {
    if let Some(stop) = memchr::memmem::find(data, SP) {
        Ok(Some(std::num::NonZero::new(stop).unwrap()))
    } else {
        Ok(None)
    }
}

pub fn protocol(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status> {
    match &data[..PROTOCOL.len()] {
        PROTOCOL => Ok(Some(std::num::NonZero::new(PROTOCOL.len()).unwrap())),
        _ => Ok(None),
    }
}

pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, code::Status> {
    match &data[..CRLF.len()] {
        CRLF => Ok(Some(std::num::NonZero::new(CRLF.len()).unwrap())),
        _ => Ok(None),
    }
}
