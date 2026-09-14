mod buffer;
mod error;

use buffer::UrlBuffer;
pub use error::*;

use super::*;

impl<'data> Url<'data> {
    /// Based off https://url.spec.whatwg.org/#url-parsing
    pub fn new(
        mut bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, ValidationErrorIter), Error> {
        assert!(!backing.is_empty());

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
    use super::*;

    pub(super) struct Context<'parsing, 'input> {
        pub cursor: &'parsing mut &'input [u8],
        pub error_bitset: &'parsing mut ValidationErrorBitSet,
    }

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
    pub(super) fn parse<'parsing, 'input>(context: Context<'parsing, 'input>) {
        let Context {
            cursor,
            error_bitset,
        } = context;

        // Leading C0 control or space
        if let Some(c) = cursor.first()
            && matchers::c0_control_or_space(*c)
        {
            error_bitset.add(ValidationError::InvalidURLUnit);
            *cursor = &cursor[1..];

            while let Some(c) = cursor.first()
                && matchers::c0_control_or_space(*c)
            {
                *cursor = &cursor[1..];
            }
        }

        // Trailing C0 control or space
        if let Some(c) = cursor.last()
            && matchers::c0_control_or_space(*c)
        {
            error_bitset.add(ValidationError::InvalidURLUnit);

            let len = cursor.len();
            *cursor = &cursor[..len - 1];

            while let Some(c) = cursor.last()
                && matchers::c0_control_or_space(*c)
            {
                let len = cursor.len();
                *cursor = &cursor[..len - 1];
            }
        }
    }
}

mod scheme {
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,
    }

    /// # [Scheme state]
    ///
    /// Tries to parse a [`Url]'s scheme, if there is any, otherwise falls back to the "no scheme"
    /// state. This is the entry point for parsing further segments.
    ///
    /// [Scheme state]: https://url.spec.whatwg.org/#scheme-start-state
    #[inline]
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
        let Context {
            cursor,
            mut buffer,
            error_bitset,
        } = context;

