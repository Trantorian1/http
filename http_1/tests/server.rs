use http_1::prelude::*;
use http_primitives::*;

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

    let mut global_request_buffer = [0; 1024];
    let mut global_response_buffer = [0; 1024];
    let mut server = Server::new(&mut global_request_buffer, &mut global_response_buffer);

    server.process(&mut stream).respond(|request, response| {
        assert_eq!(request.method(), methods::GET);
        assert_eq!(request.target(), b"/");

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

    assert_eq!(response.protocol(), PROTOCOL);
    assert_eq!(response.status(), Status::Ok.code());
}
