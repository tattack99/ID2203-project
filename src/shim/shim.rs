use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{done, Node, Result, Runtime};
use serde::{Deserialize, Serialize};

pub struct Handler {}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        let msg: Result<Request> = req.body.as_obj();
        match msg {
            Ok(Request::Init { node_ids, .. }) => {
                eprintln!("DEBUG: Entering Init handler for node {}", runtime.node_id());
                Ok(())
            }
            Ok(Request::Read { key }) => {
                runtime.reply(req, Request::ReadOk { value: 0 }).await
            }
            Ok(Request::Write { key, value }) => {
                runtime.reply(req, Request::WriteOk {}).await
            }
            Ok(Request::Cas { .. }) => {
                // 1. Intercept: Send custom message to n0
                runtime.send("n0", serde_json::json!({
                    "type": "cas_intercepted",
                    "intercepted_by": runtime.node_id()
                })).await?;

                // 2. Reply to the Client: Use .await (no '?') and then Ok(())
                runtime.reply(req, Request::CasOk {}).await; 
                Ok(())
            }
            Ok(Request::CasIntercepted { intercepted_by }) => {
                // Just log it. Do NOT reply here, or you'll create a loop.
                eprintln!("SUCCESS: Node {} received interception signal from {}", 
                    runtime.node_id(), 
                    intercepted_by
                );
                Ok(())
            }
            other => {
                done(runtime, req)
            }
        }
    }
}

pub fn create_handler(runtime: &Runtime) -> Handler {
    Handler { }
}

#[derive(Debug,Serialize, Deserialize)]
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
    CasIntercepted { intercepted_by: String },

}

