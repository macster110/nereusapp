//! Connecting to a Nereus PostgreSQL database.

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use tokio_postgres::config::{Host, SslMode as PgSslMode};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::error::{Error, Result};

/// How to use TLS. Mirrors libpq's sslmode: `prefer` and `require` encrypt
/// without checking the certificate; `verify` checks it and the host name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslMode {
    Disable,
    #[default]
    Prefer,
    Require,
    Verify,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionSettings {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub ssl: SslMode,
}

impl ConnectionSettings {
    /// Parse a `postgresql://user:pass@host:port/db?sslmode=...` URL or a
    /// libpq key=value string.
    pub fn parse(s: &str) -> Result<Self> {
        let cfg = tokio_postgres::Config::from_str(s.trim())
            .map_err(|e| Error::Setup(format!("not a valid connection string: {e}")))?;
        let host = match cfg.get_hosts().first() {
            Some(Host::Tcp(h)) => h.clone(),
            _ => "localhost".into(),
        };
        let ssl = match cfg.get_ssl_mode() {
            PgSslMode::Disable => SslMode::Disable,
            PgSslMode::Require => SslMode::Require,
            _ => SslMode::Prefer,
        };
        Ok(Self {
            host,
            port: cfg.get_ports().first().copied().unwrap_or(5432),
            database: cfg.get_dbname().unwrap_or("nereus").into(),
            user: cfg.get_user().unwrap_or("nereus_reader").into(),
            password: cfg.get_password().map(|p| String::from_utf8_lossy(p).into_owned()),
            ssl,
        })
    }

    fn pg_config(&self) -> tokio_postgres::Config {
        let mut cfg = tokio_postgres::Config::new();
        cfg.host(&self.host)
            .port(self.port)
            .dbname(&self.database)
            .user(&self.user)
            .application_name("Nereus")
            .connect_timeout(Duration::from_secs(10))
            .ssl_mode(match self.ssl {
                SslMode::Disable => PgSslMode::Disable,
                SslMode::Prefer => PgSslMode::Prefer,
                SslMode::Require | SslMode::Verify => PgSslMode::Require,
            });
        if let Some(p) = self.password.as_deref().filter(|p| !p.is_empty()) {
            cfg.password(p);
        }
        cfg
    }

    /// Short label, e.g. `nereus @ db.example.org`.
    pub fn label(&self) -> String {
        format!("{} @ {}", self.database, self.host)
    }
}

/// Install the process-wide rustls crypto provider (ring). Safe to call more than once.
pub fn install_crypto() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn tls(mode: SslMode) -> MakeRustlsConnect {
    install_crypto();
    let config = if mode == SslMode::Verify {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()
    } else {
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth()
    };
    MakeRustlsConnect::new(config)
}

/// Accepts any server certificate: libpq's `sslmode=require` behaviour.
#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// What the app shows once connected.
#[derive(Clone, Debug, Serialize)]
pub struct DbInfo {
    pub label: String,
    pub server_version: String,
    pub n_deployments: i64,
    pub n_detection_sets: i64,
    /// From the planner's statistics: exact counts of a huge table are slow.
    pub n_detections_estimate: i64,
    pub n_data_files: i64,
}

/// A connection pool to one Nereus database.
#[derive(Clone)]
pub struct Db {
    pub pool: Pool,
    pub settings: ConnectionSettings,
}

impl Db {
    /// Connect and check this is a Nereus database with the current schema.
    pub async fn connect(settings: ConnectionSettings) -> Result<(Self, DbInfo)> {
        let mgr = Manager::from_config(
            settings.pg_config(),
            tls(settings.ssl),
            ManagerConfig { recycling_method: RecyclingMethod::Fast },
        );
        let pool = Pool::builder(mgr)
            .max_size(6)
            .build()
            .map_err(|e| Error::Setup(e.to_string()))?;
        let db = Self { pool, settings };
        let info = db.info().await?;
        Ok((db, info))
    }

    async fn info(&self) -> Result<DbInfo> {
        let c = self.pool.get().await?;
        let r = c
            .query_one(
                "SELECT to_regclass('nereus.deployment') IS NOT NULL,
                        to_regclass('nereus.effort') IS NOT NULL
                        AND to_regclass('nereus.data_file') IS NOT NULL
                        AND to_regclass('nereus.summary_daily') IS NOT NULL,
                        current_setting('server_version')",
                &[],
            )
            .await?;
        let (has_nereus, current, version): (bool, bool, String) = (r.get(0), r.get(1), r.get(2));
        if !has_nereus {
            return Err(Error::NotNereus(
                "This database has no Nereus schema (no nereus.deployment table).".into(),
            ));
        }
        if !current {
            return Err(Error::NotNereus(
                "This database uses an older Nereus schema. Re-create it with the current \
                 sql/001_schema.sql and re-import the data."
                    .into(),
            ));
        }
        let r = c
            .query_one(
                "SELECT (SELECT count(*) FROM nereus.deployment),
                        (SELECT count(*) FROM nereus.detection_set),
                        (SELECT greatest(reltuples, 0)::int8 FROM pg_class
                          WHERE oid = 'nereus.detection'::regclass),
                        (SELECT count(*) FROM nereus.data_file)",
                &[],
            )
            .await?;
        Ok(DbInfo {
            label: self.settings.label(),
            server_version: version,
            n_deployments: r.get(0),
            n_detection_sets: r.get(1),
            n_detections_estimate: r.get(2),
            n_data_files: r.get(3),
        })
    }
}
