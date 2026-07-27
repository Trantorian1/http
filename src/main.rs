use std::io::Write as _;
use std::net::TcpListener;
use tracing_subscriber::prelude::*;

const ADDRESS: &str = "127.0.0.1:4221";

fn main() {
    let level = tracing::level_filters::LevelFilter::INFO;
    let env = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(level.into())
        .from_env_lossy();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(env)
        .init();

    let listener = TcpListener::bind(ADDRESS).unwrap();
    tracing::info!("Listening on {ADDRESS}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                tracing::trace!("accepted new connection");

                if let Err(err) = stream.write_all("HTTP/1.1 200 OK \r\n\r\n".as_bytes()) {
                    tracing::error!("Failed to send data back to TPC stream: {err}");
                }
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
}
