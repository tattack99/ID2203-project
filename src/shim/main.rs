use maelstrom::{Result, Runtime};
use std::sync::Arc;
use crate::shim::{create_handler};

mod shim;

fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let runtime = Runtime::new();
    let handler = Arc::new(create_handler(&runtime));
    runtime.with_handler(handler).run().await
}