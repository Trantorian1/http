const GET: &[u8] = b"GET";

pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::code::Status> {
    if data.len() < GET.len() {
        return Ok(None);
    }

    match &data[..GET.len()] {
        GET => Ok(Some(std::num::NonZero::new(GET.len()).unwrap())),
        _ => Err(crate::code::Status::NotImplemented),
    }
}

pub fn sp(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::code::Status> {
    match &data[..crate::SP.len()] {
        crate::SP => Ok(Some(std::num::NonZero::new(crate::SP.len()).unwrap())),
        _ => Ok(None),
    }
}

pub fn target(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::code::Status> {
    if let Some(stop) = memchr::memmem::find(data, crate::SP) {
        Ok(Some(std::num::NonZero::new(stop).unwrap()))
    } else {
        Ok(None)
    }
}

pub fn protocol(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::code::Status> {
    match &data[..crate::PROTOCOL.len()] {
        crate::PROTOCOL => Ok(Some(std::num::NonZero::new(crate::PROTOCOL.len()).unwrap())),
        _ => Ok(None),
    }
}

pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::code::Status> {
    match &data[..crate::CRLF.len()] {
        crate::CRLF => Ok(Some(std::num::NonZero::new(crate::CRLF.len()).unwrap())),
        _ => Ok(None),
    }
}
