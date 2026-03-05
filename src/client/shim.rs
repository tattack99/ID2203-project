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

pub enum ApiCommand {
    Put(String, String, oneshot::Sender<String>),
    Get(String, oneshot::Sender<String>),
}

pub async fn http_put(
    State(tx): State<mpsc::Sender<ApiCommand>>,
    Json(payload): Json<PutReq>,
) -> String {
    let (resp_tx, resp_rx) = oneshot::channel();
    let _ = tx.send(ApiCommand::Put(payload.key, payload.value, resp_tx)).await;
    resp_rx.await.unwrap_or_else(|_| "Error".to_string())
}

pub async fn http_get(
    State(tx): State<mpsc::Sender<ApiCommand>>,
    Path(key): Path<String>,
) -> String {
    let (resp_tx, resp_rx) = oneshot::channel();
    let _ = tx.send(ApiCommand::Get(key, resp_tx)).await;
    resp_rx.await.unwrap_or_else(|_| "0".to_string())
}

pub fn create_router(tx: mpsc::Sender<ApiCommand>) -> Router {
    Router::new()
        .route("/put", post(http_put))
        .route("/get/:key", get(http_get))
        .with_state(tx)
}
