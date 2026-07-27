//! A simple HTTP/1.1 server implementation, based off Codecrafter's [build your own HTTP server].
//!
//! See [RFC9112] for an overview of the specs.
//!
//! [build your own HTTP server]: https://app.codecrafters.io/courses/http-server/overview
//! [RFC9112]: https://datatracker.ietf.org/doc/html/rfc9112#section-2.1

use std::net::TcpListener;
use tracing_subscriber::prelude::*;

const ADDRESS: &str = "127.0.0.1:4221";
const KB: usize = 1_000;

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

    let mut global_response_buffer = http_server::Buffer::<{ 64 * KB }>::new();

    // == Main TCP data loop =======================================================================

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                tracing::trace!("accepted new connection");

                if let Err(err) = http_server::Response::new(stream, &mut global_response_buffer)
                    .with_status_code(http_server::response::Status::Ok)
                    .respond()
                {
                    tracing::error!("Failed to send data back to TPC stream: {err}");
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
