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

/// **U**niform **R**esource **L**ocators.
///
/// See [RFC3986] for more information.
///
/// [RFC3986]: https://www.rfc-editor.org/info/rfc3986
pub struct Url<'data> {
    backing: &'data [u8],

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
    /// assert_eq!(url.scheme, b"http");
    /// ```
    ///
    /// ---
    pub scheme: &'data [u8],

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
    /// assert_eq!(url.username, b"user");
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a username will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// ---
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
    pub username: &'data [u8],

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
    /// assert_eq!(url.username, b"user");
    /// assert_eq!(url.password, b"123");
    /// ```
    ///
    /// # Warning
    ///
    /// Parsing a URL with a password will log an [`InvalidCredentials`] [`ValidationError`] but
    /// will not fail parsing outright.
    ///
    /// ---
    ///
    /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
    pub password: &'data [u8],

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
    /// assert_eq!(url.host, b"example.com");
    /// ```
    ///
    /// ---
    ///
    /// [domain]: https://url.spec.whatwg.org/#concept-domain
    /// [IP address]: https://url.spec.whatwg.org/#ip-address
    /// [opaque host]: https://url.spec.whatwg.org/#opaque-host
    /// [empty host]: https://url.spec.whatwg.org/#empty-host
    pub host: &'data [u8],

    /// A URL’s port is either null or a [`u16`] integer that identifies a networking port.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com:30", &mut backing).unwrap();
    ///
    /// assert_eq!(url.port, Some(30));
    /// ```
    ///
    /// # Warning
    ///
    /// Port will be set to [`Null`] if it matches the [default port] for that [`scheme`].
    ///
    /// ---
    ///
    /// [default port]: https://url.spec.whatwg.org/#default-port
    /// [`scheme`]: Self::scheme
    pub port: Option<u16>,

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
    /// assert_eq!(url.path, b"/my/file");
    /// ```
    ///
    /// ---
    ///
    /// [URL path segment]: https://url.spec.whatwg.org/#url-path-segment
    pub path: &'data [u8],

    /// A URL’s query is either null or an ASCII string.
    ///
    /// # Example
    ///
    /// ```
    /// # use http_url::Url;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(b"https://example.com?query", &mut backing).unwrap();
    ///
    /// assert_eq!(url.query, b"query");
    /// ```
    ///
    /// # Warning
    ///
    /// URL query parsing does **not** enforce [application/x-www-form-urlencoded] conformity.
    ///
    /// ---
    ///
    /// [application/x-www-form-urlencoded]: https://url.spec.whatwg.org/#application/x-www-form-urlencoded
    pub query: &'data [u8],

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
    /// assert_eq!(url.fragment, b"about");
    /// ```
    pub fragment: &'data [u8],
}

impl std::fmt::Debug for Url<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backing = str::from_utf8(self.backing).unwrap_or_default();
        let scheme = str::from_utf8(self.scheme).unwrap_or_default();
        let username = str::from_utf8(self.username).unwrap_or_default();
        let password = str::from_utf8(self.password).unwrap_or_default();
        let host = str::from_utf8(self.host).unwrap_or_default();
        let path = str::from_utf8(self.path).unwrap_or_default();
        let query = str::from_utf8(self.query).unwrap_or_default();
        let fragment = str::from_utf8(self.fragment).unwrap_or_default();

        f.debug_struct("Url")
            .field("backing", &backing)
            .field("scheme", &scheme)
            .field("username", &username)
            .field("password", &password)
            .field("host", &host)
            .field("port", &self.port)
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
