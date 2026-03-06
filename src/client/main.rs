mod configs;
mod data_collection;
mod network;
mod shim;
mod test_client;

use crate::configs::ClientConfig;
use crate::shim::ApiCommand;
use crate::test_client::Client;
use std::net::SocketAddr;
use tokio::sync::mpsc;

#[tokio::main]
pub async fn main() {
    env_logger::init();
    let config = ClientConfig::new().expect("Failed to load config");

    let (tx, rx) = mpsc::channel(100);
    let mut client = Client::new(config).await;

    tokio::spawn(async move {
        client.run(rx).await;
    });

    let app = shim::create_router(tx);
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    
    axum::serve(listener, app).await.unwrap();
}
