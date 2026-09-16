//! [`Url`] parsing utilities.

mod buffer;
mod error;

use buffer::UrlBuffer;
pub use error::*;
use macro_util::prelude::*;

use super::Url;
use super::percent;

impl<'data> Url<'data> {
    /// Tries to parse a stream of network bytes into a URL.
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
    /// assert_utf8_eq!(url.scheme, b"http");
    /// assert_utf8_eq!(url.host, b"example.com");
    /// assert_eq!(url.port, Some(123));
    /// assert_utf8_eq!(url.path, b"/path/to/file");
    /// assert_utf8_eq!(url.query, b"query");
    /// assert_utf8_eq!(url.fragment, b"fragment");
    /// ```
    pub fn new(
        mut bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, ValidationErrorIter), Error> {
        assert_ne!(backing, []);

        let mut error_bitset = ValidationErrorBitSet::new();
        let buffer = UrlBuffer::new(backing);

        // == input sanitization ===================================================================
        //
        // Removes leading and trailing C0 control or space code points.
        //
        // =========================================================================================

        c0_control_or_space::parse(c0_control_or_space::Context {
            cursor: &mut bytes,
            error_bitset: &mut error_bitset,
        });

        // == section parsing ======================================================================
        //
        // Iterates over the input byte string and parses out the various URL segments. This is the
        // entry point for further parsing methods to be invoked deeper into the call stack.
        //
        // =========================================================================================

        scheme::parse(scheme::Context {
            cursor: &mut bytes,
            buffer,
            error_bitset: &mut error_bitset,
        })
    }
}

mod matchers {
    /// See the [URL standard], C0 control or space.
    ///
    /// > _"A C0 control or space is a [C0 control] or U+0020 SPACE."_
    ///
    /// [URL standard]: https://infra.spec.whatwg.org/#c0-control-or-space
    /// [C0 control]: https://infra.spec.whatwg.org/#c0-control
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
    pub(super) fn ascii_alpha(c: u8) -> bool {
        c.is_ascii_alphabetic()
    }
}

