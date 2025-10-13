use std::sync::LazyLock;

pub use crate::logger::{
    set_log_level, set_user_log_handler, LogCallback, LogLevel, LogOptions, Logger,
};

pub static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger::new("@firebase/app"));
