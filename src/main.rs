#![forbid(unsafe_code)]

pub(crate) mod api;
pub(crate) mod db;
pub(crate) mod error;
pub(crate) mod utils;

use std::sync::Arc;

use axum::{extract::DefaultBodyLimit, http::Method};
use clap::Parser;

use error::LTZFError;
use lettre::{SmtpTransport, transport::smtp::authentication::Credentials};
use tokio::net::TcpListener;
use tower_governor::{governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor, *};
use tower_http::{cors, limit};

pub use api::{LTZFArc, LTZFServer};
pub use error::Result;
use utils::{init_tracing, shutdown_signal};

pub type DateTime = chrono::DateTime<chrono::Utc>;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Parser, Clone, Debug, Default)]
#[command(author, version, about)]
pub struct Configuration {
    #[arg(long, env = "MAIL_SERVER")]
    pub mail_server: Option<String>,
    #[arg(long, env = "MAIL_USER")]
    pub mail_user: Option<String>,
    #[arg(long, env = "MAIL_PASSWORD")]
    pub mail_password: Option<String>,
    #[arg(long, env = "MAIL_SENDER")]
    pub mail_sender: Option<String>,
    #[arg(long, env = "MAIL_RECIPIENT")]
    pub mail_recipient: Option<String>,
    #[arg(long, env = "LTZF_HOST", default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, env = "LTZF_PORT", default_value = "80")]
    pub port: u16,
    #[arg(long, short, env = "DATABASE_URL")]
    pub db_url: String,
    #[arg(long, short)]
    pub config: Option<String>,

    #[arg(
        long,
        env = "LTZF_KEYADDER_KEY",
        help = "The API Key that is used to add new Keys. This is saved in the database."
    )]
    pub keyadder_key: String,

    #[arg(long, env = "MERGE_TITLE_SIMILARITY", default_value = "0.8")]
    pub merge_title_similarity: f32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_COUNT",
        help = "global request count that is per interval",
        default_value = "4096"
    )]
    pub req_limit_count: u32,
    #[arg(
        long,
        env = "REQUEST_LIMIT_INTERVAL",
        help = "(whole) number of seconds",
        default_value = "2"
    )]
    pub req_limit_interval: u32,
    #[arg(
        long,
        env = "PER_OBJECT_SCRAPER_LOG_SIZE",
        help = "Size of the queue keeping track of which scraper touched an object",
        default_value = "5"
    )]
    pub per_object_scraper_log_size: u32,
}

impl Configuration {
    pub async fn build_mailer(&self) -> Result<SmtpTransport> {
        if self.mail_server.is_none()
            || self.mail_user.is_none()
            || self.mail_password.is_none()
            || self.mail_sender.is_none()
            || self.mail_recipient.is_none()
        {
            return Err(LTZFError::Infrastructure {
                source: Box::new(error::InfrastructureError::Configuration {
                    message: "Mail Configuration is incomplete".into(),
                    config: Box::new(self.clone()),
                }),
            });
        }
        let mailer = SmtpTransport::relay(self.mail_server.as_ref().unwrap().as_str())?
            .credentials(Credentials::new(
                self.mail_user.clone().unwrap(),
                self.mail_password.clone().unwrap(),
            ))
            .build();
        Ok(mailer)
    }
    pub fn init() -> Self {
        Configuration::parse()
    }
}
async fn init_db_conn(db_url: &str) -> Result<sqlx::PgPool> {
    let sqlx_db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await?;

    let mut available = false;
    for i in 0..14 {
        let r = sqlx_db.acquire().await;
        match r {
            Ok(_) => {
                available = true;
                break;
            }
            Err(sqlx::Error::PoolTimedOut) => {
                tracing::warn!("Connection to Database `{}` timed out", db_url);
            }
            _ => {
                let _ = r?;
            }
        }
        let milliseconds = 2i32.pow(i) as u64;
        tracing::info!("DB Unavailable, Retrying in {} ms...", milliseconds);
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
    if !available {
        return Err(LTZFError::Other {
            message: Box::new("Server Connection failed after 10 retries".into()),
        });
    }
    tracing::debug!("Started Database Pool");
    sqlx::migrate!().run(&sqlx_db).await?;
    tracing::debug!("Executed Migrations");
    Ok(sqlx_db)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    init_tracing();

    let config = Configuration::init();
    tracing::debug!("Configuration: {:?}", &config);

    tracing::info!("Starting the Initialisation process");
    let listener = TcpListener::bind(format!("{}:{}", config.host, config.port)).await?;

    tracing::debug!("Started Listener");
    let sqlx_db = init_db_conn(&config.db_url).await?;

    // Run Key Administrative Functions

    let mut tx = sqlx_db.begin().await?;
    let key = &config.keyadder_key;
    let tag = utils::auth::keytag_of(key);
    let salt = utils::auth::generate_salt();
    let hash = utils::auth::hash_full_key(&salt, key);
    tracing::info!("Master key of this session has keytag {}", tag);

    sqlx::query!(
        "INSERT INTO api_keys(key_hash, scope, created_by, salt, keytag)
            VALUES
            ($1, (SELECT id FROM api_scope WHERE value = 'keyadder' LIMIT 1), (SELECT last_value FROM api_keys_id_seq), $2, $3)
            ON CONFLICT DO NOTHING;", hash, salt, tag)
    .execute(&mut *tx).await?;

    tx.commit().await?;
    let mailbundle = crate::utils::notify::MailBundle::new(&config).await?;

    let state = Arc::new(LTZFServer::new(sqlx_db, config, mailbundle));
    tracing::debug!("Constructed Server State");

    // Init Axum router
    let (iv, cnt) = (
        state.config.req_limit_interval as u64,
        state.config.req_limit_count,
    );
    let rl_config = Arc::new(
        GovernorConfigBuilder::default()
            .const_per_second(iv)
            .const_burst_size(cnt)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .unwrap(),
    );
    let limiter = rl_config.limiter().clone();
    let interval = std::time::Duration::from_secs(60);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            tracing::info!("rate limiting storage size: {}", limiter.len());
            limiter.retain_recent();
        }
    });
    let rate_limiter = GovernorLayer::new(rl_config);
    let body_size_limit = 1024 * 1024 * 1024 * 16; // 16 GB
    let request_size_limit = limit::RequestBodyLimitLayer::new(body_size_limit);
    let cors_layer = cors::CorsLayer::new()
        .allow_methods(vec![Method::GET])
        .allow_origin(cors::AllowOrigin::any())
        .expose_headers(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = openapi::server::new(state.clone())
        .layer(DefaultBodyLimit::max(body_size_limit))
        .layer(request_size_limit)
        .layer(rate_limiter)
        .layer(cors_layer);

    tracing::debug!("Constructed Router");
    tracing::info!(
        "Starting Server on {}:{}",
        state.config.host,
        state.config.port
    );
    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
