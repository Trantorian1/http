//! [`Url`] parsing utilitiessupport relative paths or path shortening.

pub(crate) mod buffer;
mod error;
mod path;

use buffer::UrlBuffer;
pub use error::*;
use macro_util::prelude::*;

use super::Url;
use super::percent;
use super::segment;
use crate::macros::*;

impl<'data> Url<'data> {
    /// Tries to parse a stream of raw network bytes into a URL.
    ///
    /// # Panics
    ///
    /// If the backing array passed to this method is empty.
    ///
    /// # Errors
    ///
    /// Returns a hard [`Error`] in case of failed parsing. Other errors which attest to a malformed
    /// input but are recoverable are reported as [`ValidationError`]s.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_url::Url;
    /// # use macro_util::*;
    /// let mut backing = [0; 128];
    /// let (url, validation_errors) = Url::new(
    ///     b"http://example.com:123/path/to/file?query#fragment",
    ///     &mut backing,
    /// )
    /// .unwrap();
    ///
    /// assert_eq!(validation_errors.len(), 0);
    ///
    /// assert_utf8_eq!(url.scheme(), b"http");
    /// assert_utf8_eq!(url.host(), b"example.com");
    /// assert_eq!(url.port(), Some(123));
    /// assert_utf8_eq!(url.path(), b"/path/to/file");
    /// assert_utf8_eq!(url.query(), b"query");
    /// assert_utf8_eq!(url.fragment(), b"fragment");
    /// ```
    pub fn new(
        bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, ValidationErrorIter), Error> {
        Parser::new(bytes).parse(&Url::emtpy(), UrlBuffer::new(backing))
    }

    pub fn join(
        &self,
        bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, ValidationErrorIter), Error> {
        Parser::new(bytes).parse(self, UrlBuffer::new(backing))
    }

    fn emtpy() -> Self {
        Self {
            backing: Default::default(),
            scheme: Default::default(),
            username: Default::default(),
            password: Default::default(),
            host: Default::default(),
            port: Default::default(),
            path: Default::default(),
            query: Default::default(),
            fragment: Default::default(),
        }
    }
}

mod matchers {
    /// See the [URL standard], C0 control or space.
    ///
    /// > _"A C0 control or space is a [C0 control] or U+0020 SPACE."_
    ///
    /// [URL standard]: https://infra.spec.whatwg.org/#c0-control-or-space
    /// [C0 control]: https://infra.spec.whatwg.org/#c0-control
    #[inline]
    pub(super) fn c0_control_or_space(c: u8) -> bool {
        c <= b' ' // U+0000 to U+0020
    }

    /// See the [URL standard], ASCII alpha.
    ///
    /// > _"An ASCII alpha is an [ASCII upper alpha] or [ASCII lower alpha]."_
    ///
    /// [URL standard]: https://infra.spec.whatwg.org/#ascii-alpha
    /// [ASCII upper alpha]: https://infra.spec.whatwg.org/#ascii-upper-alpha
    /// [ASCII lower alpha]: https://infra.spec.whatwg.org/#ascii-lower-alpha
    #[inline]
    pub(super) fn ascii_alpha(c: u8) -> bool {
        c.is_ascii_alphabetic()
    }
}

struct Parser<'input> {
    cursor: &'input [u8],
    error_bitset: ValidationErrorBitSet,
}

impl<'input> Parser<'input> {
    #[inline]
    fn new(bytes: &'input [u8]) -> Self {
        Self {
            cursor: bytes,
            error_bitset: ValidationErrorBitSet::new(),
        }
    }

    #[inline]
    fn parse<'base, 'output>(
        mut self,
        base: &Url<'base>,
        buffer: UrlBuffer<'output>,
    ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
        self.state_c0_control_or_space();
        self.state_scheme(base, buffer)
    }
}

/// Implementation of the URL standard [URL parsing] state machine
///
/// [URL parsing]: https://url.spec.whatwg.org/#url-parsing
mod state_machine {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    impl<'output> Parser<'_> {
        /// Input sanitization
        ///
        /// Removes leading and trailing C0 control or space code points.
        #[inline]
        pub(super) fn state_c0_control_or_space(&mut self) {
            // Leading C0 control or space
            if let Some(c0_first) = self.cursor.first()
                && matchers::c0_control_or_space(*c0_first)
            {
                self.log_error(ValidationError::InvalidURLUnit);

                forward!(self.cursor, by 1);

                while let Some(c0_continuation) = self.cursor.first()
                    && matchers::c0_control_or_space(*c0_continuation)
                {
                    forward!(self.cursor, by 1);
                }
            }

            // Trailing C0 control or space
            if let Some(c0_last) = self.cursor.last()
                && matchers::c0_control_or_space(*c0_last)
            {
                self.log_error(ValidationError::InvalidURLUnit);

                shrink!(self.cursor, by 1);

                while let Some(c0_continuation) = self.cursor.last()
                    && matchers::c0_control_or_space(*c0_continuation)
                {
                    shrink!(self.cursor, by 1);
                }
            }
        }

        /// # [Scheme state]
        ///
        /// Tries to parse a [`Url`]'s scheme, if there is any, otherwise falls back to the "no scheme"
        /// state. This is the entry point for parsing further segments.
        ///
        /// [Scheme state]: https://url.spec.whatwg.org#scheme-start-state
        #[inline]
        pub(super) fn state_scheme(
            mut self,
            base: &Url<'_>,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            let checkpoint = self.cursor;

            if let Some(first) = self.cursor.first()
                && matchers::ascii_alpha(*first)
            {
                buffer.push_byte(first.to_ascii_lowercase())?;
                forward!(self.cursor, by 1);

                while !self.cursor.is_empty() {
                    let c = self.cursor[0];

                    match c {
                        ascii_tab_or_newline!() => {
                            self.skip_ascii_tab_or_newline();
                        },

                        // Only the first character in a scheme must be strictly `ascii_alpha`. Scheme
                        // characters after that may be ASCII alphanumeric, U+002B (+), U+002D (-), or
                        // U+002E (.).
                        b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => {
                            buffer.push_byte(c)?;
                            forward!(self.cursor, by 1);
                        },

                        // Input is normalized, only lowercase characters are pushed to the final buffer
                        b'A'..=b'Z' => {
                            buffer.push_byte(c.to_ascii_lowercase())?;
                            forward!(self.cursor, by 1);
                        },

                        // End of scheme
                        b':' => {
                            buffer.push_byte(b':')?;
                            forward!(self.cursor, by 1);
                            break;
                        },

                        // Invalid character, scheme error.
                        _ => {
                            forward!(self.cursor, by 1);
                            break;
                        },
                    }
                }

                // This is safe to index as we have already pushed at least one character to `buffer`.
                if buffer.as_ref()[buffer.len() - 1] == b':' {
                    let scheme = segment::Scheme(0..buffer.len() - 1);

                    #[cfg(test)]
                    let _scheme = utf8::from_bytes(&buffer[scheme.0.clone()]);

                    #[cfg(test)]
                    let _remaining = utf8::from_bytes(self.cursor);

                    // We specify each arm explicitly instead of using pattern capture to make it
                    // easier for the compiler to inline each scheme comparison on the base URL.
                    match &buffer[scheme.0.clone()] {
                        b"file" => todo!("file state: https://url.spec.whatwg.org/#file-state"),

                        b"ftp" => {
                            return if base.scheme() == b"ftp" {
                                self.state_special_relative_or_authority(base, scheme, 21, buffer)
                            } else {
                                self.state_special_authority_slashes(scheme, 21, buffer)
                            };
                        },

                        b"http" => {
                            return if base.scheme() == b"http" {
                                self.state_special_relative_or_authority(base, scheme, 80, buffer)
                            } else {
                                self.state_special_authority_slashes(scheme, 80, buffer)
                            };
                        },

                        b"ws" => {
                            return if base.scheme() == b"ws" {
                                self.state_special_relative_or_authority(base, scheme, 80, buffer)
                            } else {
                                self.state_special_authority_slashes(scheme, 80, buffer)
                            };
                        },

                        b"https" => {
                            return if base.scheme() == b"https" {
                                self.state_special_relative_or_authority(base, scheme, 443, buffer)
                            } else {
                                self.state_special_authority_slashes(scheme, 443, buffer)
                            };
                        },

                        b"wss" => {
                            return if base.scheme() == b"wss" {
                                self.state_special_relative_or_authority(base, scheme, 443, buffer)
                            } else {
                                self.state_special_authority_slashes(scheme, 443, buffer)
                            };
                        },

                        // non-special scheme
                        _ => {
                            return self.state_nonspecial(scheme, buffer);
                        },
                    }
                }
            }

            buffer.clear();
            self.cursor = checkpoint;

            self.state_scheme_none(base, buffer)
        }

        /// # [No scheme state]
        ///
        /// [No scheme state]: https://url.spec.whatwg.org/#no-scheme-state
        pub(super) fn state_scheme_none(
            mut self,
            base: &Url<'_>,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            const SPECIAL: bool = false;

            assert!(buffer.is_empty());

            if !base.scheme.0.is_empty() {
                if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                    self.skip_ascii_tab_or_newline();
                }

                match self.cursor.first() {
                    Some(b'#') if base.is_opaque() => {
                        forward!(self.cursor, by 1);

                        buffer.push_str(&base.backing[..base.query.0.end])?;

                        let Url {
                            scheme,
                            username,
                            password,
                            host,
                            port,
                            path,
                            query,
                            ..
                        } = base;

                        #[cfg(test)]
                        {
                            let _username = utf8::from_bytes(&buffer[username.0.clone()]);
                            let _password = utf8::from_bytes(&buffer[password.0.clone()]);
                            let _host = utf8::from_bytes(&buffer[host.0.clone()]);
                            let _path = utf8::from_bytes(&buffer[path.0.clone()]);
                            let _query = utf8::from_bytes(&buffer[query.0.clone()]);
                        }

                        let fragment = self.state_fragment(&mut buffer)?;

                        #[cfg(test)]
                        {
                            let _fragment = utf8::from_bytes(&buffer[fragment.0.clone()]);
                        }

                        let backing = buffer.into_inner();
                        let error_bitset = self.error_bitset;

                        Ok((
                            Url {
                                backing,

                                scheme: scheme.clone(),
                                username: username.clone(),
                                password: password.clone(),
                                host: host.clone(),
                                port: port.clone(),
                                path: path.clone(),
                                query: query.clone(),
                                fragment,
                            },
                            error_bitset.iter(),
                        ))
                    },

                    _ if base.is_opaque() => Err(Error::MissingSchemeNonRelativeUrl),

                    _ if base.scheme() == b"file" => {
                        todo!("file state: https://url.spec.whatwg.org/#file-state")
                    },

                    Some(b'/') => {
                        forward!(self.cursor, by 1);

                        let scheme = base.scheme.clone();

                        return self.state_relative_slash::<SPECIAL>(base, scheme, 0, buffer);
                    },

                    _ => self.state_relative::<SPECIAL>(base, buffer),
                }
            } else {
                Err(Error::MissingSchemeNonRelativeUrl)
            }
        }

