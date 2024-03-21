use atomic_instant::AtomicInstant;
use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use static_init::dynamic;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::time::sleep;

const EMAIL_SUBJECT: &str = "Proxy client status";
const EMAIL_MAX_SUCCESS_TIMEOUT: u64 = 60;

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

    let last_success_time = Arc::new(AtomicInstant::now());

    send_email(
        &APP_CONFIG.email_subscriber,
        EMAIL_SUBJECT,
        "client bot is running",
    )
    .await;

    let last_success_time_1 = last_success_time.clone();
    let last_success_time_2 = last_success_time.clone();

    for i in 0..proxy_acc.len() {
        let (proxy_username, proxy_password) = proxy_acc[i].clone();
        let last_success_time = last_success_time_1.clone();
        tokio::spawn(async move {
            info!("spawned for {}", proxy_username);
            loop {
                make_request(
                    proxy_addr,
                    proxy_username,
                    proxy_password,
                    last_success_time.clone(),
                )
                .await;
                sleep(Duration::from_secs(1)).await;
            }
        });
    }

    // check for proxy failure, every 1 mins if it's keep failing sent email.
    tokio::spawn(async move {
        let last_success_time = last_success_time_2.clone();

        loop {
            if last_success_time.elapsed() > Duration::from_secs(EMAIL_MAX_SUCCESS_TIMEOUT) {
                send_email(
                    &APP_CONFIG.email_subscriber,
                    EMAIL_SUBJECT,
                    "proxy server seems down",
                )
                .await;
            }

            sleep(Duration::from_secs(60)).await;
        }
    });

    loop {}
}

// Function to make an HTTP request using the provided client
async fn make_request(
    proxy_address: &str,
    username: &str,
    password: &str,
    last_success_time: Arc<AtomicInstant>,
) {
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
                        if last_success_time.elapsed()
                            > Duration::from_secs(EMAIL_MAX_SUCCESS_TIMEOUT)
                        {
                            info!("recovered after failure");
                            send_email(
                                &APP_CONFIG.email_subscriber,
                                EMAIL_SUBJECT,
                                "recovered after failure",
                            )
                            .await;
                        }
                        last_success_time.set_now();
                    }
                    _ => {}
                },
                Err(e) => {
                    error!("error when making request err={}", e);
                }
            }
        }
        Err(e) => {
            error!("cannot create client: err={}", e);
        }
    }
}

async fn send_email(recipient: &str, subject: &str, body: &str) {
    let payload = json!({
        "from": {
            "email": APP_CONFIG.email_sender,
        },
        "to":[
            { "email": recipient }
        ],
        "subject": subject,
        "text": if body == "" { subject } else { body},
    });

    // Send the POST request
    let response = Client::new()
        .post("https://api.mailersend.com/v1/email")
        .header("Content-Type", "application/json")
        .header("X-Requested-With", "XMLHttpRequest")
        .header(
            "Authorization",
            format!("Bearer {}", APP_CONFIG.email_token),
        )
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Check if the request was successful
    if response.status().is_success() {
        info!("Email sent body={}", body);
    } else {
        error!("Failed to send email: {:?}", response.text().await.unwrap());
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
    pub email_token: String,
    pub email_sender: String,
    pub email_subscriber: String,
    pub email_sending_cooldown_secs: u64,

    pub proxy_addr: String,
    pub download_url: String,
    pub proxy_acc: Vec<String>,
}
