use chrono::{SecondsFormat, Utc};
use serde_json::Value;
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, RwLock, Weak};

static GLOBAL_LOG_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
static INSTANCES: LazyLock<Mutex<Vec<Weak<LoggerInner>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

type SharedLogHandler = Arc<dyn Fn(&Logger, LogLevel, &[LogArgument]) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct Logger {
    inner: Arc<LoggerInner>,
}

impl Logger {
    pub fn new(name: impl Into<String>) -> Self {
        let inner = Arc::new(LoggerInner::new(name.into()));
        track_instance(&inner);
        Self { inner }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    pub fn log_level(&self) -> LogLevel {
        LogLevel::from_u8(self.inner.log_level.load(Ordering::SeqCst))
    }

    pub fn set_log_level<L>(&self, level: L) -> Result<(), LogError>
    where
        L: IntoLogLevel,
    {
        let level = level.into_log_level()?;
        self.inner.log_level.store(level as u8, Ordering::SeqCst);
        Ok(())
    }

    pub fn log_handler(&self) -> SharedLogHandler {
        self.inner.log_handler.read().unwrap().clone()
    }

    pub fn set_log_handler<F>(&self, handler: F)
    where
        F: Fn(&Logger, LogLevel, &[LogArgument]) + Send + Sync + 'static,
    {
        *self.inner.log_handler.write().unwrap() = Arc::new(handler);
    }

    pub fn reset_log_handler(&self) {
        *self.inner.log_handler.write().unwrap() = default_log_handler_arc();
    }

    pub fn user_log_handler(&self) -> Option<SharedLogHandler> {
        self.inner.user_log_handler.read().unwrap().clone()
    }

    pub fn set_user_log_handler<F>(&self, handler: Option<F>)
    where
        F: Fn(&Logger, LogLevel, &[LogArgument]) + Send + Sync + 'static,
    {
        *self.inner.user_log_handler.write().unwrap() =
            handler.map(|f| Arc::new(f) as SharedLogHandler);
    }

    pub fn clear_user_log_handler(&self) {
        self.inner.user_log_handler.write().unwrap().take();
    }

    pub fn debug(&self, arg: impl IntoLogArgument) {
        self.emit_one(LogLevel::Debug, arg);
    }

    pub fn debug_with<I, T>(&self, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        self.emit_many(LogLevel::Debug, args);
    }

    pub fn log(&self, arg: impl IntoLogArgument) {
        self.emit_one(LogLevel::Verbose, arg);
    }

    pub fn log_with<I, T>(&self, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        self.emit_many(LogLevel::Verbose, args);
    }

    pub fn info(&self, arg: impl IntoLogArgument) {
        self.emit_one(LogLevel::Info, arg);
    }

    pub fn info_with<I, T>(&self, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        self.emit_many(LogLevel::Info, args);
    }

    pub fn warn(&self, arg: impl IntoLogArgument) {
        self.emit_one(LogLevel::Warn, arg);
    }

    pub fn warn_with<I, T>(&self, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        self.emit_many(LogLevel::Warn, args);
    }

    pub fn error(&self, arg: impl IntoLogArgument) {
        self.emit_one(LogLevel::Error, arg);
    }

    pub fn error_with<I, T>(&self, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        self.emit_many(LogLevel::Error, args);
    }

    fn emit_one(&self, level: LogLevel, arg: impl IntoLogArgument) {
        self.dispatch(level, vec![arg.into_log_argument()]);
    }

    fn emit_many<I, T>(&self, level: LogLevel, args: I)
    where
        I: IntoIterator<Item = T>,
        T: IntoLogArgument,
    {
        let arguments = args
            .into_iter()
            .map(|arg| arg.into_log_argument())
            .collect();
        self.dispatch(level, arguments);
    }

    fn dispatch(&self, level: LogLevel, arguments: Vec<LogArgument>) {
        let user_handler = self.user_log_handler();
        if let Some(handler) = user_handler {
            handler(self, level, &arguments);
        }
        (self.log_handler())(self, level, &arguments);
    }

    fn from_inner(inner: Arc<LoggerInner>) -> Self {
        Self { inner }
    }
}

struct LoggerInner {
    name: String,
    log_level: AtomicU8,
    log_handler: RwLock<SharedLogHandler>,
    user_log_handler: RwLock<Option<SharedLogHandler>>,
}

impl LoggerInner {
    fn new(name: String) -> Self {
        let level = GLOBAL_LOG_LEVEL.load(Ordering::SeqCst);
        Self {
            name,
            log_level: AtomicU8::new(level),
            log_handler: RwLock::new(default_log_handler_arc()),
            user_log_handler: RwLock::new(None),
        }
    }
}

fn track_instance(inner: &Arc<LoggerInner>) {
    INSTANCES.lock().unwrap().push(Arc::downgrade(inner));
}

fn default_log_handler_arc() -> SharedLogHandler {
    Arc::new(default_log_handler)
}

fn default_log_handler(logger: &Logger, level: LogLevel, args: &[LogArgument]) {
    if level < logger.log_level() {
        return;
    }

    if level == LogLevel::Silent {
        return;
    }

    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let message = build_message(args);
    let header = format!("[{}]  {}:", now, logger.name());

    match level {
        LogLevel::Warn | LogLevel::Error => {
            if message.is_empty() {
                eprintln!("{header}");
            } else {
                eprintln!("{header} {message}");
            }
        }
        _ => {
            if message.is_empty() {
                println!("{header}");
            } else {
                println!("{header} {message}");
            }
        }
    }
}

fn build_message(args: &[LogArgument]) -> String {
    args.iter()
        .filter_map(LogArgument::to_message_fragment)
        .collect::<Vec<_>>()
        .join(" ")
}

fn with_instances<F>(mut f: F)
where
    F: FnMut(Logger),
{
    let mut instances = INSTANCES.lock().unwrap();
    let mut i = 0;
    while i < instances.len() {
        match instances[i].upgrade() {
            Some(inner) => {
                f(Logger::from_inner(inner));
                i += 1;
            }
            None => {
                instances.swap_remove(i);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum LogLevel {
    Debug = 0,
    Verbose = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Silent = 5,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Verbose => "verbose",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
            LogLevel::Silent => "silent",
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => LogLevel::Debug,
            1 => LogLevel::Verbose,
            2 => LogLevel::Info,
            3 => LogLevel::Warn,
            4 => LogLevel::Error,
            _ => LogLevel::Silent,
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Verbose => "VERBOSE",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Silent => "SILENT",
        };
        f.write_str(label)
    }
}

impl FromStr for LogLevel {
    type Err = LogError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "debug" => Ok(LogLevel::Debug),
            "verbose" => Ok(LogLevel::Verbose),
            "info" => Ok(LogLevel::Info),
            "warn" | "warning" => Ok(LogLevel::Warn),
            "error" => Ok(LogLevel::Error),
            "silent" => Ok(LogLevel::Silent),
            other => Err(LogError::InvalidLogLevel(other.to_string())),
        }
    }
}

pub trait IntoLogLevel {
    fn into_log_level(self) -> Result<LogLevel, LogError>;
}

impl IntoLogLevel for LogLevel {
    fn into_log_level(self) -> Result<LogLevel, LogError> {
        Ok(self)
    }
}

impl IntoLogLevel for &str {
    fn into_log_level(self) -> Result<LogLevel, LogError> {
        LogLevel::from_str(self)
    }
}

impl IntoLogLevel for String {
    fn into_log_level(self) -> Result<LogLevel, LogError> {
        LogLevel::from_str(&self)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    pub level: Option<LogLevel>,
}

impl LogOptions {
    pub fn with_level<L>(mut self, level: L) -> Result<Self, LogError>
    where
        L: IntoLogLevel,
    {
        self.level = Some(level.into_log_level()?);
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct LogCallbackParams {
    pub level: LogLevel,
    pub message: String,
    pub args: Vec<Value>,
    pub logger_type: String,
}

impl LogCallbackParams {
    pub fn level_label(&self) -> &'static str {
        self.level.as_str()
    }
}

pub type LogCallback = Arc<dyn Fn(LogCallbackParams) + Send + Sync + 'static>;

#[derive(Debug, Clone, PartialEq)]
pub enum LogArgument {
    Text(String),
    Value(Value),
    Null,
}

impl LogArgument {
    pub fn text<S: Into<String>>(value: S) -> Self {
        LogArgument::Text(value.into())
    }