        /// # [Special relative or authority state]
        ///
        /// [Special relative or authority state]: https://url.spec.whatwg.org/#special-relative-or-authority-state
        #[inline]
        pub(super) fn state_special_relative_or_authority(
            mut self,
            base: &Url<'_>,
            scheme: segment::Scheme,
            default_scheme_port: u16,
            buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            const SPECIAL: bool = true;

            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            }

            // ASCII tab or newline  already accounted for
            match self.cursor.first() {
                Some(b'/') => {
                    forward!(self.cursor, by 1);

                    self.state_relative_slash::<SPECIAL>(base, scheme, default_scheme_port, buffer)
                },

                Some(b'\\') => {
                    forward!(self.cursor, by 1);

                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);
                    self.log_error(ValidationError::InvalidReverseSolidus);

                    self.state_relative_slash::<SPECIAL>(base, scheme, default_scheme_port, buffer)
                },

                _ => {
                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);

                    self.state_relative::<SPECIAL>(base, buffer)
                },
            }
        }

        /// # [Relative state]
        ///
        /// This isn't exactly the same state implementation as in the specs. This is because those
        /// steps which are missing take place in previous states in order to cut down on branching,
        /// so in practice the _overall_ implementation is equivalent.
        ///
        /// In particular, checking for U+005C (\) in case URL is special is done in
        /// [`state_special_relative_or_authority`] which removes an extra branch indirection.
        ///
        /// Similarly, checking for U+002F (/) has been removed as it can be done more efficiently
        /// at the call site in order to branch directly to `state_special_relative_or_authority`
        /// without having to call this function.
        ///
        /// Setting URL's scheme's to `base`'s scheme is also removed. In the case of
        /// `state_special_relative_or_authority` we don't need it, as that state only occurs if
        /// URL's scheme is the same as `base`'s. Otherwise, it is set manually, such as in
        /// [`state_scheme_none`].
        ///
        /// [Relative state]: https://url.spec.whatwg.org/#relative-state
        /// [`state_special_relative_or_authority`]: Self::state_special_relative_or_authority
        /// [`state_scheme_none`]: Self::state_scheme_none
        pub(super) fn state_relative<const SPECIAL: bool>(
            mut self,
            base: &Url<'_>,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            assert!(!matches!(
                self.cursor.first(),
                Some(ascii_tab_or_newline!())
            ));

            match self.cursor.first() {
                Some(b'?') => {
                    // This is an optimization over the naive relative state fallback.
                    //
                    // Since the next segment to be parsed will be the query we only copy base
                    // URL segments up to that point and immediately move to the query state.

                    // Skip U+003F (?) query segment delimiter
                    forward!(self.cursor, by 1);

                    buffer.push_str(&base.backing[..base.path.0.end])?;

                    let Url {
                        scheme,
                        username,
                        password,
                        host,
                        port,
                        path,
                        ..
                    } = base;

                    #[cfg(test)]
                    {
                        let _username = utf8::from_bytes(&buffer[username.0.clone()]);
                        let _password = utf8::from_bytes(&buffer[password.0.clone()]);
                        let _host = utf8::from_bytes(&buffer[host.0.clone()]);
                        let _path = utf8::from_bytes(&buffer[path.0.clone()]);
                    }

                    let (query, fragment) = self.state_query::<SPECIAL>(&mut buffer)?;

                    #[cfg(test)]
                    {
                        let _query = utf8::from_bytes(&buffer[query.0.clone()]);
                        let _fragment = utf8::from_bytes(&buffer[fragment.0.clone()]);
                    }

                    let backing = buffer.into_inner();
                    let error_bitset = self.error_bitset;

                    return Ok((
                        Url {
                            backing,

                            scheme: scheme.clone(),
                            username: username.clone(),
                            password: password.clone(),
                            host: host.clone(),
                            port: port.clone(),
                            path: path.clone(),
                            query,
                            fragment,
                        },
                        error_bitset.iter(),
                    ));
                },
                Some(b'#') => {
                    // This is an optimization over the naive relative state fallback.
                    //
                    // Since the next segment to be parsed will be the fragment we only copy
                    // base URL segments up to that point and immediately move to the fragment
                    // state.

                    // Skip U+003F (#) fragment segment delimiter
                    forward!(self.cursor, by 1);

                    buffer.push_str(&base.backing[..base.query.0.end])?;

                    let Url {
                        scheme,
                        username,
                        password,
                        host,
                        port,
                        path,
                        query,
                        ..
                    } = base;

                    #[cfg(test)]
                    {
                        let _username = utf8::from_bytes(&buffer[username.0.clone()]);
                        let _password = utf8::from_bytes(&buffer[password.0.clone()]);
                        let _host = utf8::from_bytes(&buffer[host.0.clone()]);
                        let _path = utf8::from_bytes(&buffer[path.0.clone()]);
                        let _query = utf8::from_bytes(&buffer[query.0.clone()]);
                    }

                    let fragment = self.state_fragment(&mut buffer)?;

                    #[cfg(test)]
                    {
                        let _fragment = utf8::from_bytes(&buffer[fragment.0.clone()]);
                    }

                    let backing = buffer.into_inner();
                    let error_bitset = self.error_bitset;

                    return Ok((
                        Url {
                            backing,

                            scheme: scheme.clone(),
                            username: username.clone(),
                            password: password.clone(),
                            host: host.clone(),
                            port: port.clone(),
                            path: path.clone(),
                            query: query.clone(),
                            fragment,
                        },
                        error_bitset.iter(),
                    ));
                },
                Some(c) => {
                    // U+002F (/) checks are performed in parent functions
                    assert_ne!(c, &b'/');

                    // Relative state fallback in case parsed network bytes don't start with a
                    // slash.
                    //
                    // https://url.spec.whatwg.org/#relative-state

                    buffer.push_str(&base.backing[..base.path.0.start])?;

                    let Url {
                        scheme,
                        username,
                        password,
                        host,
                        port,
                        path,
                        ..
                    } = base;

                    let path_base = &base.backing[path.0.clone()];

                    let (path, query, fragment) =
                        self.state_path::<SPECIAL>(&mut buffer, path_base)?;

                    let backing = buffer.into_inner();
                    let error_bitset = self.error_bitset;

                    return Ok((
                        Url {
                            backing,

                            scheme: scheme.clone(),
                            username: username.clone(),
                            password: password.clone(),
                            host: host.clone(),
                            port: port.clone(),
                            path: path.clone(),
                            query: query.clone(),
                            fragment,
                        },
                        error_bitset.iter(),
                    ));
                },
                None => {
                    // This is an optimization over the naive relative state fallback.
                    //
                    // Since there are no more segments left to parse we just return the base
                    // URL. Strange thing is the rust-url source code actually removes the
                    // fragment in this case? TBH the specs themselves aren't very clear as to
                    // what should happen in case we reach the EOF code point (it isn't
                    // specified).
                    //
                    // https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L713-L714

                    buffer.push_str(&base.backing[..base.fragment.0.start])?;

                    let Url {
                        scheme,
                        username,
                        password,
                        host,
                        port,
                        path,
                        query,
                        ..
                    } = base;

                    // We follow rust-url on this one
                    let fragment = segment::Fragment(query.0.end..query.0.end);

                    #[cfg(test)]
                    {
                        let _username = utf8::from_bytes(&buffer[username.0.clone()]);
                        let _password = utf8::from_bytes(&buffer[password.0.clone()]);
                        let _host = utf8::from_bytes(&buffer[host.0.clone()]);
                        let _path = utf8::from_bytes(&buffer[path.0.clone()]);
                        let _query = utf8::from_bytes(&buffer[query.0.clone()]);
                        let _fragment = utf8::from_bytes(&buffer[fragment.0.clone()]);
                    }

                    let backing = buffer.into_inner();
                    let error_bitset = self.error_bitset;

                    return Ok((
                        Url {
                            backing,

                            scheme: scheme.clone(),
                            username: username.clone(),
                            password: password.clone(),
                            host: host.clone(),
                            port: port.clone(),
                            path: path.clone(),
                            query: query.clone(),
                            fragment,
                        },
                        error_bitset.iter(),
                    ));
                },
            }
        }

        /// # [Relative slash state]
        ///
        /// The specs want us to compare the current byte once more in the Relative State,
        /// however the above checks in the Special Relative or Authority State guarantee that we
        /// will always short-circuit to Relative Slash State immediately, since
        ///
        /// > "If c is U+002F (/), then set state to relative slash state.
        /// > Otherwise, if url is special and c is U+005C (\), invalid-reverse-solidus
        /// > validation error, set state to relative slash state."
        ///
        /// 'c' being the code point checked above in the Relative or Authority State. However
        /// this part of the code executes IFF 'c' was U+002F (/) or U+005C (\) and we already
        /// know the URL is special!
        ///
        /// [Relative slash state]: https://url.spec.whatwg.org/#relative-slash-state
        pub(super) fn state_relative_slash<const SPECIAL: bool>(
            mut self,
            base: &Url<'_>,
            scheme: segment::Scheme,
            default_scheme_port: u16,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            }

            // ASCII tab or newline  already accounted for
            match self.cursor.first() {
                Some(b'/') => {
                    forward!(self.cursor, by 1);

                    return self.state_special_authority_ignore_slashes(
                        scheme,
                        default_scheme_port,
                        buffer,
                    );
                },

                Some(b'\\') => {
                    forward!(self.cursor, by 1);

                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);
                    self.log_error(ValidationError::InvalidReverseSolidus);

                    return self.state_special_authority_ignore_slashes(
                        scheme,
                        default_scheme_port,
                        buffer,
                    );
                },

                _ => {
                    // Relative slash state fallback in case parsed network bytes starts with a
                    // slash but no double slash.

                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);

                    buffer.clear();
                    buffer.push_str(&base.backing[..base.path.0.start])?;

                    return self.common_after_host::<SPECIAL>(
                        scheme,
                        base.username.clone(),
                        base.password.clone(),
                        base.host.clone(),
                        base.port.clone(),
                        buffer,
                    );
                },
            }
        }

        /// # [Special authority slashes state]
        ///
        /// Ensure the scheme is followed by two U+002F (/).
        ///
        /// [Special authority slashes state]: https://url.spec.whatwg.org/#special-authority-slashes-state
        #[inline]
        pub(super) fn state_special_authority_slashes(
            mut self,
            scheme: segment::Scheme,
            default_scheme_port: u16,
            buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            };

            // First slash
            match self.cursor.first() {
                Some(b'/') => {
                    forward!(self.cursor, by 1);
                },

                _ => {
                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);

                    return self.state_special_authority_ignore_slashes(
                        scheme,
                        default_scheme_port,
                        buffer,
                    );
                },
            }

            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            };

            // Second slash
            match self.cursor.first() {
                Some(b'/') => {
                    forward!(self.cursor, by 1);
                },

                _ => {
                    self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);
                },
            }

            return self.state_special_authority_ignore_slashes(
                scheme,
                default_scheme_port,
                buffer,
            );
        }

        /// # [Special authority ignore slashes state]
        ///
        ///  The specs aren't very clear on what happens in case an invalid combination of
        ///  slashes precedes the authority. However, based on the [rust_url source code], it seems
        ///  the correct approach is to ignore ALL slashes following the scheme.
        ///
        /// [Special authority ignore slashes state]: https://url.spec.whatwg.org/#special-authority-ignore-slashes-state
        /// [rust_url]: https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L451-L457
        #[inline]
        pub(super) fn state_special_authority_ignore_slashes(
            mut self,
            scheme: segment::Scheme,
            default_scheme_port: u16,
            buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            const SPECIAL: bool = true;

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        self.skip_ascii_tab_or_newline();
                    },

                    Some(b'/' | b'\\') => {
                        self.log_error(ValidationError::SpecialSchemeMissingFollowingSolidus);
                        forward!(self.cursor, by 1);
                    },

                    _ => {
                        return self.common_after_scheme::<SPECIAL>(
                            scheme,
                            default_scheme_port,
                            buffer,
                        );
                    },
                }
            }
        }

        #[inline]
        pub(super) fn state_nonspecial(
            mut self,
            scheme: segment::Scheme,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            const SPECIAL: bool = false;

            if let Some(b'/') = self.cursor.first() {
                // == Path or authority state ==========================================================
                //
                // https://url.spec.whatwg.org/#path-or-authority-state
                //
                // =====================================================================================
                if let Some(b'/') = self.cursor.get(1) {
                    forward!(self.cursor, by 2);

                    self.common_after_scheme::<SPECIAL>(
                        scheme,
                        // No need for an option here: only special schemes can have a default port
                        // so this is already encoded in the SPECIAL constant.
                        0, buffer,
                    )
                } else {
                    forward!(self.cursor, by 1);

                    let next = buffer.len();

                    self.common_after_host::<SPECIAL>(
                        scheme,
                        segment::Username(next..next),
                        segment::Password(next..next),
                        segment::Host(next..next),
                        segment::Port(None),
                        buffer,
                    )
                }
            } else {
                // == Opaque path state ================================================================
                //
                // https://url.spec.whatwg.org/#cannot-be-a-base-url-path-state
                //
                // =====================================================================================
                let next = buffer.len();

                let username = segment::Username(next..next);
                let password = segment::Password(next..next);
                let host = segment::Host(next..next);
                let port = segment::Port(None);

                let (path, query, fragment) = self.state_path_opaque(&mut buffer)?;

                // TODO: refactor this into a common path as well?

                #[cfg(test)]
                let _path = utf8::from_bytes(&buffer[path.0.clone()]);
                #[cfg(test)]
                let _query = utf8::from_bytes(&buffer[query.0.clone()]);
                #[cfg(test)]
                let _fragment = utf8::from_bytes(&buffer[fragment.0.clone()]);

                let backing = buffer.into_inner();

                Ok((
                    Url {
                        backing,

                        scheme,
                        username,
                        password,
                        host,
                        port,
                        path,
                        query,
                        fragment,
                    },
                    self.error_bitset.iter(),
                ))
            }
        }

        /// # Authority state
        ///
        /// Parses the `username` and `password` sections of a [`Url`], if there are any. This really
        /// only exists for legacy compatibility reasons. You should **NOT** use this to embed plain
        /// text credentials into a URL for authentication!
        ///
        /// # Validation Errors
        ///
        /// [`InvalidCredentials`] if any embedded credentials are encountered.
        ///
        /// # Errors
        ///
        /// [`HostMissing`] if there are no more code points left after `username` and `password`.
        ///
        /// [`InvalidCredentials`]: ValidationError::InvalidCredentials
        /// [`HostMissing`]: Error::HostMissing
        #[inline]
        pub(super) fn state_authority<const SPECIAL: bool>(
            &mut self,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<(segment::Username, segment::Password), Error> {
            // == authority state ======================================================================
            //
            // https://url.spec.whatwg.org/#authority-state
            //
            // The spec has a really roundabout way of wording this but essentially all we need to do is
            // linearly parse through the userinfo section, the end of which is determined by the LAST
            // U+0040 (@) code point. Any character before that gets percent-encoded according to the
            // userinfo percent-encode set. Any character before U+003A (:) is considered to be part of
            // the username, and any character after is considered to be part of the user's password. We
            // only skip the first U+003A (:) and percent-encode any other occurrences of it, even if
            // those are not percent-encoded. If we encounter any other section delimiter but the
            // remaining host section is empty AND we have seen a terminating U+0040 (@) code point,
            // then we return a host-missing error.
            //
            // =========================================================================================

            // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
            // parse them again.
            if let Some(c) = self.cursor.first()
                && matches!(c, ascii_tab_or_newline!())
            {
                self.skip_ascii_tab_or_newline();
            }

            let checkpoint = self.cursor;

            let mut at_sign = None;
            let mut char_count_authority = 0;

            // Parsing has to take place in two steps:
            //
            // 1. First, we iterate over the full range of authority characters to find the terminating
            //    userinfo U+0040 (@) delimiter.
            //
            // 2. We then iterate over all code points before the terminating userinfo U+0040 (@)
            //    delimiter and encode them into the result buffer.
            //
            // This two step process is necessary as we don't want to parse any host components yet, but
            // we still need to find the terminating U+0040 (@), userinfo delimiter which is only
            // bounded by the end of the host segment
            // of the url.
            while !self.cursor.is_empty() {
                let c = self.cursor[0];

                match c {
                    ascii_tab_or_newline!() => {
                        char_count_authority += self.skip_ascii_tab_or_newline() + 1;
                    },

                    // EOF code point is implied in the below check
                    b'/' | b'?' | b'#' => {
                        break;
                    },

                    b'\\' if SPECIAL => {
                        break;
                    },

                    b'@' => {
                        self.log_error(ValidationError::InvalidCredentials);
                        at_sign = Some(char_count_authority);
                        forward!(self.cursor, by 1);
                        char_count_authority += 1;
                    },
                    _ => {
                        forward!(self.cursor, by 1);
                        char_count_authority += 1;
                    },
                }
            }

            self.cursor = checkpoint;

            let mut char_count_userinfo = match at_sign {
                Some(n) => {
                    // We only exit the above loop if we have  reached the end of the host segment of
                    // the url or the end of the url itself. This means that if there are no more
                    // characters after the terminating userinfo U+0040 (@) delimiter then the host must
                    // be empty.
                    //
                    // The specs word this somewhat more confusingly as:
                    //
                    // > If atSignSeen is true and buffer is the empty string, host-missing validation
                    // > error, return failure.
                    //
                    // https://url.spec.whatwg.org/#authority-state
                    if char_count_authority - n - 1 == 0 {
                        return Err(Error::HostMissing);
                    }

                    n
                },
                None => 0,
            };

            // Scheme end, skipping `://`
            let userinfo_start = buffer.len();

            let mut password_token = None;

            // Here is where we actually parse the userinfo.
            while char_count_userinfo > 0 {
                let c = self.cursor[0];

                forward!(self.cursor, by 1);
                char_count_userinfo -= 1;

                match c {
                    // ASCII tab or newline code points have to be skipped again since we reset the
                    // cursor position to the previous checkpoint.
                    ascii_tab_or_newline!() => {},

                    b':' => {
                        match password_token {
                            Some(_) => {
                                // percent-encoded U+003A (:)
                                buffer.push_str(b"%3A")?;
                            },
                            None => {
                                // We only push U+003A (:) if the password is non-empty.
                                //
                                // https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L907-L914
                                if char_count_userinfo > 0 {
                                    password_token = Some(buffer.len());
                                    buffer.push_byte(b':')?;
                                }
                            },
                        }
                    },

                    _ => {
                        buffer.push_encode(c, percent::USERINFO)?;
                    },
                }
            }

            let userinfo_stop = buffer.len();

            let (username, password) = match password_token {
                Some(n) => (userinfo_start..n, n + 1..userinfo_stop),
                None => (userinfo_start..userinfo_stop, userinfo_stop..userinfo_stop),
            };

            // username and password can still be empty in case userinfo is `:@` (empty username and
            // empty password separated by the password token).
            if !username.is_empty() || !password.is_empty() {
                // SAFETY: this is either the terminating userinfo U+0040 (@) delimiter or an ascii tab
                // or newline so cursor is guaranteed to be non-empty.
                if matches!(self.cursor[0], ascii_tab_or_newline!()) {
                    self.skip_ascii_tab_or_newline();
                }

                // We need to skip over the terminating userinfo U+0040 (@) delimiter again as we
                // have reset the cursor.
                forward!(self.cursor, by 1);

                buffer.push_byte(b'@')?;
            }

            Ok((segment::Username(username), segment::Password(password)))
        }

        /// Parses out a [`Url`]'s [`host`] and [`port`] components. This is done to optimize branching
        /// so that the presence of a port is only ever checked once.
        ///
        /// # Errors
        ///
        /// Surfaces a parsing [`Error`] if any of the underlying [`host`] or [`port`] parsers error
        /// out.
        ///
        /// [`host`]: Self::state_hostname
        /// [`port`]: port::parse
        #[inline]
        pub(super) fn state_host<'parsing, const SPECIAL: bool>(
            &mut self,
            username: &'parsing segment::Username,
            password: &'parsing segment::Password,
            default_scheme_port: u16,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<
            (
                segment::Host,
                segment::Port,
                segment::Path,
                segment::Query,
                segment::Fragment,
            ),
            Error,
        > {
            // == hostname state =======================================================================
            //
            // https://url.spec.whatwg.org/#hostname-state
            //
            // =========================================================================================

            // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
            // parse them again.
            if let Some(c) = self.cursor.first()
                && matches!(c, ascii_tab_or_newline!())
            {
                self.skip_ascii_tab_or_newline();
            }

            let checkpoint = self.cursor;

            let mut inside_brackets = false;
            let mut char_count_hostname = 0;

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        char_count_hostname += self.skip_ascii_tab_or_newline() + 1;
                    },

                    Some(b':') if !inside_brackets => {
                        self.cursor = checkpoint;

                        let host = self.state_hostname::<SPECIAL, true>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        // We need to skip the port delimiter again as we reset the cursor.
                        forward!(self.cursor, by 1);

                        let (port, path, query, fragment) =
                            self.state_port::<SPECIAL>(default_scheme_port, buffer)?;

                        return Ok((host, port, path, query, fragment));
                    },

                    Some(b'/') => {
                        self.cursor = checkpoint;

                        let host = self.state_hostname::<SPECIAL, true>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        // Skip U+002F (/) path segment delimiter
                        forward!(self.cursor, by 1);

                        let port = segment::Port::empty();

                        let (path, query, fragment) = self.state_path::<SPECIAL>(buffer, &[])?;

                        return Ok((host, port, path, query, fragment));
                    },

                    Some(b'\\') if SPECIAL => {
                        self.cursor = checkpoint;

                        self.log_error(ValidationError::InvalidReverseSolidus);

                        let host = self.state_hostname::<SPECIAL, true>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        // Skip U+005C (\) path segment delimiter
                        forward!(self.cursor, by 1);

                        let port = segment::Port::empty();

                        let (path, query, fragment) = self.state_path::<SPECIAL>(buffer, &[])?;

                        return Ok((host, port, path, query, fragment));
                    },

                    Some(b'?') => {
                        self.cursor = checkpoint;

                        let host = self.state_hostname::<SPECIAL, true>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        // Skip U+003F (?) query segment delimiter
                        forward!(self.cursor, by 1);

                        let port = segment::Port::empty();
                        let path = segment::Path::empty(buffer)?;

                        let (query, fragment) = self.state_query::<SPECIAL>(buffer)?;

                        return Ok((host, port, path, query, fragment));
                    },

                    Some(b'#') => {
                        self.cursor = checkpoint;

                        let host = self.state_hostname::<SPECIAL, true>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        // Skip U+003F (#) fragment segment delimiter
                        forward!(self.cursor, by 1);

                        let port = segment::Port::empty();
                        let path = segment::Path::empty(buffer)?;
                        let query = segment::Query::empty(buffer);

                        let fragment = self.state_fragment(buffer)?;

                        return Ok((host, port, path, query, fragment));
                    },

                    Some(b'[') => {
                        inside_brackets = true;
                        char_count_hostname += 1;
                        forward!(self.cursor, by 1);
                    },

                    Some(b']') => {
                        inside_brackets = false;
                        char_count_hostname += 1;
                        forward!(self.cursor, by 1);
                    },

                    Some(_c) => {
                        char_count_hostname += 1;
                        forward!(self.cursor, by 1);
                    },

                    None => {
                        self.cursor = checkpoint;

                        let host = self.state_hostname::<SPECIAL, false>(
                            username,
                            password,
                            char_count_hostname,
                            buffer,
                        )?;

                        let port = segment::Port::empty();
                        let path = segment::Path::empty(buffer)?;
                        let query = segment::Query::empty(buffer);
                        let fragment = segment::Fragment::empty(buffer);

                        return Ok((host, port, path, query, fragment));
                    },
                }
            }
        }

        /// # [Hostname state]
        ///
        /// Parses out the host segment of a [`Url`]. This can be either:
        ///
        /// - An IPV6 host.
        /// - A domain host.
        /// - An IPV4 host.
        ///
        /// # Errors
        ///
        /// [`HostMissing`] if the host segment does not contain any code points.
        ///
        /// [Hostname state]: https://url.spec.whatwg.org/#hostname-state
        /// [`HostMissing`]: Error::HostMissing
        #[inline]
        pub(super) fn state_hostname<'parsing, const SPECIAL: bool, const PORT: bool>(
            &mut self,
            username: &'parsing segment::Username,
            password: &'parsing segment::Password,
            char_count_hostname: usize,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<segment::Host, Error> {
            // == host parsing =========================================================================
            //
            // https://url.spec.whatwg.org/#host-parsing
            //
            // =========================================================================================

            if self.cursor.is_empty() {
                return if SPECIAL || PORT {
                    Err(Error::HostMissing)
                } else {
                    let next = buffer.len();
                    Ok(segment::Host(next..next))
                };
            }

            if self.cursor[0] == b'[' {
                todo!("IPV6 parsing");
            }
            // TODO: refactor this into its own function once we support IPV6 parsing and IPV4
            // parsing as well

            // userinfo end, skipping U+0040 (@)
            let host_start = if !password.0.is_empty() {
                password.0.end + 1
            } else if !username.0.is_empty() {
                username.0.end + 1
            } else {
                username.0.end
            };

            // TODO: IDNA domain parser

            // char_count_hostname includes ASCII tab and newlines, so we can't use that outright to
            // determine the size of the host.
            let domain = percent::decode(self.cursor.iter());

            for c in domain.take(char_count_hostname) {
                if !matches!(c, ascii_tab_or_newline!()) {
                    buffer.push_byte(c)?;
                }
            }
            forward!(self.cursor, by char_count_hostname);

            // TODO: IPV4 parsing

            let host_stop = buffer.len();
            let host = host_start..host_stop;

            if host.is_empty() {
                return if SPECIAL || PORT {
                    Err(Error::HostMissing)
                } else {
                    let next = buffer.len();
                    Ok(segment::Host(next..next))
                };
            }

            Ok(segment::Host(host))
        }

        #[inline]
        pub(super) fn state_port<const SPECIAL: bool>(
            &mut self,
            default_scheme_port: u16,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<
            (
                segment::Port,
                segment::Path,
                segment::Query,
                segment::Fragment,
            ),
            Error,
        > {
            fn port_end<const SPECIAL: bool>(
                port: u32,
                default_scheme_port: u16,
                char_count_port: usize,
                checkpoint: &[u8],
                buffer: &mut UrlBuffer<'_>,
            ) -> Result<segment::Port, Error> {
                if SPECIAL && (port == u32::from(default_scheme_port) || char_count_port == 0) {
                    Ok(segment::Port(None))
                } else {
                    buffer.push_byte(b':')?;

                    for c in &checkpoint[..char_count_port] {
                        if !matches!(c, ascii_tab_or_newline!()) {
                            buffer.push_byte(*c)?;
                        }
                    }

                    Ok(segment::Port(Some(port as u16)))
                }
            }

            // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
            // parse them again.
            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            }

            let mut port = 0u32;
            let mut char_count_port = 0;

            let checkpoint = self.cursor;

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        char_count_port += self.skip_ascii_tab_or_newline() + 1;
                    },

                    Some(n @ b'0'..=b'9') => {
                        port = port * 10 + u32::from(*n) - u32::from(b'0');

                        if port > u32::from(u16::MAX) {
                            return Err(Error::PortOutOfRange);
                        }

                        forward!(self.cursor, by 1);
                        char_count_port += 1;
                    },

                    Some(b'/') => {
                        // Skip U+002F (/) path segment delimiter
                        forward!(self.cursor, by 1);

                        let port = port_end::<SPECIAL>(
                            port,
                            default_scheme_port,
                            char_count_port,
                            checkpoint,
                            buffer,
                        )?;

                        let (path, query, fragment) = self.state_path::<SPECIAL>(buffer, &[])?;

                        return Ok((port, path, query, fragment));
                    },

                    Some(b'\\') if SPECIAL => {
                        self.log_error(ValidationError::InvalidReverseSolidus);

                        // Skip U+005C (\) path segment delimiter
                        forward!(self.cursor, by 1);

                        let port = port_end::<SPECIAL>(
                            port,
                            default_scheme_port,
                            char_count_port,
                            checkpoint,
                            buffer,
                        )?;

                        let (path, query, fragment) = self.state_path::<SPECIAL>(buffer, &[])?;

                        return Ok((port, path, query, fragment));
                    },

                    Some(b'?') => {
                        // Skip U+003F (?) query segment delimiter
                        forward!(self.cursor, by 1);

                        let port = port_end::<SPECIAL>(
                            port,
                            default_scheme_port,
                            char_count_port,
                            checkpoint,
                            buffer,
                        )?;

                        let path = segment::Path::empty(buffer)?;

                        let (query, fragment) = self.state_query::<SPECIAL>(buffer)?;

                        return Ok((port, path, query, fragment));
                    },

                    Some(b'#') => {
                        // Skip U+003F (#) fragment segment delimiter
                        forward!(self.cursor, by 1);

                        let port = port_end::<SPECIAL>(
                            port,
                            default_scheme_port,
                            char_count_port,
                            checkpoint,
                            buffer,
                        )?;

                        let path = segment::Path::empty(buffer)?;
                        let query = segment::Query::empty(buffer);

                        let fragment = self.state_fragment(buffer)?;

                        return Ok((port, path, query, fragment));
                    },

                    Some(_c) => {
                        return Err(Error::PortInvalid);
                    },

                    None => {
                        let port = port_end::<SPECIAL>(
                            port,
                            default_scheme_port,
                            char_count_port,
                            checkpoint,
                            buffer,
                        )?;

                        let path = segment::Path::empty(buffer)?;
                        let query = segment::Query::empty(buffer);
                        let fragment = segment::Fragment::empty(buffer);

                        return Ok((port, path, query, fragment));
                    },
                }
            }
        }

        #[inline]
        pub(super) fn state_path<const SPECIAL: bool>(
            &mut self,
            buffer: &mut UrlBuffer<'output>,
            mut path_base: &[u8],
        ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
            fn segment_end(path_stack: &mut path::PathStack) -> Result<(), Error> {
                #[cfg(test)]
                let _segment = utf8::from_bytes(path_stack.segment_peek());

                // double and single-dot URL path segments can be any combination of raw utf8 and
                // percent-encoded, which complicates the detection logic a bit.
                match path_stack.segment_peek() {
                    [b'.', b'.', b'/']
                    | [b'%', b'2', b'e', b'%', b'2', b'e', b'/']
                    | [b'%', b'2', b'e', b'%', b'2', b'E', b'/']
                    | [b'%', b'2', b'E', b'%', b'2', b'e', b'/']
                    | [b'%', b'2', b'E', b'%', b'2', b'E', b'/']
                    | [b'%', b'2', b'e', b'.', b'/']
                    | [b'%', b'2', b'E', b'.', b'/']
                    | [b'.', b'%', b'2', b'e', b'/']
                    | [b'.', b'%', b'2', b'E', b'/'] => {
                        // Remove the current path segment (../)
                        path_stack.segment_pop();

                        // Remove the previous path segment
                        path_stack.segment_pop();
                    },

                    [b'.', ..] | [b'%', b'2', b'e', ..] | [b'%', b'2', b'E', ..] => {
                        // Remove the current path segment (./)
                        path_stack.segment_pop();
                    },
                    _ => (),
                }

                path_stack.segment_next()
            }

            let path_start = buffer.len();
            let mut path_stack = path::PathStack::new(buffer.leftover())?;

            if !path_base.is_empty() {
                assert_eq!(path_base.first(), Some(&b'/'));

                // Don't want to copy the base path's leading '/' as PathStack already adds one by
                // default.

                forward!(path_base, by 1);

                // We copy the base path up to the last folder, which has the same affect as
                // shortening.

                let mut prev = 0;
                for n in memchr::memchr_iter(b'/', path_base) {
                    path_stack.segment_push_str(&path_base[prev..n + 1])?;
                    prev = n + 1;
                }
            }

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        self.skip_ascii_tab_or_newline();
                    },

                    Some(b'/') => {
                        forward!(self.cursor, by 1);

                        path_stack.segment_push_byte(b'/')?;

                        segment_end(&mut path_stack)?;
                    },

                    Some(b'\\') if SPECIAL => {
                        self.log_error(ValidationError::InvalidReverseSolidus);
                        forward!(self.cursor, by 1);

                        path_stack.segment_push_byte(b'/')?;

                        segment_end(&mut path_stack)?;
                    },

                    Some(b'?') => {
                        // Skip U+003F (?) query segment delimiter
                        forward!(self.cursor, by 1);

                        segment_end(&mut path_stack)?;

                        let len = path_stack.len();

                        // SAFETY: path_stack performs at least the same bounds check as buffer.
                        unsafe { buffer.force_forwards(len) };

                        let path_stop = buffer.len();

                        let path = segment::Path(path_start..path_stop);

                        #[cfg(test)]
                        let _remaining_query = utf8::from_bytes(self.cursor);

                        let (query, fragment) = self.state_query::<SPECIAL>(buffer)?;

                        return Ok((path, query, fragment));
                    },

                    Some(b'#') => {
                        // Skip U+0023 (#) fragment segment delimiter
                        forward!(self.cursor, by 1);

                        segment_end(&mut path_stack)?;

                        let len = path_stack.len();

                        // SAFETY: path_stack performs at least the same bounds check as buffer.
                        unsafe { buffer.force_forwards(len) };

                        let path_stop = buffer.len();

                        let path = segment::Path(path_start..path_stop);
                        let query = segment::Query(path_stop..path_stop);

                        #[cfg(test)]
                        let _remaining_fragment = utf8::from_bytes(self.cursor);

                        let fragment = self.state_fragment(buffer)?;

                        return Ok((path, query, fragment));
                    },

                    Some(b'%') => {
                        path_stack.segment_push_byte(b'%')?;
                        self.check_percent_encoding();
                    },

                    Some(_c) => {
                        self.encode_utf8_stack(&mut path_stack, percent::PATH)?;
                    },

                    None => {
                        segment_end(&mut path_stack)?;

                        let len = path_stack.len();

                        // SAFETY: path_stack performs at least the same bounds check as buffer.
                        unsafe { buffer.force_forwards(len) };

                        let path_stop = buffer.len();

                        let path = segment::Path(path_start..path_stop);
                        let query = segment::Query(path_stop..path_stop);
                        let fragment = segment::Fragment(path_stop..path_stop);

                        return Ok((path, query, fragment));
                    },
                }
            }
        }

        #[inline]
        pub(super) fn state_path_opaque(
            &mut self,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
            const SPECIAL: bool = false;

            fn handle_query(
                parser: &mut Parser<'_>,
                buffer: &mut UrlBuffer<'_>,
                path: segment::Path,
            ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
                // Skip U+003F (?) query segment delimiter
                forward!(parser.cursor, by 1);

                #[cfg(test)]
                let _remaining_query = utf8::from_bytes(parser.cursor);

                let (query, fragment) = parser.state_query::<SPECIAL>(buffer)?;

                Ok((path, query, fragment))
            }

            fn handle_fragment(
                parser: &mut Parser<'_>,
                buffer: &mut UrlBuffer<'_>,
                path: segment::Path,
                query: segment::Query,
            ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
                // Skip U+0023 (#) fragment segment delimiter
                forward!(parser.cursor, by 1);

                #[cfg(test)]
                let _remaining_fragment = utf8::from_bytes(parser.cursor);

                let fragment = parser.state_fragment(buffer)?;

                Ok((path, query, fragment))
            }

            let path_start = buffer.len();

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        self.skip_ascii_tab_or_newline();
                    },

                    Some(b' ') => {
                        self.log_error(ValidationError::InvalidURLUnit);

                        forward!(self.cursor, by 1);

                        match self.cursor.first() {
                            Some(b'?') => {
                                buffer.push_str(b"%20")?;

                                let path_stop = buffer.len();

                                return handle_query(
                                    self,
                                    buffer,
                                    segment::Path(path_start..path_stop),
                                );
                            },
                            Some(b'#') => {
                                buffer.push_str(b"%20")?;

                                let path_stop = buffer.len();

                                return handle_fragment(
                                    self,
                                    buffer,
                                    segment::Path(path_start..path_stop),
                                    segment::Query(path_stop..path_stop),
                                );
                            },
                            _ => {
                                buffer.push_byte(b' ')?;
                            },
                        }
                    },

                    Some(b'?') => {
                        let path_stop = buffer.len();

                        return handle_query(self, buffer, segment::Path(path_start..path_stop));
                    },

                    Some(b'#') => {
                        let path_stop = buffer.len();

                        return handle_fragment(
                            self,
                            buffer,
                            segment::Path(path_start..path_stop),
                            segment::Query(path_stop..path_stop),
                        );
                    },

                    Some(b'%') => {
                        buffer.push_byte(b'%')?;
                        self.check_percent_encoding();
                    },

                    Some(_c) => {
                        self.encode_utf8_buffer(buffer, percent::C0_CONTROL)?;
                    },

                    None => {
                        let path_stop = buffer.len();

                        let path = segment::Path(path_start..path_stop);
                        let query = segment::Query(path_stop..path_stop);
                        let fragment = segment::Fragment(path_stop..path_stop);

                        return Ok((path, query, fragment));
                    },
                }
            }
        }

        #[inline]
        pub(super) fn state_query<const SPECIAL: bool>(
            &mut self,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<(segment::Query, segment::Fragment), Error> {
            buffer.push_byte(b'?')?;
            let query_start = buffer.len();

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        self.skip_ascii_tab_or_newline();
                    },

                    Some(b'#') => {
                        // Skip U+0023 (#) fragment segment delimiter
                        forward!(self.cursor, by 1);

                        let query_stop = buffer.len();

                        let query = segment::Query(query_start..query_stop);

                        #[cfg(test)]
                        let _remaining_fragment = utf8::from_bytes(self.cursor);

                        let fragment = self.state_fragment(buffer)?;

                        return Ok((query, fragment));
                    },

                    Some(b'%') => {
                        buffer.push_byte(b'%')?;
                        self.check_percent_encoding();
                    },

                    Some(_c) => {
                        let encoding = if SPECIAL {
                            percent::QUERY_SPECIAL
                        } else {
                            percent::QUERY
                        };

                        self.encode_utf8_buffer(buffer, encoding)?;
                    },

                    None => {
                        let query_stop = buffer.len();

                        let query = segment::Query(query_start..query_stop);
                        let fragment = segment::Fragment(query_stop..query_stop);

                        return Ok((query, fragment));
                    },
                }
            }
        }

        #[inline]
        pub(super) fn state_fragment(
            &mut self,
            buffer: &mut UrlBuffer<'output>,
        ) -> Result<segment::Fragment, Error> {
            buffer.push_byte(b'#')?;

            let fragment_start = buffer.len();

            loop {
                match self.cursor.first() {
                    Some(ascii_tab_or_newline!()) => {
                        self.skip_ascii_tab_or_newline();
                    },

                    Some(b'%') => {
                        buffer.push_byte(b'%')?;
                        self.check_percent_encoding();
                    },

                    Some(_c) => {
                        self.encode_utf8_buffer(buffer, percent::FRAGMENT)?;
                    },

                    None => {
                        let fragment_stop = buffer.len();

                        return Ok(segment::Fragment(fragment_start..fragment_stop));
                    },
                }
            }
        }
    }
}

