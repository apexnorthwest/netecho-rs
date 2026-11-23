use netecho_rs::{Config, serve};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() {
    let config = Config::from_env().expect("Invalid configuration");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to set up SIGTERM handler");
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to port 8080");
    println!("Server running on http://0.0.0.0:8080");

    serve(listener, config, async {
        tokio::select! {
            _ = sigint.recv() => println!("\nReceived SIGINT, shutting down..."),
            _ = sigterm.recv() => println!("\nReceived SIGTERM, shutting down..."),
        }
    })
    .await;
    println!("Server shut down");
}
