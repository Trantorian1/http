//! [Url] parsing utilities.

mod old;
pub mod parsing;
pub mod percent;
mod query;

pub use old::*;
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

    pub use super::Query;
    pub use super::QueryParameter;
    pub use super::UrlOld;
}

pub(crate) mod segment {
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
}

pub(crate) mod macros {
    pub(crate) use crate::ascii_tab_or_newline;
    pub(crate) use crate::forward;
    pub(crate) use crate::shrink;

    #[macro_export]
    macro_rules! ascii_tab_or_newline {
        () => {
            b'\t' | b'\n' | b'\r'
        };
    }

    #[macro_export]
    macro_rules! forward {
        ($cursor:expr, by $n:expr) => {
            $cursor = &$cursor[$n..];
        };
    }

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
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"http://example.com", &mut backing).unwrap();
    ///
    /// assert_eq!(url.scheme(), b"http");
    /// ```
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
    /// ```
    /// # use http_url::Url;
    /// # use http_url::ValidationError;
    /// let mut backing = [0; 128];
    /// let (url, mut validation_errors) = Url::new(b"http://user@abc.com", &mut backing).unwrap();
    ///
    /// assert_eq!(
    ///     validation_errors.next(),
    ///     Some(ValidationError::InvalidCredentials)
    /// );
    ///
    /// assert_eq!(url.username(), b"user");
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a username will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
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
    /// ```
    /// # use http_url::Url;
    /// # use http_url::ValidationError;
    /// let mut backing = [0; 128];
    /// let (url, mut validation_errors) = Url::new(b"http://user:123@abc.com", &mut backing).unwrap();
    ///
    /// assert_eq!(
    ///     validation_errors.next(),
    ///     Some(ValidationError::InvalidCredentials)
    /// );
    ///
    /// assert_eq!(url.username(), b"user");
    /// assert_eq!(url.password(), b"123");
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a password will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
    pub fn password(&self) -> &'data [u8] {
        &self.backing[self.password.0.clone()]
    }

    /// A host is a [domain], an [IP address], an [opaque host], or an [empty host]. Typically a
    /// host serves as a network address, but it is sometimes used as opaque identifier in URLs
    /// where a network address is not necessary.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com", &mut backing).unwrap();
    ///
    /// assert_eq!(url.host(), b"example.com");
    /// ```
    ///
    /// [domain]: https://url.spec.whatwg.org/#concept-domain
    /// [IP address]: https://url.spec.whatwg.org/#ip-address
    /// [opaque host]: https://url.spec.whatwg.org/#opaque-host
    /// [empty host]: https://url.spec.whatwg.org/#empty-host
    pub fn host(&self) -> &'data [u8] {
        &self.backing[self.host.0.clone()]
    }

    /// A URL’s port is either null or a [`u16`] integer that identifies a networking port.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com:30", &mut backing).unwrap();
    ///
    /// assert_eq!(url.port(), Some(30));
    /// ```
    ///
    /// # Warning
    ///
    /// Port will be set to [`Null`] if it matches the [default port] for that [`scheme`].
    ///
    /// [default port]: https://url.spec.whatwg.org/#default-port
    /// [`scheme`]: Self::scheme
    pub fn port(&self) -> Option<u16> {
        self.port.0
    }

    /// A URL path is either a [URL path segment] or a list of zero or more URL path segments. In
    /// the latter case the URL path segments never contain U+002F (/).
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com/my/file", &mut backing).unwrap();
    ///
    /// assert_eq!(url.path(), b"/my/file");
    /// ```
    ///
    /// [URL path segment]: https://url.spec.whatwg.org/#url-path-segment
    pub fn path(&self) -> &'data [u8] {
        &self.backing[self.path.0.clone()]
    }

    /// A URL’s query is either null or an ASCII string.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com?query", &mut backing).unwrap();
    ///
    /// assert_eq!(url.query(), b"query");
    /// ```
    ///
    /// # Warning
    ///
    /// URL query parsing does **not** enforce [application/x-www-form-urlencoded] conformity.
    ///
    /// [application/x-www-form-urlencoded]: https://url.spec.whatwg.org/#application/x-www-form-urlencoded
    pub fn query(&self) -> &'data [u8] {
        &self.backing[self.query.0.clone()]
    }

    /// A URL’s fragment is either null or an ASCII string that can be used for further processing
    /// on the resource the URL’s other components identify.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// # use http_url::ValidationError;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"http://example.com#about", &mut backing).unwrap();
    ///
    /// assert_eq!(url.fragment(), b"about");
    /// ```
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
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"scheme:opaque-path", &mut backing).unwrap();
    ///
    /// assert!(url.is_opaque());
    /// assert_eq!(url.path(), b"opaque-path");
    /// ```
    ///
    /// [special scheme]: https://url.spec.whatwg.org/#special-scheme
    pub fn is_opaque(&self) -> bool {
        !matches!(self.backing.get(self.scheme.0.end + 1), Some(b'/'))
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