mod common {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    impl<'output> Parser<'_> {
        #[inline]
        pub(super) fn common_after_scheme<const SPECIAL: bool>(
            mut self,
            scheme: segment::Scheme,
            default_scheme_port: u16,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            buffer.push_str(b"//")?;

            let (username, password) = self.state_authority::<SPECIAL>(&mut buffer)?;

            let (host, port, path, query, fragment) =
                self.state_host::<SPECIAL>(&username, &password, default_scheme_port, &mut buffer)?;

            let backing = buffer.into_inner();
            let error_bitset = self.error_bitset;

            Ok((
                Url {
                    backing,

                    scheme,
                    username,
                    password,
                    host,
                    port,
                    path,
                    query,
                    fragment,
                },
                error_bitset.iter(),
            ))
        }

        #[inline]
        pub(super) fn common_after_host<const SPECIAL: bool>(
            mut self,
            scheme: segment::Scheme,
            username: segment::Username,
            password: segment::Password,
            host: segment::Host,
            port: segment::Port,
            mut buffer: UrlBuffer<'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            let (path, query, fragment) = self.state_path::<SPECIAL>(&mut buffer, &[])?;

            let backing = buffer.into_inner();
            let error_bitset = self.error_bitset;

            Ok((
                Url {
                    backing,

                    scheme,
                    username,
                    password,
                    host,
                    port,
                    path,
                    query,
                    fragment,
                },
                error_bitset.iter(),
            ))
        }
    }
}

