use crate::{
    configs::ShimConfig,
    //data_collection::ClientData,
    network::Network,
};

use chrono::Utc;
use log::*;
use omnipaxos_kv::common::{kv::*, messages::*};
use rand::Rng;
use std::time::Duration;
use tokio::time::interval;

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
        println!("Running shim...")
    }
}