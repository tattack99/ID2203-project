use env_logger;

#[tokio::main]
pub async fn main(){
    env_logger::init();
    print!("Hello from shim");
}