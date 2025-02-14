use atomic_instant::AtomicInstant;
use log::{error, info};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use static_init::dynamic;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;

// Shared reqwest client for connection pooling
#[derive(Clone)]
struct ProxyClient {
    client: Client,
    username: String,
}

// State management struct
struct MonitorState {
    last_telegram_message: Arc<Mutex<LastTelegramMessage>>,
    last_success_time: Arc<AtomicInstant>,
    clients: HashMap<String, ProxyClient>,
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let state = setup_monitor_state().await;

    // Initialize monitoring tasks with controlled concurrency
    let monitor_handle = spawn_monitoring_tasks(state.clone()).await;
    let checker_handle = spawn_health_checker(state.clone()).await;

    // Wait for shutdown signal
    tokio::select! {
        _ = handle_shutdown_signals() => {
            info!("Received shutdown signal. Starting graceful shutdown...");
        }
        _ = monitor_handle => {
            error!("Monitor task unexpectedly terminated");
        }
        _ = checker_handle => {
            error!("Health checker unexpectedly terminated");
        }
    };

    send_shutdown_notification().await;
}

async fn setup_monitor_state() -> Arc<MonitorState> {
    let init_message = LastTelegramMessage {
        id: -1,
        text: String::new(),
    };

    // Initialize proxy clients with connection pooling
    let mut clients = HashMap::new();
    for (username, password) in get_proxy_credentials() {
        let client = Client::builder()
            .proxy(
                reqwest::Proxy::all(&APP_CONFIG.proxy_addr)
                    .unwrap()
                    .basic_auth(&username, &password),
            )
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build client");

        clients.insert(
            username.clone(),
            ProxyClient {
                client,
                username: username.clone(),
            },
        );
    }

    let state = MonitorState {
        last_telegram_message: Arc::new(Mutex::new(init_message)),
        last_success_time: Arc::new(AtomicInstant::now()),
        clients,
    };

    let state = Arc::new(state);

    // Send initial startup notification
    if APP_CONFIG.telegram_enable {
        let _ = notify_telegram("client bot is running", &state.last_telegram_message).await;
    }

    state
}

async fn spawn_monitoring_tasks(state: Arc<MonitorState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut handles = vec![];

        for (username, proxy_client) in &state.clients {
            let client = proxy_client.clone();
            let state = state.clone();
            let username = username.clone();

            let handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(
                    Duration::from_secs(APP_CONFIG.proxy_request_interval)
                );
                let mut consecutive_errors = 0;

                loop {
                    interval.tick().await;

                    match make_request(&client).await {
                        Ok(_) => {
                            consecutive_errors = 0;
                            state.last_success_time.set_now();

                            if APP_CONFIG.telegram_enable {
                                // Create a scope for message handling
                                let telegram_message = Arc::clone(&state.last_telegram_message);
                                if let Err(e) = notify_telegram(
                                    "recovered after failure",
                                    &telegram_message
                                ).await {
                                    error!("Failed to send telegram notification: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            error!("Request failed for {}: {}", username, e);

                            // Exponential backoff on errors
                            let backoff = Duration::from_secs(
                                std::cmp::min(60, 2_u64.pow(consecutive_errors))
                            );
                            tokio::time::sleep(backoff).await;
                        }
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all monitoring tasks
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Monitor task failed: {}", e);
            }
        }
    })
}

async fn spawn_health_checker(state: Arc<MonitorState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            if state.last_success_time.elapsed() > Duration::from_secs(APP_CONFIG.max_success_timeout) {
                if APP_CONFIG.telegram_enable {
                    let _ = notify_telegram(
                        "proxy server seems down",
                        &state.last_telegram_message
                    ).await;
                }
            }
        }
    })
}

// Make sure notify_telegram has Send + Sync bounds
async fn notify_telegram(
    message: &str,
    last_message: &Arc<Mutex<LastTelegramMessage>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let formatted_message = format!(
        r#"
From: {}
To: {} 
Message:({}) {}"#,
        APP_CONFIG.telegram_sender,
        APP_CONFIG.telegram_receiver,
        APP_CONFIG.telegram_env,
        message
    );

    let payload = json!({
        "chat_id": APP_CONFIG.telegram_group_id,
        "message_thread_id": APP_CONFIG.telegram_group_thread_id,
        "text": formatted_message
    });

    let response = Client::new()
        .post(format!(
            "https://api.telegram.org/bot{}/sendMessage",
            APP_CONFIG.telegram_bot_id
        ))
        .json(&payload)
        .send()
        .await?;

    if response.status().is_success() {
        let response_body: HttpResponse = response.json().await?;
        
        // Scope the mutex lock
        {
            let guard = last_message.lock().await;
            if guard.text != message {
                // Delete previous message if content changed
                if guard.id > 0 {
                    let msg_id = guard.id; // Store ID before dropping guard
                    drop(guard); // Release lock before await
                    delete_telegram_message(msg_id).await?;
                    let mut guard = last_message.lock().await;
                    guard.text = message.to_string();
                    guard.id = response_body.result.message_id;
                }
            }
        }
        
        Ok(())
    } else {
        let error_text = response.text().await?;
        Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            error_text
        )) as Box<dyn std::error::Error + Send + Sync>)
    }
}

