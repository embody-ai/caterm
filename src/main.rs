mod agent;
mod protocol;
mod pty;

use std::env;

use anyhow::{Result, bail};
use tracing::info;

use crate::agent::Agent;

#[tokio::main]
async fn main() -> Result<()> {
    let command = Command::parse(env::args())?;

    if command.help {
        print_help();
        return Ok(());
    }

    if command.version {
        print_version();
        return Ok(());
    }

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

#[derive(Debug, Default)]
struct Command {
    help: bool,
    version: bool,
}

impl Command {
    fn parse<I>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut command = Self::default();

        for arg in args.into_iter().skip(1) {
            match arg.as_str() {
                "-h" | "--help" => command.help = true,
                "-V" | "--version" => command.version = true,
                _ => bail!("unknown argument: {arg}\n\n{}", help_text()),
            }
        }

        Ok(command)
    }
}

fn print_help() {
    println!("{}", help_text());
}

fn print_version() {
    println!("caterm-agent {}", env!("CARGO_PKG_VERSION"));
}

fn help_text() -> String {
    format!(
        "\
CLI agent for Caterm

Usage:
  caterm-agent [OPTIONS]

Options:
  -h, --help       Print help
  -V, --version    Print version

Environment:
  CATERM_SHELL     Override the shell used for the PTY session
  SHELL            Fallback shell if CATERM_SHELL is not set
  RUST_LOG         Configure tracing output

Protocol:
  Reads JSON client messages from stdin and writes JSON agent events to stdout."
    )
}
