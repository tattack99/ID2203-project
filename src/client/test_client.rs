use crate::{configs::ClientConfig, data_collection::ClientData, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos_kv::common::{kv::*, messages::*};
use std::time::Duration;
use crate::shim::ApiCommand; 
use std::collections::HashMap;
use tokio::sync::oneshot;

const NETWORK_BATCH_SIZE: usize = 100;

pub struct Client {
    id: ClientId,
    network: Network,
    client_data: ClientData,
    config: ClientConfig,
    active_server: NodeId,
    final_request_count: Option<usize>,
    next_request_id: usize,
    pub pending_gets: HashMap<usize, oneshot::Sender<String>>,
}

impl Client {
    pub async fn new(config: ClientConfig) -> Self {
        let network = Network::new(
            vec![(config.server_id, config.server_address.clone())],
            NETWORK_BATCH_SIZE,
        )
        .await;
        Client {
            id: config.server_id,
            network,
            client_data: ClientData::new(),
            active_server: config.server_id,
            config,
            final_request_count: None,
            next_request_id: 0,
            pending_gets: std::collections::HashMap::new(), 
        }
    }

pub async fn run(&mut self, mut http_rx: tokio::sync::mpsc::Receiver<ApiCommand>) {
    match self.network.server_messages.recv().await {
        Some(ServerMessage::StartSignal(start_time)) => {
            Self::wait_until_sync_time(&mut self.config, start_time).await;
        }
        _ => panic!("Error waiting for start signal"),
    }

    info!("{}: READY - Waiting for curl...", self.id);

    loop {
        tokio::select! {
            // Handles request from shim api, can be curl or jepsen
            Some(cmd) = http_rx.recv() => {
                match cmd {
                    ApiCommand::Put(k, v) => self.send_request_manual(KVCommand::Put(k, v)).await,
                    ApiCommand::Get(k, tx) => {
                        let key = self.next_request_id;
                        self.pending_gets.insert(key, tx); 
                        self.send_request_manual(KVCommand::Get(k)).await;
                    }
                }
            }
            Some(msg) = self.network.server_messages.recv() => {
                info!("{}: SERVER LOG -> {:?}", self.id, msg);

                // CHECK: Is this a response to a GET we are tracking?
                if let ServerMessage::Read(id, value) = &msg {
                    // We use .remove(id) because we only respond once per request
                    if let Some(tx) = self.pending_gets.remove(id) {
                        let result = value.clone().unwrap_or_else(|| "Key not found".to_string());
                        let _ = tx.send(result); // This wakes up the 'http_get' function!
                    }
                }
                self.handle_server_message(msg);
            }
        }
    }
}

    fn handle_server_message(&mut self, msg: ServerMessage) {
        debug!("Recieved {msg:?}");
        match msg {
            ServerMessage::StartSignal(_) => (),
            server_response => {
                let cmd_id = server_response.command_id();
                self.client_data.new_response(cmd_id);
            }
        }
    }

    async fn send_request(&mut self, is_write: bool) {
        let key = self.next_request_id.to_string();
        let cmd = match is_write {
            true => KVCommand::Put(key.clone(), key),
            false => KVCommand::Get(key),
        };
        let request = ClientMessage::Append(self.next_request_id, cmd);
        debug!("Sending {request:?}");
        self.network.send(self.active_server, request).await;
        self.client_data.new_request(is_write);
        self.next_request_id += 1;
    }

    fn run_finished(&self) -> bool {
        if let Some(count) = self.final_request_count {
            if self.client_data.request_count() >= count {
                return true;
            }
        }
        return false;
    }

    // Wait until the scheduled start time to synchronize client starts.
    // If start time has already passed, start immediately.
    async fn wait_until_sync_time(config: &mut ClientConfig, scheduled_start_utc_ms: i64) {
        // // Desync the clients a bit
        // let mut rng = rand::thread_rng();
        // let scheduled_start_utc_ms = scheduled_start_utc_ms + rng.gen_range(1..100);
        let now = Utc::now();
        let milliseconds_until_sync = scheduled_start_utc_ms - now.timestamp_millis();
        config.sync_time = Some(milliseconds_until_sync);
        if milliseconds_until_sync > 0 {
            tokio::time::sleep(Duration::from_millis(milliseconds_until_sync as u64)).await;
        } else {
            warn!("Started after synchronization point!");
        }
    }

    fn save_results(&self) -> Result<(), std::io::Error> {
        self.client_data.save_summary(self.config.clone())?;
        self.client_data
            .to_csv(self.config.output_filepath.clone())?;
        Ok(())
    }

    async fn send_request_manual(&mut self, cmd: KVCommand) {
        let is_write = matches!(cmd, KVCommand::Put(_, _));
        let request = ClientMessage::Append(self.next_request_id, cmd);
        
        debug!("Sending {request:?}");
        self.network.send(self.active_server, request).await;
        self.client_data.new_request(is_write);
        self.next_request_id += 1;
    }

}