    pub fn value(value: Value) -> Self {
        LogArgument::Value(value)
    }

    pub fn null() -> Self {
        LogArgument::Null
    }

    pub fn to_message_fragment(&self) -> Option<String> {
        match self {
            LogArgument::Text(text) => Some(text.clone()),
            LogArgument::Value(Value::Null) | LogArgument::Null => None,
            LogArgument::Value(Value::String(text)) => Some(text.clone()),
            LogArgument::Value(Value::Bool(flag)) => Some(flag.to_string()),
            LogArgument::Value(Value::Number(number)) => Some(number.to_string()),
            LogArgument::Value(other) => Some(other.to_string()),
        }
    }

    pub fn to_callback_value(&self) -> Value {
        match self {
            LogArgument::Text(text) => Value::String(text.clone()),
            LogArgument::Value(value) => value.clone(),
            LogArgument::Null => Value::Null,
        }
    }
}

pub trait IntoLogArgument {
    fn into_log_argument(self) -> LogArgument;
}

impl IntoLogArgument for LogArgument {
    fn into_log_argument(self) -> LogArgument {
        self
    }
}

impl IntoLogArgument for &LogArgument {
    fn into_log_argument(self) -> LogArgument {
        self.clone()
    }
}

impl IntoLogArgument for String {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Text(self)
    }
}

impl IntoLogArgument for &String {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Text(self.clone())
    }
}

impl IntoLogArgument for &str {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Text(self.to_owned())
    }
}

