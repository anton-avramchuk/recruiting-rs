use std::{process::ExitCode, sync::Arc, time::Duration};

use identity::{
    MIGRATOR,
    api::{self, AppState},
    infrastructure::directory::{CachedDirectory, HttpTenantDirectory},
};
use platform_auth::KeyPair;
use platform_core::{
    config::{CommonConfig, ConfigError, require},
    shutdown, telemetry,
};

struct Config {
    common: CommonConfig,
    database_url: String,
    /// Владелец схемы для миграций; по умолчанию тот же URL.
    migration_database_url: String,
    admin_url: String,
    service_token: String,
    jwt_private_key: Option<String>,
    tenant_cache_ttl: Duration,
}

fn load() -> Result<Config, ConfigError> {
    let get = |k: &str| std::env::var(k).ok();
    let database_url = require(get, "DATABASE_URL")?;
    Ok(Config {
        common: CommonConfig::from_lookup(get)?,
        migration_database_url: get("MIGRATION_DATABASE_URL")
            .unwrap_or_else(|| database_url.clone()),
        database_url,
        admin_url: require(get, "ADMIN_URL")?,
        service_token: require(get, "SERVICE_TOKEN")?,
        jwt_private_key: get("JWT_PRIVATE_KEY").filter(|v| !v.is_empty()),
        tenant_cache_ttl: Duration::from_secs(
            get("TENANT_CACHE_TTL_SECS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        ),
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = match load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };
    telemetry::init(&cfg.common.log_filter);
    match run(cfg).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "identity failed");
            ExitCode::FAILURE
        }
    }
}

async fn run(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    // миграции - от владельца схемы, обслуживание запросов - от роли без BYPASSRLS
    let owner = platform_db::connect(&cfg.migration_database_url, 2).await?;
    platform_db::migrate(&owner, &MIGRATOR).await?;
    owner.close().await;
    let pool = platform_db::connect(&cfg.database_url, 10).await?;

    let keys = match &cfg.jwt_private_key {
        Some(b64) => KeyPair::from_pkcs8_base64("identity-1", b64)?,
        None => {
            tracing::warn!(
                "JWT_PRIVATE_KEY is not set: using an ephemeral key, tokens die with the process"
            );
            KeyPair::generate("identity-ephemeral")
        }
    };
    let directory = CachedDirectory::new(
        HttpTenantDirectory::new(&cfg.admin_url, &cfg.service_token),
        cfg.tenant_cache_ttl,
    );
    let app = api::router(AppState::new(
        pool,
        Arc::new(directory),
        &keys,
        cfg.service_token,
    ));

    let listener = tokio::net::TcpListener::bind(cfg.common.listen_addr).await?;
    tracing::info!(addr = %cfg.common.listen_addr, "identity listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await?;
    Ok(())
}