mod encode {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    impl Parser<'_> {
        #[inline]
        pub(super) fn check_percent_encoding(&mut self) {
            assert_eq!(self.cursor.first(), Some(&b'%'));

            forward!(self.cursor, by 1);

            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            }

            let checkpoint = self.cursor;

            match self.cursor.first() {
                Some(ascii_hexdigit!()) => {},
                Some(_) => {
                    self.log_error(ValidationError::InvalidURLUnit);
                },
                None => {
                    self.log_error(ValidationError::InvalidURLUnit);
                    return;
                },
            }

            forward!(self.cursor, by 1);

            if matches!(self.cursor.first(), Some(ascii_tab_or_newline!())) {
                self.skip_ascii_tab_or_newline();
            }

            if !matches!(self.cursor.first(), Some(ascii_hexdigit!())) {
                self.log_error(ValidationError::InvalidURLUnit);
            }

            self.cursor = checkpoint;
        }

        #[inline]
        pub(super) fn encode_utf8_buffer(
            &mut self,
            buffer: &mut UrlBuffer<'_>,
            encoding: crate::percent::EncodeSet,
        ) -> Result<(), Error> {
            match self.code_point() {
                // NOTE: `code_point` is non-empty so position will always be overridden.
                Some(code_point) => {
                    for c in code_point {
                        buffer.push_encode(*c, encoding)?;
                    }

                    Ok(())
                },

                // Invalid utf-8 bytes are replaced with the U+FFFD (�) utf-8 replacement code point.
                None => buffer.push_str(utf8::REPLACEMENT),
            }
        }

        // TODO: unify this into a single method through a shared interface
        #[inline]
        pub(super) fn encode_utf8_stack(
            &mut self,
            path_stack: &mut path::PathStack<'_>,
            encoding: crate::percent::EncodeSet,
        ) -> Result<(), Error> {
            match self.code_point() {
                Some(code_point) => {
                    for c in code_point {
                        path_stack.segment_push_encode(*c, encoding)?;
                    }

                    Ok(())
                },

                // Invalid utf-8 bytes are replaced with the U+FFFD (�) utf-8 replacement code point.
                None => path_stack.segment_push_str(utf8::REPLACEMENT),
            }
        }

        pub(super) fn code_point(&mut self) -> Option<&[u8]> {
            match utf8::url_code_point(self.cursor) {
                utf8::CodePointUrl::Valid { len } => {
                    let checkpoint = self.cursor;
                    forward!(self.cursor, by len.get() as usize);
                    Some(&checkpoint[..len.get() as usize])
                },

                utf8::CodePointUrl::Invalid { len } => {
                    self.log_error(ValidationError::InvalidURLUnit);

                    let checkpoint = self.cursor;
                    forward!(self.cursor, by len.get() as usize);
                    Some(&checkpoint[..len.get() as usize])
                },

                // Invalid utf-8 bytes are skipped. We don't check for utf-8 encoding in other url
                // sections. However, in the case of the path section where we potentially have to
                // deal with multi-byte url code points, this information comes as free so we might
                // as well act on it.
                utf8::CodePointUrl::InvalidUtf8 { invalid: len } => {
                    forward!(self.cursor, by len.get() as usize);
                    None
                },

                // A truncated code point indicates we have reached the end of the cursor before the
                // end of the code point. This information too is ignored, and we instead stop at
                // the last valid code point.
                utf8::CodePointUrl::Truncated => {
                    forward!(self.cursor, by 1);
                    None
                },
            }
        }
    }
}

