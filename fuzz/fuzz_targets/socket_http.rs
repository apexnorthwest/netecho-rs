#![no_main]

use libfuzzer_sys::fuzz_target;
use netecho_rs::{Config, serve};
use std::io;
use std::net::SocketAddr;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

fn check_socket_result(result: io::Result<()>) {
    if let Err(error) = result {
        match error.kind() {
            io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected => {}
            _ => panic!("unexpected socket error: {error}"),
        }
    }
}

// The server is started once per fuzzing process. Binding a listener for every
// input leaves its port unavailable for 60 s (TIME_WAIT) and, with many parallel
// workers, exhausts the ephemeral port range and fails with AddrInUse.
struct Server {
    runtime: tokio::runtime::Runtime,
    address: SocketAddr,
}

static SERVER: LazyLock<Server> = LazyLock::new(|| {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("failed to create fuzz runtime");
    let listener = runtime
        .block_on(TcpListener::bind("127.0.0.1:0"))
        .expect("failed to bind fuzz listener");
    let address = listener.local_addr().expect("failed to get fuzz address");
    let config = Config::from_env().expect("invalid server configuration");
    runtime.spawn(serve(listener, config, std::future::pending()));
    Server { runtime, address }
});

async fn connect(address: SocketAddr) -> Option<TcpStream> {
    // Client ports can run out when many workers share a machine. That is an
    // environment limit, not a server bug, so back off and skip the input.
    for _ in 0..50 {
        match TcpStream::connect(address).await {
            Ok(client) => return Some(client),
            Err(error) if error.kind() == io::ErrorKind::AddrNotAvailable => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => panic!("failed to connect to fuzz listener: {error}"),
        }
    }
    eprintln!("skipping input: no client ports available");
    None
}

async fn exchange(address: SocketAddr, data: &[u8], half_close: bool) {
    let Some(client) = connect(address).await else {
        return;
    };
    let (mut reader, mut writer) = client.into_split();
    let send = async {
        check_socket_result(writer.write_all(data).await);
        if half_close {
            check_socket_result(writer.shutdown().await);
        }
    };
    let receive = async {
        let mut buffer = [0; 8192];
        loop {
            match reader.read(&mut buffer).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) => {
                    check_socket_result(Err(error));
                    break;
                }
            }
        }
    };
    tokio::join!(send, receive);
}

fuzz_target!(
    init: {
        // A Rust `main` ignores SIGPIPE, so writing to a closed client socket
        // returns EPIPE. `no_main` skips that setup, and the default action
        // would kill the fuzzer instead.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    },
    |data: &[u8]| {
    let server = &*SERVER;
    server.runtime.block_on(async {
        // Delayed or incomplete requests must not stall the fuzzing process.
        let _ = tokio::time::timeout(Duration::from_millis(100), async {
            tokio::join!(
                exchange(server.address, data, false),
                exchange(server.address, data, true)
            );
        })
        .await;
    });
    }
);
