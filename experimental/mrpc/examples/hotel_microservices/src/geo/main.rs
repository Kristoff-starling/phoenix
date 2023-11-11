use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use structopt::StructOpt;
use futures::FutureExt;

#[path = "../config.rs"]
pub mod config;
pub mod db;
#[path = "../logging.rs"]
pub mod logging;
pub mod server;
#[path = "../tracer.rs"]
pub mod tracer;

use config::Config;
use db::initialize_database;
use server::hotel_microservices::geo::geo_server::GeoServer;
use server::GeoService;

#[derive(StructOpt, Debug, Clone)]
#[structopt(about = "Hotel reservation geo server")]
pub struct Args {
    /// The port number to listen on.
    #[structopt(short, long, default_value = "5000")]
    pub port: u16,
    #[structopt(long, default_value = "mongodb://localhost:27017")]
    pub db: String,
    #[structopt(short, long)]
    pub config: Option<PathBuf>,
    #[structopt(long)]
    pub log_path: Option<PathBuf>,
    #[structopt(short, long, default_value = "10")]
    pub threads: u16,
    #[structopt(short, long, default_value = "10")]
    pub proxy_threads: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::from_args();
    if let Some(path) = &args.config {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let config: Config = serde_json::from_reader(reader)?;
        args.db = config.geo_mongo_addr;
        args.port = config.geo_port;
        args.log_path = Some(config.log_path.join("geo.csv"));
        args.threads = config.threads;
        args.proxy_threads = config.proxy_threads;
    }
    eprintln!("args: {:?}", args);
    logging::init_env_log("RUST_LOG", "info");

    log::info!("Initializing DB connection...");
    let database = initialize_database(args.db).await?;
    log::info!("Successful");

    std::thread::scope(|s| {
        let signal = async_ctrlc::CtrlC::new()
            .map_err(|err| mrpc::Status::internal(err.to_string()))
            .unwrap().shared();
        let mut join_handles = Vec::new();
        for i in 0..args.threads {
            let tid = i;
            let log_path = args.log_path.clone();
            let database = database.clone();
            let signal = signal.clone();
            let search_receiver = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let service = GeoService::new(database, log_path)
                            .await
                            .map_err(|err| mrpc::Status::internal(err.to_string()))?;
                        mrpc::stub::LocalServer::bind(format!("0.0.0.0:{}", args.port + tid as u16))?
                            .add_service(GeoServer::new(service))
                            .serve_with_graceful_shutdown(signal)
                            .await?;
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            join_handles.push(search_receiver);
        }
        log::info!("Geo initialization complete, listening...");
    });

 
    Ok(())
}
