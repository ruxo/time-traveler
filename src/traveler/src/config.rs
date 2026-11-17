//! The service's settings (design section 7): `appsettings.toml` overlaid by `appsettings.{RUST_ENVIRONMENT}.toml`,
//! then by environment variables, deserialised into kebab-case structs. A variable nests with `__`
//! (`MONGO__CONNECTION_STRING` → `mongo.connection-string`); the names Erawan's auth add-on sets, which have one
//! underscore, map through [`ALIASES`].

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::exit
    )
)]

use std::net::SocketAddr;
use std::path::Path;

use config::builder::DefaultState;
use config::{Config, ConfigBuilder, File};
use rzlib_error::{Outcome, errorinfo};
use serde::Deserialize;

use crate::errors::Code;

/// Environment variables without `__` whose meaning is fixed: the six Erawan's auth add-on sets, `PUBLIC_URL`, and
/// single-underscore spellings of the two secrets, in case Erawan refuses `__` in a secret's name.
pub const ALIASES: [(&str, &str); 10] = [
    ("AUTH_ISSUER_URL", "auth.issuer-url"),
    ("AUTH_CLIENT_ID", "auth.client-id"),
    ("AUTH_CLIENT_SECRET", "auth.client-secret"),
    ("AUTH_JWKS_URL", "auth.jwks-url"),
    ("AUTH_WELLKNOWN_URL", "auth.well-known-url"),
    ("PUBLIC_URL", "auth.public-url"),
    ("MONGO_CONNECTION_STRING", "mongo.connection-string"),
    ("SESSION_KEY", "session.key"),
    ("BIND_ADDRESS", "bind-address"),
    ("PORT", "port"),
];

/// The shortest `session.key` accepted: 32 characters, 256 bits of text at the least.
pub const MIN_SESSION_KEY_CHARS: usize = 32;

/// Which overlay is loaded: `RUST_ENVIRONMENT`, or `production` when unset (and then no overlay file exists).
pub fn environment() -> String {
    std::env::var("RUST_ENVIRONMENT").unwrap_or_else(|_| "production".to_owned())
}

