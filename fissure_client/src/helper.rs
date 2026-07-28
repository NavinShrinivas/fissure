use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use crate::managers::client_manager::FissureClientOptions;

pub fn generate_peer_id(client_env: &FissureClientOptions) -> String {
    let id = "FS";
    let rand_string: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(14)
        .map(char::from)
        .collect();
    let id = format!("{}{}{}", id, client_env.version, rand_string);
    log::debug!("Our peer id : {}", id);
    return id.to_string();
}
