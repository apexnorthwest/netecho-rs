use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio::net::TcpListener;

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
    let listener = TcpListener::bind(addr).await.unwrap();
    println!("Server running on http://0.0.0.0:8080");
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        tokio::task::spawn(async move {
            // Finally, we bind the incoming connection to our `hello` service
            if let Err(err) = http1::Builder::new()
                // `service_fn` converts our function in a `Service`
                .serve_connection(io, service_fn(handle_request))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}
