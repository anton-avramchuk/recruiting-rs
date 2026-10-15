//! Веб-приложения: серверный рендеринг, защита маршрутов на сервере, изоляция бандлов.
use std::path::PathBuf;

use e2e::Stack;
use reqwest::{Client, StatusCode, redirect::Policy};

fn site_file(app: &str, file: &str) -> Vec<u8> {
    let root = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::read(root.join("site").join(app).join("pkg").join(file))
        .unwrap_or_else(|e| panic!("{app}/{file}: {e}"))
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    haystack.windows(n.len()).any(|w| w == n)
}

fn no_redirects() -> Client {
    Client::builder().redirect(Policy::none()).build().unwrap()
}

#[tokio::test]
async fn login_pages_are_server_rendered_html() {
    let stack = Stack::start_with_web().await;
    let http = no_redirects();

    let crm = http
        .get(format!("{}/login", stack.web_crm_url))
        .send()
        .await
        .unwrap();
    assert_eq!(crm.status(), StatusCode::OK);
    let html = crm.text().await.unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<form"), "form is rendered on the server");
    assert!(
        html.contains("name=\"tenant\""),
        "crm asks for the organization"
    );
    assert!(html.contains("Ритм"));
    assert!(html.contains("--accent"), "styles are inlined");
    assert!(
        html.contains("/pkg/web-crm.js"),
        "hydration bundle is referenced"
    );

    let admin = http
        .get(format!("{}/login", stack.web_admin_url))
        .send()
        .await
        .unwrap();
    assert_eq!(admin.status(), StatusCode::OK);
    let html = admin.text().await.unwrap();
    assert!(html.contains("<form"));
    assert!(
        !html.contains("Организация"),
        "admin login has no organization field"
    );
    assert!(html.contains("Администрирование платформы"));

    // пакеты гидратации отдаются
    for (base, name) in [
        (&stack.web_crm_url, "web-crm"),
        (&stack.web_admin_url, "web-admin"),
    ] {
        for ext in ["js", "wasm"] {
            let res = http
                .get(format!("{base}/pkg/{name}.{ext}"))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{name}.{ext}");
            // файлы без хеша в имени: браузер должен сверять версию после обновления
            assert_eq!(res.headers()["cache-control"], "no-cache", "{name}.{ext}");
        }
    }
    stack.stop().await;
}

#[tokio::test]
async fn protected_routes_redirect_unauthenticated_browsers_to_login() {
    let stack = Stack::start_with_web().await;
    let http = no_redirects();

    for (base, path) in [
        (&stack.web_crm_url, "/candidates"),
        (&stack.web_crm_url, "/"),
        (&stack.web_admin_url, "/"),
    ] {
        let res = http
            .get(format!("{base}{path}"))
            .header("accept", "text/html")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FOUND, "{base}{path}");
        let location = res.headers()["location"].to_str().unwrap().to_string();
        assert!(location.starts_with("/login"), "{location}");
        if path == "/candidates" {
            assert_eq!(
                location, "/login?next=/candidates",
                "return path is preserved"
            );
        }
    }
    // сама страница входа не редиректит
    let login = http
        .get(format!("{}/login", stack.web_crm_url))
        .header("accept", "text/html")
        .send()
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
    stack.stop().await;
}

#[tokio::test]
async fn bundles_do_not_leak_across_applications() {
    // бандлы уже собраны cargo-leptos; поднимать процессы не нужно
    Stack::build_web_only();
    let crm_wasm = site_file("web-crm", "web-crm.wasm");
    let crm_js = site_file("web-crm", "web-crm.js");
    let admin_wasm = site_file("web-admin", "web-admin.wasm");

    // в клиентском приложении нет кода управления организациями платформы
    for needle in [
        "list_tenants",
        "create_tenant",
        "set_tenant_status",
        "/api/tenants",
    ] {
        assert!(!contains(&crm_wasm, needle), "web-crm.wasm leaks {needle}");
        assert!(!contains(&crm_js, needle), "web-crm.js leaks {needle}");
    }
    // а в админском оно есть (проверка, что поиск вообще что-то находит)
    assert!(contains(&admin_wasm, "list_tenants"));
}
