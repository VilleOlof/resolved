#[cfg(feature = "tracing")]
pub fn init(id: u32) {
    use resolved_shared::instance_dir;
    use std::fs::OpenOptions;
    use tracing::subscriber::set_global_default;
    use tracing_subscriber::{Registry, fmt, layer::SubscriberExt};

    let log_path = instance_dir(id).join("module.log");

    let log_file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)
        .expect("Failed to create log file settings");

    let sub =
        Registry::default().with(fmt::layer().with_writer(log_file).with_ansi(false).pretty());

    set_global_default(sub).expect("Failed to set default tracing subscriber");
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::info!($($arg)*)
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::debug!($($arg)*)
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::error!($($arg)*)
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        #[cfg(feature = "tracing")]
        tracing::warn!($($arg)*)
    };
}
