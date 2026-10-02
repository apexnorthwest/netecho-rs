use base64::Engine;
use http_body_util::BodyExt as _;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::LazyLock;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

// Generate some random data to use in response bodies (10MiB) (lazy static)
// Allocated on the heap (not as a stack array) to avoid overflowing small thread stacks.
static RANDOM_DATA: LazyLock<Box<[u8]>> = LazyLock::new(|| {
    let mut data = vec![0u8; 1024 * 1024 * 10];
    rand::fill(&mut data[..]);
    data.into_boxed_slice()
});

async fn handle_request(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path();

    if path == "/" || path == "/ok" {
        let mut resp = Response::new(Full::new(Bytes::from(r#"{"status":"ok"}"#)));
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        Ok(resp)
    } else if path == "/echo" {
        let headers = req.headers().clone();
        let method = req.method().clone();
        let query = req.uri().query().unwrap_or("").to_string();
        let body_bytes = req.into_body().collect().await.unwrap().to_bytes();
        let body_text = String::from_utf8_lossy(&body_bytes);

        let mut resp = Response::new(Full::new(Bytes::from(format!(
            "Method: {:?}\n\nQuery: {}\n\nHeaders:\n{:#?}\n\nBody:\n{}",
            method,
            query,
            headers, body_text
        ))));
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("text/plain"),
        );
        Ok(resp)
    } else if path == "/add" {
        let query = req.uri().query().unwrap_or("");
        let mut n1: i64 = 0;
        let mut n2: i64 = 0;
        for param in query.split('&') {
            let mut parts = param.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                if key == "n1" {
                    n1 = value.parse::<i64>().unwrap_or(0);
                } else if key == "n2" {
                    n2 = value.parse::<i64>().unwrap_or(0);
                }
            }
        }
        let result = n1 + n2;
        let mut resp = Response::new(Full::new(Bytes::from(format!(
            r#"{{"result":{}}}"#,
            result
        ))));
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        Ok(resp)
    } else if path == "/bytes" || path == "/text" {
        let query = req.uri().query().unwrap_or("");
        let mut n = 1;
        for param in query.split('&') {
            let mut parts = param.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next())
                && key == "n"
            {
                n = value.parse::<usize>().unwrap_or(1);
            }
        }
        if n > RANDOM_DATA.len() {
            n = RANDOM_DATA.len();
        }
        let mut resp;
        if path == "/bytes" {
            resp = Response::new(Full::new(Bytes::from(&RANDOM_DATA[..n])));
        } else {
            let engine = base64::engine::general_purpose::STANDARD;
            resp = Response::new(Full::new(Bytes::from(engine.encode(&RANDOM_DATA[..n]))));
        }
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static(if path == "/bytes" {
                "application/octet-stream"
            } else {
                "text/plain"
            }),
        );
        Ok(resp)
    } else if path == "/delay" {
        let query = req.uri().query().unwrap_or("");
        let mut ms = 1000;
        for param in query.split('&') {
            let mut parts = param.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next())
                && key == "ms"
            {
                ms = value.parse::<u64>().unwrap_or(1000);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        let mut resp = Response::new(Full::new(Bytes::from(format!(
            r#"{{"status":"ok","waited":{}}}"#,
            ms
        ))));
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        Ok(resp)
    } else if path == "/random" || path == "/randtext" {
        let query = req.uri().query().unwrap_or("");
        let mut n = 1;
        for param in query.split('&') {
            let mut parts = param.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next())
                && key == "n"
            {
                n = value.parse::<usize>().unwrap_or(1);
            }
        }
        if n > 1024*1024*10 {
            n = 1024*1024*10;
        }
        let mut my_data: Box<[u8]> = vec![0u8; n].into_boxed_slice();
        rand::fill(&mut my_data);
        let mut resp;
        if path == "/random" {
            resp = Response::new(Full::new(Bytes::from(my_data)));
        } else {
            let engine = base64::engine::general_purpose::STANDARD;
            resp = Response::new(Full::new(Bytes::from(engine.encode(&my_data))));
        }
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static(if path == "/random" {
                "application/octet-stream"
            } else {
                "text/plain"
            }),
        );
        Ok(resp)
    } else {
        let mut resp = Response::new(Full::new(Bytes::from(r#"{"status":"not found"}"#)));
        *resp.status_mut() = hyper::StatusCode::NOT_FOUND;
        *resp.headers_mut() = hyper::HeaderMap::new();
        resp.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        Ok(resp)
    }
}

#[tokio::main]
async fn main() {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to port 8080");
    println!("Server running on http://0.0.0.0:8080");

    // Set up SIGINT signal handler
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to set up SIGINT handler");

    loop {
        tokio::select! {
            _ = sigint.recv() => {
                println!("\nReceived SIGINT, shutting down...");
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        tokio::task::spawn(async move {
                            if let Err(err) = http1::Builder::new()
                                .serve_connection(io, service_fn(handle_request))
                                .await
                            {
                                eprintln!("Error serving connection: {:?}", err);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {:?}", e);
                    }
                }
            }
        }
    }
    println!("Server shut down gracefully");
}
