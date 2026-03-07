use crate::{configs::OmniPaxosKVConfig, database::Database, network::Network};
use chrono::Utc;
use log::*;
use omnipaxos::{
    messages::Message,
    util::{LogEntry, NodeId},
    OmniPaxos, OmniPaxosConfig,
};
use omnipaxos_kv::common::{kv::*, messages::*, utils::Timestamp};
use omnipaxos_storage::persistent_storage::{PersistentStorage, PersistentStorageConfig};
use std::{fs::File, io::Write, time::Duration};

type OmniPaxosInstance = OmniPaxos<Command, PersistentStorage<Command>>;
const NETWORK_BATCH_SIZE: usize = 100;
const LEADER_WAIT: Duration = Duration::from_secs(1);
const ELECTION_TIMEOUT: Duration = Duration::from_millis(500);

/// How often we persist the database snapshot (in number of newly decided entries).
/// Set to 1 for maximum safety (every decided entry triggers a save).
/// Increase for better performance at the cost of replaying more entries on recovery.
const SNAPSHOT_INTERVAL: usize = 1;

pub struct OmniPaxosServer {
    id: NodeId,
    database: Database,
    network: Network,
    omnipaxos: OmniPaxosInstance,
    current_decided_idx: usize,
    omnipaxos_msg_buffer: Vec<Message<Command>>,
    config: OmniPaxosKVConfig,
    peers: Vec<NodeId>,
    /// Tracks how many entries since last snapshot save
    entries_since_snapshot: usize,
    /// The decided_idx from the snapshot loaded at startup.
    /// Entries at or below this index have already been applied to the database
    /// and should be skipped during catch-up. Set to 0 if no snapshot was loaded.
    snapshot_decided_idx: usize,
}

impl OmniPaxosServer {
    pub async fn new(config: OmniPaxosKVConfig) -> Self {
        let id = config.local.server_id;
        let snapshot_path = Self::snapshot_path(id);

        // Try to recover from a previous snapshot
        let (database, recovered_decided_idx) = match Database::recover(&snapshot_path) {
            Some((db, idx)) => {
                info!("ID {id}: Recovered from snapshot with decided_idx={idx}");
                (db, idx)
            }
            None => {
                info!("ID {id}: No snapshot found, starting fresh.");
                (Database::new(), 0)
            }
        };

        // Initialize OmniPaxos with PersistentStorage (RocksDB-backed).
        // On first boot this creates the DB; on recovery it loads existing state,
        // so OmniPaxos knows its previous ballot, decided index, and log entries.
        let storage_path = format!("/app/logs/omnipaxos-node-{id}");
        let mut storage_config = PersistentStorageConfig::default();
        storage_config.set_path(storage_path);
        let storage = PersistentStorage::open(storage_config);
        let omnipaxos_config: OmniPaxosConfig = config.clone().into();
        let omnipaxos_msg_buffer = Vec::with_capacity(omnipaxos_config.server_config.buffer_size);
        let omnipaxos = omnipaxos_config.build(storage).unwrap();

        // Waits for client and server network connections to be established
        let network = Network::new(config.clone(), NETWORK_BATCH_SIZE).await;

        let mut db = database;
        db.set_snapshot_path(snapshot_path);

        OmniPaxosServer {
            id,
            database: db,
            network,
            omnipaxos,
            current_decided_idx: 0,
            omnipaxos_msg_buffer,
            peers: config.get_peers(config.local.server_id),
            config,
            entries_since_snapshot: 0,
            snapshot_decided_idx: recovered_decided_idx,
        }
    }

    /// Returns the file path for this server's snapshot.
    /// Uses /app/logs/ which is mounted as a Docker volume and survives restarts.
    fn snapshot_path(id: NodeId) -> String {
        format!("/app/logs/server-{id}-snapshot.json")
    }

    pub async fn run(&mut self) {
        // Save config to output file
        self.save_output().expect("Failed to write to file");
        let mut client_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);
        let mut cluster_msg_buf = Vec::with_capacity(NETWORK_BATCH_SIZE);

        // Always establish initial leader — even after recovery, we need
        // the leader election to complete and start signals to be sent to clients.
        self.establish_initial_leader(&mut cluster_msg_buf, &mut client_msg_buf).await;