impl<'a> IntoLogArgument for Cow<'a, str> {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Text(self.into_owned())
    }
}

impl IntoLogArgument for bool {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Value(Value::Bool(self))
    }
}

macro_rules! impl_signed_int_argument {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoLogArgument for $ty {
                fn into_log_argument(self) -> LogArgument {
                    let number = serde_json::Number::from(self as i64);
                    LogArgument::Value(Value::Number(number))
                }
            }
        )*
    };
}

macro_rules! impl_unsigned_int_argument {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoLogArgument for $ty {
                fn into_log_argument(self) -> LogArgument {
                    let number = serde_json::Number::from(self as u64);
                    LogArgument::Value(Value::Number(number))
                }
            }
        )*
    };
}

impl_signed_int_argument!(i8, i16, i32, i64, isize);
impl_unsigned_int_argument!(u8, u16, u32, u64, usize);

impl IntoLogArgument for f32 {
    fn into_log_argument(self) -> LogArgument {
        (self as f64).into_log_argument()
    }
}

impl IntoLogArgument for f64 {
    fn into_log_argument(self) -> LogArgument {
        match serde_json::Number::from_f64(self) {
            Some(number) => LogArgument::Value(Value::Number(number)),
            None => LogArgument::Null,
        }
    }
}

impl IntoLogArgument for Value {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Value(self)
    }
}

impl IntoLogArgument for &Value {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Value(self.clone())
    }
}

impl<T> IntoLogArgument for Option<T>
where
    T: IntoLogArgument,
{
    fn into_log_argument(self) -> LogArgument {
        match self {
            Some(value) => value.into_log_argument(),
            None => LogArgument::Null,
        }
    }
}

impl IntoLogArgument for () {
    fn into_log_argument(self) -> LogArgument {
        LogArgument::Null
    }
}

pub fn log_arg<T>(value: T) -> LogArgument
where
    T: IntoLogArgument,
{
    value.into_log_argument()
}

#[derive(Debug, Clone)]
pub enum LogError {
    InvalidLogLevel(String),
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::InvalidLogLevel(level) => {
                write!(f, "Invalid value \"{level}\" assigned to `logLevel`")
            }
        }
    }
}

impl std::error::Error for LogError {}

pub fn set_log_level<L>(level: L) -> Result<(), LogError>
where
    L: IntoLogLevel,
{
    let level = level.into_log_level()?;
    GLOBAL_LOG_LEVEL.store(level as u8, Ordering::SeqCst);
    with_instances(|logger| {
        let _ = logger.set_log_level(level);
    });
    Ok(())
}

pub fn set_user_log_handler(callback: Option<LogCallback>, options: Option<LogOptions>) {
    let options = options.unwrap_or_default();

    match callback {
        Some(cb) => {
            let custom_level = options.level;
            with_instances(|logger| {
                let handler_cb = Arc::clone(&cb);
                logger.set_user_log_handler(Some(
                    move |instance: &Logger, level, args: &[LogArgument]| {
                        let threshold = custom_level.unwrap_or_else(|| instance.log_level());
                        if level < threshold {
                            return;
                        }
                        let message = build_message(args);
                        let params = LogCallbackParams {
                            level,
                            message,
                            args: args.iter().map(LogArgument::to_callback_value).collect(),
                            logger_type: instance.name().to_owned(),
                        };
                        handler_cb(params);
                    },
                ));
            });
        }
        None => {
            with_instances(|logger| {
                logger.clear_user_log_handler();
            });
        }
    }
}

