pub use super::query::*;

/// [`Url`] parsing errors.
///
/// [`Url`]: crate::Url
#[derive(Debug, PartialEq, Eq)]
pub enum OldError {
    /// [`URL`] with a scheme specifier (`://`) but not scheme preceding it.
    ///
    /// # Example
    ///
    /// ```text
    /// ://example.com
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingScheme,

    /// [`URL`] with a port but no host, or a scheme but no host.
    ///
    /// # Example
    ///
    /// ```text
    /// http:://
    ///
    /// or
    ///
    /// http:///
    ///
    /// or
    ///
    /// http:///
    ///
    /// or
    ///
    /// :80
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingHost,

    /// [`Url`] with a fragment specifier (`#`) but no fragment after it.
    ///
    /// # Example
    ///
    /// ```text
    /// example.com#
    /// ```
    ///
    /// [`URL`]: super::Url
    MissingFragment,

    /// [`URL`] containing a character which is not part of the allowed set for a given fragment.
    ///
    /// # Example
    ///
    /// ```text
    /// example!com
    /// ```
    ///
    /// [`URL`]: super::Url
    ReservedCharacter(u8),

    /// [`Query`] containing an empty parameter.
    ///
    /// # Example
    ///
    /// ```text
    /// ?&b=1
    /// ```
    ///
    /// [`Query`]: super::Query
    EmptyQueryParameter,

    /// [`Query`] parameter which does not adhere to `x-www-form-urlencoded`
    ///
    /// # Example
    ///
    /// ```text
    /// ?abc
    /// ```
    ///
    /// [`Query`]: super::Query
    InvalidQuery,

    /// [`Query`] parameter without a key.
    ///
    /// # Example
    ///
    /// ```text
    /// ?=value
    /// ```
    ///
    /// [`Query`]: super::Query
    MissingQueryKey,

    /// [`Query`] parameter without a value.
    ///
    /// # Example
    ///
    /// ```text
    /// ?key=
    /// ```
    ///
    /// [`Query`]: super::Query
    MissingQueryValue,
}

impl std::fmt::Display for OldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OldError::MissingScheme => f.write_str("Empty scheme before scheme delimiter"),
            OldError::MissingHost => f.write_str("Empty host in URI with non-empty port or scheme"),
            OldError::MissingFragment => f.write_str("Empty fragment after fragment delimiter"),
            OldError::ReservedCharacter(c) => write!(f, "Use of reserved character {c}"),
            OldError::EmptyQueryParameter => f.write_str("Empty query parameter"),
            OldError::InvalidQuery => {
                f.write_str("Query parameter does not adhere to key-value form")
            },
            OldError::MissingQueryKey => f.write_str("Query parameter without a key"),
            OldError::MissingQueryValue => f.write_str("Query parameter without a value"),
        }
    }
}

impl std::error::Error for OldError {}

/// Zero-copy **U**niform **R**esource **L**ocators.
///
/// See [RFC3986] for more information.
///
/// [RFC3986]: https://www.rfc-editor.org/info/rfc3986
pub struct UrlOld<'data> {
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

