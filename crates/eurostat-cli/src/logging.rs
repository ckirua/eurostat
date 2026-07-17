//! Tracing subscriber setup.

use anyhow::Result;
use tracing_subscriber::EnvFilter;

/// Initialize logging from verbosity flag and `RUST_LOG`.
pub fn init(verbose: bool) -> Result<()> {
    let default = if verbose { "debug" } else { "info" };
    let filter = EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(default))?;
    tracing_subscriber::fmt().with_env_filter(filter).init();
    Ok(())
}
