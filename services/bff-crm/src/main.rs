use std::{process::ExitCode, sync::Arc};

use bff_core::RedisSessionStore;
use bff_crm::{CrmConfig, REDIS_PREFIX, build};
use platform_core::{
    config::{CommonConfig, ConfigError, require},
    shutdown, telemetry,
};

fn load() -> Result<(CommonConfig, CrmConfig, String), ConfigError> {
    let get = |k: &str| std::env::var(k).ok();
    let common = CommonConfig::from_lookup(get)?;
    let cfg = CrmConfig {
        identity_url: require(get, "IDENTITY_URL")?,
        cookie_secure: get("COOKIE_SECURE").is_none_or(|v| v != "false"),
        base_domain: get("BASE_DOMAIN").filter(|v| !v.is_empty()),
        session_ttl_secs: get("SESSION_TTL_HOURS")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(24 * 30)
            * 3600,
    };
    let redis_url = get("REDIS_URL").unwrap_or_else(|| "redis://127.0.0.1:6379".into());
    Ok((common, cfg, redis_url))
}

#[tokio::main]
async fn main() -> ExitCode {
    let (common, cfg, redis_url) = match load() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };
    telemetry::init(&common.log_filter);

    let store = match RedisSessionStore::connect(&redis_url, REDIS_PREFIX).await {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::error!(error = %e, "cannot connect to redis");
            return ExitCode::FAILURE;
        }
    };
    let app = build(&cfg, store);

    let listener = match tokio::net::TcpListener::bind(common.listen_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, addr = %common.listen_addr, "cannot bind");
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(addr = %common.listen_addr, "bff-crm listening");
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await
    {
        tracing::error!(error = %e, "server error");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