mod segment {
    #[repr(transparent)]
    pub(super) struct Scheme(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Username(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Password(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Host(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Port(pub Option<u16>);

    #[repr(transparent)]
    pub(super) struct Path(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Query(pub std::ops::Range<usize>);

    #[repr(transparent)]
    pub(super) struct Fragment(pub std::ops::Range<usize>);
}

macro_rules! ascii_tab_or_newline {
    () => {
        b'\t' | b'\n' | b'\r'
    };
}

mod c0_control_or_space {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// # C0 control or space sanitization
    ///
    /// Remove any leading and trailing [C0 control or space] from input.
    ///
    /// # Validation Errors
    ///
    /// [`InvalidURLUnit`]: If the input contains any leading or trailing [C0 control or space]
    ///
    /// [C0 control of space]: https://infra.spec.whatwg.org/#c0-control-or-space
    /// [`InvalidURLUnit`]: ValidationError::InvalidURLUnit
    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input>(
        cursor: &'parsing mut &'input [u8],
        error_bitset: &'parsing mut ValidationErrorBitSet,
    ) {
        // Leading C0 control or space
        if let Some(c0_first) = cursor.first()
            && matchers::c0_control_or_space(*c0_first)
        {
            error_bitset.add(ValidationError::InvalidURLUnit);
            *cursor = &cursor[1..];

            while let Some(c0_continuation) = cursor.first()
                && matchers::c0_control_or_space(*c0_continuation)
            {
                *cursor = &cursor[1..];
            }
        }

        // Trailing C0 control or space
        if let Some(c0_last) = cursor.last()
            && matchers::c0_control_or_space(*c0_last)
        {
            error_bitset.add(ValidationError::InvalidURLUnit);

            let len = cursor.len();
            *cursor = &cursor[..len - 1];

            while let Some(c0_continuation) = cursor.last()
                && matchers::c0_control_or_space(*c0_continuation)
            {
                let len_continuation = cursor.len();
                *cursor = &cursor[..len_continuation - 1];
            }
        }
    }
}

mod scheme {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// # [Scheme state]
    ///
    /// Tries to parse a [`Url`]'s scheme, if there is any, otherwise falls back to the "no scheme"
    /// state. This is the entry point for parsing further segments.
    ///
    /// [Scheme state]: https://url.spec.whatwg.org#scheme-start-state
    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        mut buffer: UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
    ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
        if let Some(first) = cursor.first()
            && matchers::ascii_alpha(*first)
        {
            buffer.push(first.to_ascii_lowercase())?;
            *cursor = &cursor[1..];

            while !cursor.is_empty() {
                let c = cursor[0];

                match c {
                    ascii_tab_or_newline!() => {
                        error_bitset.add(ValidationError::InvalidURLUnit);

                        let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                        *cursor = &cursor[skip..];
                    },

                    // Only the first character in a scheme must be strictly `ascii_alpha`. Scheme
                    // characters after that may be ASCII alphanumeric, U+002B (+), U+002D (-), or
                    // U+002E (.).
                    b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => {
                        buffer.push(c)?;
                        *cursor = &cursor[1..];
                    },

                    // Input is normalized, only lowercase characters are pushed to the final buffer
                    b'A'..=b'Z' => {
                        buffer.push(c.to_ascii_lowercase())?;
                        *cursor = &cursor[1..];
                    },

                    // End of scheme
                    b':' => {
                        buffer.push(b':')?;
                        *cursor = &cursor[1..];
                        break;
                    },

                    // Invalid character, scheme error.
                    _ => {
                        *cursor = &cursor[1..];
                        break;
                    },
                }
            }

            // This is safe to index as we have already pushed at least one character to `buffer`.
            if buffer.as_ref()[buffer.len() - 1] == b':' {
                let scheme = segment::Scheme(0..buffer.len() - 1);

                #[cfg(test)]
                let _scheme = str::from_utf8(&buffer[scheme.0.clone()]).unwrap_or_default();

                #[cfg(test)]
                let _remaining = str::from_utf8(cursor).unwrap_or_default();

                match &buffer[scheme.0.clone()] {
                    b"file" => todo!("file state: https://url.spec.whatwg.org/#file-state"),

                    b"ftp" => {
                        return scheme::special::parse(scheme::special::Context {
                            cursor,
                            buffer,
                            error_bitset,
                            scheme,
                            default_scheme_port: 21,
                        });
                    },

                    b"http" | b"ws" => {
                        return scheme::special::parse(scheme::special::Context {
                            cursor,
                            buffer,
                            error_bitset,
                            scheme,
                            default_scheme_port: 80,
                        });
                    },

                    b"https" | b"wss" => {
                        return scheme::special::parse(scheme::special::Context {
                            cursor,
                            buffer,
                            error_bitset,
                            scheme,
                            default_scheme_port: 443,
                        });
                    },

                    // non-special scheme
                    _ => todo!(),
                }
            }
        }

        // == No scheme state ======================================================================
        //
        // Set buffer to the empty string and start over (from the first code point in input).
        //
        // =========================================================================================

        buffer.clear();

        todo!()
    }

    pub(super) mod special {
        #[allow(clippy::wildcard_imports)]
        use super::*;

        /// # [Special relative or authority state]
        ///
        /// Parses out each [`Url`] segment after a special scheme.
        ///
        /// ## Special scheme
        ///
        /// A special scheme is an [ASCII string] that is listed in the first column of the
        /// following table. The default port for a special scheme is listed in the second column on
        /// the same row. The default port for any other [ASCII string] is null.
        ///
        /// | Special scheme | Default port |
        /// |----------------|--------------|
        /// | "ftp"          | 21           |
        /// | "file"         | null         |
        /// | "http"         | 80           |
        /// | "https"        | 443          |
        /// | "ws"           | 80           |
        /// | "wss"          | 443          |
        ///
        /// # Validation Errors
        ///
        /// [`SpecialSchemeMissingFollowingSolidus`] if scheme is not followed by two U+002F (/) or
        /// is followed by too many U+002F (/) or  U+005C (\) code points.
        ///
        /// # Errors
        ///
        /// Surfaces a parsing [`Error`] if any of the underlying [`userinfo`] or [`host_and_port`]
        /// parsers error out.
        ///
        /// [Special relative or authority state]: https://url.spec.whatwg.org/#special-relative-or-authority-state
        /// [ASCII string]: https://infra.spec.whatwg.org/#ascii-string
        /// [`SpecialSchemeMissingFollowingSolidus`]: ValidationError::SpecialSchemeMissingFollowingSolidus
        /// [`userinfo`]: userinfo::parse
        /// [`host_and_port`]: host_and_port::parse
        #[inline]
        #[macro_derive::context]
        pub(crate) fn parse<'parsing, 'input, 'output>(
            cursor: &'parsing mut &'input [u8],
            mut buffer: UrlBuffer<'output>,
            error_bitset: &'parsing mut ValidationErrorBitSet,
            scheme: segment::Scheme,
            default_scheme_port: u16,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            // == Special authority slashes state ======================================
            //
            // Ensure the scheme is followed by two U+002F (/).
            //
            // =========================================================================

            let mut sequential_solidus = 0;
            while !cursor.is_empty() {
                match *cursor {
                    [ascii_tab_or_newline!(), ..] => {
                        error_bitset.add(ValidationError::InvalidURLUnit);

                        let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                        *cursor = &cursor[skip..];
                    },

                    [b'/', ..] => {
                        *cursor = &cursor[1..];
                        sequential_solidus += 1;

                        if sequential_solidus >= 2 {
                            break;
                        }
                    },

                    _ => {
                        error_bitset.add(ValidationError::SpecialSchemeMissingFollowingSolidus);

                        todo!("relative state: https://url.spec.whatwg.org/#relative-state")
                    },
                }
            }

            // Special authority ignore slashes state ==================================
            //
            //  The specs aren't very clear on what happens in case an invalid
            //  combination of slashes precedes the authority. However, based on the
            //  rust_url source code, it seems the correct approach is to ignore ALL
            //  slashes following the scheme.
            //
            // https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L451-L457
            //
            // =========================================================================

            while !cursor.is_empty() {
                let c = cursor[0];

                match c {
                    ascii_tab_or_newline!() => {
                        error_bitset.add(ValidationError::InvalidURLUnit);

                        let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                        *cursor = &cursor[skip..];
                    },

                    b'/' | b'\\' => {
                        error_bitset.add(ValidationError::SpecialSchemeMissingFollowingSolidus);
                        *cursor = &cursor[1..];
                    },

                    _ => break,
                }
            }

            buffer.push_str(b"//")?;

            #[cfg(test)]
            let _remaining_userinfo = str::from_utf8(cursor).unwrap_or_default();

            let (username, password) = userinfo::parse(userinfo::Context {
                cursor,
                buffer: &mut buffer,
                error_bitset,
                scheme: &scheme,
            })?;

            #[cfg(test)]
            let _username = str::from_utf8(&buffer[username.0.clone()]).unwrap_or_default();
            #[cfg(test)]
            let _password = str::from_utf8(&buffer[password.0.clone()]).unwrap_or_default();
            #[cfg(test)]
            let _remaining_host = str::from_utf8(cursor).unwrap_or_default();

            let (host, port) = host_and_port::parse(host_and_port::Context {
                cursor,
                buffer: &mut buffer,

                error_bitset,

                username: &username,
                password: &password,
                default_scheme_port,
            })?;

            #[cfg(test)]
            let _host = str::from_utf8(&buffer[host.0.clone()]).unwrap_or_default();

            let (path, query, fragment) = path::parse(path::Context {
                cursor,
                buffer: &mut buffer,
                error_bitset,
            })?;

            #[cfg(test)]
            let _path = str::from_utf8(&buffer[path.0.clone()]).unwrap_or_default();
            #[cfg(test)]
            let _query = str::from_utf8(&buffer[query.0.clone()]).unwrap_or_default();
            #[cfg(test)]
            let _fragment = str::from_utf8(&buffer[fragment.0.clone()]).unwrap_or_default();

            let backing = buffer.into_inner();

            Ok((
                Url {
                    backing,

                    scheme: &backing[scheme.0],
                    username: &backing[username.0],
                    password: &backing[password.0],
                    host: &backing[host.0],
                    port: port.0,
                    path: &backing[path.0],
                    query: &backing[query.0],
                    fragment: &backing[fragment.0],
                },
                error_bitset.iter(),
            ))
        }
    }
}

mod userinfo {
    #[allow(clippy::wildcard_imports)]
    use super::*;

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
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
        scheme: &'parsing segment::Scheme,
    ) -> Result<(segment::Username, segment::Password), Error> {
        // == authority state ======================================================
        //
        // https://url.spec.whatwg.org/#authority-state
        //
        // The spec has a really roundabout way of wording this but essentially all
        // we need to do is linearly parse through the userinfo section, the end of
        // which is determined by the LAST U+0040 (@) code point. Any character
        // before that gets percent-encoded according to the userinfo percent-encode
        // set. Any character before U+003A (:) is considered to be part of the
        // username, and any character after is considered to be part of the user's
        // password. We only skip the first U+003A (:) and percent-encode any other
        // occurrences of it, even if those are not percent-encoded. If we encounter
        // any other section delimiter but the remaining host section is empty AND
        // we have seen a terminating U+0040 (@) code point, then we return a
        // host-missing error.
        //
        // =========================================================================

        // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
        // parse them again.
        if let Some(c) = cursor.first()
            && matches!(c, ascii_tab_or_newline!())
        {
            let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
            *cursor = &cursor[skip..];
        }

        #[allow(suspicious_double_ref_op)]
        let checkpoint = cursor.clone();

        let mut at_sign = None;
        let mut char_count_authority = 0;

        // Parsing has to take place in two steps:
        //
        // 1. First, we iterate over the full range of authority characters to find
        //    the terminating userinfo U+0040 (@) delimiter.
        //
        // 2. We then iterate over all code points before the terminating userinfo
        //    U+0040 (@) delimiter and encode them into the result buffer.
        //
        // This two step process is necessary as we don't want to parse any host
        // components yet, but we still need to find the terminating U+0040 (@),
        // userinfo delimiter which is only bounded by the end of the host segment
        // of the url.
        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                    char_count_authority += skip;
                },

                // EOF code point is implied in the below check
                b'/' | b'\\' | b'?' | b'#' => {
                    break;
                },

                b'@' => {
                    error_bitset.add(ValidationError::InvalidCredentials);
                    at_sign = Some(char_count_authority);
                    *cursor = &cursor[1..];
                    char_count_authority += 1;
                },
                _ => {
                    *cursor = &cursor[1..];
                    char_count_authority += 1;
                },
            }
        }

