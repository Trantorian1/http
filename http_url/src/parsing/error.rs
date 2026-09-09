/// Fatal errors that occur during parsing.
///
/// See [`ValidationError`] for a list of non-fatal parsing errors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Parsing a URL does not fit into the given buffer.
    Overflow,

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
