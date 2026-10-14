//! Локальный запуск всей платформы одной командой: `cargo run -p e2e --example dev`.
//!
//! Поднимает identity, admin, оба BFF и оба веб-приложения на свободных портах с новыми
//! базами (данные одноразовые), создаёт демо-организацию и ждёт Ctrl+C.
use e2e::{ADMIN_EMAIL, ADMIN_PASSWORD, Stack};

#[tokio::main]
async fn main() {
    let stack = Stack::start_with_web().await;
    stack
        .seed_tenant("demo", "owner@demo.io", "demo-password-1")
        .await;
    let owner = stack
        .crm_client("demo", "owner@demo.io", "demo-password-1")
        .await;
    for person in [
        serde_json::json!({"full_name": "Анна Смирнова", "email": "anna@example.com",
            "phone": "+7 916 123-45-67", "skills": ["Rust", "SQL"], "source": "hh.ru",
            "expected_salary": 300000, "declared_experience_years": 6}),
        serde_json::json!({"full_name": "Борис Иванов", "phone": "+7 903 555-66-77",
            "skills": ["Go"], "source": "Реферал", "declared_experience_years": 2}),
        serde_json::json!({"full_name": "Виктор Петров", "skills": ["Rust", "Docker"],
            "declared_experience_years": 4}),
    ] {
        let res = owner.post("/api/persons", person).await;
        assert_eq!(res.status(), 201, "demo candidate");
    }

    println!();
    println!("Платформа запущена");
    println!("  Клиентское приложение : {}", stack.web_crm_url);
    println!("    организация demo, owner@demo.io / demo-password-1");
    println!("  Админка платформы     : {}", stack.web_admin_url);
    println!("    {ADMIN_EMAIL} / {ADMIN_PASSWORD}");
    println!(
        "  BFF crm / admin       : {} / {}",
        stack.bff_crm_url, stack.bff_admin_url
    );
    println!(
        "  identity / admin API  : {} / {}",
        stack.identity_url, stack.admin_url
    );
    println!();
    println!("Остановить: Ctrl+C");

    tokio::signal::ctrl_c().await.expect("ctrl-c handler");
    stack.stop().await;
}
