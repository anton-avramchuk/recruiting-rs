use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Инициализирует tracing. `filter` — директива в формате `RUST_LOG`.
pub fn init(filter: &str) {
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .try_init();
}
