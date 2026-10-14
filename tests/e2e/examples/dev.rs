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