async fn make_request(
    proxy_client: &ProxyClient,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let urls = &APP_CONFIG.download_urls;
    let url_index = rand::thread_rng().gen_range(0..urls.len());
    let url = &urls[url_index];  // Use reference to avoid moving

    let response = proxy_client.client
        .get(url)
        .send()
        .await?;

    let content = response.text().await?;
    
    info!(
        "{} -> {} used {} bytes",
        proxy_client.username,
        url,  // Now using the borrowed reference
        content.len()
    );

    Ok(())
}

async fn handle_shutdown_signals() -> Result<(), std::io::Error> {
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    tokio::select! {
        _ = sigint.recv() => info!("SIGINT received"),
        _ = sigterm.recv() => info!("SIGTERM received"),
    }

    Ok(())
}

async fn send_shutdown_notification() {
    if APP_CONFIG.telegram_enable {
        if let Err(e) = notify_telegram(
            "client bot shutdown",
            &Arc::new(Mutex::new(LastTelegramMessage {
                id: -1,
                text: String::new(),
            })),
        )
        .await
        {
            error!("Failed to send shutdown notification: {}", e);
        }
    }
}

async fn delete_telegram_message(message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let payload = json!({
        "chat_id": APP_CONFIG.telegram_group_id,
        "message_id": message_id,
    });

    let response = Client::new()
        .post(format!(
            "https://api.telegram.org/bot{}/deleteMessage",
            APP_CONFIG.telegram_bot_id
        ))
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            error_text
        )));
    }

    Ok(())
}

// Helper function to parse proxy credentials
fn get_proxy_credentials() -> Vec<(String, String)> {
    APP_CONFIG
        .proxy_acc
        .iter()
        .map(|acc| {
            let parts: Vec<&str> = acc.split(',').collect();
            (parts[0].to_string(), parts[1].to_string())
        })
        .collect()
}

#[dynamic]
pub static APP_CONFIG: AppConfig = {
    let mut file = std::fs::File::open("config.yaml").unwrap();
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut file, &mut contents).unwrap();
    serde_yaml::from_str(&contents).unwrap()
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AppConfig {
    pub telegram_bot_id: String,
    pub telegram_group_id: String,
    pub telegram_group_thread_id: String,
    pub telegram_sender: String,
    pub telegram_receiver: String,
    pub telegram_enable: bool,
    pub telegram_env: String,

    pub max_success_timeout: u64,
    pub proxy_request_interval: u64,
    pub proxy_addr: String,
    pub download_urls: Vec<String>,
    pub proxy_acc: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResult {
    message_id: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpResponse {
    ok: bool,
    result: HttpResult,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastTelegramMessage {
    id: i64,
    text: String,
}
#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tokio::fs;

    #[tokio::test]
    async fn gen_proxy_accs() {
        let user = "lewtran";
        let user_deposit_addr = "0x9d31d2c12dd7a2360a07f97f673189a4cd196316";
        let passwd = "Or7miIB36Xop";
        let acc_number_start = 300;
        let acc_amount = 700;

        let created_at = Utc::now().timestamp();
        let mut proxy_acc_creds_content: String = "".to_string();
        let mut proxy_acc_content: String = "".to_string();
        for i in acc_number_start..(acc_number_start + acc_amount) {
            let proxy_id = format!("0x{}_{}", user, i);
            proxy_acc_creds_content.push_str(&format!("  - {},{}\n", proxy_id, passwd));
            proxy_acc_content.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                proxy_id, passwd, 300, user_deposit_addr, 50, 0, 1562822, created_at,
            ))
        }

        let mut proxy_acc_db_content = "".to_string();
        proxy_acc_db_content.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            "id",
            "passwd",
            "ip_rotation_period",
            "user_addr",
            "rate_per_kb",
            "rate_per_second",
            "country_geoname_id",
            "created_at",
        ));
        proxy_acc_db_content.push_str(&proxy_acc_content);

        let filename_db = format!(
            "src/proxyacc_{}_{}_{}_db.csv",
            user, acc_number_start, acc_amount
        );
        let _ = fs::write(filename_db, proxy_acc_db_content).await;

        let filename_creds = format!(
            "src/proxyacc_{}_{}_{}_creds.csv",
            user, acc_number_start, acc_amount
        );
        let _ = fs::write(filename_creds, proxy_acc_creds_content).await;
    }
}
