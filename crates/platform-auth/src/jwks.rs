//! Публикация и чтение публичного ключа в формате JWKS (RFC 7517, OKP/Ed25519).
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

use crate::keys::{KeyError, PublicKey};

pub fn to_jwks(key: &PublicKey) -> Value {
    json!({ "keys": [{
        "kty": "OKP",
        "crv": "Ed25519",
        "alg": "EdDSA",
        "use": "sig",
        "kid": key.kid,
        "x": URL_SAFE_NO_PAD.encode(key.bytes),
    }]})
}

/// Берёт первый подходящий ключ документа.
pub fn from_jwks(doc: &Value) -> Result<PublicKey, KeyError> {
    let jwk = doc["keys"]
        .as_array()
        .and_then(|keys| {
            keys.iter()
                .find(|k| k["kty"] == "OKP" && k["crv"] == "Ed25519")
        })
        .ok_or_else(|| KeyError::Invalid("no Ed25519 key in JWKS".into()))?;
    let x = jwk["x"]
        .as_str()
        .ok_or_else(|| KeyError::Invalid("missing x".into()))?;
    let raw = URL_SAFE_NO_PAD
        .decode(x)
        .map_err(|e| KeyError::Invalid(e.to_string()))?;
    let bytes: [u8; 32] = raw
        .try_into()
        .map_err(|_| KeyError::Invalid("x must be 32 bytes".into()))?;
    Ok(PublicKey {
        kid: jwk["kid"].as_str().unwrap_or_default().into(),
        bytes,
    })
}

/// Маршрут `GET /.well-known/jwks.json` с публичным ключом сервиса.
pub fn router<S: Clone + Send + Sync + 'static>(key: &PublicKey) -> axum::Router<S> {
    let doc = to_jwks(key);
    axum::Router::new().route(
        "/.well-known/jwks.json",
        axum::routing::get(move || {
            let doc = doc.clone();
            async move { axum::Json(doc) }
        }),
    )
}
