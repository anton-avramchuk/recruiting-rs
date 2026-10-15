//! Запуск всей платформы настоящими процессами для сквозных тестов.
//!
//! Требуется `docker compose up -d` (PostgreSQL и Redis). Для каждого прогона создаются
//! свежие базы; сервисы мигрируют их сами и работают от роли без BYPASSRLS.
use std::{
    net::TcpListener,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use platform_db::testing::EmptyDb;
use tokio::process::{Child, Command};

pub const ADMIN_EMAIL: &str = "root@platform.io";
pub const ADMIN_PASSWORD: &str = "root-password-1";
pub const SERVICE_TOKEN: &str = "e2e-service-token";

struct Service {
    child: Child,
}

pub struct Stack {
    pub identity_url: String,
    pub admin_url: String,
    pub candidate_url: String,
    pub recruitment_url: String,
    pub bff_crm_url: String,
    pub bff_admin_url: String,
    /// Заполнены только в `start_with_web`.
    pub web_crm_url: String,
    pub web_admin_url: String,
    services: Vec<Service>,
    dbs: Vec<EmptyDb>,
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port()
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("target"))
}

/// Собирает бинарники сервисов (один раз за прогон; повторная сборка почти мгновенна).
fn build_binaries(packages: &[&str]) {
    let mut cmd = std::process::Command::new(env!("CARGO"));
    cmd.arg("build").current_dir(workspace_root());
    for p in packages {
        cmd.args(["-p", p]);
    }
    let status = cmd.status().expect("run cargo build");
    assert!(status.success(), "cargo build of service binaries failed");
}

/// Собирает веб-приложения (SSR-бинарник и wasm-пакет) через cargo-leptos.
fn build_web(projects: &[&str]) {
    for project in projects {
        let status = std::process::Command::new(env!("CARGO"))
            .args(["leptos", "build", "--project", project])
            .current_dir(workspace_root())
            .status()
            .expect("run cargo leptos (cargo install cargo-leptos)");
        assert!(
            status.success(),
            "cargo leptos build --project {project} failed"
        );
    }
}

fn spawn(name: &'static str, envs: &[(&str, String)]) -> Service {
    let exe = target_dir()
        .join("debug")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    let child = Command::new(&exe)
        .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
        .env(
            "RUST_LOG",
            std::env::var("E2E_LOG").unwrap_or_else(|_| "warn".into()),
        )
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", exe.display()));
    Service { child }
}

