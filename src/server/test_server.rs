use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::Message;

pub struct TestServer;

impl TestServer {
    pub async fn new() -> Self {
        Self
    }

    pub async fn run(&mut self) {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        eprintln!("[Server] Started and waiting for messages...");

        // Process incoming requests from the client
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                eprintln!("[Server] Received: {}", msg.content);

                // Create a response
                let response = Message {
                    sender: "server".into(),
                    content: format!("ACK: {}", msg.content),
                };

                // Serialize and send
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = stdout.write_all(format!("{}\n", json).as_bytes()).await;
                    let _ = stdout.flush().await; // Push it through the pipe immediately
                }
            }
        }
    }
}
