//! [Url] parsing utilities.

mod error;
mod query;

pub use error::Error;
pub use query::*;

/// Zero-copy **U**niform **R**esource **L**ocators.
///
/// See [RFC3986] for more information.
///
/// [RFC3986]: https://www.rfc-editor.org/info/rfc3986
pub struct Url<'data> {
    /// See [RFC3986], scheme.
    ///
    /// > _"Each URI begins with a scheme name that refers to a specification for assigning
    /// > identifiers within that scheme.  As such, the URI syntax is a federated and extensible
    /// > naming system wherein each scheme's specification may further restrict the syntax and
    /// > semantics of identifiers using that scheme."_
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.1
    pub scheme: &'data [u8],

    /// See [RFC3986], host.
    ///
    /// > _"The host subcomponent of authority is identified by an IP literal encapsulated within
    /// > square brackets, an IPv4 address in dotted-decimal form, or a registered name. The host
    /// > subcomponent is case-insensitive.  The presence of a host subcomponent within a URI does
    /// > not imply that the scheme requires access to the given host on the Internet."_
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.2.2
    pub host: &'data [u8],

    /// See [RFC3986], port.
    ///
    /// > _"The port subcomponent of authority is designated by an optional port number in decimal
    /// > following the host and delimited from it by a single colon (":") character."_
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.2.3
    pub port: &'data [u8],

    /// See [RFC3986], path.
    ///
    /// > _"The path component contains data, usually organized in hierarchical form, that, along
    /// > with data in the non-hierarchical query component [(Section 3.4)], serves to identify a
    /// > resource within the scope of the URI's scheme and naming authority (if any). The path is
    /// > terminated by the first question mark ("?") or number sign ("#") character, or by the end
    /// > of the URI."_
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.3
    /// [Section 3.4]: https://www.rfc-editor.org/info/rfc3986/#section-3.4
    pub path: &'data [u8],

    /// See [RFC3986], query.
    ///
    /// > _"The query component contains non-hierarchical data that, along with data in the path
    /// > component (Section 3.3), serves to identify a resource within the scope of the URI's
    /// > scheme and naming authority (if any)."_
    ///
    /// # Usage
    ///
    /// HTTP query data is expected to be in `x-www-form-urlencoded` format.
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.4
    /// [Section 3.3]: https://www.rfc-editor.org/info/rfc3986/#section-3.3
    pub query: Query<'data>,

    /// See [RFC3986], fragment.
    ///
    /// > _"The fragment identifier component of a URI allows indirect identification of a secondary
    /// > resource by reference to a primary resource and additional identifying information. The
    /// > identified secondary resource may be some portion or subset of the primary resource, some
    /// > view on representations of the primary resource, or some other resource defined or
    /// > described by those representations."_
    ///
    /// [RFC3986]: https://www.rfc-editor.org/info/rfc3986/#section-3.5
    pub fragment: &'data [u8],

    backing: &'data [u8],
}

