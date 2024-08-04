use crate::imports::*;
// use kaspa_notify::notification::test_helpers::BlockAddedNotification;
use kaspa_rpc_core::api::ctl::{RpcCtl, RpcState};
use kaspa_rpc_core::RpcBlock;
use kaspa_rpc_core::{
    api::ops::{RPC_API_REVISION, RPC_API_VERSION},
    model::{GetServerInfoResponse, RpcTransaction},
    notify::connection::{ChannelConnection, ChannelType},
    BlockAddedNotification, Notification, VirtualChainChangedNotification,
    VirtualDaaScoreChangedNotification,
};
use kaspa_wallet_core::rpc::{DynRpcApi, Rpc};

use kaspa_notify::{
    listener::ListenerId,
    scope::{BlockAddedScope, Scope, VirtualChainChangedScope, VirtualDaaScoreChangedScope},
};
use kaspa_wrpc_client::prelude::{ConnectOptions, KaspaRpcClient, Resolver, WrpcEncoding};

struct Inner {
    multiplexer: Multiplexer<Box<Event>>,

    network_id: NetworkId,
    rpc: Mutex<Rpc>,
    is_connected: AtomicBool,
    notification_channel: Channel<Notification>,
    listener_id: Mutex<Option<ListenerId>>,
    // are we synced with the DAG?
    is_synced: AtomicBool,
    current_daa_score: AtomicU64,
    pending: Mutex<Vec<RpcTransaction>>,

    processor: Arc<Processor>,
    sender: mpsc::Sender<Ingest>,

    shutdown: DuplexChannel<()>,
}

#[derive(Clone)]
pub struct Nexus {
    #[allow(dead_code)]
    inner: Arc<Inner>,
}

impl Nexus {
    pub async fn try_new(network_id: NetworkId, url: Option<&str>) -> Result<Self> {
        println!("NEXUS init...");
        println!("PROCESSOR init...");
        let processor = Arc::new(Processor::try_new()?);
        let sender = processor.sender();

        println!("PROCESSOR init done...");

        // for now use the default public node infrastructure
        let (resolver, url) = if let Some(url) = url {
            (None, Some(url))
        } else {
            (Some(Resolver::default()), url)
        };
        let rpc_client = Arc::new(KaspaRpcClient::new_with_args(
            WrpcEncoding::Borsh,
            url,
            resolver,
            Some(network_id),
            None,
        )?);

        let rpc_ctl = rpc_client.ctl().clone();
        let rpc_api: Arc<DynRpcApi> = rpc_client;
        let rpc = Rpc::new(rpc_api, rpc_ctl);

        Ok(Self {
            inner: Arc::new(Inner {
                multiplexer: Multiplexer::new(),
                network_id,
                rpc: Mutex::new(rpc),
                is_connected: AtomicBool::new(false),
                notification_channel: Channel::<Notification>::unbounded(),
                listener_id: Mutex::new(None),
                is_synced: AtomicBool::new(false),
                current_daa_score: AtomicU64::new(0),
                pending: Mutex::new(Vec::new()),
                processor,
                sender,
                shutdown: DuplexChannel::oneshot(),
            }),
        })
    }

    pub async fn connect(&self) -> Result<()> {
        let options = ConnectOptions {
            block_async_connect: false,
            ..Default::default()
        };

        self.rpc_client().connect(Some(options)).await?;
        Ok(())
    }

    pub async fn disconnect(&self) -> Result<()> {
        self.rpc_client().disconnect().await?;
        Ok(())
    }

    pub fn multiplexer(&self) -> &Multiplexer<Box<Event>> {
        &self.inner.multiplexer
    }

    pub fn listener_id(&self) -> Result<ListenerId> {
        self.inner
            .listener_id
            .lock()
            .unwrap()
            .ok_or(Error::ListenerId)
    }

    pub fn network_id(&self) -> NetworkId {
        self.inner.network_id
    }

    pub fn rpc_api(&self) -> Arc<DynRpcApi> {
        self.inner.rpc.lock().unwrap().rpc_api().clone()
    }

    pub fn rpc_ctl(&self) -> RpcCtl {
        self.inner.rpc.lock().unwrap().rpc_ctl().clone()
    }

    pub fn rpc_url(&self) -> Option<String> {
        self.rpc_ctl().descriptor()
    }

    pub fn rpc_client(&self) -> Arc<KaspaRpcClient> {
        self.rpc_api()
            .clone()
            .downcast_arc::<KaspaRpcClient>()
            .expect("downcast to KaspaRpcClient")
    }

