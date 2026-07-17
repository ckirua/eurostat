//! ClickHouse connection configuration.

/// Runtime configuration for ClickHouse ingest.
#[derive(Clone)]
pub struct ClickHouseConfig {
    /// Server hostname.
    pub host: String,
    /// Native protocol port from env (used by shell scripts); HTTP client uses [`Self::http_port`].
    pub port: u16,
    /// HTTP(S) port for the Rust `clickhouse` crate (default 8123; typically 8443 with TLS).
    pub http_port: u16,
    /// Database user.
    pub user: String,
    /// Database password.
    pub password: String,
    /// Target database name.
    pub database: String,
    /// When true, use HTTPS for the HTTP client (`CLICKHOUSE_TLS=1`).
    /// Required for remote hosts so credentials are not sent in cleartext.
    pub tls: bool,
}

impl std::fmt::Debug for ClickHouseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("http_port", &self.http_port)
            .field("user", &self.user)
            .field("password", &"<redacted>")
            .field("database", &self.database)
            .field("tls", &self.tls)
            .finish()
    }
}

impl ClickHouseConfig {
    /// Load from environment variables.
    ///
    /// Required: `CLICKHOUSE_PASSWORD`
    /// Optional: `CLICKHOUSE_HOST` (`127.0.0.1`), `CLICKHOUSE_PORT` (9000),
    ///           `CLICKHOUSE_HTTP_PORT` (8123), `CLICKHOUSE_USER` (default),
    ///           `CLICKHOUSE_DATABASE` (eurostat), `CLICKHOUSE_TLS` (`0`/`1`)
    pub fn from_env() -> crate::error::Result<Self> {
        let host = std::env::var("CLICKHOUSE_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string())
            .trim()
            .to_string();
        let port = std::env::var("CLICKHOUSE_PORT")
            .unwrap_or_else(|_| "9000".to_string())
            .trim()
            .parse()
            .map_err(|_| crate::error::Error::Config("invalid CLICKHOUSE_PORT".into()))?;
        let http_port = std::env::var("CLICKHOUSE_HTTP_PORT")
            .unwrap_or_else(|_| "8123".to_string())
            .trim()
            .parse()
            .map_err(|_| crate::error::Error::Config("invalid CLICKHOUSE_HTTP_PORT".into()))?;
        let user = std::env::var("CLICKHOUSE_USER")
            .unwrap_or_else(|_| "default".to_string())
            .trim()
            .to_string();
        let password = std::env::var("CLICKHOUSE_PASSWORD")
            .map_err(|_| crate::error::Error::Config("CLICKHOUSE_PASSWORD is not set".into()))?
            .trim()
            .to_string();
        let database = std::env::var("CLICKHOUSE_DATABASE")
            .unwrap_or_else(|_| "eurostat".to_string())
            .trim()
            .to_string();
        let tls = parse_env_bool("CLICKHOUSE_TLS")?;
        Ok(Self {
            host,
            port,
            http_port,
            user,
            password,
            database,
            tls,
        })
    }

    /// Build a ClickHouse HTTP(S) URL for the Rust client.
    pub fn url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{scheme}://{}:{}", self.host, self.http_port)
    }
}

fn parse_env_bool(name: &str) -> crate::error::Result<bool> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(false);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "0" | "false" | "no" | "off" => Ok(false),
        "1" | "true" | "yes" | "on" => Ok(true),
        other => Err(crate::error::Error::Config(format!(
            "invalid {name}: expected 0/1/true/false, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(tls: bool) -> ClickHouseConfig {
        ClickHouseConfig {
            host: "127.0.0.1".into(),
            port: 9000,
            http_port: if tls { 8443 } else { 8123 },
            user: "default".into(),
            password: "secret".into(),
            database: "eurostat".into(),
            tls,
        }
    }

    #[test]
    fn default_url_uses_http_port() {
        assert_eq!(sample(false).url(), "http://127.0.0.1:8123");
    }

    #[test]
    fn tls_url_uses_https() {
        assert_eq!(sample(true).url(), "https://127.0.0.1:8443");
    }

    #[test]
    fn debug_redacts_password() {
        let dbg = format!("{:?}", sample(false));
        assert!(dbg.contains("<redacted>"));
        assert!(!dbg.contains("secret"));
    }
}