        if let Some(c) = cursor.first()
            && matchers::ascii_alpha(*c)
        {
            buffer.push(c.to_ascii_lowercase())?;
            *cursor = &cursor[1..];

            while !cursor.is_empty() {
                let c = cursor[0];

                #[cfg(test)]
                let _c = char::from_u32(c as u32).unwrap_or_default();

                *cursor = &cursor[1..];

                match c {
                    ascii_tab_or_newline!() => {
                        error_bitset.add(ValidationError::InvalidURLUnit);
                    },

                    // Only the first character in a scheme must be strictly `ascii_alpha`. Scheme
                    // characters after that may be ASCII alphanumeric, U+002B (+), U+002D (-), or
                    // U+002E (.).
                    b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => {
                        buffer.push(c)?;
                    },

                    // Input is normalized, only lowercase characters are pushed to the final buffer
                    b'A'..=b'Z' => {
                        buffer.push(c.to_ascii_lowercase())?;
                    },

                    // End of scheme
                    b':' => {
                        buffer.push(b':')?;
                        break;
                    },

                    // Invalid character, scheme error.
                    _ => break,
                };
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
        use super::*;

        pub(crate) struct Context<'parsing, 'input, 'output> {
            pub cursor: &'parsing mut &'input [u8],
            pub buffer: UrlBuffer<'output>,

            pub error_bitset: &'parsing mut ValidationErrorBitSet,
            pub scheme: segment::Scheme,
            pub default_scheme_port: u16,
        }

        /// # [Special relative or authority state]
        ///
        /// Parses out each [`Url] segment after a special scheme.
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
        pub(crate) fn parse<'parsing, 'input, 'output>(
            context: Context<'parsing, 'input, 'output>,
        ) -> Result<(Url<'output>, ValidationErrorIter), Error> {
            let Context {
                cursor,
                mut buffer,
                error_bitset,
                scheme,
                default_scheme_port,
            } = context;

            // == Special authority slashes state ======================================
            //
            // Ensure the scheme is followed by two U+002F (/).
            //
            // =========================================================================

            while !cursor.is_empty() {
                match *cursor {
                    [ascii_tab_or_newline!(), ..] => {
                        error_bitset.add(ValidationError::InvalidURLUnit);
                        *cursor = &cursor[1..];
                    },

                    [b'/', b'/', ..] => {
                        *cursor = &cursor[2..];
                        break;
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

                #[cfg(test)]
                let _c = char::from_u32(c as u32).unwrap_or_default();

                match c {
                    ascii_tab_or_newline!() => {
                        error_bitset.add(ValidationError::InvalidURLUnit);
                        *cursor = &cursor[1..];
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

            let (host, port, port_size) = host_and_port::parse(host_and_port::Context {
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
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: &'parsing mut UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,
        pub scheme: &'parsing segment::Scheme,
    }

    /// # Authority state
    ///
    /// Parses the `username` and `password` sections of a [`Url], if there are any. This really
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
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<(segment::Username, segment::Password), Error> {
        let Context {
            cursor,
            buffer,
            error_bitset,
            scheme,
        } = context;

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

            #[cfg(test)]
            let _c = char::from_u32(c as u32).unwrap_or_default();

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                },

                // EOF code point is implied in the below check
                b'/' | b'\\' | b'?' | b'#' => {
                    break;
                },

                b'@' => {
                    error_bitset.add(ValidationError::InvalidCredentials);
                    at_sign = Some(char_count_authority);
                    char_count_authority += 1;
                },
                _ => {
                    char_count_authority += 1;
                },
            }

            *cursor = &cursor[1..];
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
                } else {
                    n
                }
            },
            None => 0,
        };

        // Scheme end, skipping `://`
        let userinfo_start = scheme.0.end + 3;
        let mut userinfo_stop = userinfo_start;
        let mut password_token = None;

        // Here is where we actually parse the userinfo
        while char_count_userinfo > 0 {
            let c = cursor[0];

            *cursor = &cursor[1..];
            char_count_userinfo -= 1;

            match c {
                b':' => {
                    match password_token {
                        Some(_) => {
                            // percent-encoded U+003A (:)
                            buffer.push_str(b"%3A")?;
                        },
                        None => {
                            // We only push U+003A (:) if the password is non-empty
                            //
                            // https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L907-L914
                            if char_count_userinfo > 0 {
                                password_token = Some(buffer.push(b':')? - 1);
                            }
                        },
                    }
                },
                c => {
                    #[cfg(test)]
                    let _c = char::from_u32(c as u32).unwrap_or_default();

                    userinfo_stop = buffer.push_encode_byte(c, percent::USERINFO)?;
                },
            }
        }

        let (username, password) = match password_token {
            Some(n) => {
                // We need to skip over the terminating userinfo U+0040 (@) delimiter again as we
                // have reset the cursor.
                *cursor = &cursor[1..];
                (userinfo_start..n, n + 1..userinfo_stop)
            },
            None => (userinfo_start..userinfo_stop, userinfo_stop..userinfo_stop),
        };

        if !username.is_empty() || !password.is_empty() {
            buffer.push(b'@')?;
        }

        Ok((segment::Username(username), segment::Password(password)))
    }
}

mod host_and_port {
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: &'parsing mut UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,

        pub username: &'parsing segment::Username,
        pub password: &'parsing segment::Password,
        pub default_scheme_port: u16,
    }

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
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<(segment::Host, segment::Port, usize), Error> {
        let Context {
            cursor,
            buffer,
            error_bitset,
            username,
            password,
            default_scheme_port,
        } = context;

        // == hostname state =======================================================
        //
        // https://url.spec.whatwg.org/#hostname-state
        //
        // =========================================================================

        let checkpoint = cursor.clone();
        let mut inside_brackets = false;
        let mut char_count_hostname = 0;

        while !cursor.is_empty() {
            let c = cursor[0];

            #[cfg(test)]
            let _c = char::from_u32(c as u32).unwrap_or_default();

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                },

                b':' if !inside_brackets => {
                    *cursor = checkpoint;

                    #[cfg(test)]
                    let _remaining_host = str::from_utf8(cursor).unwrap_or_default();

                    let host = host::parse(host::Context {
                        cursor,
                        buffer,

                        error_bitset,

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
                        error_bitset,
                        default_scheme_port,
                    })?;

                    return Ok((host, port.0, port.1));
                },

                b'/' | b'\\' | b'?' | b'#' => {
                    *cursor = checkpoint;

                    break;
                },

                b'[' => {
                    inside_brackets = true;
                    char_count_hostname += 1;
                },

                b']' => {
                    inside_brackets = false;
                    char_count_hostname += 1;
                },

                _ => {
                    char_count_hostname += 1;
                },
            }

            *cursor = &cursor[1..];
        }

