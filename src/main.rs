use anyhow::{Context, Result};
use http_body_util::combinators::BoxBody;
use serde::Serialize;

use std::collections::HashMap;
use std::net::SocketAddr;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

#[derive(Serialize)]
struct File<'file> {
    name: &'file str,
}

#[derive(Serialize)]
struct CdnResponse<'response> {
    msg: &'response str,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<File<'response>>>,
}

fn full<T: Into<Bytes>>(chunk: T) -> http_body_util::combinators::BoxBody<Bytes, std::io::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn response_handler(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/files") => Ok(all()),
        (&Method::POST, "/file") => Ok(upload(req).await),
        _ => Ok(catch_all()),
    }
}

async fn upload(req: Request<hyper::body::Incoming>) -> Response<BoxBody<Bytes, std::io::Error>> {
    let whole_body = req.collect().await.unwrap().to_bytes();
    // process path param
    let params = form_urlencoded::parse(whole_body.as_ref())
        .into_owned()
        .collect::<HashMap<String, String>>();

    if let Some(filename) = params.get("filename") {
        println!("{}", filename);
    }

    todo!()
}

fn all() -> Response<BoxBody<Bytes, std::io::Error>> {
    let response = CdnResponse {
        msg: "Got files",
        files: None,
    };

    Response::builder()
        .status(StatusCode::OK)
        .body(full(serde_json::to_vec(&response).unwrap()))
        .unwrap()
}

/// HTTP status code 404
fn catch_all() -> Response<BoxBody<Bytes, std::io::Error>> {
    let response = CdnResponse {
        msg: "Not found",
        files: None,
    };
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(full(serde_json::to_vec(&response).unwrap()))
        .unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = TcpListener::bind(addr)
        .await
        .context("Failed to start the server")?;

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("Failed to await stream accepting")?;

        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(response_handler))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}
