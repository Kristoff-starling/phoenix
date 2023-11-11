use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Duration;

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
use server::hotel_microservices::rate::rate_server::RateServer;
use server::RateService;

#[derive(StructOpt, Debug, Clone)]
#[structopt(about = "Hotel reservation rate server")]
pub struct Args {
    /// The port number to listen on.
    #[structopt(short, long, default_value = "5000")]
    pub port: u16,
    #[structopt(long, default_value = "mongodb://localhost:27017")]
    pub db: String,
    #[structopt(long, default_value = "memcache://localhost:11211")]
    pub memc: String,
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
        args.db = config.rate_mongo_addr;
        args.memc = config.rate_memc_addr;
        args.port = config.rate_port;
        args.log_path = Some(config.log_path.join("rate.csv"));
        args.threads = config.threads;
        args.proxy_threads = config.proxy_threads;
    }
    eprintln!("args: {:?}", args);
    logging::init_env_log("RUST_LOG", "info");

    log::info!("Initializing DB connection...");
    let database = initialize_database(args.db).await?;
    log::info!("Successful");

    log::info!("Initializing memcached client...");
    let memc_client = memcache::Client::with_pool_size(&*args.memc, 512)?;
    memc_client.set_read_timeout(Some(Duration::from_secs(2)))?;
    memc_client.set_write_timeout(Some(Duration::from_secs(2)))?;
    log::info!("Successful");

    std::thread::scope(|s| {
        let mut join_handles = Vec::new();
        let signal = async_ctrlc::CtrlC::new()
            .map_err(|err| mrpc::Status::internal(err.to_string()))
            .unwrap().shared();
        for i in 0..args.threads {
            let tid = i;
            let log_path = args.log_path.clone();
            let database = database.clone();
            let memc_client = memc_client.clone();
            let signal = signal.clone();
            let search_receiver = s.spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build().unwrap()
                    .block_on(async {
                        let service = RateService::new(database, memc_client, log_path);
                        mrpc::stub::LocalServer::bind(format!("0.0.0.0:{}", args.port + tid as u16))?
                            .add_service(RateServer::new(service))
                            .serve_with_graceful_shutdown(signal)
                            .await?;
                        Ok::<(), mrpc::Status>(())
                    }).unwrap();
            });
            join_handles.push(search_receiver);
        }
        log::info!("Rate initialization complete, listening...");
    });
    
    Ok(())
}