        *cursor = checkpoint;

        #[cfg(test)]
        let _remaining_host = str::from_utf8(cursor).unwrap_or_default();

        let host = host::parse(host::Context {
            cursor,
            buffer,

            error_bitset,

            username,
            password,
            char_count_hostname,
        })?;

        Ok((host, segment::Port(None), 0))
    }
}

mod host {
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: &'parsing mut UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,

        pub username: &'parsing segment::Username,
        pub password: &'parsing segment::Password,
        pub char_count_hostname: usize,
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
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<segment::Host, Error> {
        let Context {
            cursor,
            buffer,
            error_bitset: _,
            username,
            password,
            char_count_hostname,
        } = context;

        // == host parsing =========================================================
        //
        // https://url.spec.whatwg.org/#host-parsing
        //
        // =========================================================================

        if cursor.is_empty() {
            return Err(Error::HostMissing);
        }

        match cursor[0] {
            b'[' => {
                todo!("IPV6 parsing");
            },
            _ => {
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

                let host_stop = host_start + char_count_hostname;
                let host = host_start..host_stop;

                if host.is_empty() {
                    return Err(Error::HostMissing);
                }

                // TODO: IDNA domain parser
                let domain = percent::decode(cursor.iter());
                for c in domain.take(char_count_hostname) {
                    buffer.push(c)?;
                }
                *cursor = &cursor[char_count_hostname..];

                // TODO: IPV4 parsing

                Ok(segment::Host(host))
            },
        }
    }
}

mod port {
    use super::*;

    pub(super) struct Context<'parsing, 'input> {
        pub cursor: &'parsing mut &'input [u8],
        pub error_bitset: &'parsing mut ValidationErrorBitSet,
        pub default_scheme_port: u16,
    }

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
    pub(super) fn parse<'parsing, 'input>(
        context: Context<'parsing, 'input>,
    ) -> Result<(segment::Port, usize), Error> {
        let Context {
            cursor,
            error_bitset,
            default_scheme_port,
        } = context;

        let mut port = 0u32;
        let mut char_count_port = 0;

        while !cursor.is_empty() {
            let c = cursor[0];

            #[cfg(test)]
            let _c = char::from_u32(c as u32).unwrap_or_default();

            *cursor = &cursor[1..];

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                },

                b'0'..=b'9' => {
                    port = port * 10 + c as u32 - b'0' as u32;

                    if port > u16::MAX as u32 {
                        return Err(Error::PortOutOfRange);
                    }

                    char_count_port += 1;
                },

                b'/' | b'\\' | b'?' | b'#' => {
                    break;
                },

                _ => {
                    return Err(Error::PortInvalid);
                },
            }
        }

        if port as u16 == default_scheme_port || char_count_port == 0 {
            Ok((segment::Port(None), char_count_port))
        } else {
            Ok((segment::Port(Some(port as u16)), char_count_port))
        }
    }
}

