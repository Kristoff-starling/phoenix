use std::convert::Infallible;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use std::thread;
use crossbeam::channel::unbounded;

use hyper::service::{make_service_fn, service_fn};
use hyper::Server;
use structopt::StructOpt;
use futures::FutureExt;

#[path = "../config.rs"]
pub mod config;
#[path = "../logging.rs"]
pub mod logging;
pub mod server;
#[path = "../tracer.rs"]
pub mod tracer;

use config::Config;
use server::hotel_microservices::profile::profile_client::ProfileClient;
use server::hotel_microservices::search::search_client::SearchClient;
use server::{dispatch_fn, FrontendService, FrontendSearchCommand, FrontendProfileCommand};

#[derive(StructOpt, Debug, Clone)]
#[structopt(about = "Hotel microservices frontend server")]
pub struct Args {
    /// The port number to listen on.
    #[structopt(short, long, default_value = "5000")]
    pub port: u16,
    #[structopt(long, default_value = "search")]
    pub search_addr: String,
    #[structopt(long, default_value = "5000")]
    pub search_port: u16,
    #[structopt(long, default_value = "profile")]
    pub profile_addr: String,
    #[structopt(long, default_value = "5000")]
    pub profile_port: u16,
    #[structopt(short, long)]
    pub config: Option<PathBuf>,
    #[structopt(long)]
    pub log_path: Option<PathBuf>,
    #[structopt(short, long, default_value = "10")]
    pub threads: u16,
    #[structopt(short, long, default_value = "10")]
    pub proxy_threads: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::from_args();
    if let Some(path) = &args.config {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let config: Config = serde_json::from_reader(reader)?;
        args.port = config.frontend_port;
        args.search_addr = config.search_addr;
        args.search_port = config.search_port;
        args.profile_addr = config.profile_addr;
        args.profile_port = config.profile_port;
        args.log_path = Some(config.log_path.join("frontend.csv"));
        args.threads = config.threads;
        args.proxy_threads = config.proxy_threads;
    }
    eprintln!("args: {:?}", args);
    logging::init_env_log("RUST_LOG", "info");

    let (search_tx, search_rx) = unbounded();
    let (profile_tx, profile_rx) = unbounded();

    std::thread::scope(|s| {
        log::info!("Connecting to search server...");
        let mut search_joinhandles = Vec::new();
        for _si in 0..args.proxy_threads {
            let search_rx = search_rx.clone();
            let search_addr = args.search_addr.clone();
            let search_proxy= s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let mut sched = 0;
                        let mut search_clients = Vec::new();
                        for i in 0..args.threads {
                            let search_client = SearchClient::connect(format!("{}:{}", search_addr, args.search_port + i as u16))?;
                            search_clients.push(search_client);
                        }
                        while let Some(cmd) = search_rx.recv().unwrap() {
                            match cmd {
                                FrontendSearchCommand::Req { search_req, search_resp } => {
                                    let nearby = search_clients[sched].nearby(search_req).await?;
                                    let _ = search_resp.send(nearby.as_ref().clone());
                                    sched = (sched + 1) % args.threads as usize;
                                }
                            }
                        }
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            search_joinhandles.push(search_proxy);
        }

        log::info!("Connecting to profile server...");
        let mut profile_joinhandles = Vec::new();
        for _pi in 0..args.proxy_threads {
            let profile_rx = profile_rx.clone();
            let profile_addr = args.profile_addr.clone();
            let profile_proxy = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let mut sched = 0;
                        let mut profile_clients = Vec::new();
                        for i in 0..args.threads {
                            let profile_client = ProfileClient::connect(format!("{}:{}", profile_addr, args.profile_port + i as u16))?;
                            profile_clients.push(profile_client);
                        }
                        while let Some(cmd) = profile_rx.recv().unwrap() {
                            match cmd {
                                FrontendProfileCommand::Req { profile_req, profile_resp } => {
                                    let result = profile_clients[sched].get_profiles(profile_req).await?;
                                    let _ = profile_resp.send(result.as_ref().clone());
                                    sched = (sched + 1) % args.threads as usize;
                                }
                            }
                        }
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            profile_joinhandles.push(profile_proxy);
        }

        let signal = async_ctrlc::CtrlC::new()
            .map_err(|err| mrpc::Status::internal(err.to_string()))
            .unwrap().shared();

        let mut join_handles = Vec::new();
        for i in 0..args.threads {
            let tid = i;
            let log_path = args.log_path.clone();
            let search_tx = search_tx.clone();
            let profile_tx = profile_tx.clone();
            let signal = signal.clone();
            let user_receiver = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build().unwrap()
                    .block_on(async {
                        let frontend = Arc::new(FrontendService::new(
                            search_tx,
                            profile_tx,
                            log_path,
                        ));
                        let addr = SocketAddr::from(([0, 0, 0, 0], args.port + tid as u16));
                        let make_service = make_service_fn(move |_conn| {
                            let frontend = frontend.clone();
                            let service = service_fn(move |req| dispatch_fn(frontend.clone(), req));
                            async move { Ok::<_, Infallible>(service) }
                        });
                        let server = Server::bind(&addr).serve(make_service);

                        let graceful = server.with_graceful_shutdown(signal);
                        if let Err(e) = graceful.await {
                            log::error!("Server error: {}", e);
                        }
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            join_handles.push(user_receiver);
        }
    });

    Ok(())
}
