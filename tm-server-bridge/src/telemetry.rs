use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt};

// Initialize tracing-subscriber and return OtelGuard for opentelemetry-related termination processing
pub(crate) fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("DEBUG_LOG_LEVEL")
                .from_env_lossy(),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
