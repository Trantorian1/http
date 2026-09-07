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
                        // any other section delimiter but the userinfo is empty AND we have seen a
                        // terminating U+0040 (@) code point, then we return a host-missing error.
                        //
                        // =========================================================================

                        let mut at_sign_seen = false;
                        let mut password_token_seen = false;
                        let mut password_token = 0;
                        let mut userinfo_char_count = 0;
                        let mut last_char = 0;

                        loop {
                            let c = iter.next();

                            match c {
                                Some(b'@') => {
                                    at_sign_seen = true;
                                    // percent-encoded U+0040 (@)
                                    last_char = buffer.push_str(b"%40")?;
                                    userinfo_char_count += 1;
                                },
                                Some(b':') => {
                                    if !password_token_seen {
                                        password_token_seen = true;
                                        password_token = buffer.push(b':')?;
                                        last_char = password_token;
                                    } else {
                                        // percent-encoded U+003A (:)
                                        last_char = buffer.push_str(b"%3A")?;
                                        userinfo_char_count += 1;
                                    }
                                },
                                None | Some(b'/') | Some(b'\\') | Some(b'?') | Some(b'#') => {
                                    if at_sign_seen && userinfo_char_count == 1 {
                                        return Err(Error::HostMissing);
                                    }

                                    break;
                                },
                                Some(c) => {
                                    buffer.push_str(percent::encode_byte(*c))?;
                                    userinfo_char_count += 1;
                                },
                            }
                        }

                        if at_sign_seen {
                            last_char -= 1;
                        }

                        let (username, password) = if password_token_seen {
                            (
                                scheme.end + 3..scheme.end + 3 + password_token,
                                scheme.end + 3 + password_token..last_char,
                            )
                        } else {
                            (scheme.end + 3..last_char, last_char..last_char)
                        };

                        #[cfg(test)]
                        let _username = &backing[username.clone()];
                        #[cfg(test)]
                        let _password = &backing[password.clone()];

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

    pub(super) fn special_scheme_other_than_file(scheme: &[u8]) -> bool {
        matches!(scheme, b"ftp" | b"http" | b"https" | b"ws" | b"wss")
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn url_parse_full() {
        const URL: &str =
            "\t\n\rhttp://user:password@example.com/cute/cat/picture?cat=name&color=ginger#meow";

        let mut backing = [0; 128];
        let (url, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        // assert_eq!(validation_error, None);

        assert_str_eq!(url.scheme, b"http");
        assert_str_eq!(url.username, b"user");
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
    #[should_panic]
    fn url_err_empty_backing() {
        const URL: &str = "example.com";

        let mut backing = [0; 0];
        let _ = Url::new(URL.as_bytes(), &mut backing);
    }
}
