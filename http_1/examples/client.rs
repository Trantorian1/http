#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use http_1::prelude::*;
use http_primitives::prelude::*;
use tracing_subscriber::prelude::*;

// FIXME: we have a lot of work to do before this can output a valid request

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

    // == Client initialization ====================================================================

    tracing::info!("Initiating connection...");
    let mut connection = std::net::TcpStream::connect("example.com:80").unwrap();

    tracing::info!("Connected to `example.com`");

    let mut local_request_buffer = vec![0; 16 * KB].into_boxed_slice();
    let mut local_response_buffer = vec![0; 64 * KB].into_boxed_slice();
    let mut client = Client::new(&mut local_request_buffer, &mut local_response_buffer);

    // == Main TCP data loop =======================================================================

    tracing::info!("Sending request...");

    client
        .request(&mut connection)
        .get()
        .send()
        .expect("Failed to send request");

    // We currently don't have any way to wait for server responses.
    std::thread::sleep(std::time::Duration::from_secs(5));

    let response = client
        .response(&mut connection)
        .process()
        .expect("Failed to read response");

    assert_eq!(response.status(), Status::Ok.code());
}
