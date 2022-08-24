use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{bail, Result};
use koala::engine::datapath::graph::ChannelDescriptor;
use koala::engine::datapath::node::DataPathNode;
use nix::unistd::Pid;
use uuid::Uuid;

use interface::engine::SchedulingMode;
use ipc::customer::{Customer, ShmCustomer};
use ipc::mrpc::{cmd, dp};

use koala::engine::{EnginePair, EngineType};
use koala::module::{
    KoalaModule, ModuleCollection, ModuleDowncast, NewEngineRequest, Service, ServiceInfo, Version,
};
use koala::state_mgr::SharedStateManager;
use koala::storage::{ResourceCollection, SharedStorage};

use crate::config::MrpcConfig;

use super::engine::MrpcEngine;
use super::state::{Shared, State};
use koala::engine::datapath::meta_pool::MetaBufferPool;

pub type CustomerType =
    Customer<cmd::Command, cmd::Completion, dp::WorkRequestSlot, dp::CompletionSlot>;

pub(crate) struct MrpcEngineBuilder {
    customer: CustomerType,
    _client_pid: Pid,
    mode: SchedulingMode,
    cmd_tx: tokio::sync::mpsc::UnboundedSender<cmd::Command>,
    cmd_rx: tokio::sync::mpsc::UnboundedReceiver<cmd::Completion>,
    node: DataPathNode,
    serializer_build_cache: PathBuf,
    shared: Arc<Shared>,
}

impl MrpcEngineBuilder {
    fn new(
        customer: CustomerType,
        client_pid: Pid,
        mode: SchedulingMode,
        cmd_tx: tokio::sync::mpsc::UnboundedSender<cmd::Command>,
        cmd_rx: tokio::sync::mpsc::UnboundedReceiver<cmd::Completion>,
        node: DataPathNode,
        serializer_build_cache: PathBuf,
        shared: Arc<Shared>,
    ) -> Self {
        MrpcEngineBuilder {
            customer,
            cmd_tx,
            cmd_rx,
            node,
            _client_pid: client_pid,
            mode,
            serializer_build_cache,
            shared,
        }
    }

    fn build(self) -> Result<MrpcEngine> {
        const META_BUFFER_POOL_CAP: usize = 128;
        const BUF_LEN: usize = 32;

        let state = State::new(self.shared);

        Ok(MrpcEngine {
            _state: state,
            customer: self.customer,
            cmd_tx: self.cmd_tx,
            cmd_rx: self.cmd_rx,
            node: self.node,
            meta_buf_pool: MetaBufferPool::new(META_BUFFER_POOL_CAP),
            _mode: self.mode,
            dispatch_build_cache: self.serializer_build_cache,
            transport_type: None,
            indicator: Default::default(),
            wr_read_buffer: Vec::with_capacity(BUF_LEN),
        })
    }
}

pub struct MrpcModule {
    config: MrpcConfig,
    pub state_mgr: SharedStateManager<Shared>,
}

impl MrpcModule {
    pub const MRPC_ENGINE: EngineType = EngineType("MrpcEngine");
    pub const ENGINES: &'static [EngineType] = &[MrpcModule::MRPC_ENGINE];
    pub const DEPENDENCIES: &'static [EnginePair] =
        &[(MrpcModule::MRPC_ENGINE, EngineType("RpcAdapterEngine"))];

    pub const SERVICE: Service = Service("Mrpc");
    pub const TX_CHANNELS: &'static [ChannelDescriptor] = &[ChannelDescriptor(
        MrpcModule::MRPC_ENGINE,
        EngineType("RpcAdapterEngine"),
        0,
        0,
    )];
    pub const RX_CHANNELS: &'static [ChannelDescriptor] = &[ChannelDescriptor(
        EngineType("RpcAdapterEngine"),
        MrpcModule::MRPC_ENGINE,
        0,
        0,
    )];
}

