mod bee_processor;
mod helper;
mod models;
mod orchestration;
mod protocols;
mod settingYaml;

//External crates : 
use clap::Parser;
use env_logger::{Builder, Target};
use log::{error, info, LevelFilter};
use std::collections::HashMap;

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
            vec!["logging".to_string(), "level".to_string()],
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
    log_builder.init();

    //=======startup sequence=========

    info!("Hello, world. Starting fissure - A CLI torrent client!");

    let mut client = models::client_meta::Client::new_client(&YAML_SETTINGS);
    let name = client.add_torrent_using_file_path("../test_torrent_files/test.torrent".to_string());
    client
        .orchestrate_download(client.torrents.get(&name).unwrap().clone())
        .await;
}
