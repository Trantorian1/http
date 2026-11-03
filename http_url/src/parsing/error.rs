/// Fatal errors that occur during parsing.
///
/// See [`ValidationError`] for a list of non-fatal parsing errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parsing a URL does not fit into the given buffer.
    OOM,

    /// The input has a special scheme, but does not contain a host.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// assert_eq!(
    ///     Url::new(b"https://#fragment", &mut backing),
    ///     Err(Error::HostMissing)
    /// );
    ///
    /// assert_eq!(
    ///     Url::new(b"https://:443", &mut backing),
    ///     Err(Error::HostMissing)
    /// );
    ///
    /// assert_eq!(
    ///     Url::new(b"https://user:pass@", &mut backing),
    ///     Err(Error::HostMissing)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    HostMissing,

    /// The input’s port is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// assert_eq!(
    ///     Url::new(b"https://example.org:7z", &mut backing),
    ///     Err(Error::PortInvalid)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    PortInvalid,

    /// The input’s port is too big.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// assert_eq!(
    ///     Url::new(b"https://example.org:70000", &mut backing),
    ///     Err(Error::PortOutOfRange)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    PortOutOfRange,

    /// The input is missing a scheme, because it does not begin with an ASCII alpha, and either
    /// no base URL was provided or the base URL cannot be used as a base URL because it has an
    /// opaque path.
    ///
    /// # Example
    ///
    /// Input’s scheme is missing and no base URL is given:
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// assert_eq!(
    ///     Url::new("💩".as_bytes(), &mut backing),
    ///     Err(Error::MissingSchemeNonRelativeUrl)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Input’s scheme is missing, but the base URL has an opaque path.
    ///
    /// ```
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing_base = [0; 128];
    /// # let mut backing_url = [0; 128];
    /// let (base, errs) = Url::new(b"mailto:user@example.org", &mut backing_base)?;
    ///
    /// assert_eq!(
    ///     base.join("💩".as_bytes(), &mut backing_url),
    ///     Err(Error::MissingSchemeNonRelativeUrl)
    /// );
    /// # Ok(())
    /// # }
    /// ```
    MissingSchemeNonRelativeUrl,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::OOM => write!(f, "Parsing a URL does not fit into the given buffer"),
            Error::HostMissing => write!(f, "The input has a special scheme but no host"),
            Error::PortInvalid => write!(f, "The input’s port is invalid"),
            Error::PortOutOfRange => write!(f, "The input’s port is too big"),
            Error::MissingSchemeNonRelativeUrl => write!(f, "The input is missing a scheme"),
        }
    }
}

impl std::error::Error for Error {}

/// A set of all possible [`ValidationError`]s.
pub type ValidationErrors = ValidationErrorBitSet;