mod util {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    impl Parser<'_> {
        #[inline]
        pub(super) fn log_error(&mut self, error: ValidationError) {
            self.error_bitset.add(error);
        }

        #[inline]
        pub(super) fn skip_ascii_tab_or_newline(&mut self) -> usize {
            self.log_error(ValidationError::InvalidURLUnit);

            forward!(self.cursor, by 1);

            let skip = self
                .cursor
                .iter()
                .position(|c| !matches!(c, b'\t' | b'\r' | b'\n'))
                .unwrap_or(self.cursor.len());

            forward!(self.cursor, by skip);

            skip
        }
    }
}

/// UTF-8 parsing utilities.
pub mod utf8 {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// U+FFFD (�) utf-8 replacement code point.
    pub const REPLACEMENT: &[u8] = b"%EF%BF%BD";

    /// Bitmask of ASCII code points which are also [URL code points].
    const ASCII_URL_CODE_POINT: u128 = {
        let mut mask = 0;
        let mut c = 0;

        while c <= 0x80 {
            if matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b'-' | b'.' | b'/' | b':' | b';' | b'=' | b'?' | b'@' | b'_' | b'~')
            {
                mask |= 1 << c;
            }

            c += 1;
        }

        mask
    };

    /// Stores information about a [URL code point].
    ///
    /// [URL code point]: https://url.spec.whatwg.org/#url-code-points
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum CodePointUrl {
        /// Code point is a valid URL code point.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// # use macro_util::prelude::*;
        /// assert_eq!(
        ///     url_code_point(b"a"),
        ///     CodePointUrl::Valid { len: nonzero!(1) }
        /// );
        /// ```
        Valid {
            /// Length of the code point in bytes.
            len: std::num::NonZeroU8,
        },

        /// Code point is not a valid URL code point, but is valid utf-8.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// # use macro_util::prelude::*;
        /// assert_eq!(
        ///     url_code_point(b"%"),
        ///     CodePointUrl::Invalid { len: nonzero!(1) }
        /// );
        /// ```
        Invalid {
            /// Length of the code point in bytes.
            len: std::num::NonZeroU8,
        },

        /// Code point is invalid under utf-8. This can be because it is overlong, a utf-16
        /// surrogate, a continuation byte instead of a lead byte or has a lead byte exceeding the
        /// utf-8 max range of U+10FFFF.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// # use macro_util::prelude::*;
        /// // 0xF5 as a lead byte is outside of the utf-8 max range.
        /// assert_eq!(
        ///     url_code_point(&[0xF5]),
        ///     CodePointUrl::InvalidUtf8 {
        ///         invalid: nonzero!(1)
        ///     }
        /// );
        /// ```
        InvalidUtf8 {
            /// Number of bytes which are invalid utf-8.
            invalid: std::num::NonZeroU8,
        },

        /// Byte input ends before the code point could be fully parsed.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// // 0xF4 as a lead byte denotes a four-bytes code point,
        /// // however the remaining bytes are missing
        /// assert_eq!(url_code_point(&[0xF4]), CodePointUrl::Truncated);
        /// ```
        Truncated,
    }

    #[cfg(test)]
    pub(super) fn from_bytes(bytes: &[u8]) -> &str {
        str::from_utf8(bytes).unwrap_or_default()
    }

    /// Determines if a sequence of bytes begins with a [URL code point]
    ///
    /// Input is not assumed to be valid utf-8: this method can be passed untrusted raw network
    /// bytes to parse and will still work.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use http_url::parsing::utf8::*;
    /// # use macro_util::prelude::*;
    /// assert_eq!(
    ///     url_code_point(b"a"),
    ///     CodePointUrl::Valid { len: nonzero!(1) }
    /// );
    /// ```
    ///
    /// [URL code point]: https://url.spec.whatwg.org/#url-code-points
    #[must_use]
    pub fn url_code_point(bytes: &[u8]) -> CodePointUrl {
        let Some(b0) = bytes.first() else {
            return CodePointUrl::Truncated;
        };

        // == Step 1 ===============================================================================
        //
        // Check all valid ASCII values.
        //
        // =========================================================================================

        if b0.is_ascii() {
            return if (ASCII_URL_CODE_POINT >> *b0 & 1) > 0 {
                CodePointUrl::Valid {
                    len: std::num::NonZeroU8::MIN,
                }
            } else {
                CodePointUrl::Invalid {
                    len: std::num::NonZeroU8::MIN,
                }
            };
        }

        // == Step 2 ===============================================================================
        //
        // Make sure the rest of the input is valid utf8.
        //
        // =========================================================================================

        let (len, b1_min, b1_max) = match b0 {
            // utf8 2-byte lead is 110xxxxx, giving us 0xC0 (11000000) as the smallest possible
            // 2-byte lead. C0 and C1 are overlong though, so the range of valid 2-byte leads is
            // C2 (11000010) up to DF (11011111).
            0xC2..=0xDF => (nonzero!(2u8), 0x80, 0xBF),

            // utf8 3-byte leads 1110xxxx get more complicated. Values before U+0800 should not be
            // encoded in 3 bytes, as that would be overlong, so the second byte must start at A0.
            0xE0 => (nonzero!(3u8), 0xA0, 0xBF),

            // The rest of 3-byte utf8 values are valid, except for surrogates.
            0xE1..=0xEC | 0xEE..=0xEF => (nonzero!(3u8), 0x80, 0xBF),

            // Surrogate code points are reserved for use in utf16 and cannot be used in utf8. A
            // leading surrogate is a code point that is in the range U+D800 to U+DBFF, inclusive.
            // A trailing surrogate is a code point that is in the range U+DC00 to U+DFFF,
            // inclusive. If you look at the byte representation of surrogate code points, you will
            // notice they all start with 0xED, with a second byte in the range 0xA0 to 0xBF
            // inclusive. So the range of valid second bytes in a 3-byte code point are 0x80 to 0x9F
            // inclusive. Any other values are invalid utf8.
            0xED => (nonzero!(3u8), 0x80, 0x9F),

            // utf8 4-byte lead is 11110xxx, giving us 0xF0 (11110000) as the smallest possible
            // 4-byte lead. Values under U+10000 should not be encoded in 4 bytes, as that would be
            // overlong, so the second byte must start at 0x90.
            0xF0 => (nonzero!(4u8), 0x90, 0xBF),

            // 4-byte code points before the U+10FFFF lead byte.
            0xF1..=0xF3 => (nonzero!(4u8), 0x80, 0xBF),

            // utf8 ends at 10FFFF, with a 4-byte lead of 0xF4. The maximum value for the second
            // byte here is 0x8F without exceeding this range.
            0xF4 => (nonzero!(4u8), 0x80, 0x8F),

            // 0x80..=0xBF: a continuation byte where a lead should be.
            // 0xC0, 0xC1, 0xF5..=0xFF: never valid leads.
            _ => {
                return CodePointUrl::InvalidUtf8 {
                    invalid: std::num::NonZeroU8::MIN,
                };
            },
        };

        // Second byte must exist and be in a specific range to be valid utf8.
        let Some(b1) = bytes.get(1) else {
            return CodePointUrl::Truncated;
        };

        if *b1 < b1_min || *b1 > b1_max {
            // Only the leading byte is decisively invalid.
            return CodePointUrl::InvalidUtf8 {
                invalid: std::num::NonZeroU8::MIN,
            };
        }

        // Third and fourth bytes only need to be valid continuation bytes.
        for i in 2..len.get() as usize {
            match bytes.get(i) {
                Some(0x80..=0xBF) => (),
                // Counts all malformed continuation bytes as invalid.
                Some(_) => {
                    return CodePointUrl::InvalidUtf8 {
                        // SAFETY: `i` is guaranteed to be >= 2 in the loop range
                        invalid: unsafe { std::num::NonZeroU8::new(i as u8).unwrap_unchecked() },
                    };
                },
                None => return CodePointUrl::Truncated,
            }
        }

        // == Step 3 ===============================================================================
        //
        // Check if the code point is a URL code point.
        //
        // =========================================================================================

        // It is simpler to check for non-URL code points.
        let excluded = match len.get() {
            // All code points before U+00A0
            2 => *b0 == 0xC2 && *b1 <= 0x9F,

            3 => {
                let b2 = bytes[2];

                // All 3-byte non-character code points: U+FDD0 to U+FDEF inclusive, U+FFFE and
                // U+FFFF.
                *b0 == 0xEF
                    && ((*b1 == 0xB7 && (0x90..=0xAF).contains(&b2))
                        || (*b1 == 0xBF && (0xBE..=0xBF).contains(&b2)))
            },

            _ => {
                let b2 = bytes[2];
                let b3 = bytes[3];

                // All 4-byte non-character code points: U+FFFE and U+FFFF at the end of each utf8
                // plane. These take the form U+nFFFE and U+nFFFE, with all low 16 bits of the code
                // point set to 1, except for the last.
                //
                //                       low bytes
                //                  ┌────────┬────────┐
                //               ┌──┤   ┌────┤   ┌────┤
                // ┌────────┬────▼──▼┬──▼────▼┬──▼────▼┐
                // │11110000│10011111│10111111│1011111x│
                // └────────┴────────┴────────┴────────┘
                //     b0       b1       b2       b3

                (*b1 & 0x0F) == 0x0F && b2 == 0xBF && b3 >= 0xBE
            },
        };

        if excluded {
            CodePointUrl::Invalid { len }
        } else {
            CodePointUrl::Valid { len }
        }
    }

    /// Reference naive url code point implementation for use in formal proving.
    #[cfg(test)]
    pub(super) fn url_code_point_reference(bytes: &[u8]) -> CodePointUrl {
        let head = &bytes[..bytes.len().min(4)];

        let c = match std::str::from_utf8(head) {
            Ok(s) => match s.chars().next() {
                Some(c) => c,
                None => return CodePointUrl::Truncated,
            },
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                if valid_up_to > 0 {
                    std::str::from_utf8(&head[..valid_up_to])
                        .unwrap()
                        .chars()
                        .next()
                        .unwrap()
                } else {
                    match e.error_len() {
                        Some(len) => {
                            return CodePointUrl::InvalidUtf8 {
                                invalid: nonzero!(len as u8),
                            };
                        },
                        None => return CodePointUrl::Truncated,
                    }
                }
            },
        };

        let len = c.len_utf8() as u8;
        let code_point = c as u32;

        let is_url_code_point = match code_point {
            // ASCII code points
            0..=0x7F => (ASCII_URL_CODE_POINT >> code_point) & 1 == 1,

            // Under U+00A0 & 3-byte non-characters
            0x80..=0x9F | 0xFDD0..=0xFDEF => false,

            // Other non-characters
            _ => code_point & 0xFFFE != 0xFFFE,
        };

        if is_url_code_point {
            CodePointUrl::Valid {
                len: nonzero!(len as u8),
            }
        } else {
            CodePointUrl::Invalid {
                len: nonzero!(len as u8),
            }
        }
    }
}

