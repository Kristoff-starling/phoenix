use std::cell::RefCell;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};

use anyhow::Result;
use minstant::Instant;

use mrpc::alloc::Vec;
use mrpc::{RRef, WRef};

use super::tracer::Tracer;

pub mod hotel_microservices {
    pub mod geo {
        // The string specified here must match the proto package name
        mrpc::include_proto!("geo");
    }
    pub mod rate {
        // The string specified here must match the proto package name
        mrpc::include_proto!("rate");
    }
    pub mod search {
        // The string specified here must match the proto package name
        mrpc::include_proto!("search");
    }
}
use hotel_microservices::geo::{Request as GeoRequest, Result as GeoResult};
use hotel_microservices::rate::{Request as RateRequest, Result as RateResult};
use hotel_microservices::search::search_server::Search;
use hotel_microservices::search::{NearbyRequest as SearchRequest, SearchResult};

#[derive(Debug)]
pub enum SearchGeoCommand {
    Req {
        geo_req: GeoRequest,
        geo_resp: oneshot::Sender<GeoResult>,
    },
}

#[derive(Debug)]
pub enum SearchRateCommand {
    Req {
        rate_req: RateRequest,
        rate_resp: oneshot::Sender<RateResult>,
    },
}

pub struct SearchService {
    // geo_client: GeoClient,
    // rate_client: RateClient,
    geo_tx: mpsc::Sender<SearchGeoCommand>,
    rate_tx: mpsc::Sender<SearchRateCommand>,
    log_path: Option<PathBuf>,
    tracer: RefCell<Tracer>,
}

// TODO(wyj): revisit server stub's API
// SAFETY: it is NOT safe to send or share SearchService between threads
// the safety contract here is the futures created by SearchService
// are only pulled by a single thread
unsafe impl Send for SearchService {}
unsafe impl Sync for SearchService {}

impl Drop for SearchService {
    fn drop(&mut self) {
        let mut tracer = self.tracer.borrow_mut();
        if let Some(path) = &self.log_path {
            if let Some(parent) = path.parent() {
                if let Err(err) = std::fs::create_dir_all(parent) {
                    log::error!("Error create logging dir: {}", err);
                }
            }
            if let Err(err) = tracer.to_csv(path) {
                log::error!("Error writting logs: {}", err);
            }
        }
    }
}

#[mrpc::async_trait]
impl Search for SearchService {
    async fn nearby(
        &self,
        request: RRef<SearchRequest>,
    ) -> Result<WRef<SearchResult>, mrpc::Status> {
        log::debug!("nearby receive request");
        let result = self
            .nearby_internal(request)
            .await
            .map_err(|err| mrpc::Status::internal(err.to_string()))?;
        let wref = WRef::new(result);
        log::debug!("nearby response sent");
        Ok(wref)
    }
}

impl SearchService {
    async fn nearby_internal(&self, request: RRef<SearchRequest>) -> Result<SearchResult> {
        log::trace!("in Search Nearby");

        log::trace!("nearby lat = {:.4}", request.lat);
        log::trace!("nearby lon = {:.4}", request.lon);
        let geo_req = GeoRequest {
            lat: request.lat,
            lon: request.lon,
        };

        let start = Instant::now();
        let (geo_resp_tx, geo_resp_rx) = oneshot::channel();
        let geo_cmd = SearchGeoCommand::Req{
            geo_req: geo_req,
            geo_resp: geo_resp_tx
        };
        if self.geo_tx.send(geo_cmd).await.is_err() {
            log::error!("Search-Geo channel failed");
        }
        let nearby = geo_resp_rx.await?;

        self.tracer
            .borrow_mut()
            .record_end_to_end("geo", start.elapsed())?;

        log::trace!("get Nearby hotelId = {:?}", nearby.hotel_ids);
        let rate_req = RateRequest {
            hotel_ids: nearby.hotel_ids.clone(),
            in_date: request.in_date.clone(),
            out_date: request.out_date.clone(),
        };

        let start = Instant::now();
        let (rate_resp_tx, rate_resp_rx) = oneshot::channel();
        let rate_cmd = SearchRateCommand::Req{
            rate_req: rate_req,
            rate_resp: rate_resp_tx
        };
        if self.rate_tx.send(rate_cmd).await.is_err() {
            log::error!("Search-Rate channel failed");
        }
        let rates = rate_resp_rx.await?;
        self.tracer
            .borrow_mut()
            .record_end_to_end("rate", start.elapsed())?;

        let mut hotel_ids = Vec::with_capacity(nearby.hotel_ids.len());
        for rate_plan in rates.rate_plans.iter() {
            hotel_ids.push(rate_plan.hotel_id.clone());
        }

        let result = SearchResult { hotel_ids };
        Ok(result)
    }
}

impl SearchService {
    pub fn new(geo: mpsc::Sender<SearchGeoCommand>, rate: mpsc::Sender<SearchRateCommand>, log_path: Option<PathBuf>) -> Self {
        let mut tracer = Tracer::new();
        tracer.new_end_to_end_entry("geo");
        tracer.new_end_to_end_entry("rate");
        SearchService {
            geo_tx: geo,
            rate_tx: rate,
            log_path,
            tracer: RefCell::new(tracer),
        }
    }
}
