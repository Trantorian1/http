mod error;
mod query;

pub use error::Error;
pub use query::*;

pub struct Url<'data> {
    pub scheme: &'data [u8],
    pub host: &'data [u8],
    pub port: &'data [u8],
    pub path: &'data [u8],
    pub query: QueryForm<'data>,
    pub fragment: &'data [u8],

    backing: &'data [u8],
}

impl<'data> Url<'data> {
    pub fn new(bytes: &'data [u8]) -> Result<Self, Error> {
        // FIXME: parse out legal characters and % encoding

        // == Step 1: parse and separate URL segments ==============================================

        let (fragment, before_fragment) = match memchr::memrchr(b'#', bytes) {
            Some(n) if n >= bytes.len() => return Err(Error::MissingFragment),
            Some(n) => (n + 1..bytes.len(), ..n),
            None => (bytes.len()..bytes.len(), ..bytes.len()),
        };

        let (query, before_query) = match memchr::memrchr(b'?', &bytes[before_fragment]) {
            Some(n) => (n + 1..before_fragment.end, ..n),
            None => (before_fragment.end..before_fragment.end, before_fragment),
        };

        let (scheme, hier_part) = match memchr::memmem::rfind(&bytes[before_query], b"://") {
            Some(0) => return Err(Error::MissingScheme),
            Some(n) => (0..n, n + 3..before_query.end),
            None => (0..0, 0..before_query.end),
        };

        let (path, authority) = match memchr::memrchr(b'/', &bytes[hier_part.clone()]) {
            Some(n) => (
                hier_part.start + n + 1..hier_part.end,
                hier_part.start..hier_part.start + n,
            ),
            None => (hier_part.end..hier_part.end, hier_part.clone()),
        };

        let (host, port) = match memchr::memrchr(b':', &bytes[authority.clone()]) {
            Some(0) => return Err(Error::MissingHost),
            Some(n) => (
                authority.start..authority.start + n,
                authority.start + n + 1..authority.end,
            ),
            None => (authority.clone(), authority.end..authority.end),
        };

        if !scheme.is_empty() && host.is_empty() {
            return Err(Error::MissingHost);
        }

        // == Step 2: check each segment for illegal characters ====================================

        for i in scheme.clone() {
            if !unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

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

        for i in path.clone() {
            // URL paths allow the use of `/` characters as path separators
            if !path_abempty(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        Ok(Self {
            scheme: &bytes[scheme],
            host: &bytes[host],
            fragment: &bytes[fragment],

            port: if port.is_empty() { b"80" } else { &bytes[port] },
            path: if path.is_empty() { b"/" } else { &bytes[path] },

            query: QueryForm::new(&bytes[query])?,

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

impl<'data> PartialEq for Url<'data> {
    fn eq(&self, other: &Self) -> bool {
        self.backing == other.backing
    }
}

impl<'data> std::fmt::Debug for Url<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scheme = std::str::from_utf8(&self.scheme).unwrap_or_default();
        let host = std::str::from_utf8(&self.host).unwrap_or_default();
        let port = std::str::from_utf8(&self.port).unwrap_or_default();
        let path = std::str::from_utf8(&self.path).unwrap_or_default();
        let fragment = std::str::from_utf8(&self.fragment).unwrap_or_default();

        f.debug_struct("Url")
            .field("scheme", &scheme)
            .field("host", &host)
            .field("port", &port)
            .field("path", &path)
            .field("query", &self.query)
            .field("fragment", &fragment)
            .finish()
    }
}

impl<'data> std::fmt::Display for Url<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::str::from_utf8(self.backing).unwrap_or_default())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn url_simple() {
        let url = Url::new(b"http://example.com?hello=world#anchor").unwrap();
        assert_streq!(url.scheme, b"http");
        assert_streq!(url.host, b"example.com");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/");

        let mut parameters = url.query.iter();
        let a = parameters.next().unwrap();

        assert_streq!(a.key, b"hello");
        assert_streq!(a.val, b"world");
        assert_eq!(parameters.next(), None);

        assert_eq!(url.fragment, b"anchor")
    }

    #[test]
    fn url_only_host() {
        let url = Url::new(b"example.com").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"example.com");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/");
        assert_eq!(url.query.iter().next(), None);
        assert_eq!(url.fragment, b"")
    }

    #[test]
    fn url_only_path() {
        let url = Url::new(b"/path").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"path");
        assert_eq!(url.query.iter().next(), None);
        assert_eq!(url.fragment, b"")
    }

    #[test]
    fn url_only_query() {
        let url = Url::new(b"?a=1").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/");

        let mut parameters = url.query.iter();
        let a = parameters.next().unwrap();

        assert_streq!(a.key, b"a");
        assert_streq!(a.val, b"1");
        assert_eq!(parameters.next(), None);

        assert_streq!(url.fragment, b"");
    }

    #[test]
    fn url_only_fragment() {
        let url = Url::new(b"#fragment").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/");
        assert_eq!(url.query.iter().next(), None);
        assert_eq!(url.fragment, b"fragment")
    }

    #[test]
    fn url_invalid_only_scheme_and_port() {
        assert_eq!(Url::new(b"http://:80"), Err(Error::MissingHost));
    }

    #[test]
    fn url_invalid_only_scheme_and_path() {
        assert_eq!(Url::new(b"http:///"), Err(Error::MissingHost));
    }

    #[test]
    fn url_invalid_only_scheme() {
        assert_eq!(Url::new(b"http://"), Err(Error::MissingHost));
    }

    #[test]
    fn url_invalid_only_port() {
        assert_eq!(Url::new(b":80"), Err(Error::MissingHost));
    }
}

#[cfg(test)]
mod fuzz {
    use super::Url;

    #[test]
    fn fuzz_url() {
        bolero::check!().for_each(|bytes| {
            let _ = Url::new(bytes);
        })
    }
}