#[cfg(test)]
mod test {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[test]
    fn url_join_on_path_full() {
        const BASE: &str = "http://example.com?base-query#base-fragment";

        let mut backing_base = [0; 128];
        let (base, mut err_base) = Url::new(BASE.as_bytes(), &mut backing_base).unwrap();

        assert_eq!(err_base.next(), None);

        const URL: &str = "path/to/file?query#fragment";

        let mut backing_url = [0; 128];
        let (url, mut err_url) = base.join(URL.as_bytes(), &mut backing_url).unwrap();

        assert_eq!(err_url.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/path/to/file");
        assert_utf8_eq!(url.query(), b"query");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://example.com/path/to/file?query#fragment"
        );
    }

    #[test]
    fn url_join_on_path_extend() {
        const BASE: &str = "http://example.com/path/to/file?base-query#base-fragment";

        let mut backing_base = [0; 128];
        let (base, mut err_base) = Url::new(BASE.as_bytes(), &mut backing_base).unwrap();

        assert_eq!(err_base.next(), None);

        const URL: &str = "folder/journal?query#fragment";

        let mut backing_url = [0; 128];
        let (url, mut err_url) = base.join(URL.as_bytes(), &mut backing_url).unwrap();

        assert_eq!(err_url.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/path/to/folder/journal");
        assert_utf8_eq!(url.query(), b"query");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://example.com/path/to/folder/journal?query#fragment"
        );
    }

