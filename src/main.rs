use log::{error, info};
use serde::{Deserialize, Serialize};
use static_init::dynamic;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Set the HTTP proxy address and credentials
    let proxy_addr = &APP_CONFIG.proxy_addr;
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
            loop {
                make_request(proxy_addr, proxy_username, proxy_password).await;
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    loop {}
}

// Function to make an HTTP request using the provided client
async fn make_request(proxy_address: &str, username: &str, password: &str) {
    let url = &APP_CONFIG.download_url;

    match reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(proxy_address)
                .unwrap()
                .basic_auth(&username, &password),
        )
        .build()
    {
        Ok(client) => {
            // Make a GET request
            match client.get(url).send().await {
                Ok(rsp) => match rsp.text().await {
                    Ok(content) => {
                        // info!("{}", content);
                        info!("used {} bytes", content.as_bytes().len());
                    }
                    _ => {}
                },
                Err(e) => {
                    error!("error when making request err={}", e);
                    if e.to_string().contains("unexpected eof while tunneling") {
                        error!("maybe proxy server is down :(");
                    }
                }
            }
        }
        Err(e) => {
            error!("cannot create client: err={}", e);
        }
    }
}

#[dynamic]
pub static APP_CONFIG: AppConfig = {
    // let args: Vec<String> = env::args().collect();
    // if args.len() == 0 {
    //     panic!("no config filename provided");
    // }
    // let config_filename = &args[1];
    // println!("config filename: {}", config_filename);

    let mut file = std::fs::File::open("config.yaml").unwrap();
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
