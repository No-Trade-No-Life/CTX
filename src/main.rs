use std::net::SocketAddr;

use ctx::{App, HttpServerError};

#[tokio::main]
async fn main() -> Result<(), HttpServerError> {
    App::from_home()?
        .serve(SocketAddr::from(([127, 0, 0, 1], 8080)))
        .await
}
