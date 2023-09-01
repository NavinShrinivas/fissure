mod bee_processor;
mod helper;
mod models;
mod protocols;
mod orchestration;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ClientEnv {
    pub version: String, //Has to be 4 char long 0001(0.0.1), 0010(0.1.0)...
    pub port: String,
}

#[tokio::main]
async fn main() {
    println!("Hello, world. Starting fissure client!");
    let client_env = ClientEnv {
        version: "0001".to_string(),
        port: "6001".to_string(),
    };
    let client_state = Arc::new(RwLock::new(models::client_meta::ClientState::new_client(client_env)));
    orchestration::basic_orechestration::start_dowload(client_state, "../test_torrent_files/test.torrent").await;
}
