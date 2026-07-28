mod error;

pub use error::Error;

mod parsers;

pub struct Request<'a, 'b, const SIZE: usize, R: std::io::Read> {
    stream: &'a mut R,
    buffer: &'b mut crate::Buffer<SIZE, crate::buffer::ReadIn>,
}

impl<'a, 'b, const SIZE: usize, R: std::io::Read> Request<'a, 'b, SIZE, R> {
    pub fn new(
        stream: &'a mut R,
        buffer: &'b mut crate::Buffer<SIZE, crate::buffer::ReadIn>,
    ) -> Self {
        Self { stream, buffer }
    }

    pub fn process(self) -> Result<RequestInfo<'b>, crate::buffer::Error> {
        let (method, target) = self.buffer.read_in(self.stream, |reader| {
            let method = reader.read(parsers::method)?;
            let target = reader.read(parsers::target)?;

            reader.read(parsers::protocol)?;
            reader.read(parsers::crlf)?;

            // while let Some(header) = reader.read(header)? {
            //     // process header
            // }

            reader.read(parsers::crlf)?;

            Ok((method, target))
        })?;

        Ok(RequestInfo {
            method: &self.buffer[method],
            target: &self.buffer[target],
        })
    }
}

pub struct RequestInfo<'a> {
    /// See [RFC9112], method.
    ///
    /// > _"The method token indicates the request method to be performed on the target resource.
    /// > The request method is case-sensitive."_
    /// >
    /// > ```text
    /// >   method         = token
    /// > ```
    /// >
    /// > _"The request methods defined by this specification can be found in [Section 9] of
    /// > [[HTTP]], along with information regarding the HTTP method registry and considerations for
    /// > defining new methods."_
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#name-method
    /// [Section 9]: https://www.rfc-editor.org/rfc/rfc9110#section-9
    /// [[HTTP]]: https://datatracker.ietf.org/doc/html/rfc9110
    pub method: &'a [u8],

    /// See [RFC9112], request target.
    ///
    /// > _"The request-target identifies the target resource upon which to apply the request. The
    /// > client derives a request-target from its desired target URI. There are four distinct
    /// > formats for the request-target, depending on both the method being requested and whether
    /// > the request is to a proxy."_
    /// >
    /// > ```text
    /// >   request-target = origin-form
    /// >                  / absolute-form
    /// >                  / authority-form
    /// >                  / asterisk-form
    /// > ```
    /// >
    /// > _"No whitespace is allowed in the request-target. Unfortunately, some user agents fail to
    /// > properly encode or exclude whitespace found in hypertext references, resulting in those
    /// > disallowed characters being sent as the request-target in a malformed request-line."_
    /// >
    /// > _"Recipients of an invalid request-line **SHOULD** respond with either a 400 (Bad Request)
    /// > error or a 301 (Moved Permanently) redirect with the request-target properly encoded. A
    /// > recipient **SHOULD NOT** attempt to autocorrect and then process the request without a
    /// > redirect, since the invalid request-line might be deliberately crafted to bypass security
    /// > filters along the request chain."_
    /// >
    /// > _"A server **MUST** respond with a 400 (Bad Request) status code to any HTTP/1.1 request
    /// > message that lacks a Host header field and to any request message that contains more than
    /// > one Host header field line or a Host header field with an invalid field value.
    ///
    /// [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#section-3.2
    pub target: &'a [u8],
}

impl<'a> std::fmt::Debug for RequestInfo<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestInfo")
            .field("method", &std::str::from_utf8(self.method).unwrap())
            .field("target", &std::str::from_utf8(self.target).unwrap())
            .finish()
    }
}
