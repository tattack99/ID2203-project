use std::net::SocketAddr;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc::Sender, oneshot},
};

pub enum ApiCommand {
    Put {
        key: String,
        value: String,
        resp: oneshot::Sender<Result<(), String>>,
    },
    Get {
        key: String,
        resp: oneshot::Sender<Result<Option<String>, String>>,
    },
    Shutdown,
}

pub fn spawn_api(tx: Sender<ApiCommand>,addr: SocketAddr) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let listener = match TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind TCP listener: {e}");
                return;
            }
        };

        loop {
            match listener.accept().await {
                Ok((socket, _peer)) => {
                    let tx_clone = tx.clone();
                    tokio::spawn(async move {
                        handle_connection(socket, tx_clone).await;
                    });
                }
                Err(e) => {
                    eprintln!("Accept error: {e}");
                }
            }
        }
    })
}

async fn handle_connection(socket: TcpStream,tx: Sender<ApiCommand>) {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();

        match reader.read_line(&mut line).await {
            Ok(0) => break, // connection closed
            Ok(_) => {
                let response = process_command(line.trim(), &tx).await;

                if writer.write_all(response.as_bytes()).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

async fn process_command(input: &str,tx: &Sender<ApiCommand>) -> String {
    if input.is_empty() {
        return "ERROR empty command\n".into();
    }

    let mut parts = input.split_whitespace();
    let command = parts.next().unwrap();

    match command {
        "PUT" => {
            let key = match parts.next() {
                Some(k) => k.to_string(),
                None => return "ERROR missing key\n".into(),
            };

            let value_parts: Vec<&str> = parts.collect();
            if value_parts.is_empty() {
                return "ERROR missing value\n".into();
            }

            let value = value_parts.join(" ");

            let (resp_tx, resp_rx) = oneshot::channel();

            if tx
                .send(ApiCommand::Put {
                    key,
                    value,
                    resp: resp_tx,
                })
                .await
                .is_err()
            {
                return "ERROR internal\n".into();
            }

            match resp_rx.await {
                Ok(Ok(())) => "OK\n".into(),
                Ok(Err(e)) => format!("ERROR {e}\n"),
                Err(_) => "ERROR internal\n".into(),
            }
        }

        "GET" => {
            let key = match parts.next() {
                Some(k) => k.to_string(),
                None => return "ERROR missing key\n".into(),
            };

            let (resp_tx, resp_rx) = oneshot::channel();

            if tx
                .send(ApiCommand::Get {
                    key,
                    resp: resp_tx,
                })
                .await
                .is_err()
            {
                return "ERROR internal\n".into();
            }

            match resp_rx.await {
                Ok(Ok(Some(value))) => format!("VALUE {value}\n"),
                Ok(Ok(None)) => "NOT_FOUND\n".into(),
                Ok(Err(e)) => format!("ERROR {e}\n"),
                Err(_) => "ERROR internal\n".into(),
            }
        }

        "PING" => "OK\n".into(),

        "SHUTDOWN" => {
            let _ = tx.send(ApiCommand::Shutdown).await;
            "OK\n".into()
        }

        _ => "ERROR unknown command\n".into(),
    }
}