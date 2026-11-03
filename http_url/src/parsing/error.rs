/// Fatal errors that occur during parsing.
///
/// See [`ValidationError`] for a list of non-fatal parsing errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parsing a URL does not fit into the given buffer.
    OOM,

    /// The input has a special scheme, but does not contain a host.
    ///
    /// ```text
    /// "https://#fragment"
    ///
    /// "https://:443"
    ///
    /// "https://user:pass@"
    /// ```
    HostMissing,

    /// The input’s port is invalid.
    ///
    /// # Example
    ///
    /// ```text
    /// "https://example.org:7z"
    /// ```
    PortInvalid,

    /// The input’s port is too big.
    ///
    /// # Example
    ///
    /// ```text
    /// "https://example.org:70000"
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
    /// ```
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// let mut backing = [0; 128];
    ///
    /// assert_eq!(
    ///     Url::new("💩".as_bytes(), &mut backing),
    ///     Err(Error::MissingSchemeNonRelativeUrl)
    /// );
    /// ```
    ///
    /// Input’s scheme is missing, but the base URL has an opaque path.
    ///
    /// ```
    /// # use http_url::Url;
    /// # use http_url::Error;
    /// # fn example() -> Result<(), Error> {
    /// let mut backing_base = [0; 128];
    /// let mut backing_url = [0; 128];
    ///
    /// let (base, validation_errors) =
    ///     Url::new("mailto:user@example.org".as_bytes(), &mut backing_base)?;
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

/// A validation error indicates a mismatch between input and valid input. User agents, especially
/// conformance checkers, are encouraged to report them somewhere.
#[derive(macro_derive::BitSet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ValidationError {
    /// A code point is found that is not a [URL unit].
    ///
    /// # Example
    ///
    /// ```text
    /// "https://example.org/>"
    ///
    /// " https://example.org "
    ///
    /// "ht
    /// tps://example.org"
    ///
    /// "https://example.org/%s"
    /// ```
    ///
    /// [URL unit]: https://url.spec.whatwg.org/#url-units
    InvalidURLUnit,

    /// The input [includes credentials].
    ///
    /// # Example
    ///
    /// ```text
    /// "https://user@example.org"
    ///  
    /// "ssh://user@example.org"
    /// ```
    ///
    /// [includes credentials]: https://url.spec.whatwg.org/#include-credentials
    InvalidCredentials,

    /// The input’s scheme is not followed by "//".
    ///
    /// # Example
    ///
    /// ```text
    /// "file:c:/my-secret-folder"
    ///
    /// "https:example.org"
    /// ```
    SpecialSchemeMissingFollowingSolidus,

    /// The URL has a special scheme and it uses U+005C (\) instead of U+002F (/).
    ///
    /// # Example
    ///
    /// ```text
    /// "https://example.org\path\to\file"
    /// ```
    InvalidReverseSolidus,

    /// The input is a relative-URL string that starts with a Windows drive letter and the base
    /// URL’s scheme is "file".
    FileInvalidWindowsDriveLetter,

    /// A file URL’s host is a Windows drive letter.
    FileInvalidWindowsDriveLetterHost,
}

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
