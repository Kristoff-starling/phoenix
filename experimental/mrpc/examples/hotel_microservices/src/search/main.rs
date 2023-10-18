use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use tokio::sync::mpsc;
use std::thread;

use structopt::StructOpt;

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
    }
    eprintln!("args: {:?}", args);
    logging::init_env_log("RUST_LOG", "info");

    let (geo_tx, mut geo_rx) = mpsc::channel(32);
    let geo_thread_builder = thread::Builder::new().name("geo-proxy".to_string());
    let geo_proxy = geo_thread_builder.spawn(move || {
        let _ = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build().unwrap()
            .block_on(async {
                log::info!("Connecting to geo server...");
                let geo_client = GeoClient::connect(format!("{}:{}", args.geo_addr, args.geo_port))?;
                while let Some(cmd) = geo_rx.recv().await {
                    match cmd {
                        SearchGeoCommand::Req { geo_req, geo_resp } => {
                            log::info!("geo-proxy receive request");
                            let nearby = geo_client.nearby(geo_req).await?;
                            log::info!("geo-proxy receive response");
                            let _ = geo_resp.send(nearby);
                        }
                    }
                }
                Ok::<(), mrpc::Status>(())
            });
    }).unwrap();

    let (rate_tx, mut rate_rx) = mpsc::channel(32);
    let rate_thread_builder = thread::Builder::new().name("rate-proxy".to_string());
    let rate_proxy = rate_thread_builder.spawn(move || {
        let _ = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build().unwrap()
            .block_on(async {
                log::info!("Connecting to rate server...");
                let rate_client = RateClient::connect(format!("{}:{}", args.rate_addr, args.rate_port))?;
                while let Some(cmd) = rate_rx.recv().await {
                    match cmd {
                        SearchRateCommand::Req { rate_req, rate_resp } => {
                            log::info!("rate-proxy receive request");
                            let rates = rate_client.get_rates(rate_req).await?;
                            log::info!("rate-proxy receive response");
                            let _ = rate_resp.send(rates);
                        }
                    }
                }
                Ok::<(), mrpc::Status>(())
            });
    }).unwrap();

    let frontend_thread_builder = thread::Builder::new().name("frontend-receiver".to_string());
    let frontend_receiver = frontend_thread_builder.spawn(move || {
        let _ = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build().unwrap()
            .block_on(async {
                let service = SearchService::new(geo_tx, rate_tx, args.log_path);
                let signal = async_ctrlc::CtrlC::new()
                    .map_err(|err| mrpc::Status::internal(err.to_string()))?;
                mrpc::stub::LocalServer::bind(format!("0.0.0.0:{}", args.port))?
                    .add_service(SearchServer::new(service))
                    .serve_with_graceful_shutdown(signal)
                    .await?;
                Ok::<(), mrpc::Status>(())
            });
    }).unwrap(); 
    
    log::info!("Search initialization complete, listening...");
    let _ = geo_proxy.join().unwrap();
    let _ = rate_proxy.join().unwrap();
    let _ = frontend_receiver.join().unwrap();
    Ok(())
}