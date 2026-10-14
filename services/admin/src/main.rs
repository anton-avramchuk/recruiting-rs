use std::{process::ExitCode, sync::Arc};

use admin::{
    MIGRATOR,
    api::{self, AppState},
    infrastructure::{admins, identity_client::HttpProvisioner},
};
use platform_auth::KeyPair;
use platform_core::{
    config::{CommonConfig, ConfigError, require},
    shutdown, telemetry,
};

struct Config {
    common: CommonConfig,
    database_url: String,
    migration_database_url: String,
    identity_url: String,
    service_token: String,
    jwt_private_key: Option<String>,
    seed_admin: Option<(String, String)>,
}

fn load() -> Result<Config, ConfigError> {
    let get = |k: &str| std::env::var(k).ok();
    let database_url = require(get, "DATABASE_URL")?;
    let seed_admin = match (get("SEED_ADMIN_EMAIL"), get("SEED_ADMIN_PASSWORD")) {
        (Some(e), Some(p)) if !e.is_empty() && !p.is_empty() => Some((e, p)),
        _ => None,
    };
    Ok(Config {
        common: CommonConfig::from_lookup(get)?,
        migration_database_url: get("MIGRATION_DATABASE_URL")
            .unwrap_or_else(|| database_url.clone()),
        database_url,
        identity_url: require(get, "IDENTITY_URL")?,
        service_token: require(get, "SERVICE_TOKEN")?,
        jwt_private_key: get("JWT_PRIVATE_KEY").filter(|v| !v.is_empty()),
        seed_admin,
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
            tracing::error!(error = %e, "admin failed");
            ExitCode::FAILURE
        }
    }
}

async fn run(cfg: Config) -> Result<(), Box<dyn std::error::Error>> {
    let owner = platform_db::connect(&cfg.migration_database_url, 2).await?;
    platform_db::migrate(&owner, &MIGRATOR).await?;
    owner.close().await;
    let pool = platform_db::connect(&cfg.database_url, 10).await?;

    if let Some((email, password)) = &cfg.seed_admin {
        let created = admins::seed(&pool, email, password).await?;
        tracing::info!(created, "platform admin seed checked");
    }

    let keys = match &cfg.jwt_private_key {
        Some(b64) => KeyPair::from_pkcs8_base64("admin-1", b64)?,
        None => {
            tracing::warn!(
                "JWT_PRIVATE_KEY is not set: using an ephemeral key, tokens die with the process"
            );
            KeyPair::generate("admin-ephemeral")
        }
    };
    let provisioner = HttpProvisioner::new(&cfg.identity_url, &cfg.service_token);
    let app = api::router(AppState::new(
        pool,
        &keys,
        cfg.service_token,
        Arc::new(provisioner),
    ));

    let listener = tokio::net::TcpListener::bind(cfg.common.listen_addr).await?;
    tracing::info!(addr = %cfg.common.listen_addr, "admin listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await?;
    Ok(())
}
