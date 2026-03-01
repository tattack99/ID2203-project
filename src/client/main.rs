use crate::client::{Client, ManualCommand}; 
use crate::configs::ClientConfig;
use axum::{
    routing::{get, post}, // Add 'get' here
    Router, 
    Json, 
    extract::{State, Path} // Add 'Path' for the /get/:key route
};use std::net::SocketAddr;

mod client;
mod configs;
mod data_collection;
mod network;

#[derive(serde::Deserialize)]
struct PutPayload {
    key: String,
    value: String,
}

async fn http_put(
    State(tx): State<tokio::sync::mpsc::Sender<ManualCommand>>, 
    Json(payload): Json<PutPayload>
) -> &'static str {
    let _ = tx.send(ManualCommand::Put(payload.key, payload.value)).await;
    "Command queued"
}

async fn http_get(
    State(tx): State<tokio::sync::mpsc::Sender<ManualCommand>>,
    Path(key): Path<String>, // This extracts the :key from the URL
) -> String {
    // For now, we'll just queue it and return a string
    // until we link the oneshot channel back.
    let (resp_tx, _resp_rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(ManualCommand::Get(key, resp_tx)).await;
    "Get command queued".to_string()
}

#[tokio::main]
pub async fn main() {
    env_logger::init();
    
    let client_config = match ClientConfig::new() {
        Ok(parsed_config) => parsed_config,
        Err(e) => panic!("{e}"),
    };

    println!("!!! SHIM VERSION 2.0 BOOTING !!!");


    // Create channel
    let (tx, rx) = tokio::sync::mpsc::channel::<ManualCommand>(100);
    
    // Clone config for the background task
    let mut client = Client::new(client_config).await;

    // 1. Spawn Client in background
    tokio::spawn(async move {
        client.run(rx).await; 
    });

    // 2. Build API
    let app = Router::new()
    .route("/put", post(http_put))
    .route("/get/:key", get(http_get)) // New GET route
    .with_state(tx);

    // 3. Bind to 0.0.0.0 for Docker
    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
