const GET: &[u8] = b"GET ";

pub fn method(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::buffer::Error> {
    if let Some(start) = memchr::memmem::find(data, GET) {
        let stop = std::num::NonZero::new(start + GET.len()).unwrap();
        Ok(Some(stop))
    } else {
        Ok(None)
    }
}

pub fn target(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::buffer::Error> {
    if let Some(stop) = memchr::memmem::find(data, crate::SP) {
        Ok(Some(std::num::NonZero::new(stop).unwrap()))
    } else {
        Ok(None)
    }
}

pub fn protocol(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::buffer::Error> {
    if let Some(start) = memchr::memmem::find(data, crate::PROTOCOL) {
        let stop = std::num::NonZero::new(start + crate::PROTOCOL.len()).unwrap();
        Ok(Some(stop))
    } else {
        Ok(None)
    }
}

pub fn crlf(data: &[u8]) -> Result<Option<std::num::NonZeroUsize>, crate::buffer::Error> {
    if let Some(start) = memchr::memmem::find(data, crate::CRLF) {
        let stop = std::num::NonZero::new(start + crate::CRLF.len()).unwrap();
        Ok(Some(stop))
    } else {
        Ok(None)
    }
}
