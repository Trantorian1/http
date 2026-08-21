//! A simple HTTP/1.1 server example.
//!
//! Only accepts requests to [`ADDRESS`] on the default route.
//!
//! [`ADDRESS`]

use std::net::TcpListener;

use http_1::prelude::*;
use http_primitives::prelude::*;
use tracing_subscriber::prelude::*;

/// Default listening address.
pub const ADDRESS: &str = "127.0.0.1:4221";

fn main() {
    // == Logging ==================================================================================

    let default_level = tracing::level_filters::LevelFilter::INFO;

    let fmt = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_file(true)
        .with_line_number(true);
    let env = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(default_level.into())
        .from_env_lossy();

    tracing_subscriber::registry().with(fmt).with(env).init();

    // == Expose server on TCP port 4221 ===========================================================

    let listener = TcpListener::bind(ADDRESS).unwrap();
    tracing::info!("Listening on {ADDRESS}");

    let mut global_request_buffer = [0; 8 * KB];
    let mut global_response_buffer = [0; 64 * KB];
    let mut server = Server::new(&mut global_request_buffer, &mut global_response_buffer);

    // == Main TCP data loop =======================================================================

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                server
                    .process(stream)
                    .respond(|request, response| match request.target {
                        b"/" => response.with_status_code(Status::Ok).send(),
                        _ => response.with_status_code(Status::NotFound).send(),
                    });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
