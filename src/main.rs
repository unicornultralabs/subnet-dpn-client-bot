use atomic_instant::AtomicInstant;
use log::{error, info};
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use static_init::dynamic;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let init_last_telegram_message = LastTelegramMessage { 
        id: -1, 
        text: "".to_string()
    };
    let arc_last_telegram_message = Arc::new(Mutex::new(init_last_telegram_message));

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

    // client bot is running
    let telegram_running_msg_res = send_telegram(
        &APP_CONFIG.telegram_bot_id,
        &APP_CONFIG.telegram_group_id,
        &APP_CONFIG.telegram_group_thread_id,
        &APP_CONFIG.telegram_sender,
        &APP_CONFIG.telegram_receiver,
        "client bot is running",
    )
    .await;
    
    if let Ok(telegram_message_id) = telegram_running_msg_res {
        let mut guard_last_telegram_message = arc_last_telegram_message.lock().unwrap();

        if *guard_last_telegram_message.text.clone() != "client bot is running".to_string() {
            guard_last_telegram_message.text = "client bot is running".to_string();
        } else {
            delete_telegram_message(&APP_CONFIG.telegram_bot_id,
                &APP_CONFIG.telegram_group_id, guard_last_telegram_message.id).await;
        }
        guard_last_telegram_message.id = telegram_message_id;
        drop(guard_last_telegram_message);
    }
    

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
    let sig_term_future = sig_term();
    let task = tokio::spawn(async move {
        let last_success_time = last_success_time_2.clone();

        let last_telegram_message_clone = Arc::clone(&arc_last_telegram_message); 

        loop {
            let _last_telegram_message_clone = Arc::clone(&last_telegram_message_clone); 

            if last_success_time.elapsed() > Duration::from_secs(APP_CONFIG.max_success_timeout) {
                let telegram_seems_down_msg_res = send_telegram(
                    &APP_CONFIG.telegram_bot_id,
                    &APP_CONFIG.telegram_group_id,
                    &APP_CONFIG.telegram_group_thread_id,
                    &APP_CONFIG.telegram_sender,
                    &APP_CONFIG.telegram_receiver,
                    "proxy server seems down",
                )
                .await;

                if let Ok(telegram_message_id) = telegram_seems_down_msg_res {
                    let mut guard_last_telegram_message = _last_telegram_message_clone.lock().unwrap();
            
                    if *guard_last_telegram_message.text.clone() != "proxy server seems down".to_string() {
                        guard_last_telegram_message.text = "proxy server seems down".to_string();
                    } else {
                        tokio::spawn(async move {
                            _ = delete_telegram_message(&APP_CONFIG.telegram_bot_id.clone(),
                            &APP_CONFIG.telegram_group_id.clone(), 936).await;
                        });
                    }
                    guard_last_telegram_message.id = telegram_message_id;
                    drop(guard_last_telegram_message);
                }
            }

            sleep(Duration::from_secs(60)).await;
        }
    });
    
    tokio::select! {
        _ = sig_term_future => {
            info!("Received termination signal. Shutting down gracefully.");
        }
        _ = task => {},
    };
}


async fn sig_term(
) -> Result<(), Error> {
    let mut sigint = signal(SignalKind::interrupt()).unwrap();
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    tokio::select! {
        _ = sigint.recv() => {},
        _ = sigterm.recv() => {}
    };

    info!("SIGINT/SIGTERM received");

    _ = send_telegram(
        &APP_CONFIG.telegram_bot_id,
        &APP_CONFIG.telegram_group_id,
        &APP_CONFIG.telegram_group_thread_id,
        &APP_CONFIG.telegram_sender,
        &APP_CONFIG.telegram_receiver,
        "client bot shutdown",
    )
    .await;
    Ok(())
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
                            > Duration::from_secs(APP_CONFIG.max_success_timeout)
                        {
                            info!("recovered after failure");
                            send_telegram(
                                &APP_CONFIG.telegram_bot_id,
                                &APP_CONFIG.telegram_group_id,
                                &APP_CONFIG.telegram_group_thread_id,
                                &APP_CONFIG.telegram_sender,
                                &APP_CONFIG.telegram_receiver,
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

async fn delete_telegram_message (
    telegram_bot_id: &str,
    telegram_group_id: &str,
    message_id: i64
) {
    let payload = json!({
        "chat_id": telegram_group_id,
        "message_id": message_id,
    });
    
    let response = Client::new()
        .post(format!(
            "https://api.telegram.org/bot{}/deleteMessage",
            telegram_bot_id
        ))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .unwrap();

    if response.status().is_success() {
        info!("Delete telegram message success, sent payload={}", payload);
    } else {
        let error_message = response.text().await;
        error!("Failed to delete telegram message: {:?}", error_message);
    }
}

async fn send_telegram(
    telegram_bot_id: &str,
    telegram_group_id: &str,
    telegram_group_thread_id: &str,
    from: &str,
    recipient: &str,
    message: &str,
) -> Result<i64, Box<dyn std::error::Error>>  {
    let message_send = format!(
        r#"
    From: {}
To: {} 
Message: {}"#,
        from, recipient, message
    );

    let payload = json!({
        "chat_id": telegram_group_id,
        "message_thread_id": telegram_group_thread_id,
        "text": message_send
    });

    // Send the POST request
    let response: Response = Client::new()
        .post(format!(
            "https://api.telegram.org/bot{}/sendMessage",
            telegram_bot_id
        ))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .unwrap();

    // Check if the request was successful
    if response.status().is_success() {
        info!("Send message to telegram, sent payload={}", payload);
        let response_body = response.json::<HttpResponse>().await?;
        Ok(response_body.result.message_id)
    } else {
        let error_message = response.text().await?;
        error!("Failed to send message to telegram: {:?}", error_message);
        Err(error_message.into())
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
    pub email_subject: String,

    pub telegram_bot_id: String,
    pub telegram_group_id: String,
    pub telegram_group_thread_id: String,
    pub telegram_sender: String,
    pub telegram_receiver: String,

    pub max_success_timeout: u64,
    pub proxy_addr: String,
    pub download_url: String,
    pub proxy_acc: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResult {
    message_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    ok: bool,
    result: HttpResult
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastTelegramMessage {
    id: i64,
    text: String
}