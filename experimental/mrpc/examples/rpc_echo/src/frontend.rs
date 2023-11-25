//! This code is a simple Rust frontend application that listens for incoming connections,
//! send a RPC request to the echo server, and sends a response back to the client.

pub mod rpc_echo {
    // The string specified here must match the proto package name
    mrpc::include_proto!("rpc_hello");
}

use rpc_echo::greeter_client::GreeterClient;
use rpc_echo::{HelloRequest, HelloReply};
use std::{
    convert::Infallible,
    io::{prelude::*, BufReader},
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
};
use tokio::runtime::Runtime;

use crossbeam::channel::{unbounded, Sender, Receiver};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Server, Body, Request, Response, StatusCode};

#[derive(Debug)]
pub enum Command {
    Req {
        req: HelloRequest,
        resp: Sender<HelloReply>,
    }
}

// The main function that starts the server and listens for incoming connections.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = unbounded();

    thread::spawn(|| {
        send_proxy(rx).unwrap_or_else(|error| {
            eprintln!("Error sending request: {}", error);
        });
    });

    Runtime::new().unwrap().block_on(async {
        let make_service = make_service_fn(move |_conn| {
            let tx = tx.clone();
            let service = service_fn(move |req| handle_request(req, tx.clone()));
            async move { Ok::<_, Infallible>(service) }
        });

        let addr = SocketAddr::from(([0, 0, 0, 0], 7878));
        let server = Server::bind(&addr).serve(make_service);
        if let Err(e) = server.await {
            eprintln!("Server error: {:?}", e);
        }
    });

    Ok(())

    // let listener = TcpListener::bind("0.0.0.0:7878")?;

    // // Loop over incoming connections.
    // for stream in listener.incoming() {
    //     let stream = stream?;
    //     let tx = tx.clone();

    //     // Spawn a new thread to handle the connection.
    //     thread::spawn(|| {
    //         handle_connection(stream, tx).unwrap_or_else(|error| {
    //             eprintln!("Error handling connection: {}", error);
    //         });
    //     });
    // }
    // Ok(())
}

fn send_proxy(rx: Receiver<Command>) -> Result<(), Box<dyn std::error::Error>> {
    let client = GreeterClient::connect("rpc_echo_server:5000")?;

    loop {
        let cmd = rx.recv();
        match cmd {
            Ok(Command::Req { req, resp }) => {
                let hello_reply = smol::block_on(client.say_hello(req))?;
                let _ = resp.send(hello_reply.as_ref().clone());
            }
            Err(_) => {
                eprintln!("Command error {:?}", cmd);
            }
        }
    }
}

async fn handle_request(
    request: Request<Body>,
    tx: Sender<Command>
) -> Result<Response<Body>, hyper::Error> {
    let uri = request.uri().path();
    match uri {
        "/apple" | "/banana" => {
            let req = HelloRequest { name: uri.into() };
            let (resp_tx, resp_rx) = unbounded();
            let cmd = Command::Req {
                req: req,
                resp: resp_tx,
            };
            if tx.send(cmd).is_err() {
                eprintln!("channel error");
            }
            let reply = resp_rx.recv().unwrap();
            let reply_msg = String::from_utf8_lossy(&reply.message);
            let msg = format!("{reply_msg}");
            let response = Response::builder()
                .status(200)
                .body(msg.into()).unwrap();
            Ok(response)
        }
        _ => {
            eprintln!("Not found: {:?}", request);
            let mut not_found = Response::new(Body::empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

// Function to handle the connection, read the request, and send the response.
fn handle_connection(mut stream: TcpStream, tx: Sender<Command>) -> Result<(), Box<dyn std::error::Error>> {
    let mut buf_reader = BufReader::new(&mut stream);
    let mut request_str = String::new();
    buf_reader.read_line(&mut request_str)?;

    // Parse the request string and extract the URI.
    let request_parts: Vec<&str> = request_str.trim().split_whitespace().collect();
    let uri = request_parts[1];

    // Connect to the Greeter service and send the HelloRequest.
    let req = HelloRequest { name: uri.into() };

    let (resp_tx, resp_rx) = unbounded();
    let cmd = Command::Req {
        req: req,
        resp: resp_tx,
    };
    if tx.send(cmd).is_err() {
        eprintln!("channel error");
    }
    let reply = resp_rx.recv().unwrap();

    // Prepare and send the HTTP response.
    let status_line = "HTTP/1.1 200 OK";
    let content = String::from_utf8_lossy(&reply.message) + "\n";
    let length = content.len();

    let response = format!("{status_line}\r\nContent-Length: {length}\r\n\r\n{content}");

    stream.write_all(response.as_bytes())?;
    Ok(())
}