pub fn set_user_log_handler_fn<F>(callback: Option<F>, options: Option<LogOptions>)
where
    F: Fn(LogCallbackParams) + Send + Sync + 'static,
{
    let wrapped = callback.map(|cb| Arc::new(cb) as LogCallback);
    set_user_log_handler(wrapped, options);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    static TEST_GUARD: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn reset_logging() {
        set_log_level(LogLevel::Info).unwrap();
        set_user_log_handler(None, None);
    }

    #[test]
    fn log_methods_respect_global_level() {
        let _guard = TEST_GUARD.lock().unwrap();
        reset_logging();
        let logger = Logger::new("@firebase/logger-args-test");

        set_log_level(LogLevel::Debug).unwrap();

        let records = Arc::new(Mutex::new(Vec::new()));
        let handler_records = Arc::clone(&records);

        logger.set_log_handler(move |instance, level, args| {
            if level < instance.log_level() {
                return;
            }
            handler_records
                .lock()
                .unwrap()
                .push((level, build_message(args)));
        });

        logger.debug("debug message");
        logger.log("verbose message");
        logger.info("info message");
        logger.warn("warn message");
        logger.error("error message");

        let stored = records.lock().unwrap();
        let levels: Vec<_> = stored.iter().map(|(level, _)| *level).collect();
        assert_eq!(
            levels,
            [
                LogLevel::Debug,
                LogLevel::Verbose,
                LogLevel::Info,
                LogLevel::Warn,
                LogLevel::Error,
            ]
        );
        assert_eq!(stored[0].1, "debug message");
    }

    #[test]
    fn log_level_string_filtering() {
        let _guard = TEST_GUARD.lock().unwrap();
        reset_logging();
        let logger = Logger::new("@firebase/logger-custom-level");
        set_log_level("warn").unwrap();

        let records = Arc::new(Mutex::new(Vec::new()));
        let handler_records = Arc::clone(&records);
        logger.set_log_handler(move |instance, level, args| {
            if level < instance.log_level() {
                return;
            }
            handler_records
                .lock()
                .unwrap()
                .push((level, build_message(args)));
        });

        logger.debug("debug message");
        logger.log("verbose message");
        logger.info("info message");
        logger.warn("warn message");
        logger.error("error message");

        let stored = records.lock().unwrap();
        let levels: Vec<_> = stored.iter().map(|(level, _)| *level).collect();
        assert_eq!(levels, [LogLevel::Warn, LogLevel::Error]);
        assert_eq!(stored[0].1, "warn message");
    }

    #[test]
    fn user_log_handler_receives_arguments() {
        let _guard = TEST_GUARD.lock().unwrap();
        reset_logging();
        let logger = Logger::new("@firebase/test-logger");
        let logger_name = logger.name().to_owned();

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_cb = Arc::clone(&captured);

        set_user_log_handler_fn(
            Some({
                let logger_name = logger_name.clone();
                move |params: LogCallbackParams| {
                    if params.logger_type == logger_name {
                        captured_cb.lock().unwrap().push(params);
                    }
                }
            }),
            None,
        );

        assert!(logger.user_log_handler().is_some());

        logger.info_with(vec![
            log_arg("info message!"),
            log_arg(serde_json::json!(["hello"])),
            log_arg(1),
            log_arg(serde_json::json!({"a": 3})),
        ]);

        let records = captured.lock().unwrap().clone();
        assert_eq!(records.len(), 1, "expected a single callback invocation");
        let levels: Vec<_> = records.iter().map(|params| params.level).collect();
        assert_eq!(levels, [LogLevel::Info]);
        let params = records.into_iter().next().unwrap();
        assert_eq!(params.level, LogLevel::Info);
        assert_eq!(params.level_label(), "info");
        assert_eq!(params.message, "info message! [\"hello\"] 1 {\"a\":3}");
        assert_eq!(params.logger_type, logger_name);
        assert_eq!(params.args.len(), 4);
    }

    #[test]
    fn user_handler_respects_custom_level() {
        let _guard = TEST_GUARD.lock().unwrap();
        reset_logging();
        let logger = Logger::new("@firebase/test-logger");
        let logger_name = logger.name().to_owned();

        let captured = Arc::new(Mutex::new(Vec::new()));
        let captured_cb = Arc::clone(&captured);

        set_user_log_handler_fn(
            Some({
                let logger_name = logger_name.clone();
                move |params: LogCallbackParams| {
                    if params.logger_type == logger_name {
                        captured_cb.lock().unwrap().push(params.level);
                    }
                }
            }),
            Some(LogOptions {
                level: Some(LogLevel::Warn),
            }),
        );

        assert!(logger.user_log_handler().is_some());

        logger.info("info message");
        logger.warn("warn message");
        logger.error("error message");

        let levels = captured.lock().unwrap();
        assert_eq!(levels.as_slice(), &[LogLevel::Warn, LogLevel::Error]);
    }
}
