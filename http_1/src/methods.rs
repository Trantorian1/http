//! Available [HTTP/1.1 methods].
//!
//! > _"The request method token is the primary source of request semantics; it indicates the
//! > purpose for which the client has made this request and what is expected by the client as a
//! > successful result."_
//!
//! [HTTP/1.1 methods]: https://www.rfc-editor.org/info/rfc9110/#section-9.3

/// See [RFC9110], GET method
///
/// > _"The GET method requests transfer of a current [selected representation] for the [target
/// > resource]. A successful response reflects the quality of "sameness" identified by the target
/// > URI ([Section 1.2.2] of [\[URI\]]). Hence, retrieving identifiable information via HTTP is
/// > usually performed by making a GET request on an identifier associated with the potential for
/// > providing that information in a [200 (OK)] response."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-get
/// [selected representation]: https://www.rfc-editor.org/info/rfc9110/#selected.representation
/// [target resource]: https://www.rfc-editor.org/info/rfc9110/#target.resource
/// [Section 1.2.2]: https://www.rfc-editor.org/info/rfc3986/#section-1.2.2
/// [\[URI\]]: https://www.rfc-editor.org/info/rfc9110/#URI
/// [200 (OK)]: http_primitives::Status::Ok
pub const GET: &[u8] = b"GET";

/// See [RFC9110], HEAD method
///
/// > _"The HEAD method is identical to GET except that the server **MUST NOT** send content in the
/// > response. HEAD is used to obtain metadata about the [selected representation] without
/// > transferring its representation data, often for the sake of testing hypertext links or finding
/// > recent modifications."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-head
/// [selected representation]: https://www.rfc-editor.org/info/rfc9110/#selected.representation
pub const HEAD: &[u8] = b"HEAD";

/// See [RFC9110], POST method
///
/// > _"The POST method requests that the [target resource] process the representation enclosed in
/// > the request according to the resource's own specific semantics. For example, POST is used for
/// > the following functions (among others):_
/// >
/// > - _Providing a block of data, such as the fields entered into an HTML form, to a data-handling
/// >   process;_
/// >
/// > - _Posting a message to a bulletin board, newsgroup, mailing list, blog, or similar group of
/// >   articles;_
/// >
/// > - _Creating a new resource that has yet to be identified by the origin server; and_
/// >
/// > - _Appending data to a resource's existing representation(s)."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-post
/// [target resource]: https://www.rfc-editor.org/info/rfc9110/#target.resource
pub const POST: &[u8] = b"POST";

/// See [RFC9110], PUT method
///
/// > _"The PUT method requests that the state of the [target resource] be created or replaced with
/// > the state defined by the representation enclosed in the request message content. A successful
/// > PUT of a given representation would suggest that a subsequent GET on that same target resource
/// > will result in an equivalent representation being sent in a [200 (OK)] response. However, there
/// > is no guarantee that such a state change will be observable, since the target resource might
/// > be acted upon by other user agents in parallel, or might be subject to dynamic processing by
/// > the origin server, before any subsequent GET is received. A successful response only implies
/// > that the user agent's intent was achieved at the time of its processing by the origin server."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-put
/// [target resource]: https://www.rfc-editor.org/info/rfc9110/#target.resource
/// [200 (OK)]: http_primitives::Status::Ok
pub const PUT: &[u8] = b"PUT";

/// See [RFC9110], DELETE method
///
/// > _"The DELETE method requests that the origin server remove the association between the [target
/// > resource] and its current functionality. In effect, this method is similar to the "rm" command
/// > in UNIX: it expresses a deletion operation on the URI mapping of the origin server rather than
/// > an expectation that the previously associated information be deleted."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-delete
/// [target resource]: https://www.rfc-editor.org/info/rfc9110/#target.resource
pub const DELETE: &[u8] = b"DELETE";

/// See [RFC9110], CONNECT method
///
/// > _"The CONNECT method requests that the recipient establish a tunnel to the destination origin
/// > server identified by the request target and, if successful, thereafter restrict its behavior
/// > to blind forwarding of data, in both directions, until the tunnel is closed. Tunnels are
/// > commonly used to create an end-to-end virtual connection, through one or more proxies, which
/// > can then be secured using TLS (Transport Layer Security, [\[TLS13\]])."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-connect
/// [\[TLS13\]]: https://www.rfc-editor.org/info/rfc9110/#TLS13
pub const CONNECT: &[u8] = b"CONNECT";

/// See [RFC9110], OPTIONS method
///
/// > _"The OPTIONS method requests information about the communication options available for the
/// > target resource, at either the origin server or an intervening intermediary. This method
/// > allows a client to determine the options and/or requirements associated with a resource, or
/// > the capabilities of a server, without implying a resource action."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-options
pub const OPTIONS: &[u8] = b"OPTIONS";

/// See [RFC9110], TRACE method
///
/// > _"The TRACE method requests a remote, application-level loop-back of the request message.
/// > The final recipient of the request **SHOULD** reflect the message received, excluding some fields
/// > described below, back to the client as the content of a [200 (OK)] response. The "message/http"
/// > format ([Section 10.1] of [\[HTTP/1.1\]]) is one way to do so. The final recipient is either the
/// > origin server or the first server to receive a [Max-Forwards] value of zero (0) in the request
/// > [(Section 7.6.2)]."_
///
/// [RFC9110]: https://www.rfc-editor.org/info/rfc9110/#name-trace
/// [200 (OK)]: http_primitives::Status::Ok
/// [Section 10.1]: https://www.rfc-editor.org/info/rfc9112/#section-10.1
/// [\[HTTP/1.1\]]: https://www.rfc-editor.org/info/rfc9110/#HTTP11
/// [Max-Forwards]: https://www.rfc-editor.org/info/rfc9110/#field.max-forwards
/// [(Section 7.6.2)]: https://www.rfc-editor.org/info/rfc9110/#field.max-forwards
pub const TRACE: &[u8] = b"TRACE";
