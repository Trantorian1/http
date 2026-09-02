mod iter;

use iter::ByteIter;

use super::*;

impl<'data> Url<'data> {
    /// Based off https://url.spec.whatwg.org/#url-parsing
    pub fn new(
        mut bytes: &[u8],
        backing: &'data mut [u8],
    ) -> Result<(Self, Option<ValidationError>), Error> {
        let mut validation_error = None;

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
    /// https://infra.spec.whatwg.org/#c0-control-or-space
    pub fn c0_control_or_space(c: u8) -> bool {
        c <= b' ' // U+0000 to U+0020
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

        pretty_assertions::assert_eq!(validation_error, Some(ValidationError::InvalidURLUnit));
    }

    #[test]
    fn url_trim_c0_control_or_space_back() {
        const URL: &str = "example.com\u{20}\u{19}\u{18}\u{17}\u{16}\u{15}\u{14}\u{13}\u{12}\u{11}\u{10}\u{9}\u{8}\u{7}\u{6}\u{5}\u{4}\u{3}\u{2}\u{1}\u{0}";

        let mut backing = [0; 128];
        let (_, validation_error) = Url::new(URL.as_bytes(), &mut backing).unwrap();

        pretty_assertions::assert_eq!(validation_error, Some(ValidationError::InvalidURLUnit));
    }
}