/// A validation error indicates a mismatch between input and valid input. User agents, especially
/// conformance checkers, are encouraged to report them somewhere.
#[derive(macro_derive::BitSet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationError {
    /// A code point was found that is not a [URL unit].
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.org/>", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidURLUnit));
    ///
    /// let (url, errs) = Url::new(b" https://example.org", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidURLUnit));
    ///
    /// let (url, errs) = Url::new(b"ht\ntps://example.org", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidURLUnit));
    ///
    /// let (url, errs) = Url::new(b"https://example.org/%s", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidURLUnit));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [URL unit]: https://url.spec.whatwg.org/#url-units
    InvalidURLUnit,

    /// The input [includes credentials].
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://user@example.org", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidCredentials));
    ///
    /// let (url, errs) = Url::new(b"ssh://user@example.org", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidCredentials));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [includes credentials]: https://url.spec.whatwg.org/#include-credentials
    InvalidCredentials,

    /// The input’s scheme is not followed by "//".
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"file:c:/my-secret-folder", &mut backing)?;
    /// assert!(errs.contains(ValidationError::SpecialSchemeMissingFollowingSolidus));
    ///
    /// let (url, errs) = Url::new(b"https:example.org", &mut backing)?;
    /// assert!(errs.contains(ValidationError::SpecialSchemeMissingFollowingSolidus));
    /// # Ok(())
    /// # }
    /// ```
    SpecialSchemeMissingFollowingSolidus,

    /// The URL has a special scheme and it uses U+005C (\\) instead of U+002F (/).
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"https://example.org\\path\\to\\file", &mut backing)?;
    /// assert!(errs.contains(ValidationError::InvalidReverseSolidus));
    /// # Ok(())
    /// # }
    /// ```
    InvalidReverseSolidus,

    /// The input is a relative-URL string that starts with a Windows drive letter and the base
    /// URL’s scheme is "file".
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing_base = [0; 128];
    /// # let mut backing_url = [0; 128];
    /// let (base, errs_base) = Url::new(b"file:///c:/path/to/file.txt", &mut backing_base)?;
    /// let (url, errs_url) = base.join(b"file:c:/data.txt", &mut backing_url)?;
    ///
    /// assert!(errs_url.contains(ValidationError::FileInvalidWindowsDriveLetter));
    /// # Ok(())
    /// # }
    /// ```
    FileInvalidWindowsDriveLetter,

    /// A file URL’s host is a Windows drive letter.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # use http_url::ValidationError;
    /// # fn main() -> Result<(), Error> {
    /// # let mut backing = [0; 128];
    /// let (url, errs) = Url::new(b"file://c:/path/to/file.txt", &mut backing)?;
    ///
    /// assert!(errs.contains(ValidationError::FileInvalidWindowsDriveLetterHost));
    /// # Ok(())
    /// # }
    /// ```
    FileInvalidWindowsDriveLetterHost,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::InvalidURLUnit => {
                write!(f, "A code point is found that is not a URL unit")
            },
            ValidationError::InvalidCredentials => {
                write!(f, "The input includes plain-text credentials")
            },
            ValidationError::SpecialSchemeMissingFollowingSolidus => {
                write!(f, "The input’s scheme is not followed by \"//\"")
            },
            ValidationError::InvalidReverseSolidus => write!(
                f,
                "The URL has a special scheme and it uses U+005C (\\) instead of U+002F (/)"
            ),
            ValidationError::FileInvalidWindowsDriveLetter => write!(
                f,
                "The input is a relative-URL string that starts with a Windows drive letter and \
                 the base URL’s scheme is \"file\""
            ),
            ValidationError::FileInvalidWindowsDriveLetterHost => {
                write!(f, "A file URL’s host is a Windows drive letter")
            },
        }
    }
}

impl std::error::Error for ValidationError {}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bitset_add() {
        let mut bitset = ValidationErrorBitSet::new();

        assert!(!bitset.contains(ValidationError::InvalidURLUnit));
        assert!(!bitset.contains(ValidationError::InvalidCredentials));
        assert!(!bitset.contains(ValidationError::SpecialSchemeMissingFollowingSolidus));

        assert_eq!(bitset.len(), 0);
        assert!(bitset.is_empty());

        bitset.add(ValidationError::InvalidURLUnit);

        assert!(bitset.contains(ValidationError::InvalidURLUnit));
        assert!(!bitset.contains(ValidationError::InvalidCredentials));
        assert!(!bitset.contains(ValidationError::SpecialSchemeMissingFollowingSolidus));

        assert_eq!(bitset.len(), 1);
        assert!(!bitset.is_empty());
    }

    #[test]
    fn bitset_iter() {
        let mut bitset = ValidationErrorBitSet::new();

        bitset.add(ValidationError::SpecialSchemeMissingFollowingSolidus);
        bitset.add(ValidationError::InvalidURLUnit);

        let mut iter = bitset.iter();

        assert_eq!(iter.next(), Some(ValidationError::InvalidURLUnit));
        assert_eq!(
            iter.next(),
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );

        assert_eq!(iter.next(), None);
    }
}
