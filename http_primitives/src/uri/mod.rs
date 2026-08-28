mod error;
mod query;

pub use error::Error;

pub struct Uri<'data> {
    pub scheme: &'data [u8],
    pub host: &'data [u8],
    pub port: &'data [u8],
    pub path: &'data [u8],
    backing: &'data [u8],
}

impl<'data> Uri<'data> {
    pub fn new(bytes: &'data [u8]) -> Result<Self, Error> {
        // FIXME: parse out legal characters and % encoding

        // == Step 1: parse and separate URI segments ==============================================

        let (scheme, skip_scheme) = match memchr::memmem::find(bytes, b"://") {
            None => (0..0, 0),
            Some(n) => (0..n, n + 3),
        };

        let (authority, skip_authority) = match memchr::memchr2(b'/', b'?', &bytes[skip_scheme..]) {
            None => (skip_scheme..bytes.len(), bytes.len()),
            Some(0) => (skip_scheme..skip_scheme, skip_scheme),
            Some(n) => (skip_scheme..skip_scheme + n, skip_scheme + n + 1),
        };

        let (host, port) = match memchr::memchr(b':', &bytes[authority.clone()]) {
            None => (skip_scheme..authority.end, skip_authority..skip_authority),
            Some(0) => return Err(Error::MissingHost),
            Some(n) => (
                skip_scheme..skip_scheme + n,
                skip_scheme + n + 1..skip_authority,
            ),
        };

        let (path, query) = match memchr::memchr(b'?', &bytes[skip_authority..]) {
            None => (skip_authority..bytes.len(), bytes.len()..bytes.len()),
            Some(n) => (
                skip_authority..skip_authority + n,
                skip_authority + n + 1..bytes.len(),
            ),
        };

        if host.is_empty() && !scheme.is_empty() {
            return Err(Error::MissingHost);
        }

        // == Step 2: check each segment for illegal characters ====================================

        for i in scheme.clone() {
            if !unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        let foo = std::str::from_utf8(&bytes[host.clone()]).unwrap_or_default();

        for i in host.clone() {
            if !unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        for i in port.clone() {
            if !unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        let foo_2 = std::str::from_utf8(&bytes[path.clone()]).unwrap_or_default();

        for i in path.clone() {
            // URI paths allow the use of `/` characters as path separators
            if !path_abempty(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        Ok(Self {
            scheme: &bytes[scheme],
            host: &bytes[host],

            port: if port.is_empty() { b"80" } else { &bytes[port] },
            path: if path.is_empty() { b"/" } else { &bytes[path] },

            backing: bytes,
        })
    }
}

fn unreserved(c: u8) -> bool {
    matches!(c, b'A'..b'Z' |  b'a'..b'z' | b'0'..b'9' | b'-' | b'.' | b'_' | b'~')
}

fn path_abempty(c: u8) -> bool {
    matches!(c, b'A'..b'Z' |  b'a'..b'z' | b'0'..b'9' | b'-' | b'.' | b'_' | b'~' | b'/')
}

impl<'data> PartialEq for Uri<'data> {
    fn eq(&self, other: &Self) -> bool {
        self.backing == other.backing
    }
}

impl<'data> std::fmt::Debug for Uri<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Uri")
            .field(
                "scheme",
                &std::str::from_utf8(&self.scheme).unwrap_or_default(),
            )
            .field("host", &std::str::from_utf8(&self.host).unwrap_or_default())
            .field("port", &std::str::from_utf8(&self.port).unwrap_or_default())
            .field("path", &std::str::from_utf8(&self.path).unwrap_or_default())
            .finish()
    }
}

impl<'data> std::fmt::Display for Uri<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::str::from_utf8(self.backing).unwrap_or_default())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn uri_simple() {
        let uri = Uri::new(b"http://example.com").unwrap();
        assert_streq!(uri.scheme, b"http");
        assert_streq!(uri.host, b"example.com");
        assert_streq!(uri.port, b"80");
        assert_streq!(uri.path, b"/");
    }

    #[test]
    fn uri_only_host() {
        let uri = Uri::new(b"example.com").unwrap();
        assert_streq!(uri.scheme, b"");
        assert_streq!(uri.host, b"example.com");
        assert_streq!(uri.port, b"80");
        assert_streq!(uri.path, b"/");
    }

    #[test]
    fn uri_only_path() {
        let uri = Uri::new(b"/").unwrap();
        assert_streq!(uri.scheme, b"");
        assert_streq!(uri.host, b"");
        assert_streq!(uri.port, b"80");
        assert_streq!(uri.path, b"/");
    }

    #[test]
    fn uri_invalid_only_scheme_and_port() {
        assert_eq!(Uri::new(b"http://:80"), Err(Error::MissingHost));
    }

    #[test]
    fn uri_invalid_only_scheme_and_path() {
        assert_eq!(Uri::new(b"http:///"), Err(Error::MissingHost));
    }

    #[test]
    fn uri_invalid_only_scheme() {
        assert_eq!(Uri::new(b"http://"), Err(Error::MissingHost));
    }

    #[test]
    fn uri_invalid_only_port() {
        assert_eq!(Uri::new(b":80"), Err(Error::MissingHost));
    }
}