        *cursor = checkpoint;

        let mut char_count_userinfo = match at_sign {
            Some(n) => {
                // We only exit the above loop if we have  reached the end of the
                // host segment of the url or the end of the url itself. This means
                // that if there are no more characters after the terminating
                // userinfo U+0040 (@) delimiter then the host must be empty.
                //
                // The specs word this somewhat more confusingly as:
                //
                // > If atSignSeen is true and buffer is the empty string, host-
                // > missing validation error, return failure.
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
        let userinfo_start = scheme.0.end + 3;
        let mut userinfo_stop = userinfo_start;
        let mut password_token = None;

        // Here is where we actually parse the userinfo.
        while char_count_userinfo > 0 {
            let c = cursor[0];

            *cursor = &cursor[1..];
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
                                password_token = Some(buffer.push(b':')? - 1);
                            }
                        },
                    }
                },

                _ => {
                    userinfo_stop = buffer.push_encode_byte(c, percent::USERINFO)?;
                },
            }
        }

        let (username, password) = match password_token {
            Some(n) => (userinfo_start..n, n + 1..userinfo_stop),
            None => (userinfo_start..userinfo_stop, userinfo_stop..userinfo_stop),
        };

        // username and password can still be empty in case userinfo is `:@` (empty username and
        // empty password separated by the password token).
        if !username.is_empty() || !password.is_empty() {
            // SAFETY: this is either the terminating userinfo U+0040 (@) delimiter or an ascii tab
            // or newline so cursor is guaranteed to be non-empty.
            if matches!(cursor[0], ascii_tab_or_newline!()) {
                let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                *cursor = &cursor[skip..];
            }

            // We need to skip over the terminating userinfo U+0040 (@) delimiter again as we
            // have reset the cursor.
            *cursor = &cursor[1..];

            buffer.push(b'@')?;
        }

        Ok((segment::Username(username), segment::Password(password)))
    }
}

