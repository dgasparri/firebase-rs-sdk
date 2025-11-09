use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};

use crate::data_connect::error::{invalid_argument, DataConnectResult};

/// Default production host for the Data Connect REST API.
pub const DEFAULT_DATA_CONNECT_HOST: &str = "firebasedataconnect.googleapis.com";

/// Root connector configuration (location/connector/service) supplied by the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ConnectorConfig {
    pub location: String,
    pub connector: String,
    pub service: String,
}

impl ConnectorConfig {
    /// Validates and constructs a new connector configuration.
    pub fn new(
        location: impl Into<String>,
        connector: impl Into<String>,
        service: impl Into<String>,
    ) -> DataConnectResult<Self> {
        let config = Self {
            location: location.into(),
            connector: connector.into(),
            service: service.into(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Ensures mandatory fields are present.
    pub fn validate(&self) -> DataConnectResult<()> {
        if self.location.trim().is_empty() {
            return Err(invalid_argument("location is required"));
        }
        if self.connector.trim().is_empty() {
            return Err(invalid_argument("connector is required"));
        }
        if self.service.trim().is_empty() {
            return Err(invalid_argument("service is required"));
        }
        Ok(())
    }

    /// Stable string identifier used as the component instance key.
    pub fn identifier(&self) -> String {
        serde_json::to_string(self).expect("connector config serialization")
    }
}

impl Display for ConnectorConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.location, self.service, self.connector)
    }
}

/// Fully-qualified options passed to the transport layer once the project ID is known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataConnectOptions {
    pub connector: ConnectorConfig,
    pub project_id: String,
}

impl DataConnectOptions {
    pub fn new(
        connector: ConnectorConfig,
        project_id: impl Into<String>,
    ) -> DataConnectResult<Self> {
        let options = Self {
            connector,
            project_id: project_id.into(),
        };
        options.connector.validate()?;
        if options.project_id.trim().is_empty() {
            return Err(invalid_argument("project_id is required"));
        }
        Ok(options)
    }

    /// Returns the canonical resource path for this connector (without the API host).
    pub fn resource_path(&self) -> String {
        format!(
            "projects/{}/locations/{}/services/{}/connectors/{}",
            self.project_id,
            self.connector.location,
            self.connector.service,
            self.connector.connector
        )
    }
}

/// Host/port/SSL tuple used for emulator connections.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportOptions {
    pub host: String,
    pub port: Option<u16>,
    pub ssl_enabled: bool,
}

impl TransportOptions {
    pub fn new(host: impl Into<String>, port: Option<u16>, ssl_enabled: bool) -> Self {
        Self {
            host: host.into(),
            port,
            ssl_enabled,
        }
    }

    /// Builds the base URL given the configured host/port.
    pub fn base_url(&self) -> String {
        let scheme = if self.ssl_enabled { "https" } else { "http" };
        match self.port {
            Some(port) => format!("{scheme}://{}:{port}", self.host),
            None => format!("{scheme}://{}", self.host),
        }
    }
}

impl Default for TransportOptions {
    fn default() -> Self {
        Self {
            host: DEFAULT_DATA_CONNECT_HOST.to_string(),
            port: None,
            ssl_enabled: true,
        }
    }
}

/// Parses the `FIREBASE_DATA_CONNECT_EMULATOR_HOST` environment variable payload.
pub fn parse_transport_options(spec: &str) -> DataConnectResult<TransportOptions> {
    let (protocol, rest) = spec.split_once("://").unwrap_or(("https", spec));
    let ssl_enabled = match protocol {
        "http" => false,
        "https" => true,
        other => {
            return Err(invalid_argument(format!(
                "Unsupported protocol '{other}' in emulator host"
            )))
        }
    };

    let (host, port) = if let Some((host, port)) = rest.split_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|_| invalid_argument("Port must be a number in emulator host declaration"))?;
        (host.to_string(), Some(port))
    } else {
        (rest.to_string(), None)
    };

    if host.trim().is_empty() {
        return Err(invalid_argument(
            "Host is required for emulator connections",
        ));
    }

    Ok(TransportOptions::new(host, port, ssl_enabled))
}
