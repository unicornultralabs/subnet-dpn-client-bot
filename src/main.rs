use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use log::{error, info};
use mail_send::mail_builder::MessageBuilder;
use mail_send::SmtpClientBuilder;
use serde::{Deserialize, Serialize};
use static_init::dynamic;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

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

    let last_sent_at = Arc::new(RwLock::new(Instant::now()));
    info!("0");

    send_email(
        ("Phat Luu", "lnp279@gmail.com"),
        "Proxy client bot is running!",
        "",
    )
    .await;

    for i in 0..proxy_acc.len() {
        let (proxy_username, proxy_password) = proxy_acc[i].clone();
        let last_sent_at = last_sent_at.clone();
        tokio::spawn(async move {
            info!("spawned for {}", proxy_username);
            loop {
                make_request(
                    proxy_addr,
                    proxy_username,
                    proxy_password,
                    last_sent_at.clone(),
                )
                .await;
                thread::sleep(Duration::from_secs(1));
            }
        });
    }

    loop {}
}

// Function to make an HTTP request using the provided client
async fn make_request(
    proxy_address: &str,
    username: &str,
    password: &str,
    last_sent_at: Arc<RwLock<Instant>>,
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
                    }
                    _ => {}
                },
                Err(e) => {
                    error!("error when making request err={}", e);
                    if e.to_string().contains("unexpected eof while tunneling") {
                        error!("maybe proxy server is down :(");
                        let last_sent_at = { last_sent_at.read().await };
                        // every 5 mins
                        if last_sent_at.elapsed() > Duration::from_secs(5 * 60) {
                            send_email(
                                ("Phat Luu", "lnp279@gmail.com"),
                                "Proxy server seems down!",
                                "",
                            )
                            .await;
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("cannot create client: err={}", e);
        }
    }
}

async fn send_email(recipient: (&str, &str), subject: &str, body: &str) {
    let message = MessageBuilder::new()
        .from(("MS_DzG3KF", "MS_DzG3KF@trial-ynrw7gyq93j42k8e.mlsender.net"))
        .to(recipient)
        .subject(subject)
        .text_body(body);

    SmtpClientBuilder::new("smtp.mailersend.net", 587)
        .implicit_tls(false)
        .credentials((
            "MS_DzG3KF@trial-ynrw7gyq93j42k8e.mlsender.net",
            "QOsNI89pqoMYIT7q",
        ))
        .connect()
        .await
        .unwrap()
        .send(message)
        .await
        .unwrap();

    // info!("1");

    // let email = Message::builder()
    //     .from(
    //         "MS_DzG3KF <MS_DzG3KF@trial-ynrw7gyq93j42k8e.mlsender.net>"
    //             .parse()
    //             .unwrap(),
    //     )
    //     .to("Hei <lnp@gmail.com>".parse().unwrap())
    //     .subject("Happy new year")
    //     .header(ContentType::TEXT_PLAIN)
    //     .body(String::from("Be happy!"))
    //     .unwrap();

    // info!("2");

    // let creds = Credentials::new(
    //     "c7335a8abd8134".to_owned(),
    //     "2cbfacb65068bf".to_owned(),
    // );

    // // Open a remote connection to gmail
    // let mailer = SmtpTransport::starttls_relay("sandbox.smtp.mailtrap.io")
    //     .unwrap()
    //     .credentials(creds)
    //     .build();

    // info!("3");

    // // Send the email
    // match mailer.send(&email) {
    //     Ok(r) => {
    //         info!("{:#?}", r);
    //         info!("Email sent successfully!")
    //     }
    //     Err(e) => panic!("Could not send email: {e:?}"),
    // }

    // info!("4");
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
    pub email_server: String,
    pub email_port: u16,
    pub email_username: String,
    pub email_password: String,
    pub email_subscriber: String,

    pub proxy_addr: String,
    pub download_url: String,
    pub proxy_acc: Vec<String>,
}
