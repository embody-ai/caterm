use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::agent::AgentConfig;
use crate::pty::PtySession;

pub struct SessionManagerServer {
    config: AgentConfig,
    socket_path: PathBuf,
    sessions: BTreeMap<u64, ManagedSession>,
    next_id: u64,
}

struct ManagedSession {
    id: u64,
    name: String,
    shell: String,
    pty: PtySession,
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub socket_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum SessionRequest {
    Create { name: Option<String> },
    Delete { target: String },
    List,
    Stop,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub ok: bool,
    pub message: String,
}

impl SessionManagerServer {
    pub fn new(config: AgentConfig, socket_path: PathBuf) -> Self {
        Self {
            config,
            socket_path,
            sessions: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        if self.socket_path.exists() {
            bail!(
                "socket already exists at {}. If the server is not running, remove the stale socket first",
                self.socket_path.display()
            );
        }

        if let Some(parent) = self.socket_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("failed to bind {}", self.socket_path.display()))?;

        info!(socket = %self.socket_path.display(), "caterm server started");
        print_server_banner(&self.socket_path).await?;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let should_stop = self.handle_connection(stream).await?;
                    if should_stop {
                        break;
                    }
                }
                signal_result = tokio::signal::ctrl_c() => {
                    signal_result?;
                    info!("received shutdown signal");
                    break;
                }
            }
        }

        self.shutdown_all().await?;

        match tokio::fs::remove_file(&self.socket_path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!(%error, socket = %self.socket_path.display(), "failed to remove socket")
            }
        }

        Ok(())
    }

    async fn handle_connection(&mut self, stream: UnixStream) -> Result<bool> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();

        let Some(line) = lines.next_line().await? else {
            return Ok(false);
        };

        let request = serde_json::from_str::<SessionRequest>(&line)
            .with_context(|| "failed to decode session request")?;
        let should_stop = matches!(request, SessionRequest::Stop);
        let response = match self.handle_request(request).await {
            Ok(message) => SessionResponse { ok: true, message },
            Err(error) => SessionResponse {
                ok: false,
                message: error.to_string(),
            },
        };

        let payload = serde_json::to_string(&response)?;
        writer.write_all(payload.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(should_stop)
    }

    async fn handle_request(&mut self, request: SessionRequest) -> Result<String> {
        match request {
            SessionRequest::Create { name } => self.create_session(name).await,
            SessionRequest::Delete { target } => self.delete_session(&target).await,
            SessionRequest::List => Ok(self.list_sessions()),
            SessionRequest::Stop => Ok("stopping Caterm server".to_string()),
            SessionRequest::Ping => Ok("pong".to_string()),
        }
    }

    async fn create_session(&mut self, name: Option<String>) -> Result<String> {
        let id = self.next_id;
        self.next_id += 1;

        let session_name = name.unwrap_or_else(|| format!("session-{id}"));
        if self
            .sessions
            .values()
            .any(|session| session.name == session_name)
        {
            bail!("session name already exists: {session_name}");
        }

        let mut pty = PtySession::spawn(&self.config.shell, self.config.cols, self.config.rows)?;
        pty.start_discard_output().await?;

        let session = ManagedSession {
            id,
            name: session_name.clone(),
            shell: self.config.shell.clone(),
            pty,
        };

        self.sessions.insert(id, session);

        Ok(format!("created session {id} ({session_name})"))
    }

    async fn delete_session(&mut self, target: &str) -> Result<String> {
        let id = self.resolve_session_id(target)?;
        let mut session = self
            .sessions
            .remove(&id)
            .ok_or_else(|| anyhow!("session {id} not found"))?;

        session.pty.terminate().await?;
        let _ = session.pty.wait().await;

        Ok(format!("deleted session {} ({})", session.id, session.name))
    }

    fn resolve_session_id(&self, target: &str) -> Result<u64> {
        if let Ok(id) = target.parse::<u64>() {
            return Ok(id);
        }

        self.sessions
            .values()
            .find(|session| session.name == target)
            .map(|session| session.id)
            .ok_or_else(|| anyhow!("session not found: {target}"))
    }

    fn list_sessions(&self) -> String {
        if self.sessions.is_empty() {
            return "no active sessions".to_string();
        }

        let mut lines = Vec::with_capacity(self.sessions.len() + 1);
        lines.push("active sessions:".to_string());

        for session in self.sessions.values() {
            lines.push(format!(
                "  {}  {}  {}",
                session.id, session.name, session.shell
            ));
        }

        lines.join("\n")
    }

    async fn shutdown_all(&mut self) -> Result<()> {
        let ids: Vec<u64> = self.sessions.keys().copied().collect();

        for id in ids {
            let _ = self.delete_session(&id.to_string()).await;
        }

        Ok(())
    }
}

pub async fn send_client_request(
    options: &ClientOptions,
    request: SessionRequest,
) -> Result<SessionResponse> {
    let stream = UnixStream::connect(&options.socket_path)
        .await
        .with_context(|| {
            format!(
                "failed to connect to Caterm server at {}",
                options.socket_path.display()
            )
        })?;
    let (reader, mut writer) = stream.into_split();
    let payload = serde_json::to_string(&request)?;

    writer.write_all(payload.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut lines = BufReader::new(reader).lines();
    let response_line = lines
        .next_line()
        .await?
        .context("server closed connection without responding")?;
    let response = serde_json::from_str::<SessionResponse>(&response_line)?;

    Ok(response)
}

pub fn default_socket_path() -> PathBuf {
    if let Ok(path) = env::var("CATERM_SOCKET") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    let user = env::var("USER").unwrap_or_else(|_| "default".to_string());
    env::temp_dir().join(format!("caterm-{user}.sock"))
}

pub async fn is_server_running(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }

    let options = ClientOptions {
        socket_path: socket_path.to_path_buf(),
    };

    matches!(
        send_client_request(&options, SessionRequest::Ping).await,
        Ok(response) if response.ok
    )
}

async fn print_server_banner(socket_path: &Path) -> Result<()> {
    let mut stdout = tokio::io::stdout();
    stdout
        .write_all(
            format!(
                "Starting Caterm server.\nSocket: {}\nUse `caterm create`, `caterm list`, and `caterm delete` to manage sessions.\n",
                socket_path.display()
            )
            .as_bytes(),
        )
        .await?;
    stdout.flush().await?;
    Ok(())
}
