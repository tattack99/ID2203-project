use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{done, Node, Result, Runtime};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub(crate) fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let runtime = Runtime::new();
    let handler = Arc::new(create_handler(runtime.clone()));
    runtime.with_handler(handler).run().await
}

#[derive(Clone)]
struct Handler {
    node_id: String
}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        let msg: Result<Request> = req.body.as_obj();
        match msg {
            Ok(Request::Read { key }) => {
                eprintln!("Node {} received READ for key {}", self.node_id, key);
                runtime.reply(req, Request::ReadOk { value: 0 }).await
            }
            Ok(Request::Write { key, value }) => {
                eprintln!("Node {} received WRITE: {}={}", self.node_id, key, value);
                runtime.reply(req, Request::WriteOk {}).await
            }
            Ok(Request::Cas { .. }) => {
                eprintln!("Node {} received CAS (not implemented)", self.node_id);
                runtime.reply(req, Request::CasOk {}).await
            }
            _ => { 
                // 1. Get the slice of all node IDs
                let all_nodes = runtime.nodes(); 
                
                // 2. Get the length of the slice (this will be 5)
                let node_count = all_nodes.len();
                
                // 3. Get this specific node's ID
                let current_id = runtime.node_id();

                eprintln!("Node {} initializing. Total nodes in cluster: {}. All nodes: {:?}", 
                    current_id, 
                    node_count, 
                    all_nodes
                );
                done(runtime, req)
            }
        }
    }
}

fn create_handler(runtime: Runtime) -> Handler {
    Handler {
        node_id: runtime.node_id().to_string(),
    }
}


#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum Request {
    Read { key: u64 },
    ReadOk { value: i64 },
    Write { key: u64, value: i64 },
    WriteOk {},
    Cas { 
        key: u64, 
        from: i64, 
        to: i64, 
        #[serde(default, rename = "create_if_not_exists")]
        put: bool 
    },
    CasOk {},
}