mod host_and_port {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// Parses out a [`Url`]'s [`host`] and [`port`] components. This is done to optimize branching
    /// so that the presence of a port is only ever checked once.
    ///
    /// # Errors
    ///
    /// Surfaces a parsing [`Error`] if any of the underlying [`host`] or [`port`] parsers error
    /// out.
    ///
    /// [`host`]: host::parse
    /// [`port`]: port::parse
    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
        username: &'parsing segment::Username,
        password: &'parsing segment::Password,
        default_scheme_port: u16,
    ) -> Result<(segment::Host, segment::Port), Error> {
        // == hostname state =======================================================
        //
        // https://url.spec.whatwg.org/#hostname-state
        //
        // =========================================================================

        // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
        // parse them again.
        if let Some(c) = cursor.first()
            && matches!(c, ascii_tab_or_newline!())
        {
            let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
            *cursor = &cursor[skip..];
        }

        #[allow(suspicious_double_ref_op)]
        let checkpoint = cursor.clone();

        let mut inside_brackets = false;
        let mut char_count_hostname = 0;

        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                    char_count_hostname += skip;
                },

                b':' if !inside_brackets => {
                    *cursor = checkpoint;

                    #[cfg(test)]
                    let _remaining_host = str::from_utf8(cursor).unwrap_or_default();

                    let host = host::parse(host::Context {
                        cursor,
                        buffer,
                        username,
                        password,
                        char_count_hostname,
                    })?;

                    #[cfg(test)]
                    let _remaining_port = str::from_utf8(cursor).unwrap_or_default();

                    // We need to skip the port delimiter again as we reset the cursor.
                    *cursor = &cursor[1..];

                    let port = port::parse(port::Context {
                        cursor,
                        buffer,
                        error_bitset,
                        default_scheme_port,
                    })?;

                    return Ok((host, port));
                },

                b'/' | b'\\' | b'?' | b'#' => {
                    break;
                },

                b'[' => {
                    inside_brackets = true;
                    char_count_hostname += 1;
                    *cursor = &cursor[1..];
                },

                b']' => {
                    inside_brackets = false;
                    char_count_hostname += 1;
                    *cursor = &cursor[1..];
                },

                _ => {
                    char_count_hostname += 1;
                    *cursor = &cursor[1..];
                },
            }
        }

        *cursor = checkpoint;

        #[cfg(test)]
        let _remaining_host = str::from_utf8(cursor).unwrap_or_default();

        let host = host::parse(host::Context {
            cursor,
            buffer,

            username,
            password,
            char_count_hostname,
        })?;

        Ok((host, segment::Port(None)))
    }
}

