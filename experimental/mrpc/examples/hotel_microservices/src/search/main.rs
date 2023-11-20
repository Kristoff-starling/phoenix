use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tokio::sync::mpsc;
use std::thread;
use crossbeam::channel::unbounded;

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
use server::hotel_microservices::geo::geo_client::GeoClient;
use server::hotel_microservices::rate::rate_client::RateClient;
use server::hotel_microservices::search::search_server::SearchServer;
use server::{SearchService, SearchGeoCommand, SearchRateCommand};

#[derive(StructOpt, Debug, Clone)]
#[structopt(about = "Hotel microservices search server")]
pub struct Args {
    /// The port number to listen on.
    #[structopt(short, long, default_value = "5000")]
    pub port: u16,
    #[structopt(long, default_value = "geo")]
    pub geo_addr: String,
    #[structopt(long, default_value = "5000")]
    pub geo_port: u16,
    #[structopt(long, default_value = "rate")]
    pub rate_addr: String,
    #[structopt(long, default_value = "5000")]
    pub rate_port: u16,
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
        args.port = config.search_port;
        args.geo_addr = config.geo_addr;
        args.geo_port = config.geo_port;
        args.rate_addr = config.rate_addr;
        args.rate_port = config.rate_port;
        args.log_path = Some(config.log_path.join("search.csv"));
        args.threads = config.threads;
        args.proxy_threads = config.proxy_threads;
    }
    eprintln!("args: {:?}", args);
    logging::init_env_log("RUST_LOG", "info");

    let (geo_tx, geo_rx) = unbounded();
    let (rate_tx, rate_rx) = unbounded();

    std::thread::scope(|s| {
        log::info!("Connecting to geo server...");
        let mut geo_joinhandles = Vec::new();
        for _gi in 0..args.proxy_threads {
            let geo_rx = geo_rx.clone();
            let geo_addr = args.geo_addr.clone();
            let geo_proxy = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let mut sched = 0;
                        let mut geo_clients = Vec::new();
                        for i in 0..args.threads {
                            let geo_client = GeoClient::connect(format!("{}:{}", geo_addr, args.geo_port + i as u16))?;
                            geo_clients.push(geo_client);
                        }
                        while let Some(cmd) = geo_rx.recv().unwrap() {
                            match cmd {
                                SearchGeoCommand::Req { geo_req, geo_resp } => {
                                    let nearby = geo_clients[sched].nearby(geo_req).await?;
                                    let _ = geo_resp.send(nearby.as_ref().clone());
                                    sched = (sched + 1) % args.threads as usize;
                                }
                            }
                        }
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            geo_joinhandles.push(geo_proxy);
        }

        log::info!("Connecting to rate server...");
        let mut rate_joinhandles = Vec::new();
        for _ri in 0..args.proxy_threads {
            let rate_rx = rate_rx.clone();
            let rate_addr = args.rate_addr.clone();
            let rate_proxy = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let mut sched = 0;
                        let mut rate_clients = Vec::new();
                        for i in 0..args.threads {
                            let rate_client = RateClient::connect(format!("{}:{}", rate_addr, args.rate_port + i as u16))?;
                            rate_clients.push(rate_client);
                        }
                        while let Some(cmd) = rate_rx.recv().unwrap() {
                            match cmd {
                                SearchRateCommand::Req { rate_req, rate_resp } => {
                                    let rates = rate_clients[sched].get_rates(rate_req).await?;
                                    let _ = rate_resp.send(rates.as_ref().clone());
                                    sched = (sched + 1) % args.threads as usize;
                                }
                            }
                        }
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            rate_joinhandles.push(rate_proxy);
        }

        let signal = async_ctrlc::CtrlC::new()
            .map_err(|err| mrpc::Status::internal(err.to_string()))
            .unwrap().shared();
        let mut join_handles = Vec::new();
        for i in 0..args.threads {
            let tid = i;
            let log_path = args.log_path.clone();
            let geo_tx = geo_tx.clone();
            let rate_tx = rate_tx.clone();
            let signal = signal.clone();
            let frontend_receiver = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let service = SearchService::new(geo_tx, rate_tx, log_path);
                        mrpc::stub::LocalServer::bind(format!("0.0.0.0:{}", args.port + tid as u16))?
                            .add_service(SearchServer::new(service))
                            .serve_with_graceful_shutdown(signal)
                            .await?;
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            join_handles.push(frontend_receiver);
        }

        log::info!("Search initialization completed, listening...");
    });

    Ok(())
}