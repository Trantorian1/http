mod buffer;
mod error;
mod iter;

use buffer::UrlBuffer;
pub use error::Error;
pub use error::ValidationError;
use iter::ByteIter;

use super::Url;

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
        // Schemes are bounded by a first ASCII alphabetic character and an end U+003A (:)
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

                if &buffer[scheme.clone()] == b"file" {
                    if !iter.starts_with(b"//") {
                        validation_error
                            .get_or_insert(ValidationError::SpecialSchemeMissingFollowingSolidus);
                    }
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
    /// See the [URL standard], C0 control or space
    ///
    /// > _"A C0 control or space is a [C0 control] or U+0020 SPACE."_
    ///
    /// [URL standard]: https://infra.spec.whatwg.org/#c0-control-or-space
    /// [C0 control]: https://infra.spec.whatwg.org/#c0-control
    pub(super) fn c0_control_or_space(c: u8) -> bool {
        c <= b' ' // U+0000 to U+0020
    }

    /// See the [URL standard], ASCII alpha
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
    fn url_file_scheme_missing_following_solidus() {
        const URL: &str = "file:c:/my-secret-folder";

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
