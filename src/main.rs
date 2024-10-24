use anyhow::{Context, Result};
use http_body_util::combinators::BoxBody;
use serde::Serialize;
use tokio::fs;

use core::str;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Bytes};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

type FileStore = Arc<Mutex<HashMap<String, File>>>;

#[derive(Serialize, Clone)]
struct File {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Serialize)]
struct CdnResponse<'response> {
    msg: &'response str,
    #[serde(skip_serializing_if = "Option::is_none")]
    files: Option<Vec<File>>,
}

fn init_store() -> Result<FileStore> {
    let store = Arc::new(Mutex::new(HashMap::new()));
    let mut lock = store.lock().unwrap();
    std::fs::read_dir("./store")?
        .flatten()
        .filter(|e| !e.metadata().unwrap().is_dir())
        .flat_map(|file: std::fs::DirEntry| -> Result<File> {
            Ok(File {
                name: file.file_name().to_str().unwrap().to_string(),
                content: String::from_utf8(std::fs::read(file.path())?).ok(),
            })
        })
        .for_each(move |file| {
            lock.insert(file.name.clone(), file);
        });
    println!(
        "cdn: Found {} File(s) on disk, loading into memory store",
        store.lock().unwrap().len()
    );
    Ok(store)
}

fn full<T: Into<Bytes>>(chunk: T) -> http_body_util::combinators::BoxBody<Bytes, std::io::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

/// response generates a Result<Response, ...> from the http statuscode and a message, containing
///     { msg: msg }
fn response(code: StatusCode, msg: &str) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    Ok(Response::builder()
        .status(code)
        .body(full(serde_json::to_vec(&CdnResponse { msg, files: None })?))?)
}

async fn response_handler(
    req: Request<hyper::body::Incoming>,
    db_handle: FileStore,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let path = req
        .uri()
        .path()
        .split("/")
        .filter(|e| !e.is_empty())
        .collect::<Vec<&str>>();

    match (req.method(), path[0]) {
        (&Method::GET, "files") => all(db_handle).await,
        (&Method::POST, "file") => upload(req, db_handle).await,
        (&Method::GET, "file") => {
            if path.get(1).is_none() {
                return response(StatusCode::NOT_FOUND, "No file path");
            }
            download(db_handle, path[1]).await
        }
        _ => response(StatusCode::NOT_FOUND, "Not Found"),
    }
}

async fn upload(
    req: Request<hyper::body::Incoming>,
    db_handle: FileStore,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let whole_body = req.collect().await.unwrap().to_bytes();
    // process path param
    let params = form_urlencoded::parse(whole_body.as_ref())
        .into_owned()
        .collect::<HashMap<String, String>>();

    if !(params.contains_key("name") && params.contains_key("content")) {
        return response(
            StatusCode::BAD_REQUEST,
            "Missing name or content in request body",
        );
    }

    let file = File {
        name: params.get("name").unwrap().to_string(),
        content: { Some(params.get("content").unwrap_or(&String::from("")).clone()) },
    };
    let mut lock = db_handle.lock().unwrap();
    let filename = file.name.clone();

    std::fs::write(
        Path::new(".").join("store").join(&filename),
        file.content.clone().unwrap(),
    )?;

    lock.insert(filename, file);

    response(
        StatusCode::CREATED,
        &format!("Stored file '{}'", params.get("name").unwrap()),
    )
}

async fn download(
    db_handle: FileStore,
    file_name: &str,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    // edge case if only /file is called
    if file_name == "file" {
        return response(StatusCode::BAD_REQUEST, "No file path given");
    }

    let mut file_name = file_name;

    // anti path traversal
    if let Some(base) = Path::new(file_name).file_name() {
        file_name = base.to_str().unwrap();
    }

    let lock = db_handle.lock().unwrap();
    if let Some(file) = lock.get(file_name) {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(full(file.content.clone().unwrap_or_default()))?);
    }

    response(
        StatusCode::NOT_FOUND,
        &format!("File '{}' not found in store", file_name),
    )
}

async fn all(db: FileStore) -> Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let handle = db.lock().unwrap();
    let files = handle
        .keys()
        .map(|key| File {
            name: String::from(key),
            content: None,
        })
        .collect::<Vec<File>>();
    let response = CdnResponse {
        msg: match &files.len() {
            0 => "Got no files",
            _ => &format!("Got {} files", files.len()),
        },
        files: Some(files),
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(full(serde_json::to_vec(&response).unwrap()))?)
}

/// rust_cdn works by making all writes on disk but all reads are performed from the in memory FileStore data type, this makes reads extremly fast
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let listener = TcpListener::bind(addr)
        .await
        .context("Failed to start the server")?;

    fs::create_dir_all("./store")
        .await
        .context("Failed to create file store")?;
    let db = init_store()?;

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("Failed to await stream accepting")?;

        let addr = stream.peer_addr()?;
        let io = TokioIo::new(stream);
        let db_handle = db.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| {
                        let method = req.method().to_string();
                        let path = req.uri().path().to_string();
                        let res = response_handler(req, Arc::clone(&db_handle));
                        async move {
                            let r = res.await;
                            if let Ok(ok) = &r {
                                println!(
                                    "|{: ^5}|{: ^7}| {: <25} | {: >4}b | {}",
                                    ok.status().as_u16(),
                                    method,
                                    path,
                                    ok.body().size_hint().exact().unwrap_or(0),
                                    addr,
                                );
                            }
                            r
                        }
                    }),
                )
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}