    #[test]
    fn url_join_on_query() {
        const BASE: &str = "http://example.com/path/to/file?base-query#base-fragment";

        let mut backing_base = [0; 128];
        let (base, mut err_base) = Url::new(BASE.as_bytes(), &mut backing_base).unwrap();

        assert_eq!(err_base.next(), None);

        const URL: &str = "?query#fragment";

        let mut backing_url = [0; 128];
        let (url, mut err_url) = base.join(URL.as_bytes(), &mut backing_url).unwrap();

        assert_eq!(err_url.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/path/to/file");
        assert_utf8_eq!(url.query(), b"query");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://example.com/path/to/file?query#fragment"
        );
    }

    #[test]
    fn url_join_on_fragment() {
        const BASE: &str = "http://example.com/path/to/file?query#base-fragment";

        let mut backing_base = [0; 128];
        let (base, mut err_base) = Url::new(BASE.as_bytes(), &mut backing_base).unwrap();

        assert_eq!(err_base.next(), None);

        const URL: &str = "#fragment";

        let mut backing_url = [0; 128];
        let (url, mut err_url) = base.join(URL.as_bytes(), &mut backing_url).unwrap();

        assert_eq!(err_url.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/path/to/file");
        assert_utf8_eq!(url.query(), b"query");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://example.com/path/to/file?query#fragment"
        );
    }

