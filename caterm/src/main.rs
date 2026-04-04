mod config;
mod pty;
mod relay;
mod session_manager;

use std::env;
use std::fs::OpenOptions;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use tokio::io::{self, AsyncWriteExt};
use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;

use crate::config::DaemonConfig;
use crate::session_manager::{
    ClientOptions, CommandResponse, CommandResult, DaemonStatus, EventEnvelope, PROTOCOL_VERSION,
    ServerEvent, ServerSnapshot, SessionManagerServer, SessionRequest, attach_client_stream,
    cleanup_stale_socket, daemon_status, default_socket_path, is_server_running,
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

    let config = DaemonConfig::from_env();
    let _tracing_guard = init_tracing(&config)?;
    let shell = config.shell.clone();
    let explicit_foreground = command.foreground;
    let daemonize = command.daemonize;
    let client_options = ClientOptions {
        socket_path: command
            .socket_path
            .clone()
            .unwrap_or_else(default_socket_path),
    };

    match command.mode {
        Mode::Start => {
            if daemonize && !explicit_foreground {
                return run_daemonized_start(&client_options);
            }
            if is_server_running(&client_options.socket_path).await {
                bail!(
                    "Caterm server is already running at {}",
                    client_options.socket_path.display()
                );
            }
            cleanup_stale_socket(&client_options.socket_path).await?;

            info!(shell = %shell, socket = %client_options.socket_path.display(), "starting Caterm server");
            let mut server = SessionManagerServer::new(config, client_options.socket_path);
            server.run().await
        }
        Mode::Status => run_status_command(&client_options).await,
        Mode::Attach {
            session,
            window,
            pane,
        } => run_attach_command(&client_options, session, window, pane).await,
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
        Mode::SplitHorizontal {
            session,
            window,
            name,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::SplitHorizontal {
                    session,
                    window,
                    name,
                },
            )
            .await
        }
        Mode::SplitVertical {
            session,
            window,
            name,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::SplitVertical {
                    session,
                    window,
                    name,
                },
            )
            .await
        }
        Mode::SelectWindow { session, target } => {
            run_client_command(
                &client_options,
                SessionRequest::SelectWindow { session, target },
            )
            .await
        }
        Mode::SelectPane {
            session,
            window,
            target,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::SelectPane {
                    session,
                    window,
                    target,
                },
            )
            .await
        }
        Mode::RenameSession { target, name } => {
            run_client_command(
                &client_options,
                SessionRequest::RenameSession { target, name },
            )
            .await
        }
        Mode::RenameWindow {
            session,
            target,
            name,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::RenameWindow {
                    session,
                    target,
                    name,
                },
            )
            .await
        }
        Mode::RenamePane {
            session,
            window,
            target,
            name,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::RenamePane {
                    session,
                    window,
                    target,
                    name,
                },
            )
            .await
        }
        Mode::ResizePane {
            session,
            window,
            target,
            delta,
        } => {
            run_client_command(
                &client_options,
                SessionRequest::ResizePane {
                    session,
                    window,
                    target,
                    delta,
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

fn init_tracing(config: &DaemonConfig) -> Result<Option<WorkerGuard>> {
    let filter = env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact();

    if let Some(path) = &config.log_file {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        let (writer, guard) = tracing_appender::non_blocking(file);
        builder.with_writer(writer).init();
        Ok(Some(guard))
    } else {
        builder.init();
        Ok(None)
    }
}

#[derive(Debug, Default)]
struct Command {
    mode: Mode,
    socket_path: Option<std::path::PathBuf>,
    version: bool,
    foreground: bool,
    daemonize: bool,
}

#[derive(Debug, Default, Clone)]
enum Mode {
    Start,
    Status,
    Attach {
        session: Option<String>,
        window: Option<String>,
        pane: Option<String>,
    },
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
    SplitHorizontal {
        session: String,
        window: String,
        name: Option<String>,
    },
    SplitVertical {
        session: String,
        window: String,
        name: Option<String>,
    },
    SelectWindow {
        session: String,
        target: String,
    },
    SelectPane {
        session: String,
        window: String,
        target: String,
    },
    RenameSession {
        target: String,
        name: String,
    },
    RenameWindow {
        session: String,
        target: String,
        name: String,
    },
    RenamePane {
        session: String,
        window: String,
        target: String,
        name: String,
    },
    ResizePane {
        session: String,
        window: String,
        target: String,
        delta: i16,
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
            foreground: false,
            daemonize: false,
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
                "--foreground" => command.foreground = true,
                "--daemonize" => command.daemonize = true,
                "start" if !mode_set => {
                    command.mode = Mode::Start;
                    mode_set = true;
                }
                "status" if !mode_set => {
                    command.mode = Mode::Status;
                    mode_set = true;
                }
                "attach" if !mode_set => {
                    let session = iter.next();
                    let window = iter.next();
                    let pane = iter.next();
                    if iter.next().is_some() {
                        bail!("attach accepts at most a session, window, and pane target");
                    }
                    command.mode = Mode::Attach {
                        session,
                        window,
                        pane,
                    };
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
                "split-horizontal" if !mode_set => {
                    let session = iter
                        .next()
                        .context("split-horizontal requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("split-horizontal requires a window id or name")?;
                    let name = iter.next();
                    command.mode = Mode::SplitHorizontal {
                        session,
                        window,
                        name,
                    };
                    mode_set = true;
                }
                "split-vertical" if !mode_set => {
                    let session = iter
                        .next()
                        .context("split-vertical requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("split-vertical requires a window id or name")?;
                    let name = iter.next();
                    command.mode = Mode::SplitVertical {
                        session,
                        window,
                        name,
                    };
                    mode_set = true;
                }
                "select-window" if !mode_set => {
                    let session = iter
                        .next()
                        .context("select-window requires a session id or name")?;
                    let target = iter
                        .next()
                        .context("select-window requires a window id or name")?;
                    command.mode = Mode::SelectWindow { session, target };
                    mode_set = true;
                }
                "select-pane" if !mode_set => {
                    let session = iter
                        .next()
                        .context("select-pane requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("select-pane requires a window id or name")?;
                    let target = iter
                        .next()
                        .context("select-pane requires a pane id or name")?;
                    command.mode = Mode::SelectPane {
                        session,
                        window,
                        target,
                    };
                    mode_set = true;
                }
                "rename-session" if !mode_set => {
                    let target = iter
                        .next()
                        .context("rename-session requires a session id or name")?;
                    let name = iter.next().context("rename-session requires a new name")?;
                    command.mode = Mode::RenameSession { target, name };
                    mode_set = true;
                }
                "rename-window" if !mode_set => {
                    let session = iter
                        .next()
                        .context("rename-window requires a session id or name")?;
                    let target = iter
                        .next()
                        .context("rename-window requires a window id or name")?;
                    let name = iter.next().context("rename-window requires a new name")?;
                    command.mode = Mode::RenameWindow {
                        session,
                        target,
                        name,
                    };
                    mode_set = true;
                }
                "rename-pane" if !mode_set => {
                    let session = iter
                        .next()
                        .context("rename-pane requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("rename-pane requires a window id or name")?;
                    let target = iter
                        .next()
                        .context("rename-pane requires a pane id or name")?;
                    let name = iter.next().context("rename-pane requires a new name")?;
                    command.mode = Mode::RenamePane {
                        session,
                        window,
                        target,
                        name,
                    };
                    mode_set = true;
                }
                "resize-pane" if !mode_set => {
                    let session = iter
                        .next()
                        .context("resize-pane requires a session id or name")?;
                    let window = iter
                        .next()
                        .context("resize-pane requires a window id or name")?;
                    let target = iter
                        .next()
                        .context("resize-pane requires a pane id or name")?;
                    let delta = iter
                        .next()
                        .context("resize-pane requires a signed delta")?
                        .parse::<i16>()
                        .context("resize-pane delta must be an integer")?;
                    command.mode = Mode::ResizePane {
                        session,
                        window,
                        target,
                        delta,
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

        if command.foreground && command.daemonize {
            bail!("--foreground and --daemonize cannot be used together");
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
  attach           Subscribe to live daemon events for an optional session/window/pane target
  new-session      Create a session with an initial window and pane
  new-window       Create a window in a session
  new-pane         Create a pane in a window
  split-horizontal Split a window into horizontal panes
  split-vertical   Split a window into vertical panes
  select-window    Select the active window in a session
  select-pane      Select the active pane in a window
  rename-session   Rename a session
  rename-window    Rename a window in a session
  rename-pane      Rename a pane in a window
  resize-pane      Resize a pane by a signed delta within its window
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
  --foreground     Run `start` in the foreground explicitly
  --daemonize      Run `start` as a detached background process

Environment:
  CATERM_SHELL     Override the shell used for the PTY session
  CATERM_SOCKET    Override the default Unix socket path
  CATERM_LOG_FILE  Write daemon logs to the given file
  SHELL            Fallback shell if CATERM_SHELL is not set
  RUST_LOG         Configure tracing output"
    )
}

async fn run_client_command(options: &ClientOptions, request: SessionRequest) -> Result<()> {
    let response = send_client_request(options, request).await?;
    let mut stdout = io::stdout();
    let rendered = render_command_response(&response);

    if !rendered.is_empty() {
        stdout.write_all(rendered.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
    }
    stdout.flush().await?;

    if response.ok {
        Ok(())
    } else {
        let message = response
            .error
            .unwrap_or_else(|| "request failed".to_string());
        bail!("{message}")
    }
}

async fn run_status_command(options: &ClientOptions) -> Result<()> {
    let mut stdout = io::stdout();

    let message = match daemon_status(&options.socket_path).await? {
        DaemonStatus::Running { pid } => match pid {
            Some(pid) => format!(
                "Caterm daemon is running on {} (pid {})",
                options.socket_path.display(),
                pid
            ),
            None => format!(
                "Caterm daemon is running on {}",
                options.socket_path.display()
            ),
        },
        DaemonStatus::Stale {
            pid,
            metadata_path,
            socket_exists,
        } => {
            if pid > 0 {
                format!(
                    "Caterm daemon is not responding. Stale pid {} recorded in {} for socket {}{}",
                    pid,
                    metadata_path.display(),
                    options.socket_path.display(),
                    if socket_exists {
                        ""
                    } else {
                        " (socket already missing)"
                    }
                )
            } else {
                format!(
                    "Caterm daemon is not responding, but a stale socket exists at {}",
                    options.socket_path.display()
                )
            }
        }
        DaemonStatus::Stopped => format!(
            "Caterm daemon is not running. Expected socket: {}",
            options.socket_path.display()
        ),
    };

    stdout.write_all(message.as_bytes()).await?;
    stdout.write_all(b"\n").await?;
    stdout.flush().await?;

    Ok(())
}

fn run_daemonized_start(options: &ClientOptions) -> Result<()> {
    let current_exe = env::current_exe().context("failed to locate current executable")?;
    let mut child = std::process::Command::new(current_exe);
    child
        .arg("start")
        .arg("--foreground")
        .arg("--socket")
        .arg(&options.socket_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let child = child
        .spawn()
        .context("failed to launch background daemon")?;
    println!(
        "Started Caterm daemon in background on {} (pid {})",
        options.socket_path.display(),
        child.id()
    );
    Ok(())
}

async fn run_attach_command(
    options: &ClientOptions,
    session: Option<String>,
    window: Option<String>,
    pane: Option<String>,
) -> Result<()> {
    let mut lines = attach_client_stream(
        options,
        SessionRequest::Attach {
            session,
            window,
            pane,
        },
    )
    .await?;
    let mut stdout = io::stdout();
    let mut saw_event = false;

    while let Some(line) = lines.next_line().await? {
        let envelope: EventEnvelope = serde_json::from_str(&line)?;
        if envelope.protocol_version != PROTOCOL_VERSION {
            bail!(
                "protocol version mismatch: daemon={} client={}",
                envelope.protocol_version,
                PROTOCOL_VERSION
            );
        }
        let event = envelope.event;
        if !saw_event {
            if let ServerEvent::Error { message, .. } = &event {
                bail!("{message}");
            }
            saw_event = true;
        }
        let rendered = render_event(&event);
        if rendered.is_empty() {
            continue;
        }

        stdout.write_all(rendered.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

fn render_command_response(response: &CommandResponse) -> String {
    if !response.ok {
        return response
            .error
            .clone()
            .unwrap_or_else(|| "request failed".to_string());
    }

    match &response.result {
        Some(result) => render_command_result(result),
        None => String::new(),
    }
}

fn render_command_result(result: &CommandResult) -> String {
    match result {
        CommandResult::SessionCreated {
            session,
            initial_window: _,
            initial_pane,
        } => format!(
            "Created session {} ({})\nCreated pane {}:{} ({})",
            session.id, session.name, initial_pane.index, initial_pane.id, initial_pane.name
        ),
        CommandResult::WindowCreated {
            session_id,
            window,
            initial_pane,
        } => format!(
            "Created window {}:{} ({}) in session {}\nCreated pane {}:{} ({}) in session {}, window {}",
            window.index,
            window.id,
            window.name,
            session_id,
            initial_pane.index,
            initial_pane.id,
            initial_pane.name,
            session_id,
            window.id
        ),
        CommandResult::PaneCreated {
            session_id,
            window_id,
            pane,
        } => format!(
            "Created pane {}:{} ({}) in session {}, window {}",
            pane.index, pane.id, pane.name, session_id, window_id
        ),
        CommandResult::PaneSplit {
            session_id,
            window_id,
            layout,
            pane,
        } => format!(
            "Split window {} in session {} with {} layout; created pane {}:{} ({})",
            window_id, session_id, layout, pane.index, pane.id, pane.name
        ),
        CommandResult::WindowSelected { session_id, window } => format!(
            "Selected window {}:{} ({}) in session {}",
            window.index, window.id, window.name, session_id
        ),
        CommandResult::PaneSelected {
            session_id,
            window_id,
            pane,
        } => format!(
            "Selected pane {}:{} ({}) in session {}, window {}",
            pane.index, pane.id, pane.name, session_id, window_id
        ),
        CommandResult::SessionRenamed { session } => {
            format!("Renamed session {} to {}", session.id, session.name)
        }
        CommandResult::WindowRenamed { session_id, window } => format!(
            "Renamed window {}:{} to {} in session {}",
            window.index, window.id, window.name, session_id
        ),
        CommandResult::PaneRenamed {
            session_id,
            window_id,
            pane,
        } => format!(
            "Renamed pane {}:{} to {} in session {}, window {}",
            pane.index, pane.id, pane.name, session_id, window_id
        ),
        CommandResult::PaneResized {
            session_id,
            window_id,
            pane,
        } => format!(
            "Resized pane {}:{} ({}) to {}% in session {}, window {}",
            pane.index, pane.id, pane.name, pane.size, session_id, window_id
        ),
        CommandResult::SessionDeleted { session_id } => format!("Deleted session {}", session_id),
        CommandResult::WindowDeleted {
            session_id,
            window_id,
        } => format!("Deleted window {} from session {}", window_id, session_id),
        CommandResult::PaneDeleted {
            session_id,
            window_id,
            pane_id,
        } => format!(
            "Deleted pane {} from session {}, window {}",
            pane_id, session_id, window_id
        ),
        CommandResult::InputAccepted {
            session_id,
            window_id,
            pane_id,
        } => format!(
            "Accepted input for pane {} in session {}, window {}",
            pane_id, session_id, window_id
        ),
        CommandResult::SessionList { snapshot } => render_snapshot(snapshot),
        CommandResult::Stopped => "Stopped Caterm daemon".to_string(),
        CommandResult::Pong => "Pong".to_string(),
    }
}

fn render_event(event: &ServerEvent) -> String {
    match event {
        ServerEvent::SessionCreated { session } => {
            format!("Created session {} ({})", session.id, session.name)
        }
        ServerEvent::WindowCreated { session_id, window } => format!(
            "Created window {}:{} ({}) in session {}",
            window.index, window.id, window.name, session_id
        ),
        ServerEvent::PaneCreated {
            session_id,
            window_id,
            pane,
        } => format!(
            "Created pane {}:{} ({}) in session {}, window {}",
            pane.index, pane.id, pane.name, session_id, window_id
        ),
        ServerEvent::ActiveWindowChanged {
            session_id,
            window_id,
        } => format!(
            "Active window changed to {} in session {}",
            window_id, session_id
        ),
        ServerEvent::ActivePaneChanged {
            session_id,
            window_id,
            pane_id,
        } => format!(
            "Active pane changed to {} in session {}, window {}",
            pane_id, session_id, window_id
        ),
        ServerEvent::SessionDeleted { session_id } => {
            format!("Deleted session {}", session_id)
        }
        ServerEvent::WindowDeleted {
            session_id,
            window_id,
        } => format!("Deleted window {} from session {}", window_id, session_id),
        ServerEvent::PaneDeleted {
            session_id,
            window_id,
            pane_id,
        } => format!(
            "Deleted pane {} from session {}, window {}",
            pane_id, session_id, window_id
        ),
        ServerEvent::PtyOutput { data, .. } => data.trim_end_matches(['\r', '\n']).to_string(),
        ServerEvent::PtyOutputSnapshot { data, .. } => {
            data.trim_end_matches(['\r', '\n']).to_string()
        }
        ServerEvent::PaneExited {
            session_id,
            window_id,
            pane_id,
            exit_code,
        } => format!(
            "Pane {} in session {}, window {} exited with code {}",
            pane_id, session_id, window_id, exit_code
        ),
        ServerEvent::SessionList { sessions } => render_snapshot(&ServerSnapshot {
            active_session_id: None,
            active_window_id: None,
            active_pane_id: None,
            sessions: sessions.clone(),
        }),
        ServerEvent::Snapshot { snapshot } => render_snapshot(snapshot),
        ServerEvent::Pong => "Pong".to_string(),
        ServerEvent::Error { message, .. } => format!("Error: {message}"),
    }
}

fn render_snapshot(snapshot: &ServerSnapshot) -> String {
    if snapshot.sessions.is_empty() {
        return "No active sessions".to_string();
    }

    let mut lines = Vec::new();
    if snapshot.active_session_id.is_some()
        || snapshot.active_window_id.is_some()
        || snapshot.active_pane_id.is_some()
    {
        lines.push(format!(
            "client_active session={} window={} pane={}",
            snapshot
                .active_session_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            snapshot
                .active_window_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            snapshot
                .active_pane_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
    }
    lines.push("Sessions:".to_string());
    for session in &snapshot.sessions {
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
                "  window {}:{} ({}) layout={} active_pane={}",
                window.index,
                window.id,
                window.name,
                window.layout,
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
                    "    pane {}:{} ({}) size={} shell={}{}",
                    pane.index, pane.id, pane.name, pane.size, pane.shell, exit_suffix
                ));
            }
        }
    }

    lines.join("\n")
}