/// A value that must never be printed. `Debug` prints `<redacted>`.
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    pub bind_address: SocketAddr,
    /// Erawan sets `PORT`; when present it replaces the port of `bind-address`.
    #[serde(default)]
    pub port: Option<u16>,
    pub mongo: MongoSettings,
    pub session: SessionSettings,
    pub auth: AuthSettings,
    pub logging: LoggingSettings,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MongoSettings {
    #[serde(default)]
    pub connection_string: Option<Secret>,
    pub database: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionSettings {
    #[serde(default)]
    pub key: Option<Secret>,
    pub member_days: u64,
    pub admin_hours: u64,
    pub setup_hours: u64,
    pub oidc_minutes: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AuthSettings {
    #[serde(default)]
    pub issuer_url: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<Secret>,
    #[serde(default)]
    pub jwks_url: Option<String>,
    #[serde(default)]
    pub well_known_url: Option<String>,
    #[serde(default)]
    pub public_url: Option<String>,
    #[serde(default)]
    pub dev_login: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LoggingSettings {
    pub level: String,
}

/// The identity provider's settings once all three required values are present.
#[derive(Debug, Clone)]
pub struct Oidc {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Secret,
    pub jwks_url: Option<String>,
    pub well_known_url: String,
}

fn invalid(message: impl Into<String>) -> rzlib_error::ErrorInfo {
    errorinfo!(Code::SERVER_ERROR, message, "loading settings")
}

impl Settings {
    /// The TOML layers from `dir` with the process environment over them, checked.
    pub fn load(dir: &Path, environment: &str) -> Outcome<Self> {
        let vars: Vec<(String, String)> = std::env::vars_os()
            .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
            .collect();
        Self::from_layers(toml_layers(dir, environment), vars)
    }

    /// `builder` with `vars` applied over it, deserialised and checked.
    pub fn from_layers(builder: ConfigBuilder<DefaultState>, vars: impl IntoIterator<Item = (String, String)>) -> Outcome<Self> {
        let settings: Self = apply_environment(builder, vars)?
            .build()
            .and_then(|config| config.try_deserialize())
            .map_err(|e| invalid(format!("the settings: {e}")))?;
        settings.check()?;
        Ok(settings)
    }

    /// Every value that can be wrong, refused by key.
    pub fn check(&self) -> Outcome<()> {
        if self.mongo.connection_string.is_none() {
            return Err(invalid("mongo.connection-string (MONGO__CONNECTION_STRING) is required"));
        }
        match &self.session.key {
            None => return Err(invalid("session.key (SESSION__KEY) is required")),
            Some(key) if key.expose().chars().count() < MIN_SESSION_KEY_CHARS => {
                return Err(invalid(format!("session.key must be at least {MIN_SESSION_KEY_CHARS} characters")));
            }
            Some(_) => {}
        }
        let given = [
            ("auth.issuer-url", self.auth.issuer_url.is_some()),
            ("auth.client-id", self.auth.client_id.is_some()),
            ("auth.client-secret", self.auth.client_secret.is_some()),
        ];
        let present = given.iter().filter(|(_, set)| *set).count();
        if present != 0 && present != given.len() {
            let missing: Vec<&str> = given.iter().filter(|(_, set)| !set).map(|(key, _)| *key).collect();
            return Err(invalid(format!("auth is partly configured; missing {}", missing.join(", "))));
        }
        if self.auth.dev_login && present == given.len() {
            return Err(invalid(
                "auth.dev-login is true while the identity provider is configured; a deployment never has both",
            ));
        }
        for (key, value) in [
            ("session.member-days", self.session.member_days),
            ("session.admin-hours", self.session.admin_hours),
            ("session.setup-hours", self.session.setup_hours),
            ("session.oidc-minutes", self.session.oidc_minutes),
        ] {
            if value == 0 {
                return Err(invalid(format!("{key} must be positive")));
            }
        }
        Ok(())
    }

    /// The identity provider, when configured.
    pub fn oidc(&self) -> Option<Oidc> {
        let issuer = self.auth.issuer_url.as_deref()?.trim_end_matches('/').to_owned();
        let client_id = self.auth.client_id.clone()?;
        let client_secret = self.auth.client_secret.clone()?;
        let well_known_url = self
            .auth
            .well_known_url
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| format!("{issuer}/.well-known/openid-configuration"));
        Some(Oidc {
            issuer,
            client_id,
            client_secret,
            jwks_url: self.auth.jwks_url.clone().filter(|url| !url.is_empty()),
            well_known_url,
        })
    }

    /// `auth.public-url` without a trailing slash.
    pub fn public_url(&self) -> Option<String> {
        self.auth
            .public_url
            .as_deref()
            .map(|url| url.trim_end_matches('/').to_owned())
            .filter(|url| !url.is_empty())
    }

    /// Where to listen: `bind-address`, its port replaced by `PORT` when Erawan sets one.
    pub fn listen_address(&self) -> SocketAddr {
        let mut address = self.bind_address;
        if let Some(port) = self.port {
            address.set_port(port);
        }
        address
    }

    pub fn connection_string(&self) -> &str {
        self.mongo.connection_string.as_ref().map_or("", Secret::expose)
    }

    pub fn session_key(&self) -> &[u8] {
        self.session.key.as_ref().map_or(&[], |key| key.expose().as_bytes())
    }
}

/// `appsettings.toml` (required) overlaid by `appsettings.{environment}.toml` (optional).
pub fn toml_layers(dir: &Path, environment: &str) -> ConfigBuilder<DefaultState> {
    Config::builder()
        .add_source(File::from(dir.join("appsettings.toml")).required(true))
        .add_source(File::from(dir.join(format!("appsettings.{environment}.toml"))).required(false))
}

/// Environment variables over the files: an alias by its fixed key, any other variable with `__` by its nested
/// kebab-case path. An empty value is unset; every other variable is ignored.
pub fn apply_environment(mut builder: ConfigBuilder<DefaultState>, vars: impl IntoIterator<Item = (String, String)>) -> Outcome<ConfigBuilder<DefaultState>> {
    for (name, value) in vars {
        if value.is_empty() {
            continue;
        }
        let key = if let Some((_, key)) = ALIASES.iter().find(|(alias, _)| *alias == name) {
            (*key).to_owned()
        } else if name.contains("__") {
            name.split("__")
                .filter(|s| !s.is_empty())
                .map(|s| s.to_ascii_lowercase().replace('_', "-"))
                .collect::<Vec<_>>()
                .join(".")
        } else {
            continue;
        };
        builder = builder.set_override(&key, value).map_err(|e| invalid(format!("the variable {name}: {e}")))?;
    }
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
bind-address = "0.0.0.0:8080"
[mongo]
database = "traveler"
[session]
member-days = 30
admin-hours = 12
setup-hours = 24
oidc-minutes = 15
[auth]
dev-login = false
[logging]
level = "info"
"#;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
    }

    fn layers(dir: &Path) -> ConfigBuilder<DefaultState> {
        std::fs::write(dir.join("appsettings.toml"), BASE).expect("fixture");
        toml_layers(dir, "production")
    }

    const KEY: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn nested_and_alias_variables_land_on_their_keys() {
        let dir = tempfile::tempdir().expect("temp dir");
        let settings = Settings::from_layers(
            layers(dir.path()),
            vars(&[
                ("MONGO__CONNECTION_STRING", "mongodb://x"),
                ("SESSION__KEY", KEY),
                ("AUTH_ISSUER_URL", "https://auth.example/"),
                ("AUTH_CLIENT_ID", "cid"),
                ("AUTH_CLIENT_SECRET", "sec"),
                ("PUBLIC_URL", "https://app.example/"),
                ("PORT", "9000"),
                ("SESSION__MEMBER_DAYS", "7"),
                ("Path", "C:\\Windows"),
            ]),
        )
        .expect("loads");

        assert_eq!(settings.connection_string(), "mongodb://x");
        assert_eq!(settings.session.member_days, 7);
        let oidc = settings.oidc().expect("configured");
        assert_eq!(oidc.issuer, "https://auth.example");
        assert_eq!(oidc.well_known_url, "https://auth.example/.well-known/openid-configuration");
        assert_eq!(settings.public_url().as_deref(), Some("https://app.example"));
        assert_eq!(settings.listen_address().port(), 9000);
        assert_eq!(format!("{:?}", settings.session.key), "Some(<redacted>)");
    }

    #[test]
    fn missing_or_short_secrets_and_partial_auth_are_refused_by_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let error = Settings::from_layers(layers(dir.path()), vars(&[("SESSION__KEY", KEY)])).expect_err("no mongo");
        assert!(error.message.contains("mongo.connection-string"), "{}", error.message);

        let error = Settings::from_layers(layers(dir.path()), vars(&[("MONGO__CONNECTION_STRING", "m"), ("SESSION__KEY", "short")])).expect_err("short key");
        assert!(error.message.contains("session.key"), "{}", error.message);

        let error = Settings::from_layers(
            layers(dir.path()),
            vars(&[("MONGO__CONNECTION_STRING", "m"), ("SESSION__KEY", KEY), ("AUTH_ISSUER_URL", "https://a")]),
        )
        .expect_err("partial auth");
        assert!(error.message.contains("auth.client-id"), "{}", error.message);
    }

    #[test]
    fn dev_login_without_a_provider_is_fine_and_with_one_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let ok = Settings::from_layers(
            layers(dir.path()),
            vars(&[("MONGO__CONNECTION_STRING", "m"), ("SESSION__KEY", KEY), ("AUTH__DEV_LOGIN", "true")]),
        )
        .expect("dev login alone");
        assert!(ok.auth.dev_login);
        assert!(ok.oidc().is_none());

        let error = Settings::from_layers(
            layers(dir.path()),
            vars(&[
                ("MONGO__CONNECTION_STRING", "m"),
                ("SESSION__KEY", KEY),
                ("AUTH__DEV_LOGIN", "true"),
                ("AUTH_ISSUER_URL", "https://a"),
                ("AUTH_CLIENT_ID", "c"),
                ("AUTH_CLIENT_SECRET", "s"),
            ]),
        )
        .expect_err("both");
        assert!(error.message.contains("dev-login"), "{}", error.message);
    }
}