mod path {
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: &'parsing mut UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,
    }

    #[inline]
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<(segment::Path, segment::Query, segment::Fragment), Error> {
        let Context {
            cursor,
            buffer,
            error_bitset,
        } = context;

        let path_start = buffer.push(b'/')?;
        let mut path_stop = path_start;

        while !cursor.is_empty() {
            let c = cursor[0];

            #[cfg(test)]
            let _c = char::from_u32(c as u32).unwrap_or_default();

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                    *cursor = &cursor[1..];
                },

                b'/' | b'\\' => {
                    if c == b'\\' {
                        error_bitset.add(ValidationError::InvalidReverseSolidus);
                    }

                    *cursor = &cursor[1..];

                    path_stop = buffer.push(c)?;
                },

                b'?' => {
                    let path = segment::Path(path_start..path_stop);

                    // Skip U+003F (?) query segment delimiter
                    *cursor = &cursor[1..];

                    #[cfg(test)]
                    let _remaining_query = str::from_utf8(cursor).unwrap_or_default();

                    let (query, fragment) = query::parse(query::Context {
                        cursor,
                        buffer,
                        error_bitset,
                    })?;

                    return Ok((path, query, fragment));
                },

                b'#' => todo!(),

                b'%' => {
                    // Invalid percent-encodings are still serialized and an error is logged.
                    *cursor = &cursor[1..];
                    path_stop = buffer.push(b'%')?;

                    if let Some(b0) = cursor.first()
                        && let Some(b1) = cursor.get(1)
                    {
                        if !b0.is_ascii_hexdigit() || !b1.is_ascii_hexdigit() {
                            error_bitset.add(ValidationError::InvalidURLUnit);
                        } else {
                            *cursor = &cursor[2..];
                            buffer.push(*b0)?;
                            path_stop = buffer.push(*b1)?;
                        }
                    } else {
                        error_bitset.add(ValidationError::InvalidURLUnit);
                    }
                },

                _ => {
                    // Invalid utf-8 bytes are skipped and do not terminate parsing.
                    let len = match utf8::url_code_point(cursor) {
                        utf8::UrlCodePoint::Valid { len } => len,

                        utf8::UrlCodePoint::Invalid { len } => {
                            error_bitset.add(ValidationError::InvalidURLUnit);
                            len
                        },

                        // Invalid utf-8 bytes are skipped. We don't check for utf-8 encoding in
                        // other url sections. However, in the case of the path section where we
                        // potentially have to deal with multi-byte url code points, this
                        // information comes as free so we might as well act on it.
                        utf8::UrlCodePoint::InvalidUtf8 { len } => {
                            path_stop = buffer.push_str(utf8::REPLACEMENT)?;
                            *cursor = &cursor[len as usize..];
                            continue;
                        },

                        // A truncated code point indicates we have reached the end of the cursor
                        // before the end of the code point. This information too is ignored, and we
                        // instead stop at the last valid code point.
                        utf8::UrlCodePoint::Truncated => {
                            path_stop = buffer.push_str(utf8::REPLACEMENT)?;
                            *cursor = &[];
                            break;
                        },
                    };

                    for c in &cursor[..len as usize] {
                        path_stop = buffer.push_encode_byte(*c, percent::QUERY_SPECIAL)?;
                    }

                    *cursor = &cursor[len as usize..];
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
    use super::*;

    pub(super) struct Context<'parsing, 'input, 'output> {
        pub cursor: &'parsing mut &'input [u8],
        pub buffer: &'parsing mut UrlBuffer<'output>,

        pub error_bitset: &'parsing mut ValidationErrorBitSet,
    }

    #[inline]
    pub(super) fn parse<'parsing, 'input, 'output>(
        context: Context<'parsing, 'input, 'output>,
    ) -> Result<(segment::Query, segment::Fragment), Error> {
        let Context {
            cursor,
            buffer,
            error_bitset,
        } = context;

        let query_start = buffer.push(b'?')?;
        let mut query_stop = query_start;

        while !cursor.is_empty() {
            let c = cursor[0];

            #[cfg(test)]
            let _c = char::from_u32(c as u32).unwrap_or_default();

            match c {
                ascii_tab_or_newline!() => {
                    error_bitset.add(ValidationError::InvalidURLUnit);
                    *cursor = &cursor[1..];
                },

                b'#' => todo!(),

                b'%' => {
                    // Invalid percent-encodings are still serialized and an error is logged.
                    *cursor = &cursor[1..];
                    query_stop = buffer.push(b'%')?;

                    if let Some(b0) = cursor.first()
                        && let Some(b1) = cursor.get(1)
                    {
                        if !b0.is_ascii_hexdigit() || !b1.is_ascii_hexdigit() {
                            error_bitset.add(ValidationError::InvalidURLUnit);
                        } else {
                            *cursor = &cursor[2..];
                            buffer.push(*b0)?;
                            query_stop = buffer.push(*b1)?;
                        }
                    } else {
                        error_bitset.add(ValidationError::InvalidURLUnit);
                    }
                },

                _ => {
                    // Invalid utf-8 bytes are skipped and do not terminate parsing.
                    let len = match utf8::url_code_point(cursor) {
                        utf8::UrlCodePoint::Valid { len } => len,

                        utf8::UrlCodePoint::Invalid { len } => {
                            error_bitset.add(ValidationError::InvalidURLUnit);
                            len
                        },

                        // Invalid utf-8 bytes are skipped. We don't check for utf-8 encoding in
                        // other url sections. However, in the case of the query section where we
                        // potentially have to deal with multi-byte url code points, this
                        // information comes as free so we might as well act on it.
                        utf8::UrlCodePoint::InvalidUtf8 { len } => {
                            query_stop = buffer.push_str(utf8::REPLACEMENT)?;
                            *cursor = &cursor[len as usize..];
                            continue;
                        },

                        // A truncated code point indicates we have reached the end of the cursor
                        // before the end of the code point. This information too is ignored, and we
                        // instead stop at the last valid code point.
                        utf8::UrlCodePoint::Truncated => {
                            query_stop = buffer.push_str(utf8::REPLACEMENT)?;
                            *cursor = &[];
                            break;
                        },
                    };

                    for c in &cursor[..len as usize] {
                        query_stop = buffer.push_encode_byte(*c, percent::QUERY_SPECIAL)?;
                    }

                    *cursor = &cursor[len as usize..];
                },
            }
        }

        let query = segment::Query(query_start..query_stop);
        let fragment = segment::Fragment(query_stop..query_stop);

        Ok((query, fragment))
    }
}