mod host {
    #[allow(clippy::wildcard_imports)]
    use super::*;

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
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        username: &'parsing segment::Username,
        password: &'parsing segment::Password,
        char_count_hostname: usize,
    ) -> Result<segment::Host, Error> {
        // == host parsing =========================================================
        //
        // https://url.spec.whatwg.org/#host-parsing
        //
        // =========================================================================

        if cursor.is_empty() {
            return Err(Error::HostMissing);
        }

        if cursor[0] == b'[' {
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
        let mut host_stop = host_start;
        let domain = percent::decode(cursor.iter());

        for c in domain.take(char_count_hostname) {
            if !matches!(c, ascii_tab_or_newline!()) {
                buffer.push(c)?;
                host_stop += 1;
            }
        }
        *cursor = &cursor[char_count_hostname..];

        // TODO: IPV4 parsing

        let host = host_start..host_stop;
        if host.is_empty() {
            return Err(Error::HostMissing);
        }

        Ok(segment::Host(host))
    }
}

mod port {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    /// # [Port state]
    ///
    /// Parses out the port segment of a [`Url`]. Ports are normalized according to that scheme's
    /// default port.
    ///
    /// # Example
    ///
    /// ```text
    /// http://example.com:80
    /// ```
    ///
    /// Will have it's port normalized to [`None`]. This is done to guarantee parser/serializer
    /// indempotence in accordance with the [URL specification goals], so that the following are
    /// seen as equivalent when compared after parsing.
    ///
    /// ```text
    /// http://example.com:80
    /// http://example.com:
    /// http://example.com
    /// ```
    ///
    /// # Errors
    ///
    /// [`PortOutOfRange`] if the value of the port is greater than [`u16::MAX`].
    ///
    /// [`PortInvalid`] if the port contains non [ASCII digit] code points.
    ///
    /// [Port state]: https://url.spec.whatwg.org/#port-state
    /// [URL specification goals]: https://url.spec.whatwg.org/#goals
    /// [`PortOutOfRange`]: Error::PortOutOfRange
    /// [`PortInvalid`]: Error::PortInvalid
    /// [ASCII digit]: https://infra.spec.whatwg.org/#ascii-digit
    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
        default_scheme_port: u16,
    ) -> Result<segment::Port, Error> {
        // ASCII tab or newline characters are skipped first so as not to cause the checkpoint to
        // parse them again.
        if let Some(c) = cursor.first()
            && matches!(c, ascii_tab_or_newline!())
        {
            let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
            *cursor = &cursor[skip..];
        }

        let mut port = 0u32;
        let mut char_count_port = 0;

        #[allow(suspicious_double_ref_op)]
        let checkpoint = cursor.clone();

        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                    char_count_port += skip;
                },

                b'0'..=b'9' => {
                    port = port * 10 + u32::from(c) - u32::from(b'0');

                    if port > u32::from(u16::MAX) {
                        return Err(Error::PortOutOfRange);
                    }

                    *cursor = &cursor[1..];
                    char_count_port += 1;
                },

                b'/' | b'\\' | b'?' | b'#' => {
                    *cursor = &cursor[1..];
                    break;
                },

                _ => {
                    return Err(Error::PortInvalid);
                },
            }
        }

        if port as u16 == default_scheme_port || char_count_port == 0 {
            Ok(segment::Port(None))
        } else {
            buffer.push(b':')?;

            for c in &checkpoint[..char_count_port] {
                if !matches!(c, ascii_tab_or_newline!()) {
                    buffer.push(*c)?;
                }
            }

            Ok(segment::Port(Some(port as u16)))
        }
    }
}

