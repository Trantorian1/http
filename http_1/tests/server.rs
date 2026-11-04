#![allow(missing_docs)]

use http_1::prelude::*;
use http_primitives::*;
use macro_util::prelude::*;

#[test]
fn get_origin_200_ok() {
    let mut stream_backing = [0; 1024];
    let mut stream = ByteStream::new(&mut stream_backing);

    let mut local_request_buffer = [0; 1024];
    let mut local_response_buffer = [0; 1024];
    let mut client = Client::new(&mut local_request_buffer, &mut local_response_buffer);

    client
        .request(&mut stream)
        .get()
        .target(b"/")
        .send()
        .expect("send should not fail");

    let mut global_buffer_request = [0; 1024];
    let mut global_buffer_response = [0; 1024];
    let mut global_buffer_url = [0; 1024];

    let mut server = Server::new(
        &mut global_buffer_request,
        &mut global_buffer_response,
        &mut global_buffer_url,
    );

    server.process(&mut stream).respond(|request, response| {
        assert_utf8_eq!(request.method, methods::GET);
        assert_utf8_eq!(request.target.path(), b"/");

        response
            .with_status_code(Status::Ok)
            .send()
            .expect("send should not fail");

        Ok(())
    });

    let response = client
        .response(&mut stream)
        .process()
        .expect("Response should not fail parsing");

    assert_utf8_eq!(response.protocol(), PROTOCOL);
    assert_utf8_eq!(response.status(), Status::Ok.code());
}
