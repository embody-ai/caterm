mod config;
mod pty;
mod relay;
mod session_manager;

use std::env;

use anyhow::{Context, Result, bail};
use tokio::io::{self, AsyncWriteExt};
use tracing::info;

use crate::config::DaemonConfig;
use crate::session_manager::{
    ClientOptions, ServerEvent, SessionManagerServer, SessionRequest, default_socket_path,
    is_server_running, send_client_request,
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
        Mode::Status => run_status_command(&client_options).await,
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
        Mode::SendInput {
            session,
            window,
            pane,
            data,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::SendInput {
                    session,
                    window,
                    pane,
                    data,
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
    Status,
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
    SendInput {
        session: String,
        window: String,
        pane: String,
        data: String,
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
                "status" if !mode_set => {
                    command.mode = Mode::Status;
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
                "send-input" if !mode_set => {
                    let session = iter
                        .next()
                        .context("send-input requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("send-input requires a window id or name")?;
                    let pane = iter
                        .next()
                        .context("send-input requires a pane id or name")?;
                    let rest: Vec<String> = iter.by_ref().collect();
                    if rest.is_empty() {
                        bail!("send-input requires input data");
                    }
                    command.mode = Mode::SendInput {
                        session,
                        window,
                        pane,
                        data: rest.join(" "),
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
  status           Show whether the local Caterm server is running
  new-session      Create a session with an initial window and pane
  new-window       Create a window in a session
  new-pane         Create a pane in a window
  kill-session     Delete a session by id or name
  kill-window      Delete a window by id or name within a session
  kill-pane        Delete a pane by id or name within a window
  send-input       Send input bytes to a pane PTY
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
    let response = send_client_request(options, request.clone()).await?;
    let mut stdout = io::stdout();
    let rendered = render_response(&request, &response.events);

    if !rendered.is_empty() {
        stdout.write_all(rendered.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
    }
    stdout.flush().await?;

    if response.ok {
        Ok(())
    } else {
        let message = response
            .events
            .into_iter()
            .find_map(|event| match event {
                ServerEvent::Error { message } => Some(message),
                _ => None,
            })
            .unwrap_or_else(|| "request failed".to_string());
        bail!("{message}")
    }
}

async fn run_status_command(options: &ClientOptions) -> Result<()> {
    let socket_exists = options.socket_path.exists();
    let running = is_server_running(&options.socket_path).await;
    let mut stdout = io::stdout();

    let message = if running {
        format!(
            "Caterm daemon is running on {}",
            options.socket_path.display()
        )
    } else if socket_exists {
        format!(
            "Caterm daemon is not responding, but a stale socket exists at {}",
            options.socket_path.display()
        )
    } else {
        format!(
            "Caterm daemon is not running. Expected socket: {}",
            options.socket_path.display()
        )
    };

    stdout.write_all(message.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;

    Ok(())
}

fn render_response(request: &SessionRequest, events: &[ServerEvent]) -> String {
    let mut lines = Vec::new();
    let show_pty_output = matches!(request, SessionRequest::SendInput { .. });

    for event in events {
        match event {
            ServerEvent::SessionCreated { session } => {
                lines.push(format!("Created session {} ({})", session.id, session.name));
            }
            ServerEvent::WindowCreated { session_id, window } => {
                lines.push(format!(
                    "Created window {}:{} ({}) in session {}",
                    window.index, window.id, window.name, session_id
                ));
            }
            ServerEvent::PaneCreated {
                session_id,
                window_id,
                pane,
            } => {
                lines.push(format!(
                    "Created pane {}:{} ({}) in session {}, window {}",
                    pane.index, pane.id, pane.name, session_id, window_id
                ));
            }
            ServerEvent::SessionDeleted { session_id } => {
                lines.push(format!("Deleted session {}", session_id));
            }
            ServerEvent::WindowDeleted {
                session_id,
                window_id,
            } => {
                lines.push(format!(
                    "Deleted window {} from session {}",
                    window_id, session_id
                ));
            }
            ServerEvent::PaneDeleted {
                session_id,
                window_id,
                pane_id,
            } => {
                lines.push(format!(
                    "Deleted pane {} from session {}, window {}",
                    pane_id, session_id, window_id
                ));
            }
            ServerEvent::PtyOutput { data, .. } => {
                if show_pty_output {
                    let trimmed = data.trim_end_matches(['\r', '\n']);
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                }
            }
            ServerEvent::PaneExited {
                session_id,
                window_id,
                pane_id,
                exit_code,
            } => {
                lines.push(format!(
                    "Pane {} in session {}, window {} exited with code {}",
                    pane_id, session_id, window_id, exit_code
                ));
            }
            ServerEvent::SessionList { sessions } => {
                if sessions.is_empty() {
                    lines.push("No active sessions".to_string());
                } else {
                    lines.push("Sessions:".to_string());
                    for session in sessions {
                        lines.push(format!(
                            "session {} ({}) active_window={}",
                            session.id,
                            session.name,
                            session
                                .active_window_index
                                .map(|index| index.to_string())
                                .unwrap_or_else(|| "-".to_string())
                        ));
                        for window in &session.windows {
                            lines.push(format!(
                                "  window {}:{} ({}) active_pane={}",
                                window.index,
                                window.id,
                                window.name,
                                window
                                    .active_pane_index
                                    .map(|index| index.to_string())
                                    .unwrap_or_else(|| "-".to_string())
                            ));
                            for pane in &window.panes {
                                let exit_suffix = pane
                                    .exit_code
                                    .map(|code| format!(" exit={code}"))
                                    .unwrap_or_default();
                                lines.push(format!(
                                    "    pane {}:{} ({}) shell={}{}",
                                    pane.index, pane.id, pane.name, pane.shell, exit_suffix
                                ));
                            }
                        }
                    }
                }
            }
            ServerEvent::Snapshot { .. } => {}
            ServerEvent::Pong => {
                lines.push("Pong".to_string());
            }
            ServerEvent::Error { message } => {
                lines.push(format!("Error: {message}"));
            }
        }
    }

    lines.join("\n")
}