async fn wait_healthy(url: &str, name: &str) {
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(res) = http.get(format!("{url}/health")).send().await
            && res.status().is_success()
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{name} did not become healthy at {url}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_ready(url: &str, name: &str) {
    let http = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(res) = http.get(url).send().await
            && res.status().is_success()
        {
            return;
        }
        assert!(Instant::now() < deadline, "{name} did not serve {url}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Клиент клиентского слоя (bff-crm): вошёл пользователь организации, cookie и CSRF хранятся.
pub struct CrmClient {
    http: reqwest::Client,
    base: String,
    csrf: String,
}

impl CrmClient {
    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .expect("crm get")
    }

    pub async fn post(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        self.http
            .post(format!("{}{path}", self.base))
            .header("x-csrf-token", &self.csrf)
            .json(&body)
            .send()
            .await
            .expect("crm post")
    }

    pub async fn put(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        self.http
            .put(format!("{}{path}", self.base))
            .header("x-csrf-token", &self.csrf)
            .json(&body)
            .send()
            .await
            .expect("crm put")
    }

    pub async fn patch(&self, path: &str, body: serde_json::Value) -> reqwest::Response {
        self.http
            .patch(format!("{}{path}", self.base))
            .header("x-csrf-token", &self.csrf)
            .json(&body)
            .send()
            .await
            .expect("crm patch")
    }

    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.http
            .delete(format!("{}{path}", self.base))
            .header("x-csrf-token", &self.csrf)
            .send()
            .await
            .expect("crm delete")
    }
}

impl Stack {
    /// Поднимает identity, admin, bff-crm и bff-admin.
    pub async fn start() -> Self {
        Self::start_inner(false).await
    }

    /// То же и веб-приложения web-crm и web-admin (нужен `cargo-leptos`).
    pub async fn start_with_web() -> Self {
        Self::start_inner(true).await
    }

    async fn start_inner(with_web: bool) -> Self {
        tokio::task::spawn_blocking(move || {
            // Сборка один раз на процесс: параллельные тесты не пересобирают бинарники,
            // которые уже запущены соседним тестом (на Windows их нельзя перезаписать).
            static BACKEND: std::sync::Once = std::sync::Once::new();
            static WEB: std::sync::Once = std::sync::Once::new();
            BACKEND.call_once(|| {
                build_binaries(&[
                    "identity",
                    "admin",
                    "candidate",
                    "recruitment",
                    "bff-crm",
                    "bff-admin",
                ]);
            });
            if with_web {
                WEB.call_once(|| build_web(&["web-crm", "web-admin"]));
            }
        })
        .await
        .expect("build task");

        let identity_db = EmptyDb::create("e2e_identity").await;
        let admin_db = EmptyDb::create("e2e_admin").await;
        let candidate_db = EmptyDb::create("e2e_candidate").await;
        let recruitment_db = EmptyDb::create("e2e_recruitment").await;
        let redis =
            std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());

        let ports = [
            free_port(),
            free_port(),
            free_port(),
            free_port(),
            free_port(),
            free_port(),
            free_port(),
            free_port(),
        ];
        let url = |p: u16| format!("http://127.0.0.1:{p}");
        let listen = |p: u16| ("LISTEN_ADDR", format!("127.0.0.1:{p}"));
        let (identity_url, admin_url) = (url(ports[0]), url(ports[1]));
        let (bff_crm_url, bff_admin_url) = (url(ports[2]), url(ports[3]));
        let candidate_url = url(ports[6]);
        let recruitment_url = url(ports[7]);

        let mut stack = Stack {
            identity_url: identity_url.clone(),
            admin_url: admin_url.clone(),
            candidate_url: candidate_url.clone(),
            recruitment_url: recruitment_url.clone(),
            bff_crm_url: bff_crm_url.clone(),
            bff_admin_url: bff_admin_url.clone(),
            web_crm_url: url(ports[4]),
            web_admin_url: url(ports[5]),
            services: Vec::new(),
            dbs: Vec::new(),
        };

        stack.services.push(spawn(
            "identity",
            &[
                listen(ports[0]),
                ("DATABASE_URL", identity_db.app_url.clone()),
                ("MIGRATION_DATABASE_URL", identity_db.owner_url.clone()),
                ("ADMIN_URL", admin_url.clone()),
                ("SERVICE_TOKEN", SERVICE_TOKEN.into()),
                ("TENANT_CACHE_TTL_SECS", "1".into()),
                ("ACCESS_TTL_SECS", "5".into()),
            ],
        ));
        stack.services.push(spawn(
            "admin",
            &[
                listen(ports[1]),
                ("DATABASE_URL", admin_db.app_url.clone()),
                ("MIGRATION_DATABASE_URL", admin_db.owner_url.clone()),
                ("IDENTITY_URL", identity_url.clone()),
                ("SERVICE_TOKEN", SERVICE_TOKEN.into()),
                ("SEED_ADMIN_EMAIL", ADMIN_EMAIL.into()),
                ("SEED_ADMIN_PASSWORD", ADMIN_PASSWORD.into()),
            ],
        ));
        stack.services.push(spawn(
            "candidate",
            &[
                listen(ports[6]),
                ("DATABASE_URL", candidate_db.app_url.clone()),
                ("MIGRATION_DATABASE_URL", candidate_db.owner_url.clone()),
                (
                    "IDENTITY_JWKS_URL",
                    format!("{identity_url}/.well-known/jwks.json"),
                ),
                ("SERVICE_TOKEN", SERVICE_TOKEN.into()),
            ],
        ));
        stack.services.push(spawn(
            "recruitment",
            &[
                listen(ports[7]),
                ("DATABASE_URL", recruitment_db.app_url.clone()),
                ("MIGRATION_DATABASE_URL", recruitment_db.owner_url.clone()),
                (
                    "IDENTITY_JWKS_URL",
                    format!("{identity_url}/.well-known/jwks.json"),
                ),
                ("SERVICE_TOKEN", SERVICE_TOKEN.into()),
                ("CANDIDATE_URL", candidate_url.clone()),
            ],
        ));
        stack.services.push(spawn(
            "bff-crm",
            &[
                listen(ports[2]),
                ("IDENTITY_URL", identity_url),
                ("CANDIDATE_URL", candidate_url.clone()),
                ("RECRUITMENT_URL", recruitment_url.clone()),
                ("REDIS_URL", redis.clone()),
                ("COOKIE_SECURE", "false".into()),
                // браузер ходит в BFF напрямую, источник — адрес веб-приложения
                ("CORS_ORIGINS", stack.web_crm_url.clone()),
            ],
        ));
        stack.services.push(spawn(
            "bff-admin",
            &[
                listen(ports[3]),
                ("ADMIN_URL", admin_url),
                ("REDIS_URL", redis),
                ("COOKIE_SECURE", "false".into()),
                ("CORS_ORIGINS", stack.web_admin_url.clone()),
            ],
        ));
        if with_web {
            for (name, port, bff) in [
                ("web-crm", ports[4], bff_crm_url.clone()),
                ("web-admin", ports[5], bff_admin_url.clone()),
            ] {
                stack.services.push(spawn(
                    name,
                    &[
                        ("BFF_URL", bff.clone()),
                        ("BFF_PUBLIC_URL", bff),
                        ("LEPTOS_OUTPUT_NAME", name.into()),
                        (
                            "LEPTOS_SITE_ROOT",
                            target_dir().join("site").join(name).display().to_string(),
                        ),
                        ("LEPTOS_SITE_PKG_DIR", "pkg".into()),
                        ("LEPTOS_SITE_ADDR", format!("127.0.0.1:{port}")),
                        ("LEPTOS_ENV", "PROD".into()),
                    ],
                ));
            }
        }
        stack
            .dbs
            .extend([identity_db, admin_db, candidate_db, recruitment_db]);

        for (url, name) in [
            (&stack.identity_url, "identity"),
            (&stack.admin_url, "admin"),
            (&stack.candidate_url, "candidate"),
            (&stack.recruitment_url, "recruitment"),
            (&stack.bff_crm_url, "bff-crm"),
            (&stack.bff_admin_url, "bff-admin"),
        ] {
            wait_healthy(url, name).await;
        }
        if with_web {
            wait_ready(&format!("{}/login", stack.web_crm_url), "web-crm").await;
            wait_ready(&format!("{}/login", stack.web_admin_url), "web-admin").await;
        }
        stack
    }

    /// Только собирает веб-приложения (без запуска процессов).
    pub fn build_web_only() {
        build_web(&["web-crm", "web-admin"]);
    }

    /// Создаёт организацию с владельцем через админский BFF (как это делает администратор платформы).
    pub async fn seed_tenant(&self, slug: &str, owner_email: &str, owner_password: &str) {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("http client");
        let login: serde_json::Value = http
            .post(format!("{}/bff/login", self.bff_admin_url))
            .header("x-csrf-token", "login")
            .json(&serde_json::json!({"email": ADMIN_EMAIL, "password": ADMIN_PASSWORD}))
            .send()
            .await
            .expect("admin login")
            .json()
            .await
            .expect("admin login body");
        let csrf = login["csrf_token"]
            .as_str()
            .expect("csrf token")
            .to_string();
        let res = http
            .post(format!("{}/api/tenants", self.bff_admin_url))
            .header("x-csrf-token", csrf)
            .json(&serde_json::json!({
                "slug": slug,
                "name": slug,
                "owner": {"email": owner_email, "password": owner_password}
            }))
            .send()
            .await
            .expect("create tenant");
        assert_eq!(res.status(), 201, "seed tenant {slug}");
    }

    /// Входит в клиентский слой как пользователь организации.
    pub async fn crm_client(&self, tenant: &str, email: &str, password: &str) -> CrmClient {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("http client");
        let res = http
            .post(format!("{}/bff/login", self.bff_crm_url))
            .header("x-csrf-token", "login")
            .json(&serde_json::json!({"tenant": tenant, "email": email, "password": password}))
            .send()
            .await
            .expect("crm login");
        assert_eq!(res.status(), 200, "crm login of {email}");
        let body: serde_json::Value = res.json().await.expect("login body");
        CrmClient {
            http,
            base: self.bff_crm_url.clone(),
            csrf: body["csrf_token"].as_str().expect("csrf").to_string(),
        }
    }

    /// Создаёт пользователя организации с ролью (от имени владельца).
    pub async fn add_user(&self, owner: &CrmClient, email: &str, password: &str, role: &str) {
        let res = owner
            .post(
                "/api/users",
                serde_json::json!({"email": email, "password": password, "roles": [role]}),
            )
            .await;
        assert_eq!(res.status(), 201, "create user {email}");
    }

    /// Приостанавливает (`suspend`) или возобновляет (`activate`) организацию от имени администратора платформы.
    pub async fn set_tenant_status(&self, slug: &str, action: &str) {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .build()
            .expect("http client");
        let login: serde_json::Value = http
            .post(format!("{}/bff/login", self.bff_admin_url))
            .header("x-csrf-token", "login")
            .json(&serde_json::json!({"email": ADMIN_EMAIL, "password": ADMIN_PASSWORD}))
            .send()
            .await
            .expect("admin login")
            .json()
            .await
            .expect("admin login body");
        let csrf = login["csrf_token"].as_str().expect("csrf").to_string();
        let page: serde_json::Value = http
            .get(format!("{}/api/tenants?q={slug}", self.bff_admin_url))
            .send()
            .await
            .expect("list tenants")
            .json()
            .await
            .expect("tenants body");
        let id = page["items"]
            .as_array()
            .and_then(|items| items.iter().find(|t| t["slug"] == slug))
            .and_then(|t| t["id"].as_str())
            .unwrap_or_else(|| panic!("tenant {slug} not found"))
            .to_string();
        let res = http
            .post(format!("{}/api/tenants/{id}/{action}", self.bff_admin_url))
            .header("x-csrf-token", csrf)
            .send()
            .await
            .expect("tenant status");
        assert_eq!(res.status(), 200, "{action} {slug}");
    }

    /// Останавливает процессы и удаляет базы.
    pub async fn stop(mut self) {
        for svc in &mut self.services {
            let _ = svc.child.kill().await;
        }
        for db in self.dbs.drain(..) {
            db.close().await;
        }
    }
}