    #[test]
    fn url_parse_full_special() {
        const URL: &str = "http://user:password@example.com:123/path/to/file?query#fragment";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"user");
        assert_utf8_eq!(url.password(), b"password");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), Some(123));
        assert_utf8_eq!(url.path(), b"/path/to/file");
        assert_utf8_eq!(url.query(), b"query");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://user:password@example.com:123/path/to/file?query#fragment"
        );
    }

    #[test]
    fn url_parse_full_nonspecial_authority() {
        const URL: &str =
            "scheme://user:password@example.com:123/\\path\\to\\file?query\\nonspecial#fragment";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // `\` is not a valid URL code point, even if it is not part of the PATH percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"user");
        assert_utf8_eq!(url.password(), b"password");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), Some(123));
        assert_utf8_eq!(url.path(), b"/\\path\\to\\file");
        assert_utf8_eq!(url.query(), b"query\\nonspecial");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "scheme://user:password@example.com:123/\\path\\to\\file?query\\nonspecial#fragment"
        );
    }

    #[test]
    fn url_parse_full_nonspecial_path() {
        const URL: &str =
            "scheme:/user:password@example.com:123/\\path\\to\\file?query\\nonspecial#fragment";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // `\` is not a valid URL code point, even if it is not part of the PATH percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(
            url.path(),
            b"/user:password@example.com:123/\\path\\to\\file"
        );
        assert_utf8_eq!(url.query(), b"query\\nonspecial");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "scheme:/user:password@example.com:123/\\path\\to\\file?query\\nonspecial#fragment"
        );
    }

    #[test]
    fn url_parse_full_nonspecial_opaque() {
        const URL: &str =
            "scheme:user : password@example.com:123/path/to/file ?query\\nonspecial#fragment";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // ` ` is a valid code point but its use in opaque paths still raises an InvalidURLUnit
        // error, even if it is not part of the PATH percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(
            url.path(),
            b"user : password@example.com:123/path/to/file%20"
        );
        assert_utf8_eq!(url.query(), b"query\\nonspecial");
        assert_utf8_eq!(url.fragment(), b"fragment");

        assert_eq!(
            format!("{url}"),
            "scheme:user : password@example.com:123/path/to/file%20?query\\nonspecial#fragment"
        );
    }

    #[test]
    fn url_parse_userinfo_special() {
        const URL: &str = "http://user:password@example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"user");
        assert_utf8_eq!(url.password(), b"password");

        assert_eq!(format!("{url}"), "http://user:password@example.com/");
    }

    #[test]
    fn url_parse_userinfo_nonspecial() {
        const URL: &str = "scheme://\\user:\\password\\@example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"%5Cuser");
        assert_utf8_eq!(url.password(), b"%5Cpassword%5C");

        assert_eq!(
            format!("{url}"),
            "scheme://%5Cuser:%5Cpassword%5C@example.com/"
        );
    }

    #[test]
    fn url_parse_userinfo_only_user() {
        const URL: &str = "http://user@example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"user");
        assert_utf8_eq!(url.password(), b"");

        assert_eq!(format!("{url}"), "http://user@example.com/");
    }

    #[test]
    fn url_parse_userinfo_only_password() {
        const URL: &str = "http://:password@example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"password");

        assert_eq!(format!("{url}"), "http://:password@example.com/");
    }

    #[test]
    fn url_parse_userinfo_with_encoded() {
        const URL: &str = "http://user:password@@example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"user");
        assert_utf8_eq!(url.password(), b"password%40");

        assert_eq!(format!("{url}"), "http://user:password%40@example.com/");
    }

    #[test]
    fn url_parse_host_domain_special() {
        const URL: &str = "http://example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_host_domain_nonspecial_full() {
        const URL: &str = "scheme://example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(format!("{url}"), "scheme://example.com/");
    }

    #[test]
    fn url_parse_host_domain_nonspecial_empty() {
        // Non-special schemes allow for an empty host
        const URL: &str = "scheme://";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"");
        assert_utf8_eq!(url.path(), b"/");

        assert_eq!(format!("{url}"), "scheme:///");
    }

    #[test]
    fn url_parse_port_special() {
        const URL: &str = "http://example.com:123";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(format!("{url}"), "http://example.com:123/");

        assert_eq!(url.port(), Some(123));
    }

    #[test]
    fn url_parse_port_nonspecial() {
        const URL: &str = "scheme://example.com:123";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(format!("{url}"), "scheme://example.com:123/");

        assert_eq!(url.port(), Some(123));
    }

    #[test]
    fn url_parse_port_default_ftp() {
        const URL: &str = "ftp://example.com:21";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"ftp");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "ftp://example.com/");
    }

    #[test]
    fn url_parse_port_default_http() {
        const URL: &str = "http://example.com:80";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_port_default_ws() {
        const URL: &str = "ws://example.com:80";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"ws");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "ws://example.com/");
    }

    #[test]
    fn url_parse_port_default_https() {
        const URL: &str = "https://example.com:443";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"https");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "https://example.com/");
    }

    #[test]
    fn url_parse_port_default_wss() {
        const URL: &str = "wss://example.com:443";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"wss");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "wss://example.com/");
    }

    #[test]
    fn url_parse_port_empty() {
        const URL: &str = "http://example.com:";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");

        assert_eq!(url.port(), None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_path_special_absolute() {
        const URL: &str = "http://example.com/Documents/code/rust/http/examples/client.rs";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/Documents/code/rust/http/examples/client.rs");

        assert_eq!(
            format!("{url}"),
            "http://example.com/Documents/code/rust/http/examples/client.rs"
        );
    }

    #[test]
    fn url_parse_path_special_relative() {
        const URL: &str = "http://example.com/.././Documents/code/python/./../rust/http/src/../examples/server/../client.rs";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/Documents/code/rust/http/examples/client.rs");

        assert_eq!(
            format!("{url}"),
            "http://example.com/Documents/code/rust/http/examples/client.rs"
        );
    }

    #[test]
    fn url_parse_path_special_emtpy_segments() {
        const URL: &str = "http://example.com//..//..//../";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/");

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_path_special_strange_solidus() {
        const URL: &str = "http://example.com\\/\\////\\\\\\///////";

        let mut backing = [0; 512];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // `\` is not a valid URL code point, even if it is not part of the PATH percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidReverseSolidus)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/////////////////");

        assert_eq!(format!("{url}"), "http://example.com/////////////////");
    }

    #[test]
    fn url_parse_path_nonspecial() {
        const URL: &str = "scheme://example.com/\\cat\\pictures\\";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // `\` is not a valid URL code point, even if it is not part of the PATH percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/\\cat\\pictures\\");

        assert_eq!(format!("{url}"), "scheme://example.com/\\cat\\pictures\\");
    }

    #[test]
    fn url_parse_path_opaque_absolute() {
        const URL: &str = "scheme:Documents/code/rust/http/examples/client.rs";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"Documents/code/rust/http/examples/client.rs");

        assert_eq!(
            format!("{url}"),
            "scheme:Documents/code/rust/http/examples/client.rs"
        );
    }

    #[test]
    fn url_parse_path_opaque_relative() {
        const URL: &str =
            "scheme:.././Documents/code/python/./../rust/http/src/../examples/server/../client.rs";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        // Opaque path URLs do not support relative paths or path shortening
        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(
            url.path(),
            b".././Documents/code/python/./../rust/http/src/../examples/server/../client.rs"
        );

        assert_eq!(
            format!("{url}"),
            "scheme:.././Documents/code/python/./../rust/http/src/../examples/server/../client.rs"
        );
    }

    #[test]
    fn url_parse_query_special() {
        const URL: &str = "http://example.com?name=cat.txt";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/");
        assert_utf8_eq!(url.query(), b"name=cat.txt");

        assert_eq!(format!("{url}"), "http://example.com/?name=cat.txt");
    }

    #[test]
    fn url_parse_query_nonspecial() {
        const URL: &str = "scheme://example.com?\\name=\\cat.txt";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // `\` is not a valid URL code point, even if it is not part of the QUERY percent-encode set.
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"scheme");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/");
        assert_utf8_eq!(url.query(), b"\\name=\\cat.txt");

        assert_eq!(format!("{url}"), "scheme://example.com/?\\name=\\cat.txt");
    }

    #[test]
    fn url_parse_fragment_special() {
        const URL: &str = "http://example.com#about";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/");
        assert_utf8_eq!(url.query(), b"");
        assert_utf8_eq!(url.fragment(), b"about");

        assert_eq!(format!("{url}"), "http://example.com/#about");
    }

    #[test]
    fn url_percent_encoded_valid() {
        const URL: &str = "http://example.com/pictures/of%20my%20cat/";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pictures/of%20my%20cat/");

        assert_eq!(
            format!("{url}"),
            "http://example.com/pictures/of%20my%20cat/"
        );
    }

    #[test]
    fn url_percent_encoded_empty() {
        const URL: &str = "http://example.com/pictures/of% my% cat/";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pictures/of%%20my%%20cat/");

        assert_eq!(
            format!("{url}"),
            "http://example.com/pictures/of%%20my%%20cat/"
        );
    }

    #[test]
    fn url_percent_encoded_invalid() {
        const URL: &str = "http://example.com/pictures/of%Azmy%Zacat/";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pictures/of%Azmy%Zacat/");

        assert_eq!(
            format!("{url}"),
            "http://example.com/pictures/of%Azmy%Zacat/"
        );
    }

    #[test]
    fn url_code_point_percent_encode_non_url_code_points() {
        const URL: &str = "http://example.com/pictures/of my cat/";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pictures/of%20my%20cat/");

        assert_eq!(
            format!("{url}"),
            "http://example.com/pictures/of%20my%20cat/"
        );
    }

    #[test]
    fn url_code_point_percent_encode_utf8_invalid() {
        // http://example.com/pi\{0x80}c\{0xC0}tu\{0xF5}res
        const URL: &[u8] = &[
            104, 116, 116, 112, 58, 47, 47, 101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109, 47,
            112, 105, 0x80, 99, 0xC0, 116, 117, 0xF5, 114, 101, 115,
        ];

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL, &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pi%EF%BF%BDc%EF%BF%BDtu%EF%BF%BDres");

        assert_eq!(
            format!("{url}"),
            "http://example.com/pi%EF%BF%BDc%EF%BF%BDtu%EF%BF%BDres"
        );
    }

    #[test]
    fn url_code_point_percent_encode_utf8_truncated() {
        // http://example.com/pictures\{0xF4}
        const URL: &[u8] = &[
            104, 116, 116, 112, 58, 47, 47, 101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109, 47,
            112, 105, 99, 116, 117, 114, 101, 115, 0xF4,
        ];

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL, &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme(), b"http");
        assert_utf8_eq!(url.username(), b"");
        assert_utf8_eq!(url.password(), b"");
        assert_utf8_eq!(url.host(), b"example.com");
        assert_eq!(url.port(), None);
        assert_utf8_eq!(url.path(), b"/pictures%EF%BF%BD");

        assert_eq!(format!("{url}"), "http://example.com/pictures%EF%BF%BD");
    }

    #[test]
    fn url_trim_c0_control_or_space_front() {
        const URL: &str = "\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{9}\u{10}\u{11}\u{12}\u{13}\u{14}\u{15}\u{16}\u{17}\u{18}\u{19}\u{20}http://example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_trim_c0_control_or_space_back() {
        const URL: &str = "http://example.com\u{20}\u{19}\u{18}\u{17}\u{16}\u{15}\u{14}\u{13}\u{12}\u{11}\u{10}\u{9}\u{8}\u{7}\u{6}\u{5}\u{4}\u{3}\u{2}\u{1}\u{0}";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_skip_ascii_tab_or_newline() {
        let url_input = "http://username:password@example.com:123/path/to/file?query#fragment"
            .chars()
            .fold(String::new(), |mut acc, c| {
                acc.push_str("\t\r\n");
                acc.push(c);
                acc
            });

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(url_input.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_eq!(
            format!("{url}"),
            "http://username:password@example.com:123/path/to/file?query#fragment"
        );
    }

    #[test]
    #[ignore = "not implemented"]
    fn url_scheme_missing_following_solidus_file() {
        const URL: &str = "file:c:/my-secret-folder";

        let mut backing = [0; 128];
        let (_, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
        assert_eq!(validation_errors.next(), None);
    }

    #[test]
    #[ignore = "not implemented"]
    fn url_scheme_missing_following_solidus_special_non_file() {
        const URL: &str = "http:example.com";

        let mut backing = [0; 128];
        let (_, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
        assert_eq!(validation_errors.next(), None);
    }

    #[test]
    fn url_scheme_missing_expected_double_slash_non_file() {
        const URL: &str = "http://\\\\/\\///example.com";

        let mut backing = [0; 128];
        let (_, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
        assert_eq!(validation_errors.next(), None);
    }

    #[test]
    fn url_err_overflow() {
        const URL: &str = "example.com";

        let mut backing = [0; 1];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::OOM);
    }

    #[test]
    fn url_err_empty_host_after_userinfo() {
        const URL: &str = "http://user:password@";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::HostMissing);
    }

    #[test]
    fn url_err_empty_host_domain() {
        const URL: &str = "http://";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::HostMissing);
    }

    #[test]
    fn url_err_empty_host_domain_query() {
        const URL: &str = "http://?query";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::HostMissing);
    }

    #[test]
    fn url_err_port_out_of_range() {
        const URL: &str = "http://example.com:70000";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::PortOutOfRange);
    }

    #[test]
    fn url_err_port_invalid() {
        const URL: &str = "http://example.com:7z";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::PortInvalid);
    }

    #[test]
    #[should_panic = "assertion `left != right` failed\n  left: []\n right: []"]
    fn url_err_empty_backing() {
        const URL: &str = "example.com";

        let mut backing = [0; 0];
        let _ = Url::new(URL.as_bytes(), &mut backing);
    }

    #[test]
    fn utf8_url_code_point_matches_reference() {
        let mut backing = [0; 4];

        for code_point in 0..0x10_FFFF {
            let Some(c) = char::from_u32(code_point) else {
                continue;
            };

            let bytes = c.encode_utf8(&mut backing);

            let actual = utf8::url_code_point(bytes.as_bytes());
            let expected = utf8::url_code_point_reference(bytes.as_bytes());

            assert_eq!(actual, expected);
        }
    }

    #[test]
    #[cfg_attr(kani, kani::proof)]
    #[cfg_attr(kani, kani::unwind(5))]
    fn utf8_url_code_point_harness() {
        bolero::check!().with_max_len(4).for_each(|bytes| {
            let actual = utf8::url_code_point(bytes);
            let expected = utf8::url_code_point_reference(bytes);

            assert_eq!(actual, expected);
        });
    }

    // #[test]
    // fn utf8_url_code_point_harness_failure() {
    //     let bytes = &[0xf6];
    //
    //     let actual = utf8::url_code_point(bytes);
    //     let expected = utf8::url_code_point_reference(bytes);
    //
    //     assert_eq!(actual, expected);
    // }
}