/// UTF-8 parsing utilities.
pub mod utf8 {
    /// U+FFFD (�) utf-8 replacement character
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
    pub enum UrlCodePoint {
        /// Code point is a valid URL code point.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// assert_eq!(url_code_point(b"a"), UrlCodePoint::Valid { len: 1 });
        /// ```
        Valid {
            /// Length of the code point in bytes.
            len: u8,
        },

        /// Code point is not a valid URL code point, but is valid utf-8.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// assert_eq!(url_code_point(b"%"), UrlCodePoint::Invalid { len: 1 });
        /// ```
        Invalid {
            /// Length of the code point in bytes.
            len: u8,
        },

        /// Code point is invalid under utf-8. This can be because it is overlong, a utf-16
        /// surrogate, a continuation byte instead of a lead byte or has a lead byte exceeding the
        /// utf-8 max range of U+10FFFF.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// // 0xF5 as a lead byte is outside of the utf-8 max range.
        /// assert_eq!(
        ///     url_code_point(&[0xF5]),
        ///     UrlCodePoint::InvalidUtf8 { len: 1 }
        /// );
        /// ```
        InvalidUtf8 {
            /// Number of bytes which are invalid utf-8.
            len: u8,
        },

        /// Byte input ends before the code point could be fully parsed.
        ///
        /// # Examples
        ///
        /// ```rust
        /// # use http_url::parsing::utf8::*;
        /// // 0xF4 as a lead byte denotes a four-bytes code point,
        /// // however the remaining bytes are missing
        /// assert_eq!(url_code_point(&[0xF4]), UrlCodePoint::Truncated);
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
    /// assert_eq!(url_code_point(b"a"), UrlCodePoint::Valid { len: 1 });
    /// ```
    ///
    /// [URL code point]: https://url.spec.whatwg.org/#url-code-points
    pub fn url_code_point(bytes: &[u8]) -> UrlCodePoint {
        let Some(b0) = bytes.first() else {
            return UrlCodePoint::Truncated;
        };

        // == Step 1 ===============================================================================
        //
        // Check all valid ASCII values.
        //
        // =========================================================================================

        if b0.is_ascii() {
            if (ASCII_URL_CODE_POINT >> *b0 & 1) > 0 {
                return UrlCodePoint::Valid { len: 1 };
            } else {
                return UrlCodePoint::Invalid { len: 1 };
            }
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
            0xC2..=0xDF => (2, 0x80, 0xBF),

            // utf8 3-byte leads 1110xxxx get more complicated. Values before U+0800 should not be
            // encoded in 3 bytes, as that would be overlong, so the second byte must start at A0.
            0xE0 => (3, 0xA0, 0xBF),

            // The rest of 3-byte utf8 values are valid, except for surrogates.
            0xE1..=0xEC => (3, 0x80, 0xBF),

            // Surrogate code points are reserved for use in utf16 and cannot be used in utf8. A
            // leading surrogate is a code point that is in the range U+D800 to U+DBFF, inclusive.
            // A trailing surrogate is a code point that is in the range U+DC00 to U+DFFF,
            // inclusive. If you look at the byte representation of surrogate code points, you will
            // notice they all start with 0xED, with a second byte in the range 0xA0 to 0xBF
            // inclusive. So the range of valid second bytes in a 3-byte code point are 0x80 to 0x9F
            // inclusive. Any other values are invalid utf8.
            0xED => (3, 0x80, 0x9F),

            // The rest of the 3-byte range.
            0xEE..=0xEF => (3, 0x80, 0xBF),

            // utf8 4-byte lead is 11110xxx, giving us 0xF0 (11110000) as the smallest possible
            // 4-byte lead. Values under U+10000 should not be encoded in 4 bytes, as that would be
            // overlong, so the second byte must start at 0x90.
            0xF0 => (4, 0x90, 0xBF),

            // 4-byte code points before the U+10FFFF lead byte.
            0xF1..=0xF3 => (4, 0x80, 0xBF),

            // utf8 ends at 10FFFF, with a 4-byte lead of 0xF4. The maximum value for the second
            // byte here is 0x8F without exceeding this range.
            0xF4 => (4, 0x80, 0x8F),

            // 0x80..=0xBF: a continuation byte where a lead should be.
            // 0xC0, 0xC1, 0xF5..=0xFF: never valid leads.
            _ => return UrlCodePoint::InvalidUtf8 { len: 1 },
        };

        // Second byte must exist and be in a specific range to be valid utf8.
        let Some(b1) = bytes.get(1) else {
            return UrlCodePoint::Truncated;
        };

        if *b1 < b1_min || *b1 > b1_max {
            // Only the leading byte is decisively invalid.
            return UrlCodePoint::InvalidUtf8 { len: 1 };
        }

        // Third and fourth bytes only need to be valid continuation bytes.
        for i in 2..len {
            match bytes.get(i) {
                Some(0x80..=0xBF) => continue,
                // Counts all malformed continuation bytes as invalid.
                Some(_) => return UrlCodePoint::InvalidUtf8 { len: i as u8 },
                None => return UrlCodePoint::Truncated,
            }
        }

        // == Step 3 ===============================================================================
        //
        // Check if the code point is a URL code point.
        //
        // =========================================================================================

        // It is simpler to check for non-URL code points.
        let excluded = match len {
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
            UrlCodePoint::Invalid { len: len as u8 }
        } else {
            UrlCodePoint::Valid { len: len as u8 }
        }
    }

