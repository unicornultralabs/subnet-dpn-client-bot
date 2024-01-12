use log::{error, info};
use reqwest::Client;
use std::thread;
use std::time::Duration;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    // Set the HTTP proxy address and credentials
    let proxy_address = "54.169.160.241:9091";
    let proxy_acc = vec![
        ("clienttest1", "reXZyqedjlTC"),
        ("clienttest2", "AXUYWB5HivVR"),
        ("clienttest3", "LJDpOAkhY4WR"),
        ("clienttest4", "tZYUPtt5ogxu"),
        ("clienttest5", "xzDHGOMZQQTj"),
        ("clienttest6", "yT2S8n23kA9u"),
        ("clienttest7", "y7Jr8YjdqhTg"),
    ];

    // Spawn 10 threads
    for i in 0..proxy_acc.len() {
        let (proxy_username, proxy_password) = proxy_acc[i];
        tokio::spawn(async move {
            info!("spawned for {}", proxy_username);

            let client = reqwest::Client::builder()
                .proxy(
                    reqwest::Proxy::all(proxy_address)
                        .unwrap()
                        .basic_auth(proxy_username, proxy_password),
                )
                .build()
                .unwrap();

            loop {
                make_request(&client).await;

                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    loop {}
}

// Function to make an HTTP request using the provided client
async fn make_request(client: &Client) {
    let url = "https://github.com/unicornultrafoundation/u2u-genesis/raw/main/testnet.g";

    // Make a GET request
    match client.get(url).send().await {
        Ok(rsp) => {
            // let content = rsp.text().await;
            // info!("{}", content.unwrap_or("".to_string()));

            info!(
                "used {} bytes",
                rsp.bytes().await.map(|bz| bz.len()).unwrap_or_default()
            );
        }
        Err(e) => {
            error!("error when making request err=no peers online");
        }
    }
}
