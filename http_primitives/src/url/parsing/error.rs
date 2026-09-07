#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// A validation error indicates a mismatch between input and valid input. User agents, especially
/// conformance checkers, are encouraged to report them somewhere.
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