    /// Reference naive url code point implementation for use in tests.
    #[cfg(test)]
    pub(super) fn url_code_point_reference(bytes: &[u8]) -> UrlCodePoint {
        let head = &bytes[..bytes.len().min(4)];

        let c = match std::str::from_utf8(head) {
            Ok(s) => match s.chars().next() {
                Some(c) => c,
                None => return UrlCodePoint::Truncated,
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
                        Some(len) => return UrlCodePoint::InvalidUtf8 { len: len as u8 },
                        None => return UrlCodePoint::Truncated,
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
            UrlCodePoint::Valid { len }
        } else {
            UrlCodePoint::Invalid { len }
        }
    }
}

#[cfg(test)]
mod test {
    use macro_util::prelude::*;

    use super::*;

    #[test]
    fn url_parse_userinfo_full() {
        const URL: &str = "http://user:password@example.com:123";

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
    }

    #[test]
    fn url_parse_port_valid() {
        const URL: &str = "http://example.com:123";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");

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
    }

    #[test]
    fn url_parse_path_percent_encoded_valid() {
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
    }

    #[test]
    fn url_parse_path_percent_encoded_empty() {
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
    }

    #[test]
    fn url_parse_path_percent_encoded_invalid() {
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
    }

    #[test]
    fn url_parse_path_percent_encode_non_url_code_points() {
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
    }

