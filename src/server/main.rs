// use crate::{configs::OmniPaxosKVConfig, server::OmniPaxosServer};
// use env_logger;

// mod configs;
// mod database;
// mod network;
// mod server;

// #[tokio::main]
// pub async fn main() {
//     env_logger::init();
//     let server_config = match OmniPaxosKVConfig::new() {
//         Ok(parsed_config) => parsed_config,
//         Err(e) => panic!("{e}"),
//     };
//     let mut server = OmniPaxosServer::new(server_config).await;
//     server.run().await;
// }

mod test_server;
use test_server::TestServer;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub sender: String,
    pub content: String,
}

#[tokio::main]
async fn main() {
    let mut server = TestServer::new().await;
    server.run().await;
}


// # Create a pipe file
// mkfifo my_pipe

// # Run them in a loop:
// # Client reads from my_pipe and writes to stdout
// # Server reads from Client's stdout and writes to my_pipe
// cargo run --bin client < my_pipe | cargo run --bin server > my_pipe