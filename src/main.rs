mod config;
mod pty;
mod session_manager;

use std::env;

use anyhow::{Context, Result, bail};
use tokio::io::{self, AsyncWriteExt};
use tracing::info;

use crate::config::DaemonConfig;
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

    let config = DaemonConfig::from_env();
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
        Mode::NewSession { name } => {
            run_client_command(&client_options, SessionRequest::CreateSession { name }).await
        }
        Mode::NewWindow { session, name } => {
            run_client_command(
                &client_options,
                SessionRequest::CreateWindow { session, name },
            )
            .await
        }
        Mode::NewPane {
            session,
            window,
            name,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::CreatePane {
                    session,
                    window,
                    name,
                },
            )
            .await
        }
        Mode::KillSession { target } => {
            run_client_command(&client_options, SessionRequest::DeleteSession { target }).await
        }
        Mode::KillWindow { session, target } => {
            run_client_command(
                &client_options,
                SessionRequest::DeleteWindow { session, target },
            )
            .await
        }
        Mode::KillPane {
            session,
            window,
            target,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::DeletePane {
                    session,
                    window,
                    target,
                },
            )
            .await
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
    NewSession {
        name: Option<String>,
    },
    NewWindow {
        session: String,
        name: Option<String>,
    },
    NewPane {
        session: String,
        window: String,
        name: Option<String>,
    },
    KillSession {
        target: String,
    },
    KillWindow {
        session: String,
        target: String,
    },
    KillPane {
        session: String,
        window: String,
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
                "new-session" if !mode_set => {
                    let name = iter.next();
                    command.mode = Mode::NewSession { name };
                    mode_set = true;
                }
                "new-window" if !mode_set => {
                    let session = iter
                        .next()
                        .context("new-window requires a session id or name")?;
                    let name = iter.next();
                    command.mode = Mode::NewWindow { session, name };
                    mode_set = true;
                }
                "new-pane" if !mode_set => {
                    let session = iter
                        .next()
                        .context("new-pane requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("new-pane requires a window id or name")?;
                    let name = iter.next();
                    command.mode = Mode::NewPane {
                        session,
                        window,
                        name,
                    };
                    mode_set = true;
                }
                "kill-session" if !mode_set => {
                    let target = iter
                        .next()
                        .context("kill-session requires a session id or name")?;
                    command.mode = Mode::KillSession { target };
                    mode_set = true;
                }
                "kill-window" if !mode_set => {
                    let session = iter
                        .next()
                        .context("kill-window requires a session id or name")?;
                    let target = iter
                        .next()
                        .context("kill-window requires a window id or name")?;
                    command.mode = Mode::KillWindow { session, target };
                    mode_set = true;
                }
                "kill-pane" if !mode_set => {
                    let session = iter
                        .next()
                        .context("kill-pane requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("kill-pane requires a window id or name")?;
                    let target = iter
                        .next()
                        .context("kill-pane requires a pane id or name")?;
                    command.mode = Mode::KillPane {
                        session,
                        window,
                        target,
                    };
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
Caterm daemon

Usage:
  caterm [COMMAND] [OPTIONS]

Commands:
  start            Start the local Caterm server (default)
  new-session      Create a session with an initial window and pane
  new-window       Create a window in a session
  new-pane         Create a pane in a window
  kill-session     Delete a session by id or name
  kill-window      Delete a window by id or name within a session
  kill-pane        Delete a pane by id or name within a window
  list             List the session/window/pane hierarchy
  stop             Stop the local Caterm server

Options:
  -h, --help       Print help
  -V, --version    Print version
  --socket <path>  Override the Unix socket path for the local server

Environment:
  CATERM_SHELL     Override the shell used for the PTY session
  CATERM_SOCKET    Override the default Unix socket path
  SHELL            Fallback shell if CATERM_SHELL is not set
  RUST_LOG         Configure tracing output"
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