mod path {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
    ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
        // A path is never empty, it always contains at least the leading U+002F (/) code point
        let path_start = buffer.push(b'/')? - 1;
        let mut path_stop = path_start + 1;

        // Skip first leading U+002F (/) or U+005C (\) code points so we don't end up appending `//`
        // or `\/` to the buffer if the path is non-empty.
        if let Some(c) = cursor.first() {
            if *c == b'/' {
                *cursor = &cursor[1..];
            } else if *c == b'\\' {
                error_bitset.add(ValidationError::InvalidReverseSolidus);
                *cursor = &cursor[1..];
            }
        }

        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                },

                b'/' => {
                    *cursor = &cursor[1..];
                    path_stop = buffer.push(c)?;
                },

                b'\\' => {
                    error_bitset.add(ValidationError::InvalidReverseSolidus);
                    *cursor = &cursor[1..];
                    path_stop = buffer.push(c)?;
                },

                b'?' => {
                    // Skip U+003F (?) query segment delimiter
                    *cursor = &cursor[1..];

                    let path = segment::Path(path_start..path_stop);

                    #[cfg(test)]
                    let _remaining_query = str::from_utf8(cursor).unwrap_or_default();

                    let (query, fragment) = query::parse(query::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;

                    return Ok((path, query, fragment));
                },

                b'#' => {
                    // Skip U+0023 (#) fragment segment delimiter
                    *cursor = &cursor[1..];

                    let path = segment::Path(path_start..path_stop);
                    let query = segment::Query(path_stop..path_stop);

                    #[cfg(test)]
                    let _remaining_fragment = str::from_utf8(cursor).unwrap_or_default();

                    let fragment = fragment::parse(fragment::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;

                    return Ok((path, query, fragment));
                },

                b'%' => {
                    path_stop = common::percent::delimiter(common::percent::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;
                },

                _ => {
                    path_stop = common::url_cp::encode(common::url_cp::Context {
                        cursor,
                        buffer,
                        error_bitset,
                        encoding: percent::PATH,
                    })?;
                },
            }
        }

        let path = segment::Path(path_start..path_stop);
        let query = segment::Query(path_stop..path_stop);
        let fragment = segment::Fragment(path_stop..path_stop);

        Ok((path, query, fragment))
    }
}

mod query {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
    ) -> Result<(segment::Query, segment::Fragment), Error> {
        let query_start = buffer.push(b'?')?;
        let mut query_stop = query_start;

        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                },

                b'#' => {
                    // Skip U+0023 (#) fragment segment delimiter
                    *cursor = &cursor[1..];

                    let query = segment::Query(query_start..query_stop);

                    #[cfg(test)]
                    let _remaining_fragment = str::from_utf8(cursor).unwrap_or_default();

                    let fragment = fragment::parse(fragment::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;

                    return Ok((query, fragment));
                },

                b'%' => {
                    query_stop = common::percent::delimiter(common::percent::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;
                },

                _ => {
                    query_stop = common::url_cp::encode(common::url_cp::Context {
                        cursor,
                        buffer,
                        error_bitset,
                        encoding: percent::QUERY_SPECIAL,
                    })?;
                },
            }
        }

        let query = segment::Query(query_start..query_stop);
        let fragment = segment::Fragment(query_stop..query_stop);

        Ok((query, fragment))
    }
}