impl<'data> Url<'data> {
    /// Tries to parse a byte string into a valid URL.
    ///
    /// # Errors
    ///
    /// Returns [`MissingScheme`] if the URL contains a scheme specifier (`://`) but no scheme
    /// preceding it.
    ///
    /// Returns [`MissingHost`] if the URL contains a port and no host, or a scheme and no host.
    ///
    /// Returns [`MissingFragment`] if the URL contains a fragment specified (`#`) but no fragment
    /// data.
    ///
    /// See [`Query::new`] for a list of query-related errors.
    ///
    /// [`MissingScheme`]: Error::MissingScheme
    /// [`MissingHost`]: Error::MissingHost
    /// [`MissingFragment`]: Error::MissingFragment
    pub fn new(bytes: &'data [u8]) -> Result<Self, Error> {
        // FIXME: validate % encoding

        // == Step 1: parse and separate URL segments ==============================================

        let (fragment, before_fragment) = match memchr::memrchr(b'#', bytes) {
            Some(n) if n >= bytes.len() => return Err(Error::MissingFragment),
            Some(n) => (n + 1..bytes.len(), ..n),
            None => (bytes.len()..bytes.len(), ..bytes.len()),
        };

        #[cfg(test)]
        let _fragment = std::str::from_utf8(&bytes[fragment.clone()]).unwrap_or_default();

        let (query, before_query) = match memchr::memrchr(b'?', &bytes[before_fragment]) {
            Some(n) => (n + 1..before_fragment.end, ..n),
            None => (before_fragment.end..before_fragment.end, before_fragment),
        };

        #[cfg(test)]
        let _query = std::str::from_utf8(&bytes[query.clone()]).unwrap_or_default();

        let (scheme, hier_part) = match memchr::memmem::rfind(&bytes[before_query], b"://") {
            Some(0) => return Err(Error::MissingScheme),
            Some(n) => (0..n, n + 3..before_query.end),
            None => (0..0, 0..before_query.end),
        };

        #[cfg(test)]
        let _scheme = std::str::from_utf8(&bytes[scheme.clone()]).unwrap_or_default();

        let (path, authority) = match memchr::memchr(b'/', &bytes[hier_part.clone()]) {
            Some(n) => (
                hier_part.start + n..hier_part.end,
                hier_part.start..hier_part.start + n,
            ),
            None => (hier_part.end..hier_part.end, hier_part.clone()),
        };

        #[cfg(test)]
        let _path = std::str::from_utf8(&bytes[path.clone()]).unwrap_or_default();

        let (host, port) = match memchr::memrchr(b':', &bytes[authority.clone()]) {
            Some(0) => return Err(Error::MissingHost),
            Some(n) => (
                authority.start..authority.start + n,
                authority.start + n + 1..authority.end,
            ),
            None => (authority.clone(), authority.end..authority.end),
        };

        #[cfg(test)]
        let _host = std::str::from_utf8(&bytes[host.clone()]).unwrap_or_default();

        #[cfg(test)]
        let _port = std::str::from_utf8(&bytes[port.clone()]).unwrap_or_default();

        if !scheme.is_empty() && host.is_empty() {
            return Err(Error::MissingHost);
        }

        // == Step 2: check each segment for illegal characters ====================================

        for i in scheme.clone() {
            if !is_valid_scheme(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        for i in host.clone() {
            if !is_valid_unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        for i in port.clone() {
            if !is_valid_unreserved(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        for i in path.clone() {
            // URL paths allow the use of `/` characters as path separators
            if !is_valid_path_abempty(bytes[i]) {
                return Err(Error::ReservedCharacter(bytes[i]));
            }
        }

        Ok(Self {
            scheme: &bytes[scheme],
            host: &bytes[host],
            fragment: &bytes[fragment],

            port: if port.is_empty() { b"80" } else { &bytes[port] },
            path: if path.is_empty() { b"/" } else { &bytes[path] },

            query: Query::new(&bytes[query])?,

            backing: bytes,
        })
    }
}

fn is_valid_scheme(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.')
}

fn is_valid_unreserved(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' |  b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
}

fn is_valid_path_abempty(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' |  b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/')
}

impl PartialEq for Url<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.backing == other.backing
    }
}

impl std::fmt::Debug for Url<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let scheme = std::str::from_utf8(self.scheme).unwrap_or_default();
        let host = std::str::from_utf8(self.host).unwrap_or_default();
        let port = std::str::from_utf8(self.port).unwrap_or_default();
        let path = std::str::from_utf8(self.path).unwrap_or_default();
        let fragment = std::str::from_utf8(self.fragment).unwrap_or_default();

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

impl std::fmt::Display for Url<'_> {
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

        assert_eq!(url.fragment, b"anchor");
    }

    #[test]
    fn url_nested_path() {
        let url = Url::new(b"/path/to/resource/name").unwrap();
        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/path/to/resource/name");
        assert_eq!(url.query.iter().next(), None);
        assert_streq!(url.fragment, b"");
    }

    #[test]
    fn url_only_host() {
        let url = Url::new(b"example.com").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"example.com");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/");
        assert_eq!(url.query.iter().next(), None);
        assert_streq!(url.fragment, b"");
    }

    #[test]
    fn url_only_path() {
        let url = Url::new(b"/path").unwrap();

        assert_streq!(url.scheme, b"");
        assert_streq!(url.host, b"");
        assert_streq!(url.port, b"80");
        assert_streq!(url.path, b"/path");
        assert_eq!(url.query.iter().next(), None);
        assert_streq!(url.fragment, b"");
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
        assert_streq!(url.fragment, b"fragment");
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
        });
    }
}
