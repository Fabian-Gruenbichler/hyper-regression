use std::path::Path;
use std::sync::Arc;

use anyhow::{format_err, Error};
use futures::*;
use hyper::{Body, Request, Response};
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use tokio::net::{TcpListener, TcpStream};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut acceptor = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls()).unwrap();
    acceptor
        .set_private_key_file(Path::new("./key.pem"), SslFiletype::PEM)
        .map_err(|err| format_err!("unable to read key - {}", err))?;
    acceptor
        .set_certificate_chain_file(Path::new("./cert.pem"))
        .map_err(|err| format_err!("unable to read cert - {}", err))?;
    acceptor.check_private_key().unwrap();

    let acceptor = Arc::new(acceptor.build());

    let mut listener =
        TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 8008))).await?;

    println!("listening on {:?}", listener.local_addr());

    loop {
        let (socket, _addr) = listener.accept().await?;
        tokio::spawn(handle_connection(socket, Arc::clone(&acceptor)).map(|res| {
            if let Err(err) = res {
                eprintln!("Error: {}", err);
            }
        }));
    }
}

async fn handle_connection(socket: TcpStream, acceptor: Arc<SslAcceptor>) -> Result<(), Error> {
    socket.set_nodelay(true).unwrap();

    let socket = tokio_openssl::accept(acceptor.as_ref(), socket).await?;

    let mut http = hyper::server::conn::Http::new();
    http.http2_only(true);
    // increase window size: todo - find optiomal size
    let max_window_size = (1 << 31) - 2;
    http.http2_initial_stream_window_size(max_window_size);
    http.http2_initial_connection_window_size(max_window_size);

    let service = hyper::service::service_fn(|_req: Request<Body>| {
        println!("Got request");
        let buffer = vec![65u8; 4 * 1024 * 1024]; // nonsense [A,A,A,A...]
        let body = Body::from(buffer);

        let response = Response::builder()
            .status(http::StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "application/octet-stream")
            .body(body)
            .unwrap();
        future::ok::<_, Error>(response)
    });

    http.serve_connection(socket, service)
        .map_err(Error::from)
        .await?;

    println!("H2 connection CLOSE !");
    Ok(())
}
