use tracing::Level;
use tracing_subscriber::{filter, layer::SubscriberExt as _, util::SubscriberInitExt};

// Initialize tracing-subscriber and return OtelGuard for opentelemetry-related termination processing
pub(crate) fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::INFO,
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();
}
