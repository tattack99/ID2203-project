use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::{mpsc, oneshot};

#[derive(Deserialize)]
pub struct PutReq {
    pub key: String,
    pub value: String,
}

pub enum ManualCommand {
    Put(String, String),
    Get(String, oneshot::Sender<String>),
}

pub async fn http_put(
    State(tx): State<mpsc::Sender<ManualCommand>>,
    Json(payload): Json<PutReq>,
) -> &'static str {
    let _ = tx.send(ManualCommand::Put(payload.key, payload.value)).await;
    "Put Accepted"
}

pub async fn http_get(
    State(tx): State<mpsc::Sender<ManualCommand>>,
    Path(key): Path<String>,
) -> String {
    let (resp_tx, resp_rx) = oneshot::channel();
    let _ = tx.send(ManualCommand::Get(key, resp_tx)).await;
    resp_rx.await.unwrap_or_else(|_| "0".to_string())
}

pub fn create_router(tx: mpsc::Sender<ManualCommand>) -> Router {
    Router::new()
        .route("/put", post(http_put))
        .route("/get/:key", get(http_get))
        .with_state(tx)
}
