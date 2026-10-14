use std::net::SocketAddr;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("required environment variable {0} is not set")]
    Missing(String),
    #[error("environment variable {key} has invalid value {value:?}")]
    Invalid { key: String, value: String },
}

/// Настройки, общие для всех процессов платформы.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommonConfig {
    pub listen_addr: SocketAddr,
    pub log_filter: String,
}

impl CommonConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let listen_raw = get("LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:8080".into());
        let listen_addr = listen_raw.parse().map_err(|_| ConfigError::Invalid {
            key: "LISTEN_ADDR".into(),
            value: listen_raw,
        })?;
        let log_filter = get("RUST_LOG").unwrap_or_else(|| "info".into());
        Ok(Self {
            listen_addr,
            log_filter,
        })
    }
}

/// Обязательная переменная окружения (строка подключения, секреты).
pub fn require(get: impl Fn(&str) -> Option<String>, key: &str) -> Result<String, ConfigError> {
    get(key)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ConfigError::Missing(key.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn defaults_when_env_is_empty() {
        let cfg = CommonConfig::from_lookup(env(&[])).unwrap();
        assert_eq!(cfg.listen_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(cfg.log_filter, "info");
    }

    #[test]
    fn reads_overrides() {
        let cfg = CommonConfig::from_lookup(env(&[
            ("LISTEN_ADDR", "127.0.0.1:9000"),
            ("RUST_LOG", "debug"),
        ]))
        .unwrap();
        assert_eq!(cfg.listen_addr, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(cfg.log_filter, "debug");
    }

    #[test]
    fn rejects_invalid_listen_addr() {
        let err = CommonConfig::from_lookup(env(&[("LISTEN_ADDR", "nope")])).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { ref key, .. } if key == "LISTEN_ADDR"));
    }

    #[test]
    fn require_fails_on_missing_or_empty() {
        assert_eq!(
            require(env(&[]), "DATABASE_URL").unwrap_err(),
            ConfigError::Missing("DATABASE_URL".into())
        );
        assert!(require(env(&[("DATABASE_URL", "")]), "DATABASE_URL").is_err());
        assert_eq!(
            require(env(&[("DATABASE_URL", "x")]), "DATABASE_URL").unwrap(),
            "x"
        );
    }
}
