use base64::Engine;
use http_body_util::BodyExt as _;
use http_body_util::{Full, LengthLimitError, Limited};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::convert::Infallible;
use std::future::Future;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinSet;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
pub struct Config {
    echo_max_bytes: usize,
    echo_secrets: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let echo_max_bytes = match std::env::var("NETECHO_ECHO_MAX_BYTES") {
            Ok(value) => value
                .parse::<usize>()
                .map_err(|_| "NETECHO_ECHO_MAX_BYTES must be a non-negative integer".to_string())?,
            Err(std::env::VarError::NotPresent) => 10 * 1024 * 1024,
            Err(err) => return Err(format!("Invalid NETECHO_ECHO_MAX_BYTES: {err}")),
        };
        let echo_secrets = match std::env::var("NETECHO_ECHO_SECRETS") {
            Ok(value) => value
                .parse::<bool>()
                .map_err(|_| "NETECHO_ECHO_SECRETS must be true or false".to_string())?,
            Err(std::env::VarError::NotPresent) => false,
            Err(err) => return Err(format!("Invalid NETECHO_ECHO_SECRETS: {err}")),
        };
        Ok(Self {
            echo_max_bytes,
            echo_secrets,
        })
    }
}

fn error_response(status: StatusCode, message: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(message.as_bytes())));
    *response.status_mut() = status;
    response
}

// Generate some random data to use in response bodies (10MiB) (lazy static)
// Allocated on the heap (not as a stack array) to avoid overflowing small thread stacks.
static RANDOM_DATA: LazyLock<Box<[u8]>> = LazyLock::new(|| {
    let mut data = vec![0u8; 1024 * 1024 * 10];
    rand::fill(&mut data[..]);
    data.into_boxed_slice()
});

