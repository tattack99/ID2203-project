use shim::Shim;
use configs::ShimConfig;
use core::panic;
use env_logger;
// use std::{thread, time::Duration};
mod configs;
mod shim;
mod network;

#[tokio::main]
pub async fn main(){
    env_logger::init();
    println!("Hello from shim");
    let shim_config = match ShimConfig::new() {
        Ok(parsed_config) => parsed_config,
        Err(e) => panic!("{e}"),
    };
    // let t = true;
    // while t{
    //     Duration::from_secs(2);
    // }
    let mut shim = Shim::new(shim_config).await;
    shim.run().await;
}