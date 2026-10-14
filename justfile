set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# Поднять PostgreSQL и Redis
up:
    docker compose up -d --wait

down:
    docker compose down

# Миграции применяются сервисами при старте; цель зарезервирована под ручной прогон
migrate:
    cargo run -p identity -- migrate
    cargo run -p admin -- migrate

check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