async fn handle_request<B>(
    req: Request<B>,
    config: Config,
) -> Result<Response<Full<Bytes>>, Infallible>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
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
        let mut headers = req.headers().clone();
        if !config.echo_secrets {
            for (name, value) in &mut headers {
                if ["token", "secret", "cookie", "authorization"]
                    .iter()
                    .any(|secret| name.as_str().contains(secret))
                {
                    *value = hyper::header::HeaderValue::from_static("[REDACTED]");
                }
            }
        }
        let method = req.method().clone();
        let query = req.uri().query().unwrap_or("").to_string();
        let body_bytes = match Limited::new(req.into_body(), config.echo_max_bytes)
            .collect()
            .await
        {
            Ok(body) => body.to_bytes(),
            Err(err) if err.is::<LengthLimitError>() => {
                return Ok(error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Request body too large",
                ));
            }
            Err(err) => {
                eprintln!("Error reading request body: {err}");
                return Ok(error_response(
                    StatusCode::BAD_REQUEST,
                    "Error reading request body",
                ));
            }
        };
        let body_text = String::from_utf8_lossy(&body_bytes);

        let mut resp = Response::new(Full::new(Bytes::from(format!(
            "Method: {:?}\n\nQuery: {}\n\nHeaders:\n{:#?}\n\nBody:\n{}",
            method, query, headers, body_text
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
        // This endpoint exercises arithmetic; an overflowing result is allowed to wrap.
        let result = n1.checked_add(n2).unwrap_or_else(|| n1.wrapping_add(n2));
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
        if n > 1024 * 1024 * 10 {
            n = 1024 * 1024 * 10;
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

async fn handle_with_timeout<B>(
    req: Request<B>,
    config: Config,
    timeout: Duration,
) -> Result<Response<Full<Bytes>>, Infallible>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    match tokio::time::timeout(timeout, handle_request(req, config)).await {
        Ok(response) => response,
        Err(_) => {
            eprintln!("Request handling timed out");
            Ok(error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "Request timed out",
            ))
        }
    }
}

pub async fn serve(listener: TcpListener, config: Config, shutdown: impl Future<Output = ()>) {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                break;
            }
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        let mut shutdown_rx = shutdown_rx.clone();
                        connections.spawn(async move {
                            let mut builder = http1::Builder::new();
                            builder.timer(TokioTimer::new()).header_read_timeout(REQUEST_TIMEOUT);
                            let connection = builder.serve_connection(
                                io,
                                service_fn(move |req| handle_with_timeout(req, config, REQUEST_TIMEOUT)),
                            );
                            tokio::pin!(connection);
                            let result = tokio::select! {
                                result = &mut connection => result,
                                _ = shutdown_rx.changed() => {
                                    connection.as_mut().graceful_shutdown();
                                    connection.await
                                }
                            };
                            if let Err(err) = result {
                                eprintln!("Error serving connection: {:?}", err);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {:?}", e);
                    }
                }
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(err) = result {
                    eprintln!("Connection task failed: {err}");
                }
            }
        }
    }
    drop(listener);
    let _ = shutdown_tx.send(true);
    if tokio::time::timeout(REQUEST_TIMEOUT, async {
        while let Some(result) = connections.join_next().await {
            if let Err(err) = result {
                eprintln!("Connection task failed: {err}");
            }
        }
    })
    .await
    .is_err()
    {
        eprintln!("Shutdown timed out; closing remaining connections");
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::oneshot;

    fn config() -> Config {
        Config {
            echo_max_bytes: 4,
            echo_secrets: false,
        }
    }

    #[test]
    fn config_from_environment() {
        for case in ["default", "override", "invalid-size", "invalid-secrets"] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "tests::config_from_environment_child"])
                .env("NETECHO_CONFIG_TEST", case)
                .env_remove("NETECHO_ECHO_MAX_BYTES")
                .env_remove("NETECHO_ECHO_SECRETS");
            match case {
                "override" => {
                    command.env("NETECHO_ECHO_MAX_BYTES", "17");
                    command.env("NETECHO_ECHO_SECRETS", "true");
                }
                "invalid-size" => {
                    command.env("NETECHO_ECHO_MAX_BYTES", "-1");
                }
                "invalid-secrets" => {
                    command.env("NETECHO_ECHO_SECRETS", "yes");
                }
                _ => {}
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{case}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
    }

    #[test]
    fn config_from_environment_child() {
        match std::env::var("NETECHO_CONFIG_TEST").as_deref() {
            Ok("default") => {
                let config = Config::from_env().unwrap();
                assert_eq!(config.echo_max_bytes, 10 * 1024 * 1024);
                assert!(!config.echo_secrets);
            }
            Ok("override") => {
                let config = Config::from_env().unwrap();
                assert_eq!(config.echo_max_bytes, 17);
                assert!(config.echo_secrets);
            }
            Ok("invalid-size" | "invalid-secrets") => assert!(Config::from_env().is_err()),
            _ => {}
        }
    }

    async fn body_text(response: Response<Full<Bytes>>) -> String {
        String::from_utf8(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn echo_limits_body_and_redacts_all_matching_headers() {
        let request = Request::builder()
            .uri("/echo")
            .header("X-Api-Token", "token-value")
            .header("x-api-token", "another-token")
            .header("x-secret-key", "secret-value")
            .header("cookie", "cookie-value")
            .header("proxy-authorization", "authorization-value")
            .header("x-ordinary", "visible-value")
            .body(Full::new(Bytes::from_static(b"1234")))
            .unwrap();
        let response = handle_request(request, config()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert_eq!(body.matches("[REDACTED]").count(), 5);
        for secret in [
            "token-value",
            "another-token",
            "secret-value",
            "cookie-value",
            "authorization-value",
        ] {
            assert!(!body.contains(secret));
        }
        assert!(body.contains("visible-value"));
        assert!(body.ends_with("1234"));

        for (limit, input, status) in [
            (4, "12345", StatusCode::PAYLOAD_TOO_LARGE),
            (0, "", StatusCode::OK),
            (0, "1", StatusCode::PAYLOAD_TOO_LARGE),
        ] {
            let request = Request::builder()
                .uri("/echo")
                .body(Full::new(Bytes::from(input)))
                .unwrap();
            let response = handle_request(
                request,
                Config {
                    echo_max_bytes: limit,
                    ..config()
                },
            )
            .await
            .unwrap();
            assert_eq!(response.status(), status);
        }
    }

    #[tokio::test]
    async fn echo_secrets_can_be_enabled() {
        let request = Request::builder()
            .uri("/echo")
            .header("authorization", "visible-secret")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let response = handle_request(
            request,
            Config {
                echo_secrets: true,
                ..config()
            },
        )
        .await
        .unwrap();
        assert!(body_text(response).await.contains("visible-secret"));
    }

    #[tokio::test]
    async fn addition_wraps_in_both_directions() {
        for (query, expected) in [
            ("n1=9223372036854775807&n2=1", "-9223372036854775808"),
            ("n1=-9223372036854775808&n2=-1", "9223372036854775807"),
            ("n1=2&n2=3", "5"),
        ] {
            let request = Request::builder()
                .uri(format!("/add?{query}"))
                .body(Full::new(Bytes::new()))
                .unwrap();
            let response = handle_request(request, config()).await.unwrap();
            assert_eq!(
                body_text(response).await,
                format!(r#"{{"result":{expected}}}"#)
            );
        }
    }

    #[tokio::test]
    async fn request_timeout_preserves_nine_second_delay() {
        let request = Request::builder()
            .uri("/delay?ms=9000")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let response = handle_with_timeout(request, config(), REQUEST_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn request_times_out_after_ten_seconds() {
        let request = Request::builder()
            .uri("/delay?ms=11000")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let started = tokio::time::Instant::now();
        let response = handle_with_timeout(request, config(), REQUEST_TIMEOUT)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(started.elapsed() >= REQUEST_TIMEOUT);
        assert!(started.elapsed() < Duration::from_secs(15));
    }

    async fn start_server() -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(serve(listener, config(), async {
            let _ = shutdown_rx.await;
        }));
        (addr, shutdown_tx, task)
    }

    async fn read_headers(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        loop {
            let mut byte = [0];
            stream.read_exact(&mut byte).await.unwrap();
            bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn shutdown_drains_in_flight_request() {
        let (addr, shutdown, server) = start_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n")
            .await
            .unwrap();
        assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 100"));
        shutdown.send(()).unwrap();
        stream.write_all(b"1234").await.unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        assert!(response.ends_with("1234"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn chunked_body_limit_and_keep_alive() {
        let (addr, shutdown, server) = start_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /ok HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 200"));
        let mut body = [0; 15];
        stream.read_exact(&mut body).await.unwrap();
        assert_eq!(&body, br#"{"status":"ok"}"#);
        stream
            .write_all(b"POST /echo HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n12\r\n3\r\n345\r\n0\r\n\r\n")
            .await
            .unwrap();
        assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 413"));
        shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn fuzz_seeds_reach_their_endpoints_over_tcp() {
        let (addr, shutdown, server) = start_server().await;
        for (request, status, expected) in [
            (
                &include_bytes!("../fuzz/seeds/socket_http/ok")[..],
                "200",
                "application/json",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/echo")[..],
                "200",
                "Body:\ntest",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/chunked")[..],
                "200",
                "Body:\ntest",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/add")[..],
                "200",
                "-9223372036854775808",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/bytes")[..],
                "200",
                "application/octet-stream",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/text")[..],
                "200",
                "text/plain",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/random")[..],
                "200",
                "application/octet-stream",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/randtext")[..],
                "200",
                "text/plain",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/delay")[..],
                "200",
                r#""waited":0"#,
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/pipelined")[..],
                "200",
                r#""result":5"#,
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/not_found")[..],
                "404",
                "not found",
            ),
            (
                &include_bytes!("../fuzz/seeds/socket_http/malformed")[..],
                "400",
                "",
            ),
        ] {
            let mut stream = TcpStream::connect(addr).await.unwrap();
            stream.write_all(request).await.unwrap();
            let mut response = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
                .await
                .unwrap()
                .unwrap();
            let response = String::from_utf8_lossy(&response);
            assert!(
                response.starts_with(&format!("HTTP/1.1 {status}")),
                "request: {:?}, response: {response}",
                String::from_utf8_lossy(request)
            );
            assert!(
                response.contains(expected),
                "request: {:?}, response: {response}",
                String::from_utf8_lossy(request)
            );
        }
        shutdown.send(()).unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn incomplete_headers_time_out() {
        let (addr, shutdown, server) = start_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(15), stream.read_to_end(&mut response))
            .await
            .unwrap()
            .unwrap();
        shutdown.send(()).unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn incomplete_body_times_out() {
        let (addr, shutdown, server) = start_server().await;
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n")
            .await
            .unwrap();
        assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 100"));
        let response = tokio::time::timeout(Duration::from_secs(15), read_headers(&mut stream))
            .await
            .unwrap();
        assert!(response.starts_with("HTTP/1.1 503"));
        shutdown.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
}
