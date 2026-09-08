#![forbid(unsafe_code)]

mod ai;
mod crypto;
mod db;
mod language;
mod web;

use std::net::SocketAddr;
use std::path::PathBuf;

use auth_mini_axum::{AuthMiniLayer, JwksCachePolicy};
use db::Database;
use thiserror::Error;

pub use web::router;

#[derive(Clone, Debug)]
pub struct App {
    database: Database,
}

#[derive(Debug, Error)]
pub enum HttpServerError {
    #[error("failed to prepare the CTX data directory")]
    DataDirectory(#[source] std::io::Error),
    #[error("failed to initialize the CTX database")]
    Database(#[from] db::DatabaseError),
    #[error("failed to initialize Auth Mini verification")]
    Auth(#[from] auth_mini_axum::AuthMiniError),
    #[error("HTTP server I/O error")]
    Io(#[from] std::io::Error),
}

impl App {
    /// Opens CTX's host-local state directory and `SQLite` database.
    ///
    /// # Errors
    ///
    /// Returns an error when the state directory or database cannot be prepared.
    pub fn from_home() -> Result<Self, HttpServerError> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let state_directory = home.join(".ctx");
        std::fs::create_dir_all(&state_directory).map_err(HttpServerError::DataDirectory)?;
        Ok(Self {
            database: Database::open(state_directory)?,
        })
    }

    /// Starts CTX's authenticated HTTP server at the supplied local address.
    ///
    /// # Errors
    ///
    /// Returns an error when Auth Mini setup, listener binding, or serving fails.
    pub async fn serve(self, address: SocketAddr) -> Result<(), HttpServerError> {
        let auth = AuthMiniLayer::from_issuer_background(
            "https://auth.ntnl.io",
            "ctx.ntnl.io",
            JwksCachePolicy::default(),
        )?;
        let app = web::router(self.database, auth);
        let listener = tokio::net::TcpListener::bind(address).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}