impl MrpcModule {
    pub fn new(config: MrpcConfig) -> Self {
        MrpcModule {
            config,
            state_mgr: SharedStateManager::new(),
        }
    }
}

impl KoalaModule for MrpcModule {
    fn service(&self) -> Option<ServiceInfo> {
        let group = vec![Self::MRPC_ENGINE, EngineType("RpcAdapterEngine")];
        let service = ServiceInfo {
            service: MrpcModule::SERVICE,
            engine: MrpcModule::MRPC_ENGINE,
            tx_channels: MrpcModule::TX_CHANNELS,
            rx_channels: MrpcModule::RX_CHANNELS,
            scheduling_groups: vec![group],
        };
        Some(service)
    }

    fn engines(&self) -> &[EngineType] {
        MrpcModule::ENGINES
    }

    fn dependencies(&self) -> &[EnginePair] {
        MrpcModule::DEPENDENCIES
    }

    fn check_compatibility(&self, _prev: Option<&Version>, _curr: &HashMap<&str, Version>) -> bool {
        true
    }

    fn decompose(self: Box<Self>) -> ResourceCollection {
        let module = *self;
        let mut collections = ResourceCollection::new();
        collections.insert("state_mgr".to_string(), Box::new(module.state_mgr));
        collections.insert("config".to_string(), Box::new(module.config));
        collections
    }

    fn migrate(&mut self, prev_module: Box<dyn KoalaModule>) {
        // NOTE(wyj): we may better call decompose here
        let prev_concrete = unsafe { *prev_module.downcast_unchecked::<Self>() };
        self.state_mgr = prev_concrete.state_mgr;
    }

    fn create_engine(
        &mut self,
        ty: EngineType,
        request: NewEngineRequest,
        shared: &mut SharedStorage,
        _global: &mut ResourceCollection,
        node: DataPathNode,
        _plugged: &ModuleCollection,
    ) -> Result<Option<Box<dyn koala::engine::Engine>>> {
        if ty != MrpcModule::MRPC_ENGINE {
            bail!("invalid engine type {:?}", ty)
        }
        if let NewEngineRequest::Service {
            sock,
            client_path,
            mode,
            cred,
        } = request
        {
            // generate a path and bind a unix domain socket to it
            let uuid = Uuid::new_v4();
            let instance_name = format!("{}-{}.sock", self.config.engine_basename, uuid);
            let engine_path = self.config.prefix.join(instance_name);

            // create customer stub
            let customer =
                Customer::from_shm(ShmCustomer::accept(sock, client_path, mode, engine_path)?);

            let client_pid = Pid::from_raw(cred.pid.unwrap());
            let shared_state = self.state_mgr.get_or_create(client_pid)?;

            // obtain senders/receivers of command queues with RpcAdapterEngine
            // the sender/receiver ends are already created,
            // as the RpcAdapterEngine is built first
            // according to the topological order
            let cmd_tx = shared
                .command_path
                .get_sender(&EngineType("RpcAdapterEngine"))?;
            let cmd_rx = shared.command_path.get_receiver(&MrpcModule::MRPC_ENGINE)?;

            let builder = MrpcEngineBuilder::new(
                customer,
                client_pid,
                mode,
                cmd_tx,
                cmd_rx,
                node,
                self.config.build_cache.clone(),
                shared_state,
            );
            let engine = builder.build()?;

            Ok(Some(Box::new(engine)))
        } else {
            bail!("invalid request type");
        }
    }

    fn restore_engine(
        &mut self,
        ty: EngineType,
        local: ResourceCollection,
        shared: &mut SharedStorage,
        global: &mut ResourceCollection,
        node: DataPathNode,
        plugged: &ModuleCollection,
        prev_version: Version,
    ) -> Result<Box<dyn koala::engine::Engine>> {
        if ty != MrpcModule::MRPC_ENGINE {
            bail!("invalid engine type {:?}", ty)
        }
        let engine = MrpcEngine::restore(local, shared, global, node, plugged, prev_version)?;
        Ok(Box::new(engine))
    }
}