    pub fn processor(&self) -> &Arc<Processor> {
        &self.inner.processor
    }

    pub fn sender(&self) -> &mpsc::Sender<Ingest> {
        &self.inner.sender
    }

    /// Signifies **valid and negotiated** connection to the node.
    /// This flag is set to true only after the connection is established
    /// and the node is validated for the required features / state.
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected.load(Ordering::SeqCst)
    }

    pub async fn notify(&self, event: Event) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| Error::Custom("multiplexer channel error during notify".to_string()))?;
        Ok(())
    }

    pub fn try_notify(&self, event: Event) -> Result<()> {
        self.multiplexer()
            .try_broadcast(Box::new(event))
            .map_err(|_| {
                Error::Custom("multiplexer channel error during try_notify".to_string())
            })?;
        Ok(())
    }

    // placeholder for client broadcasting
    // pub fn publish(&self, event: Box<??>) {}

    pub async fn init_state_from_server(&self) -> Result<bool> {
        let GetServerInfoResponse {
            server_version,
            network_id: server_network_id,
            has_utxo_index: _,
            is_synced,
            virtual_daa_score,
            rpc_api_version,
            rpc_api_revision,
        } = self.rpc_api().get_server_info().await?;

        let network_id = self.network_id();
        if network_id != server_network_id {
            return Err(Error::InvalidNetworkType(
                network_id.to_string(),
                server_network_id.to_string(),
            ));
        }

        if rpc_api_version > RPC_API_VERSION {
            let current = [RPC_API_VERSION, RPC_API_REVISION]
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(".");
            let connected = [rpc_api_version, rpc_api_revision]
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(".");
            return Err(Error::RpcApiVersion(current, connected));
        }

        self.inner
            .current_daa_score
            .store(virtual_daa_score, Ordering::SeqCst);

        log_trace!(
            "Connected to {}",
            self.rpc_url().unwrap_or("N/A".to_string())
        );
        log_trace!("kaspad '{server_version}' on '{server_network_id}';  SYNC: {is_synced}  DAA: {virtual_daa_score}");
        // self.notify(Events::ServerStatus { server_version, is_synced, network_id, url: self.rpc_url() }).await?;

        Ok(is_synced)
    }

    pub async fn handle_connect_impl(&self) -> Result<()> {
        let is_node_synced = self.init_state_from_server().await?;

        if !is_node_synced {
            // This will cascade back to handle_connect which will trigger
            // disconnect.  If running with a resolver, this will result
            // in a reconnection attempt to a different synced node.
            return Err(Error::NodeNotSynced);
        }

        self.inner.is_connected.store(true, Ordering::SeqCst);

        self.inner.is_synced.store(false, Ordering::SeqCst);
        self.register_notification_listener().await?;
        self.notify(Event::Start).await?;

        // TODO:
        // obtain blocks using low-hash
        self.sync().await?;
        // TODO:
        // 1. drain data from sync
        // 2. drain data from transactions that came in during sync
        self.drain().await?;
        self.inner.is_synced.store(true, Ordering::SeqCst);
        self.notify(Event::Synced).await?;

        Ok(())
    }

    pub async fn handle_connect(&self) -> Result<()> {
        match self.handle_connect_impl().await {
            Err(err) => {
                log_error!("Error while connecting to node: {err}");
                // force disconnect the client if we have failed
                // to negotiate the connection to the node.
                // self.rpc_client().trigger_abort()?;
                self.disconnect().await?;
                task::sleep(Duration::from_secs(3)).await;
                self.connect().await?;
                Err(err)
            }
            Ok(_) => Ok(()),
        }
    }

    pub async fn handle_disconnect(&self) -> Result<()> {
        self.inner.is_connected.store(false, Ordering::SeqCst);

        self.unregister_notification_listener().await?;
        self.notify(Event::Stop).await?;
        self.cleanup().await?;

        Ok(())
    }

    pub async fn cleanup(&self) -> Result<()> {
        // TODO - determine if pending cleanup should occur on disconnect
        self.inner.pending.lock().unwrap().clear();
        Ok(())
    }

    async fn register_notification_listener(&self) -> Result<()> {
        let listener_id = self.rpc_api().register_new_listener(ChannelConnection::new(
            "NEXUS",
            self.inner.notification_channel.sender.clone(),
            ChannelType::Persistent,
        ));
        *self.inner.listener_id.lock().unwrap() = Some(listener_id);

        let rpc_api = self.rpc_api();

        rpc_api
            .start_notify(
                listener_id,
                Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {}),
            )
            .await?;

        rpc_api
            .start_notify(listener_id, Scope::BlockAdded(BlockAddedScope {}))
            .await?;

        rpc_api
            .start_notify(
                listener_id,
                Scope::VirtualChainChanged(VirtualChainChangedScope {
                    include_accepted_transaction_ids: false,
                }),
            )
            .await?;

        Ok(())
    }

    async fn unregister_notification_listener(&self) -> Result<()> {
        let listener_id = self.inner.listener_id.lock().unwrap().take();
        if let Some(id) = listener_id {
            // we do not need this as we are unregister the entire listener here...
            self.rpc_api().unregister_listener(id).await?;
        }
        Ok(())
    }

    async fn handle_notification(&self, notification: Notification) -> Result<()> {
        match notification {
            Notification::VirtualDaaScoreChanged(virtual_daa_score_changed_notification) => {
                let VirtualDaaScoreChangedNotification { virtual_daa_score } =
                    virtual_daa_score_changed_notification;
                self.handle_daa_score_change(virtual_daa_score).await?;
            }

            Notification::BlockAdded(block_added_notification) => {
                let BlockAddedNotification { block } = block_added_notification;

                let block = Arc::try_unwrap(block)
                    .expect("Unable to unwrap block in BlockAddedNotification");

                let RpcBlock {
                    header: _,
                    transactions,
                    verbose_data: _,
                } = block;

                // Skip coinbase tx
                // for tx in block_added_notification.block.transactions.iter().skip(1) {
                // for tx in transactions.into_iter().skip(1) {
                for tx in transactions.into_iter() {
                    self.handle_transaction(tx)?;
                }

                self.drain().await?;
            }

            Notification::VirtualChainChanged(virtual_chain_changed_notification) => {
                // self.inner
                //     .sender
                //     .send(Ingest::VirtualChainChanged(virtual_chain_changed_notification.into()));

                self.handle_virtual_chain_changed(virtual_chain_changed_notification)?;
                // .await?;

                // let VirtualChainChangedNotification {
                //     removed_chain_block_hashes,
                //     added_chain_block_hashes,
                //     accepted_transaction_ids,
                // } = virtual_chain_changed_notification;

                // // TODO - TBD
                // removed_chain_block_hashes.iter().for_each(|_hash| {});
                // added_chain_block_hashes.iter().for_each(|_hash| {});
                // accepted_transaction_ids.iter().for_each(|_txid| {});
                // println!("VirtualChainChanged: {:?}", virtual_chain_changed_notification);
            }

            // Notification::UtxosChanged(utxos_changed_notification) => {
            //     if !self.is_synced() {
            //         self.sync_proc().track(true).await?;
            //     }

            //     self.handle_utxo_changed(utxos_changed_notification).await?;
            // }
            _ => {
                log_warn!("unknown notification: {:?}", notification);
            }
        }

        Ok(())
    }

    pub async fn handle_daa_score_change(&self, current_daa_score: u64) -> Result<()> {
        self.inner
            .current_daa_score
            .store(current_daa_score, Ordering::SeqCst);
        self.notify(Event::DaaScoreChange { current_daa_score })
            .await?;

        // println!("DAA Score: {current_daa_score}");

        Ok(())
    }

    #[inline]
    fn handle_transaction(&self, transaction: RpcTransaction) -> Result<()> {
        // TODO
        // Ignore standard transactions
        // Place protocol transactions into a pending queue

        self.sender()
            .send(Ingest::Transaction(transaction.clone().into()))?;

        let Some(_txid) = transaction
            .verbose_data
            .as_ref()
            .map(|data| data.transaction_id)
        else {
            return Ok(());
        };

        // just some fake pre-selection logic
        // if txid.as_bytes()[0] < 200 {
        // return Ok(());
        // } else {
        self.inner.pending.lock().unwrap().push(transaction.clone());
        // }

        Ok(())
    }

    #[inline]
    fn handle_virtual_chain_changed(
        &self,
        notification: VirtualChainChangedNotification,
    ) -> Result<()> {
        // TODO
        self.sender()
            .send(Ingest::VirtualChainChanged(notification.into()))?;

        Ok(())
    }

    async fn sync(&self) -> Result<()> {
        // TODO get low hash and loop over blocks

        // Check if transactions / blocks have already been processed
        // process transactions as they come in
        // (while this occurs we are also receiving current block notifications
        // where the results are going into the pending queue)

        Ok(())
    }

    async fn drain(&self) -> Result<()> {
        // ignore drain requests while not synced
        if !self.inner.is_synced.load(Ordering::SeqCst) {
            return Ok(());
        }

        // TODO drain pending blocks

        // General concept:
        // receive current block notifications while performing sync
        // place transactions in the pending queue (db?)
        // once sync is complete, drain the pending queue
        // all received notifications should be placed in the pending queue
        // and then drained

        self.inner
            .pending
            .lock()
            .unwrap()
            .drain(..)
            .for_each(|transaction| {
                self.try_notify(Event::Transaction {
                    transaction: Box::new(transaction),
                })
                .unwrap_or_else(|err| log_error!("Unable to post transaction event: {err}"));
            });

        Ok(())
    }

    async fn task(self: Arc<Self>) -> Result<()> {
        let rpc_ctl_channel = self.rpc_ctl().multiplexer().channel();
        let notification_receiver = self.inner.notification_channel.receiver.clone();

        loop {
            select_biased! {
                msg = rpc_ctl_channel.receiver.recv().fuse() => {
                    match msg {
                        Ok(msg) => {
                            // handle RPC channel connection and disconnection events
                            match msg {
                                RpcState::Connected => {

                                    if !self.is_connected() {
                                        if let Err(err) = self.handle_connect().await {
                                            log_error!("Nexus sync task error: {err}");
                                        } else {
                                            self.inner.multiplexer.try_broadcast(Box::new(Event::Connect {
                                                network_id : self.network_id(),
                                                url : self.rpc_url()
                                            })).unwrap_or_else(|err| log_error!("{err}"));
                                        }
                                    }
                                },
                                RpcState::Disconnected => {
                                    if self.is_connected() {
                                        self.inner.multiplexer.try_broadcast(Box::new(Event::Disconnect {
                                            network_id : self.network_id(),
                                            url : self.rpc_url()
                                        })).unwrap_or_else(|err| log_error!("{err}"));
                                        self.handle_disconnect().await.unwrap_or_else(|err| log_error!("{err}"));
                                    } else {
                                        log_error!("NEXUS disconnected from {:?}", self.rpc_url());
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            panic!("Nexus RpcCtl channel error: {err}");
                        }
                    }
                }
                notification = notification_receiver.recv().fuse() => {
                    match notification {
                        Ok(notification) => {
                            if let Err(err) = self.handle_notification(notification).await {
                                log_error!("error while handling notification: {err}");
                            }
                        }
                        Err(err) => {
                            panic!("RPC notification channel error: {err}");
                        }
                    }
                },

                // we use select_biased to drain rpc_ctl
                // and notifications before shutting down
                // as such task_ctl is last in the poll order
                _ = self.inner.shutdown.request.recv().fuse() => {
                    break;
                },

            }
        }

        // handle power down on rpc channel that remains connected
        if self.is_connected() {
            self.handle_disconnect()
                .await
                .unwrap_or_else(|err| log_error!("{err}"));
        }

        self.inner.shutdown.response.send(()).await?;

        Ok(())
    }
}

impl Nexus {
    pub async fn ping_call(
        &self,
        _ctx: &dyn ContextT,
        _request: PingRequest,
    ) -> Result<PingResponse> {
        println!();
        println!("+------+");
        println!("| PING |");
        println!("+------+");
        println!();

        let response = PingResponse {};
        Ok(response)
    }

    pub async fn get_status_call(
        &self,
        _ctx: &dyn ContextT,
        _request: GetStatusRequest,
    ) -> Result<GetStatusResponse> {
        let response = GetStatusResponse {
            sparkled_version: std::env!("CARGO_PKG_VERSION").to_string(),
            network_id: self.network_id(),
        };
        Ok(response)
    }
}

const SERVICE: &str = "NEXUS";

#[async_trait]
impl Service for Nexus {
    async fn spawn(self: Arc<Self>, runtime: Runtime) -> ServiceResult<()> {
        // log_trace!("starting {SERVICE}...");

        self.inner.processor.clone().spawn(runtime.clone()).await?;

        self.connect()
            .await
            .map_err(|err| ServiceError::custom(format!("{SERVICE} RPC connect error: {err}")))?;

        task::spawn(async move {
            self.task()
                .await
                .unwrap_or_else(|err| log_error!("{SERVICE} error: {err}"));
        });

        Ok(())
    }

    fn terminate(self: Arc<Self>) {
        // log_trace!("sending an exit signal to {SERVICE}");
        self.inner.shutdown.request.try_send(()).unwrap();

        self.inner.processor.clone().terminate();
    }

    async fn join(self: Arc<Self>) -> ServiceResult<()> {
        self.inner.shutdown.response.recv().await?;

        self.inner.processor.clone().join().await?;

        Ok(())
    }
}
