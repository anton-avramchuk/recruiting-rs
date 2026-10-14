#![allow(dead_code)]
use std::{sync::Arc, time::Duration};

use bff_core::{
    AllowRule, Bff, BffConfig, HttpUpstreamAuth, MemorySessionStore, SessionStore, router,
    testing::{Client, Upstream},
};

pub const COOKIE: &str = "crm_session";

pub struct Harness {
    pub upstream: Upstream,
    pub store: Arc<dyn SessionStore>,
    pub client: Client,
}

pub fn config(base: &str) -> BffConfig {
    BffConfig {
        cookie_name: COOKIE.into(),
        cookie_secure: true,
        session_ttl: Duration::from_secs(3600),
        upstream_base_url: base.into(),
        allow: vec![AllowRule::any("/users")],
        upstreams: Default::default(),
        login_transform: None,
    }
}

pub async fn harness() -> Harness {
    harness_with(|_| {}).await
}

pub async fn harness_with(tweak: impl FnOnce(&mut BffConfig)) -> Harness {
    let upstream = Upstream::new();
    let base = upstream.serve().await;
    let mut cfg = config(&base);
    tweak(&mut cfg);
    let cookie = cfg.cookie_name.clone();
    let store: Arc<dyn SessionStore> = Arc::new(MemorySessionStore::new());
    let auth = Arc::new(HttpUpstreamAuth::new(
        &base,
        "/auth/login",
        Some("/auth/refresh"),
        Some("/auth/logout"),
    ));
    let bff = Bff::new(cfg, store.clone(), auth);
    Harness {
        upstream,
        store,
        client: Client::new(router(bff), &cookie),
    }
}
