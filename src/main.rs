mod agent;
mod protocol;
mod pty;
mod session_manager;

use std::env;

use anyhow::{Context, Result, bail};
use tokio::io::{self, AsyncWriteExt};
use tracing::info;

use crate::agent::Agent;
use crate::session_manager::{
    ClientOptions, SessionManagerServer, SessionRequest, default_socket_path, is_server_running,
    send_client_request,
};

#[tokio::main]
async fn main() -> Result<()> {
    let command = Command::parse(env::args())?;

    if matches!(command.mode, Mode::Help) {
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
    let client_options = ClientOptions {
        socket_path: command
            .socket_path
            .clone()
            .unwrap_or_else(default_socket_path),
    };

    match command.mode {
        Mode::Start => {
            if is_server_running(&client_options.socket_path).await {
                bail!(
                    "Caterm server is already running at {}",
                    client_options.socket_path.display()
                );
            }

            if client_options.socket_path.exists() {
                std::fs::remove_file(&client_options.socket_path).with_context(|| {
                    format!(
                        "failed to remove stale socket {}",
                        client_options.socket_path.display()
                    )
                })?;
            }

            info!(shell = %shell, socket = %client_options.socket_path.display(), "starting Caterm server");
            let mut server = SessionManagerServer::new(config, client_options.socket_path);
            server.run().await
        }
        Mode::Agent => {
            info!(shell = %shell, "starting Caterm agent");
            let mut agent = Agent::new(config)?;
            agent.run().await
        }
        Mode::Create { name } => {
            run_client_command(&client_options, SessionRequest::Create { name }).await
        }
        Mode::Delete { target } => {
            run_client_command(&client_options, SessionRequest::Delete { target }).await
        }
        Mode::List => run_client_command(&client_options, SessionRequest::List).await,
        Mode::Stop => run_client_command(&client_options, SessionRequest::Stop).await,
        Mode::Help => Ok(()),
    }
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
    mode: Mode,
    socket_path: Option<std::path::PathBuf>,
    version: bool,
}

#[derive(Debug, Default, Clone)]
enum Mode {
    Start,
    Agent,
    Create {
        name: Option<String>,
    },
    Delete {
        target: String,
    },
    List,
    Stop,
    #[default]
    Help,
}

impl Command {
    fn parse<I>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = String>,
    {
        let mut command = Self {
            mode: Mode::Start,
            socket_path: None,
            version: false,
        };
        let mut mode_set = false;
        let mut iter = args.into_iter().skip(1);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => command.mode = Mode::Help,
                "-V" | "--version" => command.version = true,
                "--socket" => {
                    let value = iter.next().context("--socket requires a path")?;
                    command.socket_path = Some(value.into());
                }
                "start" if !mode_set => {
                    command.mode = Mode::Start;
                    mode_set = true;
                }
                "agent" if !mode_set => {
                    command.mode = Mode::Agent;
                    mode_set = true;
                }
                "create" if !mode_set => {
                    let name = iter.next();
                    command.mode = Mode::Create { name };
                    mode_set = true;
                }
                "delete" if !mode_set => {
                    let target = iter
                        .next()
                        .context("delete requires a session id or name")?;
                    command.mode = Mode::Delete { target };
                    mode_set = true;
                }
                "list" if !mode_set => {
                    command.mode = Mode::List;
                    mode_set = true;
                }
                "stop" if !mode_set => {
                    command.mode = Mode::Stop;
                    mode_set = true;
                }
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
    println!("caterm {}", env!("CARGO_PKG_VERSION"));
}

fn help_text() -> String {
    format!(
        "\
CLI agent for Caterm

Usage:
  caterm [COMMAND] [OPTIONS]

Commands:
  start            Start the local Caterm server (default)
  create [name]    Create a session via the local server
  delete <target>  Delete a session by id or name
  list             List sessions managed by the local server
  stop             Stop the local Caterm server
  agent            Run the JSON PTY agent mode

Options:
  -h, --help       Print help
  -V, --version    Print version
  --socket <path>  Override the Unix socket path for the local server

Environment:
  CATERM_SHELL     Override the shell used for the PTY session
  CATERM_SOCKET    Override the default Unix socket path
  SHELL            Fallback shell if CATERM_SHELL is not set
  RUST_LOG         Configure tracing output

Protocol:
  `caterm agent` reads JSON client messages from stdin and writes JSON agent events to stdout."
    )
}

async fn run_client_command(options: &ClientOptions, request: SessionRequest) -> Result<()> {
    let response = send_client_request(options, request).await?;
    let mut stdout = io::stdout();

    stdout.write_all(response.message.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;

    if response.ok {
        Ok(())
    } else {
        bail!("{}", response.message)
    }
}