impl<'data> UrlOld<'data> {
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
    pub fn new(bytes: &'data [u8]) -> Result<Self, OldError> {
        // FIXME: validate % encoding

        // == Step 1: parse and separate URL segments ==============================================

        let (fragment, before_fragment) = match memchr::memrchr(b'#', bytes) {
            Some(n) if n >= bytes.len() => return Err(OldError::MissingFragment),
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
            Some(0) => return Err(OldError::MissingScheme),
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
            Some(0) => return Err(OldError::MissingHost),
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
            return Err(OldError::MissingHost);
        }

        // == Step 2: check each segment for illegal characters ====================================

        for i in scheme.clone() {
            if !is_valid_scheme(bytes[i]) {
                return Err(OldError::ReservedCharacter(bytes[i]));
            }
        }

        for i in host.clone() {
            if !is_valid_unreserved(bytes[i]) {
                return Err(OldError::ReservedCharacter(bytes[i]));
            }
        }

        for i in port.clone() {
            if !is_valid_unreserved(bytes[i]) {
                return Err(OldError::ReservedCharacter(bytes[i]));
            }
        }

        for i in path.clone() {
            // URL paths allow the use of `/` characters as path separators
            if !is_valid_path_abempty(bytes[i]) {
                return Err(OldError::ReservedCharacter(bytes[i]));
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

pub fn is_valid_scheme(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.')
}

pub fn is_valid_unreserved(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' |  b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~')
}

pub fn is_valid_path_abempty(c: u8) -> bool {
    matches!(c, b'A'..=b'Z' |  b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/')
}

impl PartialEq for UrlOld<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.backing == other.backing
    }
}

impl std::fmt::Debug for UrlOld<'_> {
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

impl std::fmt::Display for UrlOld<'_> {
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
        let url = UrlOld::new(b"http://example.com?hello=world#anchor").unwrap();
        assert_str_eq!(url.scheme, b"http");
        assert_str_eq!(url.host, b"example.com");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/");

        let mut parameters = url.query.iter();
        let a = parameters.next().unwrap();

        assert_str_eq!(a.key, b"hello");
        assert_str_eq!(a.val, b"world");
        assert_eq!(parameters.next(), None);

        assert_eq!(url.fragment, b"anchor");
    }

    #[test]
    fn url_nested_path() {
        let url = UrlOld::new(b"/path/to/resource/name").unwrap();
        assert_str_eq!(url.scheme, b"");
        assert_str_eq!(url.host, b"");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/path/to/resource/name");
        assert_eq!(url.query.iter().next(), None);
        assert_str_eq!(url.fragment, b"");
    }

    #[test]
    fn url_only_host() {
        let url = UrlOld::new(b"example.com").unwrap();

        assert_str_eq!(url.scheme, b"");
        assert_str_eq!(url.host, b"example.com");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/");
        assert_eq!(url.query.iter().next(), None);
        assert_str_eq!(url.fragment, b"");
    }

    #[test]
    fn url_only_path() {
        let url = UrlOld::new(b"/path").unwrap();

        assert_str_eq!(url.scheme, b"");
        assert_str_eq!(url.host, b"");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/path");
        assert_eq!(url.query.iter().next(), None);
        assert_str_eq!(url.fragment, b"");
    }

    #[test]
    fn url_only_query() {
        let url = UrlOld::new(b"?a=1").unwrap();

        assert_str_eq!(url.scheme, b"");
        assert_str_eq!(url.host, b"");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/");

        let mut parameters = url.query.iter();
        let a = parameters.next().unwrap();

        assert_str_eq!(a.key, b"a");
        assert_str_eq!(a.val, b"1");
        assert_eq!(parameters.next(), None);

        assert_str_eq!(url.fragment, b"");
    }

    #[test]
    fn url_only_fragment() {
        let url = UrlOld::new(b"#fragment").unwrap();

        assert_str_eq!(url.scheme, b"");
        assert_str_eq!(url.host, b"");
        assert_str_eq!(url.port, b"80");
        assert_str_eq!(url.path, b"/");
        assert_eq!(url.query.iter().next(), None);
        assert_str_eq!(url.fragment, b"fragment");
    }

    #[test]
    fn url_invalid_only_scheme_and_port() {
        assert_eq!(UrlOld::new(b"http://:80"), Err(OldError::MissingHost));
    }

    #[test]
    fn url_invalid_only_scheme_and_path() {
        assert_eq!(UrlOld::new(b"http:///"), Err(OldError::MissingHost));
    }

    #[test]
    fn url_invalid_only_scheme() {
        assert_eq!(UrlOld::new(b"http://"), Err(OldError::MissingHost));
    }

    #[test]
    fn url_invalid_only_port() {
        assert_eq!(UrlOld::new(b":80"), Err(OldError::MissingHost));
    }
}

#[cfg(test)]
mod fuzz {
    use super::UrlOld;

    #[test]
    fn fuzz_url() {
        bolero::check!().for_each(|bytes| {
            let _ = UrlOld::new(bytes);
        });
    }
}
