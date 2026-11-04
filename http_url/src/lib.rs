//! [Url] parsing utilities.

pub mod parsing;
pub mod percent;

pub use parsing::*;

pub mod prelude {
    //! A “prelude” for crates using the `http_url` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use http_url::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use super::Error;
    pub use super::Url;
}

pub(crate) mod segment {
    use super::*;

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Scheme(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Username(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Password(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Host(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Port(pub Option<u16>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Path(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Query(pub std::ops::Range<usize>);

    #[repr(transparent)]
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub(super) struct Fragment(pub std::ops::Range<usize>);

    impl Username {
        pub(super) fn empty(buffer: &buffer::UrlBuffer<'_>) -> Self {
            let n = buffer.len();
            Self(n..n)
        }
    }

    impl Password {
        pub(super) fn empty(buffer: &buffer::UrlBuffer<'_>) -> Self {
            let n = buffer.len();
            Self(n..n)
        }
    }

    impl Host {
        pub(super) fn empty(buffer: &buffer::UrlBuffer<'_>) -> Self {
            let n = buffer.len();
            Self(n..n)
        }
    }

    impl Port {
        pub(super) fn empty() -> Self {
            Self::default()
        }
    }

    impl Path {
        pub(super) fn empty(buffer: &mut buffer::UrlBuffer<'_>) -> Result<Self, Error> {
            let path_start = buffer.len();

            // Path is never empty;
            buffer.push_byte(b'/')?;

            let path_stop = buffer.len();

            Ok(Self(path_start..path_stop))
        }
    }

    impl Query {
        pub(super) fn empty(buffer: &buffer::UrlBuffer<'_>) -> Self {
            let n = buffer.len();
            Self(n..n)
        }
    }

    impl Fragment {
        pub(super) fn empty(buffer: &buffer::UrlBuffer<'_>) -> Self {
            let n = buffer.len();
            Self(n..n)
        }
    }
}

pub(crate) mod macros {
    pub(crate) use crate::ascii_alpha;
    pub(crate) use crate::ascii_hexdigit;
    pub(crate) use crate::ascii_tab_or_newline;
    pub(crate) use crate::forward;
    pub(crate) use crate::shrink;

    #[doc(hidden)]
    #[macro_export]
    macro_rules! ascii_alpha {
        () => {
            b'A'..=b'Z' | b'a'..=b'z'
        };
    }

    #[doc(hidden)]
    #[macro_export]
    macro_rules! ascii_tab_or_newline {
        () => {
            b'\t' | b'\n' | b'\r'
        };
    }

    #[doc(hidden)]
    #[macro_export]
    macro_rules! ascii_hexdigit {
        () => {
            b'0'..=b'9' | b'A'..=b'F' | b'a'..=b'f'
        };
    }

    #[doc(hidden)]
    #[macro_export]
    macro_rules! forward {
        ($cursor:expr, by $n:expr) => {
            $cursor = &$cursor[$n..];
        };
    }

    #[doc(hidden)]
    #[macro_export]
    macro_rules! shrink {
        ($cursor:expr, by $n:expr) => {{
            let len = $cursor.len();
            $cursor = &$cursor[..len - $n];
        }};
    }
}

/// **U**niform **R**esource **L**ocators.
///
/// See [RFC3986] for more information.
///
/// [RFC3986]: https://www.rfc-editor.org/info/rfc3986
pub struct Url<'data> {
    backing: &'data [u8],

    scheme: segment::Scheme,
    username: segment::Username,
    password: segment::Password,
    host: segment::Host,
    port: segment::Port,
    path: segment::Path,
    query: segment::Query,
    fragment: segment::Fragment,
}

impl<'data> Url<'data> {
    /// A URL’s scheme is an ASCII string that identifies the type of URL and can be used to
    /// dispatch a URL for further processing after parsing.
    ///
    /// # Example
    ///
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"http://example.com", &mut backing)?;
    ///
    /// assert_eq!(url.scheme(), b"http");
    /// assert_eq!(url.host(), b"example.com");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn scheme(&self) -> &'data [u8] {
        &self.backing[self.scheme.0.clone()]
    }

    /// A URL’s username is an ASCII string identifying a username.
    ///
    /// This is present for legacy reason and compatibility with old system which still rely on
    /// the userinfo segment of a URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"http://user@abc.com", &mut backing)?;
    ///
    /// assert!(errs.contains(ValidationError::InvalidCredentials));
    /// assert_eq!(url.username(), b"user");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a username will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
    #[must_use]
    pub fn username(&self) -> &'data [u8] {
        &self.backing[self.username.0.clone()]
    }

    /// A URL’s password is an ASCII string identifying a password.
    ///
    /// This is present for legacy reason and compatibility with old system which still rely on
    /// the userinfo segment of a URL.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"http://user:123@abc.com", &mut backing)?;
    ///
    /// assert!(errs.contains(ValidationError::InvalidCredentials));
    ///
    /// assert_eq!(url.username(), b"user");
    /// assert_eq!(url.password(), b"123");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a password will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
    #[must_use]
    pub fn password(&self) -> &'data [u8] {
        &self.backing[self.password.0.clone()]
    }

    /// A host is a [domain], an [IP address], an [opaque host], or an [empty host]. Typically a
    /// host serves as a network address, but it is sometimes used as opaque identifier in URLs
    /// where a network address is not necessary.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.com", &mut backing)?;
    ///
    /// assert_eq!(url.host(), b"example.com");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [domain]: https://url.spec.whatwg.org/#concept-domain
    /// [IP address]: https://url.spec.whatwg.org/#ip-address
    /// [opaque host]: https://url.spec.whatwg.org/#opaque-host
    /// [empty host]: https://url.spec.whatwg.org/#empty-host
    #[must_use]
    pub fn host(&self) -> &'data [u8] {
        &self.backing[self.host.0.clone()]
    }

