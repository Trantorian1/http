mod buffer;
mod error;
mod iter;

use buffer::UrlBuffer;
pub use error::Error;
pub use error::ValidationError;
use iter::ByteIter;

use super::*;

impl<'data> Url<'data> {
    /// Based off https://url.spec.whatwg.org/#url-parsing
    pub fn new(
        mut bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, Option<ValidationError>), Error> {
        assert!(!backing.is_empty());

        let mut validation_error = None;
        let mut buffer = UrlBuffer::new(backing);

        // == C0 control or space sanitization =====================================================
        //
        // - 1.2. If input contains any leading or trailing C0 control or space, invalid-URL-unit
        //        validation error.
        //
        // - 1.3. Remove any leading and trailing C0 control or space from input.
        //
        // =========================================================================================

        // Leading C0 control or space
        if let Some(c) = bytes.first()
            && matchers::c0_control_or_space(*c)
        {
            validation_error.get_or_insert(ValidationError::InvalidURLUnit);
            bytes = &bytes[1..];

            while let Some(c) = bytes.first()
                && matchers::c0_control_or_space(*c)
            {
                bytes = &bytes[1..];
            }
        }

        // Trailing C0 control or space
        if let Some(c) = bytes.last()
            && matchers::c0_control_or_space(*c)
        {
            validation_error.get_or_insert(ValidationError::InvalidURLUnit);
            let len = bytes.len();
            bytes = &bytes[..len - 1];

            while let Some(c) = bytes.last()
                && matchers::c0_control_or_space(*c)
            {
                let len = bytes.len();
                bytes = &bytes[..len - 1];
            }
        }

        // == ASCII tab or newline sanitization ====================================================
        //
        // - 2. If input contains any ASCII tab or newline, invalid-URL-unit validation error.
        //
        // - 3. Remove all ASCII tab or newline from input.
        //
        // =========================================================================================

        let mut iter = ByteIter::new(bytes);

        // == Scheme parsing =======================================================================
        //
        // Schemes are bounded by a first ASCII alphabetic character and end at the first U+003A (:)
        // delimiter.
        //
        // =========================================================================================

        if let Some(c) = iter.next()
            && matchers::ascii_alpha(*c)
        {
            buffer.push(c.to_ascii_lowercase())?;

            while let Some(c) = iter.next() {
                match c {
                    // Only the first character in a scheme must be strictly `ascii_alpha`. Scheme
                    // characters after that may be ASCII alphanumeric, U+002B (+), U+002D (-), or
                    // U+002E (.).
                    b'a'..=b'z' | b'0'..=b'9' | b'+' | b'-' | b'.' => buffer.push(*c)?,

                    // Input is normalized, only lowercase characters are pushed to the final buffer
                    b'A'..=b'Z' => buffer.push(c.to_ascii_lowercase())?,

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
                let scheme = 0..buffer.len() - 1;

                #[cfg(test)]
                let _scheme = str::from_utf8(&buffer[scheme.clone()]).unwrap_or_default();

                // See the [URL standard], special schemes.
                //
                // > _"A special scheme is an [ASCII string] that is listed in the first column of
                // > the following table. The default port for a special scheme is listed in the
                // > second column on the same row. The default port for any other [ASCII string] is
                // > null."_
                // >
                // > | Special scheme | Default port |
                // > |----------------|--------------|
                // > | "ftp"          | 21           |
                // > | "file"         | null         |
                // > | "http"         | 80           |
                // > | "https"        | 443          |
                // > | "ws"           | 80           |
                // > | "wss"          | 443          |
                //
                // [URL standard]: https://url.spec.whatwg.org/#special-scheme
                // [ASCII string]: https://infra.spec.whatwg.org/#ascii-string
                match &buffer[scheme.clone()] {
                    b"file" => {
                        if !iter.skip_if_matches(b"//") {
                            validation_error.get_or_insert(
                                ValidationError::SpecialSchemeMissingFollowingSolidus,
                            );
                        }

                        buffer.push_str(b"//")?;
                    },

                    b"ftp" | b"http" | b"https" | b"ws" | b"wss" => {
                        // == Special authority slashes state ======================================
                        //
                        // Ensure the scheme is followed by two U+002F (/).
                        //
                        // =========================================================================

                        if !iter.skip_if_matches(b"//") {
                            validation_error.get_or_insert(
                                ValidationError::SpecialSchemeMissingFollowingSolidus,
                            );

                            // TODO: relative state
                            // https://url.spec.whatwg.org/#relative-state

                            todo!()
                        }

                        // Special authority ignore slashes state ==================================
                        //
                        //  The specs aren't very clear on what happens in case an invalid
                        //  combination of slashes precedes the authority. However, based on the
                        //  rust_url source code, it seems the correct approach is to ignore ALL
                        //  slashes following the scheme and emit an error if this does not exactly
                        //  match two U+002F (/).
                        //
                        // https://github.com/servo/rust-url/blob/00a6ce58d02f4e0d43c5ca0702c0bedb8b1ebf3a/url/src/parser.rs#L451-L457
                        //
                        // =========================================================================

                        if iter.skip_while_matches2(b'/', b'\\') {
                            validation_error.get_or_insert(
                                ValidationError::SpecialSchemeMissingFollowingSolidus,
                            );
                        }

                        buffer.push_str(b"//")?;

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

                        let checkpoint = iter.checkpoint();

                        let mut at_sign = None;
                        let mut char_count = 0;

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
                        while let Some(c) = iter.next() {
                            match c {
                                b'@' => {
                                    validation_error
                                        .get_or_insert(ValidationError::InvalidCredentials);
                                    at_sign = Some(char_count);
                                },
                                // EOF code point is implied in the below check
                                b'/' | b'\\' | b'?' | b'#' => break,
                                _ => char_count += 1,
                            }
                        }

                        iter.reset_to(checkpoint);

                        let mut userinfo_char_count = match at_sign {
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
                                if char_count - n == 0 {
                                    return Err(Error::HostMissing);
                                } else {
                                    n
                                }
                            },
                            None => 0,
                        };

                        // Scheme end, plus `://`
                        let userinfo_start = scheme.end + 3;
                        let mut userinfo_stop = userinfo_start;
                        let mut password_token = None;

                        // Here is where we actually parse the userinfo
                        while userinfo_char_count > 0 {
                            userinfo_char_count -= 1;

                            // SAFETY: we have already iterated over these characters so we know
                            // they exist.
                            match unsafe { iter.next().unwrap_unchecked() } {
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
                                            if userinfo_char_count > 0 {
                                                password_token = Some(buffer.push(b':')? - 1);
                                            }
                                        },
                                    }
                                },
                                c => {
                                    #[cfg(test)]
                                    let _c = char::from_u32(*c as u32).unwrap_or_default();

                                    userinfo_stop =
                                        buffer.push_encode_byte(*c, percent::USERINFO)?;
                                },
                            }
                        }

                        let (username, password) = match password_token {
                            Some(n) => (userinfo_start..n, n + 1..userinfo_stop),
                            None => (userinfo_start..userinfo_stop, userinfo_stop..userinfo_stop),
                        };

                        #[cfg(test)]
                        let _username =
                            str::from_utf8(&backing[username.clone()]).unwrap_or_default();
                        #[cfg(test)]
                        let _password =
                            str::from_utf8(&backing[password.clone()]).unwrap_or_default();

                        return Ok((
                            Url {
                                backing,
                                scheme: &backing[scheme],
                                username: &backing[username],
                                password: &backing[password],
                                host: &[],
                                port: &[],
                                path: &[],
                                query: &[],
                                fragment: &[],
                            },
                            validation_error,
                        ));
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
        iter.reset();

        let url = Url {
            backing,

            scheme: &[],
            username: &[],
            password: &[],
            host: &[],
            port: &[],
            path: &[],
            query: &[],
            fragment: &[],
        };

        Ok((url, validation_error))
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn url_parse_userinfo_full() {
        const URL: &str = "http://user:password@example.com";

        let mut backing = [0; 128];
        let (url, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_error, Some(ValidationError::InvalidCredentials));

        assert_str_eq!(url.scheme, b"http");
        assert_str_eq!(url.username, b"user");
        assert_str_eq!(url.password, b"password");
    }

    #[test]
    fn url_parse_userinfo_only_user() {
        const URL: &str = "http://user@example.com";

        let mut backing = [0; 128];
        let (url, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_error, Some(ValidationError::InvalidCredentials));

        assert_str_eq!(url.scheme, b"http");
        assert_str_eq!(url.username, b"user");
        assert_str_eq!(url.password, b"");
    }

    #[test]
    fn url_parse_userinfo_only_password() {
        const URL: &str = "http://:password@example.com";

        let mut backing = [0; 128];
        let (url, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_error, Some(ValidationError::InvalidCredentials));

        assert_str_eq!(url.scheme, b"http");
        assert_str_eq!(url.username, b"");
        assert_str_eq!(url.password, b"password");
    }

    #[test]
    fn url_trim_c0_control_or_space_front() {
        const URL: &str = "\u{0}\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\u{9}\u{10}\u{11}\u{12}\u{13}\u{14}\u{15}\u{16}\u{17}\u{18}\u{19}\u{20}example.com";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_error, Some(ValidationError::InvalidURLUnit));
    }

    #[test]
    fn url_trim_c0_control_or_space_back() {
        const URL: &str = "example.com\u{20}\u{19}\u{18}\u{17}\u{16}\u{15}\u{14}\u{13}\u{12}\u{11}\u{10}\u{9}\u{8}\u{7}\u{6}\u{5}\u{4}\u{3}\u{2}\u{1}\u{0}";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(validation_error, Some(ValidationError::InvalidURLUnit));
    }

    #[test]
    fn url_scheme_missing_following_solidus_file() {
        const URL: &str = "file:c:/my-secret-folder";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_error,
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
    }

    #[test]
    fn url_scheme_missing_following_solidus_special_non_file() {
        const URL: &str = "http:example.com";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_error,
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
    }

    #[test]
    fn url_scheme_missing_expected_double_slash_non_file() {
        const URL: &str = "http://\\\\/\\///example.com";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        assert_eq!(
            validation_error,
            Some(ValidationError::SpecialSchemeMissingFollowingSolidus)
        );
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
    #[should_panic]
    fn url_err_empty_backing() {
        const URL: &str = "example.com";

        let mut backing = [0; 0];
        let _ = Url::new(URL.as_bytes(), &mut backing);
    }
}
