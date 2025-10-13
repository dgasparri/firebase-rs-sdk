use std::sync::LazyLock;

use crate::logger::Logger;

pub static LOGGER: LazyLock<Logger> = LazyLock::new(|| Logger::new("@firebase/app-check"));
