use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Timeout for how long an HTTP request waits for a response from the
/// OmniPaxos cluster before returning an error to the caller.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

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
    if tx.send(ApiCommand::Put(payload.key, payload.value, resp_tx)).await.is_err() {
        return "Error: client unavailable".to_string();
    }
    match tokio::time::timeout(REQUEST_TIMEOUT, resp_rx).await {
        Ok(Ok(val)) => val,
        Ok(Err(_)) => "Error: request dropped".to_string(),
        Err(_) => "Error: timeout".to_string(),
    }
}

pub async fn http_get(
    State(tx): State<mpsc::Sender<ApiCommand>>,
    Path(key): Path<String>,
) -> String {
    let (resp_tx, resp_rx) = oneshot::channel();
    if tx.send(ApiCommand::Get(key, resp_tx)).await.is_err() {
        return "Error: client unavailable".to_string();
    }
    match tokio::time::timeout(REQUEST_TIMEOUT, resp_rx).await {
        Ok(Ok(val)) => val,
        Ok(Err(_)) => "Error: request dropped".to_string(),
        Err(_) => "Error: timeout".to_string(),
    }
}

pub fn create_router(tx: mpsc::Sender<ApiCommand>) -> Router {
    Router::new()
        .route("/put", post(http_put))
        .route("/get/:key", get(http_get))
        .with_state(tx)
}