    #[test]
    fn url_parse_path_utf8_invalid() {
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
    }

    #[test]
    fn url_parse_path_utf8_truncated() {
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
    }

    #[test]
    fn url_parse_query_simple() {
        const URL: &str = "http://example.com/path/to/file?name=cat.txt";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=cat.txt");
    }

    #[test]
    fn url_parse_query_percent_encode_valid() {
        const URL: &str = "http://example.com/path/to/file?name=my%20cat.txt";

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=my%20cat.txt");
    }

    #[test]
    fn url_parse_query_percent_encode_empty() {
        const URL: &str = "http://example.com/path/to/file?name=my% cat.txt";

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
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=my%%20cat.txt");
    }

    #[test]
    fn url_parse_query_percent_encode_invalid() {
        const URL: &str = "http://example.com/path/to/file?name=my%Azcat%Za.txt";

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
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=my%Azcat%Za.txt");
    }

    #[test]
    fn url_parse_query_percent_encode_non_url_code_points() {
        const URL: &str = "http://example.com/path/to/file?name=my cat.txt";

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
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=my%20cat.txt");
    }

    #[test]
    fn url_parse_query_percent_utf8_invalid() {
        const URL: &[u8] = &[
            104, 116, 116, 112, 58, 47, 47, 101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109, 47,
            112, 97, 116, 104, 47, 116, 111, 47, 102, 105, 108, 101, 63, 110, 97, 109, 0x80, 101,
            0xC0, 61, 99, 0xF5, 97, 116, 46, 116, 120, 116,
        ];

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL, &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"nam%EF%BF%BDe%EF%BF%BD=c%EF%BF%BDat.txt");
    }

    #[test]
    fn url_parse_query_percent_utf8_truncated() {
        const URL: &[u8] = &[
            104, 116, 116, 112, 58, 47, 47, 101, 120, 97, 109, 112, 108, 101, 46, 99, 111, 109, 47,
            112, 97, 116, 104, 47, 116, 111, 47, 102, 105, 108, 101, 63, 110, 97, 109, 101, 61, 99,
            97, 116, 46, 116, 120, 116, 0xF4,
        ];

        let mut backing = [0; 128];
        let (url, mut validation_errors) = Url::new(URL, &mut backing).unwrap();

        assert_eq!(validation_errors.next(), None);

        assert_utf8_eq!(url.scheme, b"http");
        assert_utf8_eq!(url.username, b"");
        assert_utf8_eq!(url.password, b"");
        assert_utf8_eq!(url.host, b"example.com");
        assert_eq!(url.port, None);
        assert_utf8_eq!(url.path, b"/path/to/file");
        assert_utf8_eq!(url.query, b"name=cat.txt%EF%BF%BD");
    }

    #[test]
    fn url_trim_c0_control_or_space_front() {
        const URL: &str = "\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{9}\u{10}\u{11}\u{12}\u{13}\u{14}\u{15}\u{16}\u{17}\u{18}\u{19}\u{20}http://example.com";

        let mut backing = [0; 128];
        let (_, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);
    }

    #[test]
    fn url_trim_c0_control_or_space_back() {
        const URL: &str = "http://example.com\u{20}\u{19}\u{18}\u{17}\u{16}\u{15}\u{14}\u{13}\u{12}\u{11}\u{10}\u{9}\u{8}\u{7}\u{6}\u{5}\u{4}\u{3}\u{2}\u{1}\u{0}";

        let mut backing = [0; 128];
        let (_, mut validation_errors) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_errors.next(),
            Some(ValidationError::InvalidURLUnit)
        );
        assert_eq!(validation_errors.next(), None);
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
        bolero::check!()
            .with_max_len(4)
            .exhaustive()
            .for_each(|bytes| {
                let actual = utf8::url_code_point(bytes);
                let expected = utf8::url_code_point_reference(bytes);

                assert_eq!(actual, expected);
            });
    }

    // #[test]
    // fn utf8_url_code_point_harness_failure() {
    //     let bytes = [0xf6];
    //
    //     let actual = utf8::url_code_point(&bytes);
    //     let expected = utf8::url_code_point_reference(&bytes);
    //
    //     assert_eq!(actual, expected);
    // }
}
