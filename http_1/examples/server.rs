use std::net::TcpListener;

use http_1::prelude::*;
use http_core::prelude::*;
use tracing_subscriber::prelude::*;

const ADDRESS: &str = "127.0.0.1:4221";

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

    let mut server = Server::default();

    // == Main TCP data loop =======================================================================

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                server
                    .process(stream)
                    .respond(|request, response| match request.target {
                        b"/" => response.with_status_code(Status::Ok).respond(),
                        _ => response.with_status_code(Status::NotFound).respond(),
                    });
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
