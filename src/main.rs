use headless_chrome::protocol::cdp::Target::CreateTarget;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use static_init::dynamic;
use std::ffi::OsStr;
use std::thread::sleep;
use std::time::Duration;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Set the HTTP proxy address and credentials
    let proxy_address = &APP_CONFIG.proxy_addr;
    let proxy_acc: Vec<(&str, &str)> = APP_CONFIG
        .proxy_acc
        .iter()
        .map(|pad| {
            let parses: Vec<&str> = pad.split(",").collect();
            let username = parses[0];
            let password = parses[1];
            (username, password)
        })
        .collect();

    let mut set = JoinSet::new();

    for i in 0..proxy_acc.len() {
        let (proxy_username, proxy_password) = proxy_acc[i].clone();
        set.spawn(async move {
            info!("proxy_address={}", proxy_address);
            info!("spawned for {}:{}", proxy_username, proxy_password);
            // let proxy_addr = format!("http://{}", proxy_address);
            // let proxy_addr = format!("http://{}:{}@{}", proxy_username, proxy_password, proxy_address);

            // Launch headless Chrome with proxy settings
            let browser = Browser::new(
                LaunchOptionsBuilder::default()
                    .headless(APP_CONFIG.headless)
                    .args(vec![OsStr::new(&format!(
                        "--proxy-server={}",
                        proxy_address
                    ))])
                    .build()
                    .expect("Failed to create browser"),
            )
            .expect("Failed to launch browser");

            let tab = browser
                .new_tab_with_options(CreateTarget {
                    url: APP_CONFIG.download_url.clone(),
                    width: Some(300),
                    height: Some(300),
                    browser_context_id: None,
                    enable_begin_frame_control: None,
                    new_window: None,
                    background: None,
                })
                .unwrap();
            sleep(Duration::from_secs(1));

            _ = tab.authenticate(
                Some(proxy_username.to_string()),
                Some(proxy_password.to_string()),
            );

            sleep(Duration::from_secs(2));

            loop {
                _ = tab.navigate_to(&APP_CONFIG.download_url);
                // _ = tab.navigate_to(&APP_CONFIG.download_url);
                sleep(Duration::from_secs(3));
            }
        });
    }

    while let Some(_) = set.join_next().await {}
}

#[dynamic]
pub static APP_CONFIG: AppConfig = {
    let mut file = std::fs::File::open("config_prod.yaml").unwrap();
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents).unwrap();
    serde_yaml::from_str(&contents).unwrap()
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AppConfig {
    pub headless: bool,
    pub proxy_addr: String,
    pub download_url: String,
    pub proxy_acc: Vec<String>,
}
