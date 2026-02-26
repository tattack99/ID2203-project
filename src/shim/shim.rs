use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{done, Node, Result, Runtime};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Handler {
    pub node_id: String,
}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        let msg: Result<Request> = req.body.as_obj();
        match msg {
            Ok(Request::Init { .. }) => {
                let all_nodes = runtime.nodes();
                eprintln!(
                    "Node {} initializing. Total nodes: {}. All nodes: {:?}",
                    runtime.node_id(),
                    all_nodes.len(),
                    all_nodes
                );
                runtime.reply(req, Request::InitOk {}).await
            }
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
            _ => done(runtime, req),
        }
    }
}

pub fn create_handler(runtime: &Runtime) -> Handler {
    Handler {
        node_id: runtime.node_id().to_string(),
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Request {
    Init { node_id: String, node_ids: Vec<String> },
    InitOk {},
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