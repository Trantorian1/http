//! A simple HTTP/1.1 server example.
//!
//! Only accepts requests to [`ADDRESS`] on the default route.
//!
//! [`ADDRESS`]

#![allow(clippy::unwrap_used)]

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

    let listener = std::net::TcpListener::bind(ADDRESS).unwrap();
    tracing::info!("Listening on {ADDRESS}");

    let mut global_request_buffer = vec![0; 16 * KB].into_boxed_slice();
    let mut global_response_buffer = vec![0; 64 * KB].into_boxed_slice();
    let mut server = Server::new(&mut global_request_buffer, &mut global_response_buffer);

    // == Main TCP data loop =======================================================================

    for stream in listener.incoming() {
        match stream {
            Ok(mut connection) => {
                server
                    .process(&mut connection)
                    .respond(|request, response| {
                        tracing::info!(
                            path = std::str::from_utf8(request.target.path).unwrap_or_default()
                        );

                        match (request.method, request.target.path) {
                            (methods::GET, b"/") => response.with_status_code(Status::Ok).send(),
                            (methods::GET, [b'/', b'e', b'c', b'h', b'o', b'/', param @ ..]) => {
                                response
                                    .with_status_code(Status::Ok)
                                    .with_content(content::TEXT_PLAIN, param)
                                    .send()
                            },
                            (methods::GET, _) => response.with_status_code(Status::NotFound).send(),
                            (..) => response.with_status_code(Status::NotImplemented).send(),
                        }
                    });
            },
            Err(e) => {
                tracing::error!("error: {e}");
            },
        }
    }
}