mod fragment {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[inline]
    #[macro_derive::context]
    pub(super) fn parse<'parsing, 'input, 'output>(
        cursor: &'parsing mut &'input [u8],
        buffer: &'parsing mut UrlBuffer<'output>,
        error_bitset: &'parsing mut ValidationErrorBitSet,
    ) -> Result<segment::Fragment, Error> {
        let fragment_start = buffer.push(b'#')?;
        let mut fragment_stop = fragment_start;

        while !cursor.is_empty() {
            let c = cursor[0];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);

                    let skip = search::skip_ascii_tab_or_newline(&cursor[1..]) + 1;
                    *cursor = &cursor[skip..];
                },

                b'%' => {
                    fragment_stop = common::percent::delimiter(common::percent::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;
                },

                _ => {
                    fragment_stop = common::url_cp::encode(common::url_cp::Context {
                        cursor,
                        buffer,
                        error_bitset,
                        encoding: percent::FRAGMENT,
                    })?;
                },
            }
        }

        Ok(segment::Fragment(fragment_start..fragment_stop))
    }
}

mod common {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    pub(super) mod percent {
        #[allow(clippy::wildcard_imports)]
        use super::*;

        #[inline]
        #[macro_derive::context]
        pub(crate) fn delimiter<'parsing, 'input, 'output>(
            cursor: &'parsing mut &'input [u8],
            buffer: &'parsing mut UrlBuffer<'output>,
            error_bitset: &'parsing mut ValidationErrorBitSet,
        ) -> Result<usize, Error> {
            // Invalid percent-encodings are still serialized and an error is logged.
            let mut position = buffer.push(b'%')?;

            let mut len = 1;

            if let Some(b0) = cursor.get(1)
                && let Some(b1) = cursor.get(2)
            {
                if !b0.is_ascii_hexdigit() || !b1.is_ascii_hexdigit() {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                } else {
                    buffer.push(*b0)?;
                    position = buffer.push(*b1)?;
                    len = 3;
                }
            } else {
                error_bitset.add(ValidationError::InvalidURLUnit);
            }

            *cursor = &cursor[len..];
            Ok(position)
        }
    }

    pub(super) mod url_cp {
        #[allow(clippy::wildcard_imports)]
        use super::*;

        #[inline]
        #[macro_derive::context]
        pub(crate) fn encode<'parsing, 'input, 'output>(
            cursor: &'parsing mut &'input [u8],
            buffer: &'parsing mut UrlBuffer<'output>,
            error_bitset: &'parsing mut ValidationErrorBitSet,
            encoding: crate::percent::EncodeSet,
        ) -> Result<usize, Error> {
            #[cfg(test)]
            let _encoding_before = str::from_utf8(*cursor).unwrap_or_default();

            // Invalid utf-8 bytes are replaced with the U+FFFD (�) utf-8 replacement code point.
            let len = match utf8::url_code_point(cursor) {
                utf8::CodePointUrl::Valid { len } => len,

                utf8::CodePointUrl::Invalid { len } => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                    len
                },

                // Invalid utf-8 bytes are skipped. We don't check for utf-8 encoding in other url
                // sections. However, in the case of the path section where we potentially have to
                // deal with multi-byte url code points, this information comes as free so we might
                // as well act on it.
                utf8::CodePointUrl::InvalidUtf8 { invalid: len } => {
                    let position = buffer.push_str(utf8::REPLACEMENT)?;
                    *cursor = &cursor[len.get() as usize..];

                    return Ok(position);
                },

                // A truncated code point indicates we have reached the end of the cursor before the
                // end of the code point. This information too is ignored, and we instead stop at
                // the last valid code point.
                utf8::CodePointUrl::Truncated => {
                    let position = buffer.push_str(utf8::REPLACEMENT)?;
                    *cursor = &cursor[1..];

                    return Ok(position);
                },
            };

            // NOTE: len is nonzero so position will always be overridden
            let mut position = 0;

            for c in &cursor[..len.get() as usize] {
                position = buffer.push_encode_byte(*c, encoding)?;
            }

            #[cfg(test)]
            let _encoding_after = str::from_utf8(buffer.as_ref()).unwrap_or_default();

            *cursor = &cursor[len.get() as usize..];

            #[cfg(test)]
            let _remaining = str::from_utf8(*cursor).unwrap_or_default();

            Ok(position)
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

    /// Reference naive url code point implementation for use in tests.
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
            // Under U+00A0
            0x80..=0x9F => false,
            // 3-byte non-characters
            0xFDD0..=0xFDEF => false,
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

mod search {
    // PERF: This already compiles down to a pretty optimal implementation, no need to further optimize.
    pub(super) fn skip_ascii_tab_or_newline(haystack: &[u8]) -> usize {
        haystack
            .iter()
            .position(|c| !matches!(c, b'\t' | b'\r' | b'\n'))
            .unwrap_or(haystack.len())
    }
}

