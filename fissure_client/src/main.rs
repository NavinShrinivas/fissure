mod bee_processor;
mod helper;
mod models;
mod orchestration;
mod protocols;

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
    let mut client = models::client_meta::Client::new_client(client_env);
    let name = client.add_torrent_using_file_path("../test_torrent_files/test.torrent".to_string());
    client
        .orchestrate_download(client.torrents.get(&name).unwrap().clone())
        .await;
}