    /// A URL’s port is either null or a [`u16`] integer that identifies a networking port.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.com:30", &mut backing)?;
    ///
    /// assert_eq!(url.port(), Some(30));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Warning
    ///
    /// Port will be set to [`None`] if it matches the [default port] for that [`scheme`].
    ///
    /// [default port]: https://url.spec.whatwg.org/#default-port
    /// [`scheme`]: Self::scheme
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.port.0
    }

    /// A URL path is either a [URL path segment] or a list of zero or more URL path segments. In
    /// the latter case the URL path segments never contain U+002F (/).
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.com/my/file", &mut backing)?;
    ///
    /// assert_eq!(url.path(), b"/my/file");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [URL path segment]: https://url.spec.whatwg.org/#url-path-segment
    #[must_use]
    pub fn path(&self) -> &'data [u8] {
        &self.backing[self.path.0.clone()]
    }

    /// A URL’s query is either null or an ASCII string.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.com?query", &mut backing)?;
    ///
    /// assert_eq!(url.query(), b"query");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Warning
    ///
    /// URL query parsing does **not** enforce [application/x-www-form-urlencoded] conformity.
    ///
    /// [application/x-www-form-urlencoded]: https://url.spec.whatwg.org/#application/x-www-form-urlencoded
    #[must_use]
    pub fn query(&self) -> &'data [u8] {
        &self.backing[self.query.0.clone()]
    }

    /// A URL’s fragment is either null or an ASCII string that can be used for further processing
    /// on the resource the URL’s other components identify.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"http://example.com#about", &mut backing)?;
    ///
    /// assert_eq!(url.fragment(), b"about");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn fragment(&self) -> &'data [u8] {
        &self.backing[self.fragment.0.clone()]
    }

    /// Checks if the URL has an opaque path.
    ///
    /// A url has an opaque path if it does not have a [special scheme] and if the code points after
    /// its scheme do not start with U+002F (/).
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"scheme:opaque-path", &mut backing)?;
    ///
    /// assert!(url.is_opaque());
    /// assert_eq!(url.path(), b"opaque-path");
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [special scheme]: https://url.spec.whatwg.org/#special-scheme
    #[must_use]
    pub fn is_opaque(&self) -> bool {
        !matches!(self.backing.get(self.scheme.0.end + 1), Some(b'/'))
    }
}

impl std::cmp::PartialEq for Url<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.backing == other.backing
    }
}

impl std::cmp::Eq for Url<'_> {}

impl std::cmp::PartialOrd for Url<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::cmp::Ord for Url<'_> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.backing.cmp(other.backing)
    }
}

impl std::fmt::Debug for Url<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backing = str::from_utf8(self.backing).unwrap_or_default();
        let scheme = str::from_utf8(self.scheme()).unwrap_or_default();
        let username = str::from_utf8(self.username()).unwrap_or_default();
        let password = str::from_utf8(self.password()).unwrap_or_default();
        let host = str::from_utf8(self.host()).unwrap_or_default();
        let path = str::from_utf8(self.path()).unwrap_or_default();
        let query = str::from_utf8(self.query()).unwrap_or_default();
        let fragment = str::from_utf8(self.fragment()).unwrap_or_default();

        f.debug_struct("Url")
            .field("backing", &backing)
            .field("scheme", &scheme)
            .field("username", &username)
            .field("password", &password)
            .field("host", &host)
            .field("port", &self.port())
            .field("path", &path)
            .field("query", &query)
            .field("fragment", &fragment)
            .finish()
    }
}

impl std::fmt::Display for Url<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(str::from_utf8(self.backing).unwrap_or_default())
    }
}
