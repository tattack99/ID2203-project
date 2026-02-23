use crate::{
    configs::ShimConfig,
    //data_collection::ClientData,
    network::Network,
};

use omnipaxos_kv::common::{kv::*, messages::*};
use tokio::time::{interval, Duration};
use log::debug;

const NETWORK_BATCH_SIZE: usize = 100;

pub struct Shim {
    id: ClientId,
    network: Network,
    //client_data: ClientData,
    config: ShimConfig,
    active_server: NodeId,
    final_request_count: Option<usize>,
    next_request_id: usize,
}

impl Shim {
    pub async fn new(config: ShimConfig) -> Self {
        let network = Network::new(
            vec![(config.server_id, config.server_address.clone())],
            NETWORK_BATCH_SIZE,
        )
        .await;
        Shim {
            id: config.server_id,
            network,
            //client_data: ClientData::new(),
            active_server: config.server_id,
            config,
            final_request_count: None,
            next_request_id: 0,
        }
    }

    pub async fn run(&mut self) {
        println!("Running shim {}...", self.id);

        // 1. Setup Request Timing (e.g., one request every 500ms)
        let mut request_interval = interval(Duration::from_millis(500));
        
        // 2. Define the limit
        let max_requests = 20; 

        loop {
            // Check for completion at the start of every iteration
            if self.next_request_id >= max_requests {
                println!("Reached max requests ({}), exiting loop.", max_requests);
                break;
            }

            tokio::select! {
                biased;

                // Branch 1: Handle server responses
                Some(msg) = self.network.server_messages.recv() => {
                    self.handle_server_message(msg);
                }

                // Branch 2: Send new requests
                _ = request_interval.tick() => {
                    let is_write = self.next_request_id % 2 == 0;
                    self.send_request(is_write).await;
                }
            }
        }
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        debug!("Recieved {msg:?}");
        match msg {
            ServerMessage::StartSignal(_) => (),
            server_response => {
                //let cmd_id = server_response.command_id();
                //self.client_data.new_response(cmd_id);
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
        self.next_request_id += 1;
    }

}