        // Main event loop with leader election
        let mut election_interval = tokio::time::interval(ELECTION_TIMEOUT);
        let mut election_tick_counter = 0;
        loop {
            tokio::select! {
                _ = election_interval.tick() => {
                    self.omnipaxos.tick();
                    if election_tick_counter >= 10 {
                        election_tick_counter = 0;
                        match self.omnipaxos.get_current_leader() {
                            Some((leader, is_accept)) => info!("Leader: {leader}, Accept: {is_accept}"),
                            None => info!("Waiting for leader election..."),
                        }
                    }
                    else {
                        election_tick_counter = election_tick_counter + 1;
                    }
                    self.send_outgoing_msgs();
                },
                _ = self.network.cluster_messages.recv_many(&mut cluster_msg_buf, NETWORK_BATCH_SIZE) => {
                    self.handle_cluster_messages(&mut cluster_msg_buf).await;
                },
                _ = self.network.client_messages.recv_many(&mut client_msg_buf, NETWORK_BATCH_SIZE) => {
                    self.handle_client_messages(&mut client_msg_buf).await;
                },
                Some(new_conn) = self.network.recovery_receiver.recv() => {
                    self.network.apply_connection(new_conn);
                    info!("ID {}: Re-established connection with a recovered node.", self.id);
                        // If we have already decided on a leader and the experiment started:
                    if let Some((leader_id, _)) = self.omnipaxos.get_current_leader() {
                        // Option A: If I am the leader, I send the start signal again
                        info!("ID {}: leader: {}", self.id, leader_id);
                        if leader_id == self.id {
                            let current_time = Utc::now().timestamp_millis();
                            self.send_cluster_start_signals(current_time);
                            self.send_client_start_signals(current_time);
                            info!("ID {}: Re-sent start signals to recovered node.", self.id);
                        }
                    }
                    self.send_outgoing_msgs();
                },
            }
        }
    }

    // Ensures cluster is connected and initial leader is promoted before returning.
    // Once the leader is established it chooses a synchronization point which the
    // followers relay to their clients to begin the experiment.
    async fn establish_initial_leader(
        &mut self,
        cluster_msg_buffer: &mut Vec<(NodeId, ClusterMessage)>,
        client_msg_buffer: &mut Vec<(ClientId, ClientMessage)>,
    ) {
        let mut leader_takeover_interval = tokio::time::interval(LEADER_WAIT);
        info!("{}: Establish initial leader", self.id);
        loop {
            tokio::select! {
                _ = leader_takeover_interval.tick(), if self.config.cluster.initial_leader == self.id => {
                    info!("ID {}: Initial leader tick. Checking status...", self.id);
                    if let Some((curr_leader, is_accept_phase)) = self.omnipaxos.get_current_leader(){
                        info!("ID {}: Found leader {} (accept: {})", self.id, curr_leader, is_accept_phase);
                        if curr_leader == self.id && is_accept_phase {
                            info!("{}: Leader fully initialized. Breaking loop.", self.id);
                            let experiment_sync_start = (Utc::now() + Duration::from_secs(2)).timestamp_millis();
                            self.send_cluster_start_signals(experiment_sync_start);
                            self.send_client_start_signals(experiment_sync_start);
                            break;
                        }
                    }
                    info!("{}: Attempting to take leadership", self.id);
                    self.omnipaxos.try_become_leader();
                    self.send_outgoing_msgs();
                },
                _ = self.network.cluster_messages.recv_many(cluster_msg_buffer, NETWORK_BATCH_SIZE) => {
                    let recv_start = self.handle_cluster_messages(cluster_msg_buffer).await;
                    if recv_start {
                        info!("ID {}: Received start signal from leader. Exiting establish loop.", self.id);
                        break;
                    }
                },
                _ = self.network.client_messages.recv_many(client_msg_buffer, NETWORK_BATCH_SIZE) => {
                    self.handle_client_messages(client_msg_buffer).await;
                },
                Some(new_conn) = self.network.recovery_receiver.recv() => {
                    self.network.apply_connection(new_conn);
                    info!("ID {}: Re-established connection with a recovered node.", self.id);
                        // If we have already decided on a leader and the experiment started:
                    if let Some((leader_id, _)) = self.omnipaxos.get_current_leader() {
                        // Option A: If I am the leader, I send the start signal again
                        info!("ID {}: leader: {}", self.id, leader_id);
                        if leader_id == self.id {
                            let current_time = Utc::now().timestamp_millis();
                            self.send_cluster_start_signals(current_time);
                            self.send_client_start_signals(current_time);
                            info!("ID {}: Re-sent start signals to recovered node.", self.id);
                        }
                    }
                    self.send_outgoing_msgs();
                },
            }
        }
    }

    fn handle_decided_entries(&mut self) {
        let new_decided_idx = self.omnipaxos.get_decided_idx();
        if self.current_decided_idx < new_decided_idx {
            let decided_entries = self
                .omnipaxos
                .read_decided_suffix(self.current_decided_idx)
                .unwrap();
            let old_decided_idx = self.current_decided_idx;
            self.current_decided_idx = new_decided_idx;
            debug!("Decided {new_decided_idx}");

            let decided_commands: Vec<Command> = decided_entries
                .into_iter()
                .filter_map(|e| match e {
                    LogEntry::Decided(cmd) => Some(cmd),
                    _ => unreachable!(),
                })
                .collect();

            // For each decided command:
            // - If its log position is <= snapshot_decided_idx, the database
            //   already has it from the snapshot loaded at startup. Skip it.
            // - If its log position is > snapshot_decided_idx, apply normally.
            let mut new_entries_applied = 0;
            for (i, command) in decided_commands.into_iter().enumerate() {
                let entry_idx = old_decided_idx + i + 1; // 1-indexed log position
                if entry_idx <= self.snapshot_decided_idx {
                    // Already applied to database from snapshot — skip DB update
                    debug!("Skipping already-applied entry at idx {entry_idx}");
                    continue;
                }
                let read = self.database.handle_command(command.kv_cmd);
                new_entries_applied += 1;
                if command.coordinator_id == self.id {
                    let response = match read {
                        Some(read_result) => ServerMessage::Read(command.id, read_result),
                        None => ServerMessage::Write(command.id),
                    };
                    self.network.send_to_client(command.client_id, response);
                }
            }

            // Persist snapshot if we applied new entries
            if new_entries_applied > 0 {
                self.entries_since_snapshot += new_entries_applied;
                if self.entries_since_snapshot >= SNAPSHOT_INTERVAL {
                    self.database.save_snapshot(self.current_decided_idx);
                    self.entries_since_snapshot = 0;
                }
            }
        }
    }

    fn send_outgoing_msgs(&mut self) {
        self.omnipaxos.take_outgoing_messages(&mut self.omnipaxos_msg_buffer);
        for msg in self.omnipaxos_msg_buffer.drain(..) {
            let to = msg.get_receiver();
            let cluster_msg = ClusterMessage::OmniPaxosMessage(msg);
            self.network.send_to_cluster(to, cluster_msg);
        }
    }

    async fn handle_client_messages(&mut self, messages: &mut Vec<(ClientId, ClientMessage)>) {
        for (from, message) in messages.drain(..) {
            match message {
                ClientMessage::Append(command_id, kv_command) => {
                    self.append_to_log(from, command_id, kv_command);
                }
            }
        }
        self.send_outgoing_msgs();
    }

    async fn handle_cluster_messages(
        &mut self,
        messages: &mut Vec<(NodeId, ClusterMessage)>,
    ) -> bool {
        let mut received_start_signal = false;
        for (from, message) in messages.drain(..) {
            trace!("{}: Received {message:?}", self.id);
            match message {
                ClusterMessage::OmniPaxosMessage(m) => {
                    self.omnipaxos.handle_incoming(m);
                    self.handle_decided_entries();
                }
                ClusterMessage::LeaderStartSignal(start_time) => {
                    debug!("Received start message from peer {from}");
                    received_start_signal = true;
                    self.send_client_start_signals(start_time);
                }
            }
        }
        self.send_outgoing_msgs();
        received_start_signal
    }

    fn append_to_log(&mut self, from: ClientId, command_id: CommandId, kv_command: KVCommand) {
        let command = Command {
            client_id: from,
            coordinator_id: self.id,
            id: command_id,
            kv_cmd: kv_command,
        };
        self.omnipaxos
            .append(command)
            .expect("Append to Omnipaxos log failed");
    }

    fn send_cluster_start_signals(&mut self, start_time: Timestamp) {
        for peer in &self.peers {
            debug!("Sending start message to peer {peer}");
            let msg = ClusterMessage::LeaderStartSignal(start_time);
            self.network.send_to_cluster(*peer, msg);
        }
    }

    fn send_client_start_signals(&mut self, start_time: Timestamp) {
        for client_id in 1..self.config.local.num_clients as ClientId + 1 {
            debug!("Sending start message to client {client_id}");
            let msg = ServerMessage::StartSignal(start_time);
            self.network.send_to_client(client_id, msg);
        }
    }

    fn save_output(&mut self) -> Result<(), std::io::Error> {
        let config_json = serde_json::to_string_pretty(&self.config)?;
        let mut output_file = File::create(&self.config.local.output_filepath)?;
        output_file.write_all(config_json.as_bytes())?;
        output_file.flush()?;
        Ok(())
    }
}