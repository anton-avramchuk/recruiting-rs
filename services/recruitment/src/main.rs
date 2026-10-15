use std::{process::ExitCode, sync::Arc, time::Duration};

use platform_auth::{Audience, TokenVerifier};
use platform_core::{
    config::{CommonConfig, ConfigError, require},
    shutdown, telemetry,
};
use recruitment::{
    MIGRATOR,
    api::{self, AppState},
    application::HttpPersonFeed,
};

/// Как часто можно перечитывать ключ identity при неверной подписи.
const KEY_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

struct Config {
    common: CommonConfig,
    database_url: String,
    /// Владелец схемы для миграций; по умолчанию тот же URL.
    migration_database_url: String,
    identity_jwks_url: String,
    service_token: String,
    candidate_url: String,
}

fn load() -> Result<Config, ConfigError> {
    let get = |k: &str| std::env::var(k).ok();
    let database_url = require(get, "DATABASE_URL")?;
    Ok(Config {
        common: CommonConfig::from_lookup(get)?,
        migration_database_url: get("MIGRATION_DATABASE_URL")
            .unwrap_or_else(|| database_url.clone()),
        database_url,
        identity_jwks_url: require(get, "IDENTITY_JWKS_URL")?,
        service_token: require(get, "SERVICE_TOKEN")?,
        candidate_url: require(get, "CANDIDATE_URL")?,
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
            tracing::error!(error = %e, "recruitment failed");
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

    // ключ identity загружается лениво: сервис стартует и без identity (запросы получают 401)
    let verifier = Arc::new(TokenVerifier::remote(
        &cfg.identity_jwks_url,
        Audience::Tenant,
        KEY_REFRESH_INTERVAL,
    ));
    let feed = Arc::new(HttpPersonFeed::new(&cfg.candidate_url, &cfg.service_token));
    let app = api::router(AppState::new(pool, verifier, feed));

    let listener = tokio::net::TcpListener::bind(cfg.common.listen_addr).await?;
    tracing::info!(addr = %cfg.common.listen_addr, "recruitment listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await?;
    Ok(())
}
