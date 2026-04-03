mod agent;
mod protocol;
mod pty;

use std::env;

use anyhow::Result;
use tracing::info;

use crate::agent::Agent;

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = agent::AgentConfig::from_env();
    let shell = config.shell.clone();

    info!(shell = %shell, "starting Caterm agent");

    let mut agent = Agent::new(config)?;
    agent.run().await
}

fn init_tracing() {
    let filter = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
