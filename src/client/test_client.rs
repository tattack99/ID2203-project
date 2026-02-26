use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use crate::Message;

pub struct TestClient;

impl TestClient {
    pub async fn new() -> Self {
        Self
    }

    pub async fn run(&mut self) {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin).lines();
        let mut stdout = io::stdout();

        // Example: Send an initial message
        let init = Message { sender: "client".into(), content: "Hello Server".into() };
        let _ = stdout.write_all(format!("{}\n", serde_json::to_string(&init).unwrap()).as_bytes()).await;
        let _ = stdout.flush().await;

        // Loop to process incoming server responses
        while let Ok(Some(line)) = reader.next_line().await {
            if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                // Log to stderr so we don't corrupt our own stdout JSON stream
                eprintln!("[Client] Received from {}: {}", msg.sender, msg.content);
            }
        }
    }
}
