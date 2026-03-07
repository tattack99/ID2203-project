use crate::{configs::ClientConfig, data_collection::ClientData, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos_kv::common::{kv::*, messages::*};
use std::time::Duration;
use crate::shim::ApiCommand;
use std::collections::HashMap;
use tokio::sync::oneshot;

const NETWORK_BATCH_SIZE: usize = 100;
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

pub struct Client {
    id: ClientId,
    network: Network,
    client_data: ClientData,
    config: ClientConfig,
    active_server: NodeId,
    final_request_count: Option<usize>,
    next_request_id: usize,
    pub pending_gets: HashMap<usize, oneshot::Sender<String>>,
    pub pending_puts: HashMap<usize, oneshot::Sender<String>>,
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
            pending_gets: HashMap::new(),
            pending_puts: HashMap::new(),
        }
    }

    pub async fn run(&mut self, mut http_rx: tokio::sync::mpsc::Receiver<ApiCommand>) {
        // Wait for the initial start signal from the server
        self.wait_for_start_signal().await;

        info!("{}: READY - Waiting for requests...", self.id);

        loop {
            tokio::select! {
                // Handle HTTP requests from the shim API
                Some(cmd) = http_rx.recv() => {
                    match cmd {
                        ApiCommand::Put(k, v, tx) => {
                            let key = self.next_request_id;
                            // If send fails, immediately return error to caller
                            if self.send_request_manual(KVCommand::Put(k, v)).await.is_err() {
                                let _ = tx.send("Error: server disconnected".to_string());
                            } else {
                                self.pending_puts.insert(key, tx);
                            }
                        }
                        ApiCommand::Get(k, tx) => {
                            let key = self.next_request_id;
                            if self.send_request_manual(KVCommand::Get(k)).await.is_err() {
                                let _ = tx.send("Error: server disconnected".to_string());
                            } else {
                                self.pending_gets.insert(key, tx);
                            }
                        }
                    }
                }
                // Handle messages from the server
                msg_opt = self.network.server_messages.recv() => {
                    match msg_opt {
                        Some(msg) => {
                            if let ServerMessage::Write(id) = &msg {
                                if let Some(tx) = self.pending_puts.remove(id) {
                                    let _ = tx.send("Ok".to_string());
                                }
                            }
                            if let ServerMessage::Read(id, value) = &msg {
                                if let Some(tx) = self.pending_gets.remove(id) {
                                    let result = value.clone().unwrap_or_else(|| "Key not found".to_string());
                                    let _ = tx.send(result);
                                }
                            }
                            self.handle_server_message(msg);
                        }
                        None => {
                            // Channel closed — server connection is dead.
                            // This shouldn't happen since we hold the sender,
                            // but guard against it anyway.
                            warn!("{}: server_messages channel closed unexpectedly", self.id);
                            self.fail_all_pending("Error: connection lost");
                            self.reconnect_loop().await;
                        }
                    }
                }
                // Detect when no server messages arrive for a while.
                // The reader task ending (connection drop) will cause recv()
                // to just never return anything. We use a periodic check to
                // detect this and trigger reconnection.
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    if !self.network.is_connected(self.active_server) {
                        warn!("{}: Detected server {} is disconnected", self.id, self.active_server);
                        self.fail_all_pending("Error: connection lost");
                        self.reconnect_loop().await;
                    }
                }
            }
        }
    }

    /// Fail all pending GET and PUT requests back to HTTP callers with an error message.
    /// This prevents Jepsen/curl from hanging forever when the server is down.
    fn fail_all_pending(&mut self, error_msg: &str) {
        let put_count = self.pending_puts.len();
        let get_count = self.pending_gets.len();
        for (_, tx) in self.pending_puts.drain() {
            let _ = tx.send(error_msg.to_string());
        }
        for (_, tx) in self.pending_gets.drain() {
            let _ = tx.send(error_msg.to_string());
        }
        if put_count + get_count > 0 {
            warn!("{}: Failed {} pending requests ({} puts, {} gets)",
                self.id, put_count + get_count, put_count, get_count);
        }
    }

    /// Keep trying to reconnect to the server until we succeed,
    /// then resume normal operation immediately.
    async fn reconnect_loop(&mut self) {
        info!("{}: Starting reconnection loop to server {}...", self.id, self.active_server);
        loop {
            tokio::time::sleep(RECONNECT_DELAY).await;
            if self.network.reconnect(self.active_server).await {
                info!("{}: Reconnected to server {}. Resuming.", self.id, self.active_server);
                return;
            }
            warn!("{}: Reconnect to server {} failed, retrying in {:?}...",
                self.id, self.active_server, RECONNECT_DELAY);
        }
    }

    /// Wait for a StartSignal from the server.
    /// Used both on initial startup and after reconnection.
    async fn wait_for_start_signal(&mut self) {
        loop {
            match self.network.server_messages.recv().await {
                Some(ServerMessage::StartSignal(start_time)) => {
                    Self::wait_until_sync_time(&mut self.config, start_time).await;
                    return;
                }
                Some(other) => {
                    // Got a non-start message (e.g. a response to a stale request).
                    // Just discard it and keep waiting.
                    debug!("{}: Ignoring message while waiting for StartSignal: {:?}", self.id, other);
                }
                None => {
                    // Channel unexpectedly closed during wait. This shouldn't
                    // normally happen. Sleep and hope reconnect_loop is called.
                    warn!("{}: server_messages closed while waiting for StartSignal", self.id);
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    return;
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
        let _ = self.network.send(self.active_server, request).await;
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

    async fn send_request_manual(&mut self, cmd: KVCommand) -> Result<(), ()> {
        let is_write = matches!(cmd, KVCommand::Put(_, _));
        let request = ClientMessage::Append(self.next_request_id, cmd);

        debug!("Sending {request:?}");
        let result = self.network.send(self.active_server, request).await;
        if result.is_ok() {
            self.client_data.new_request(is_write);
            self.next_request_id += 1;
        }
        result
    }
}