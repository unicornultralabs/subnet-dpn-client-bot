use headless_chrome::{Browser, LaunchOptionsBuilder};
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use static_init::dynamic;
use std::ffi::OsStr;
use std::thread;
use std::time::Duration;

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

    for i in 0..proxy_acc.len() {
        let (proxy_username, proxy_password) = proxy_acc[i].clone();
        tokio::spawn(async move {
            info!("spawned for {}", proxy_username);

            // Define proxy settings with basic authentication
            let proxy_server = format!("https://{}", proxy_address);
            info!("{}", proxy_server);

            // Launch headless Chrome with proxy settings
            let browser = Browser::new(
                LaunchOptionsBuilder::default()
                    .headless(false)
                    .args(vec![OsStr::new(&format!(
                        "--proxy-server={}",
                        proxy_server
                    ))])
                    .build()
                    .expect("Failed to create browser"),
            )
            .expect("Failed to launch browser");
            let tab = browser.new_tab().expect("new tab failed");
            tab.authenticate(
                Some(proxy_username.to_string()),
                Some(proxy_password.to_string()),
            )
            .expect("add auth");

            loop {
                // info!("proxy acc loaded vnexpress.net id={}", proxy_username);
                tab.navigate_to("https://vnexpress.net")
                    .expect("page load failed");
                thread::sleep(Duration::from_secs(2));
            }
        });
    }

    loop {}
}

// Function to make an HTTP request using the provided client
async fn make_request(client: &Client) {
    let url = &APP_CONFIG.download_url;

    // Make a GET request
    match client.get(url).send().await {
        Ok(rsp) => match rsp.text().await {
            Ok(content) => {
                // info!("{}", content);
                info!("used {} bytes", content.as_bytes().len());
            }
            Err(_) => {
                error!("invalid response format");
            }
        },
        Err(e) => {
            error!("error when making request err={}", e);
        }
    }
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
    pub proxy_addr: String,
    pub download_url: String,
    pub proxy_acc: Vec<String>,
}
