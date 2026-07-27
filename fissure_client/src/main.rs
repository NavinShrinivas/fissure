mod bee_processor;
mod helper;
mod models;
mod orchestration;
mod protocols;
mod settingYaml;
mod managers;

//External crates :
use clap::Parser;
use env_logger::{Builder, Target};
use log::{info, LevelFilter};
use std::collections::HashMap;

use crate::managers::client_manager::ClientManager;


#[derive(Parser, Debug)]
#[command(name = "fissure")]
#[command(version = "1.0.0")]
#[command(about = "A CLI Torrent client", long_about = None)]
pub struct ClientEnv {
    #[arg(long, default_value = "settings.yaml")]
    settings_yaml: String,
}

#[tokio::main]
async fn main() {
    //console_subscriber::init();
    static LOCAL_CLIENT_ENV: once_cell::sync::Lazy<ClientEnv> =
        once_cell::sync::Lazy::new(|| ClientEnv::parse());

    static YAML_SETTINGS: once_cell::sync::Lazy<HashMap<std::string::String, serde_yaml::Value>> =
        once_cell::sync::Lazy::new(|| {
            settingYaml::settingYaml::load_yaml_from_file(LOCAL_CLIENT_ENV.settings_yaml.as_str())
        });

    let mut log_builder = Builder::from_default_env();

    //Setting up client logging  :
    log_builder.target(Target::Stdout);
    log_builder.filter_module("tower_http::trace::make_span", LevelFilter::Warn);
    log_builder.filter_module("tower_http::trace::on_response", LevelFilter::Warn);
    log_builder.filter_level(
        match settingYaml::settingYaml::get_inner_value(
            &YAML_SETTINGS,
            vec!["client".to_string(), "log_level".to_string()],
            "INFO".to_string(),
        )
        .as_str()
        {
            "OFF" => LevelFilter::Off,
            "TRACE" => LevelFilter::Trace,
            "INFO" => LevelFilter::Info,
            "DEBUG" => LevelFilter::Debug,
            "WARN" => LevelFilter::Warn,
            "ERROR" => LevelFilter::Error,
            _ => LevelFilter::Info,
        },
    );
    log_builder.try_init();


    info!("Hello, world. Starting fissure - A CLI torrent client!");

    let client = ClientManager::new(&YAML_SETTINGS);
    let download_path =  settingYaml::settingYaml::get_inner_value(&YAML_SETTINGS, 
        vec!["client".to_string(), "default_download_path".to_string()],
         "./".to_string());
    let _ = client
        .add_torrent("../test_torrent_files/happyness.torrent".to_string(), download_path)
        .await;

    info!("Torrent request queued. Waiting for shutdown signal...");
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for Ctrl+C");
    info!("Shutting down fissure client");
}