#[cfg(test)]
mod test {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[test]
    fn url_parse_userinfo_full() {
        const URL: &str = "http://user:password@example.com:123/path/to/file?query#fragment";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidCredentials)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"user");
        assert_utf8_eq!(url.password, b"password");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, Some(123));
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"query");
        assert_utf8_eq!(url.fragment, b"fragment");

        assert_eq!(
            format!("{url}"),
            "http://user:password@example.com:123/path/to/file?query#fragment"
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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"user");
        assert_utf8_eq!(url.password, b"");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"password");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"user");
        assert_utf8_eq!(url.password, b"password%40");

        assert_eq!(format!("{url}"), "http://user:password%40@example.com/");
    }

    #[test]
    fn url_parse_host_domain() {
        const URL: &str = "http://example.com";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_port_simple() {
        const URL: &str = "http://example.com:123";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(format!("{url}"), "http://example.com:123/");

        assert_eq!(url.port, Some(123));
    }

    #[test]
    fn url_parse_port_default_ftp() {
        const URL: &str = "ftp://example.com:21";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"ftp");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "ftp://example.com/");
    }

    #[test]
    fn url_parse_port_default_http() {
        const URL: &str = "http://example.com:80";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_port_default_ws() {
        const URL: &str = "ws://example.com:80";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"ws");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "ws://example.com/");
    }

    #[test]
    fn url_parse_port_default_https() {
        const URL: &str = "https://example.com:443";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"https");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "https://example.com/");
    }

    #[test]
    fn url_parse_port_default_wss() {
        const URL: &str = "wss://example.com:443";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"wss");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "wss://example.com/");
    }

    #[test]
    fn url_parse_port_empty() {
        const URL: &str = "http://example.com:";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

        assert_eq!(url.port, None);

        assert_eq!(format!("{url}"), "http://example.com/");
    }

    #[test]
    fn url_parse_path_simple() {
        const URL: &str = "http://example.com/cat/pictures";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/cat/pictures");

        assert_eq!(format!("{url}"), "http://example.com/cat/pictures");
    }

    #[test]
    fn url_parse_path_strange_solidus() {
        const URL: &str = "http://example.com//\\////\\\\\\///////";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidReverseSolidus)
        );
        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"//\\////\\\\\\///////");

        assert_eq!(format!("{url}"), "http://example.com//\\////\\\\\\///////");
    }

    #[test]
    fn url_parse_query_simple() {
        const URL: &str = "http://example.com?name=cat.txt";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/");
        assert_utf8_eq!(url.query, b"name=cat.txt");

        assert_eq!(format!("{url}"), "http://example.com/?name=cat.txt");
    }

    #[test]
    fn url_parse_fragment_simple() {
        const URL: &str = "http://example.com#about";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/");
        assert_utf8_eq!(url.query, b"");
        assert_utf8_eq!(url.fragment, b"about");

        assert_eq!(format!("{url}"), "http://example.com/#about");
    }

    #[test]
    fn url_percent_encoded_valid() {
        const URL: &str = "http://example.com/pictures/of%20my%20cat/";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pictures/of%20my%20cat/");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pictures/of%%20my%%20cat/");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pictures/of%Azmy%Zacat/");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pictures/of%20my%20cat/");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pi%EF%BF%BDc%EF%BF%BDtu%EF%BF%BDres");

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

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/pictures%EF%BF%BD");

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

        println!("{url_input:?}");

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
    #[ignore]
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
    #[ignore]
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

        assert_eq!(err, Error::Overflow);
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

        assert_eq!(err, Error::HostMissing)
    }

    #[test]
    fn url_err_port_out_of_range() {
        const URL: &str = "http://example.com:70000";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::PortOutOfRange)
    }

    #[test]
    fn url_err_port_invalid() {
        const URL: &str = "http://example.com:7z";

        let mut backing = [0; 128];
        let err = Url::new(URL.as_bytes(), &mut backing).unwrap_err();

        assert_eq!(err, Error::PortInvalid)
    }

    #[test]
    #[should_panic]
    fn url_err_empty_backing() {
        const URL: &str = "example.com";

        let mut backing = [0; 0];
        let _ = Url::new(URL.as_bytes(), &mut backing);
    }

    #[test]
    fn utf8_url_code_point_matches_reference() {
        let mut backing = [0; 4];

        for code_point in 0..0x10FFFF {
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
