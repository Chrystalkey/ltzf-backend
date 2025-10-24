// TODO:
// 1. console logging according to RUST_LOG (already exists)
// 2. error logging for everything {warn, error}
// 3. a subscriber that catches the (for now) email warnings (`Actionable`)
// 4. a subscriber that logs object creations, deletions and merges

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Function to initialize tracing for logging
pub fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "RUST_LOG=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
