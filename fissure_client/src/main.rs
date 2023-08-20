mod bee_processor;
mod helper;
mod models;
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
    let mut client_state = models::client_meta::ClientState::new_client(client_env);
    client_state.add_torrent_using_file_path("../test_torrent_files/test.torrent".to_string());
    println!(
        "{:#?}",
        protocols::tracker::refresh_peer_list_from_tracker(
            &client_state.torrents.get(0).unwrap(),
            &client_state,
        )
        .await
        .unwrap()
    );
}
