use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::{info, warn};

use crate::config::DaemonConfig;
use crate::pty::PtySession;

use super::pane::Pane;
use super::request::{ClientOptions, SessionRequest, SessionResponse};
use super::session::Session;
use super::window::Window;

pub struct SessionManagerServer {
    config: DaemonConfig,
    socket_path: PathBuf,
    sessions: BTreeMap<u64, Session>,
    next_session_id: u64,
    next_window_id: u64,
    next_pane_id: u64,
}

impl SessionManagerServer {
    pub fn new(config: DaemonConfig, socket_path: PathBuf) -> Self {
        Self {
            config,
            socket_path,
            sessions: BTreeMap::new(),
            next_session_id: 1,
            next_window_id: 1,
            next_pane_id: 1,
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
            SessionRequest::CreateSession { name } => self.create_session(name).await,
            SessionRequest::CreateWindow { session, name } => {
                self.create_window(&session, name).await
            }
            SessionRequest::CreatePane {
                session,
                window,
                name,
            } => self.create_pane(&session, &window, name).await,
            SessionRequest::DeleteSession { target } => self.delete_session(&target).await,
            SessionRequest::DeleteWindow { session, target } => {
                self.delete_window(&session, &target).await
            }
            SessionRequest::DeletePane {
                session,
                window,
                target,
            } => self.delete_pane(&session, &window, &target).await,
            SessionRequest::List => Ok(self.list_sessions()),
            SessionRequest::Stop => Ok("stopping Caterm server".to_string()),
            SessionRequest::Ping => Ok("pong".to_string()),
        }
    }

    async fn create_session(&mut self, name: Option<String>) -> Result<String> {
        let id = self.next_session_id;
        self.next_session_id += 1;

        let session_name = name.unwrap_or_else(|| format!("session-{id}"));
        if self
            .sessions
            .values()
            .any(|session| session.name == session_name)
        {
            bail!("session name already exists: {session_name}");
        }

        let mut session = Session {
            id,
            name: session_name.clone(),
            windows: BTreeMap::new(),
            active_window_id: None,
        };

        let window_id = self.next_window_id;
        self.next_window_id += 1;
        let window_index = 0;
        let window_name = format!("window-{window_index}");

        let pane = self
            .spawn_pane(format!("pane-{}", self.next_pane_id), 0)
            .await?;
        let pane_id = pane.id;
        let pane_index = pane.index;
        let pane_name = pane.name.clone();

        let mut window = Window {
            id: window_id,
            index: window_index,
            name: window_name.clone(),
            panes: BTreeMap::new(),
            active_pane_id: Some(pane_id),
        };
        window.panes.insert(pane.id, pane);
        session.windows.insert(window.id, window);
        session.active_window_id = Some(window_id);

        self.sessions.insert(id, session);

        Ok(format!(
            "created session {id} ({session_name}) with window {window_index}:{window_id} ({window_name}) and pane {pane_index}:{pane_id} ({pane_name})"
        ))
    }

    async fn create_window(
        &mut self,
        session_target: &str,
        name: Option<String>,
    ) -> Result<String> {
        let session_id = self.resolve_session_id(session_target)?;
        let window_id = self.next_window_id;
        self.next_window_id += 1;
        let window_index = self
            .sessions
            .get(&session_id)
            .ok_or_else(|| anyhow!("session {session_id} not found"))?
            .next_window_index();
        let window_name = name.unwrap_or_else(|| format!("window-{window_index}"));

        {
            let session = self
                .sessions
                .get(&session_id)
                .ok_or_else(|| anyhow!("session {session_id} not found"))?;
            if session
                .windows
                .values()
                .any(|window| window.name == window_name)
            {
                bail!("window name already exists in session {session_id}: {window_name}");
            }
        }

        let pane = self
            .spawn_pane(format!("pane-{}", self.next_pane_id), 0)
            .await?;
        let pane_id = pane.id;
        let pane_index = pane.index;
        let pane_name = pane.name.clone();

        let mut window = Window {
            id: window_id,
            index: window_index,
            name: window_name.clone(),
            panes: BTreeMap::new(),
            active_pane_id: Some(pane_id),
        };
        window.panes.insert(pane.id, pane);

        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;
        session.windows.insert(window.id, window);
        session.active_window_id = Some(window_id);

        Ok(format!(
            "created window {window_index}:{window_id} ({window_name}) in session {session_id} with pane {pane_index}:{pane_id} ({pane_name})"
        ))
    }

    async fn create_pane(
        &mut self,
        session_target: &str,
        window_target: &str,
        name: Option<String>,
    ) -> Result<String> {
        let session_id = self.resolve_session_id(session_target)?;
        let window_id = self.resolve_window_id(session_id, window_target)?;
        let pane_id = self.next_pane_id;
        let pane_name = name.unwrap_or_else(|| format!("pane-{pane_id}"));
        let pane_index = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.windows.get(&window_id))
            .ok_or_else(|| anyhow!("window {window_id} not found"))?
            .next_pane_index();

        {
            let window = self
                .sessions
                .get(&session_id)
                .and_then(|session| session.windows.get(&window_id))
                .ok_or_else(|| anyhow!("window {window_id} not found"))?;
            if window.panes.values().any(|pane| pane.name == pane_name) {
                bail!("pane name already exists in window {window_id}: {pane_name}");
            }
        }

        let pane = self.spawn_pane(pane_name.clone(), pane_index).await?;
        let window = self
            .sessions
            .get_mut(&session_id)
            .and_then(|session| session.windows.get_mut(&window_id))
            .ok_or_else(|| anyhow!("window {window_id} not found"))?;
        window.panes.insert(pane.id, pane);
        window.active_pane_id = Some(pane_id);

        Ok(format!(
            "created pane {pane_index}:{pane_id} ({pane_name}) in session {session_id}, window {window_id}"
        ))
    }

    async fn spawn_pane(&mut self, name: String, index: u32) -> Result<Pane> {
        let id = self.next_pane_id;
        self.next_pane_id += 1;

        let mut pty = PtySession::spawn(&self.config.shell, self.config.cols, self.config.rows)?;
        pty.start_discard_output().await?;

        Ok(Pane {
            id,
            index,
            name,
            shell: self.config.shell.clone(),
            pty,
        })
    }

    async fn delete_session(&mut self, target: &str) -> Result<String> {
        let id = self.resolve_session_id(target)?;
        let session = self
            .sessions
            .remove(&id)
            .ok_or_else(|| anyhow!("session {id} not found"))?;

        self.terminate_session(session).await;

        Ok(format!("deleted session {id}"))
    }

    async fn delete_window(&mut self, session_target: &str, target: &str) -> Result<String> {
        let session_id = self.resolve_session_id(session_target)?;
        let window_id = self.resolve_window_id(session_id, target)?;
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;

        let window = session
            .windows
            .remove(&window_id)
            .ok_or_else(|| anyhow!("window {window_id} not found"))?;
        if session.active_window_id == Some(window_id) {
            session.active_window_id = session.windows.keys().next().copied();
        }

        self.terminate_window(window).await;

        Ok(format!(
            "deleted window {window_id} from session {session_id}"
        ))
    }

    async fn delete_pane(
        &mut self,
        session_target: &str,
        window_target: &str,
        target: &str,
    ) -> Result<String> {
        let session_id = self.resolve_session_id(session_target)?;
        let window_id = self.resolve_window_id(session_id, window_target)?;
        let pane_id = self.resolve_pane_id(session_id, window_id, target)?;

        let window = self
            .sessions
            .get_mut(&session_id)
            .and_then(|session| session.windows.get_mut(&window_id))
            .ok_or_else(|| anyhow!("window {window_id} not found"))?;

        let mut pane = window
            .panes
            .remove(&pane_id)
            .ok_or_else(|| anyhow!("pane {pane_id} not found"))?;
        if window.active_pane_id == Some(pane_id) {
            window.active_pane_id = window.panes.keys().next().copied();
        }
        pane.pty.terminate().await?;
        let _ = pane.pty.wait().await;

        Ok(format!(
            "deleted pane {pane_id} from session {session_id}, window {window_id}"
        ))
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

    fn resolve_window_id(&self, session_id: u64, target: &str) -> Result<u64> {
        let session = self
            .sessions
            .get(&session_id)
            .ok_or_else(|| anyhow!("session {session_id} not found"))?;

        if let Ok(id) = target.parse::<u64>() {
            if session.windows.contains_key(&id) {
                return Ok(id);
            }
        }

        session
            .windows
            .values()
            .find(|window| window.matches_target(target))
            .map(|window| window.id)
            .ok_or_else(|| anyhow!("window not found in session {session_id}: {target}"))
    }

    fn resolve_pane_id(&self, session_id: u64, window_id: u64, target: &str) -> Result<u64> {
        let window = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.windows.get(&window_id))
            .ok_or_else(|| anyhow!("window {window_id} not found"))?;

        if let Ok(id) = target.parse::<u64>() {
            if window.panes.contains_key(&id) {
                return Ok(id);
            }
        }

        window
            .panes
            .values()
            .find(|pane| pane.matches_target(target))
            .map(|pane| pane.id)
            .ok_or_else(|| anyhow!("pane not found in window {window_id}: {target}"))
    }

    fn list_sessions(&self) -> String {
        if self.sessions.is_empty() {
            return "no active sessions".to_string();
        }

        let mut lines = Vec::new();
        lines.push("sessions:".to_string());

        for session in self.sessions.values() {
            lines.push(format!(
                "session {} ({}) active_window={}",
                session.id,
                session.name,
                session
                    .active_window_id
                    .and_then(|id| session.windows.get(&id))
                    .map(|window| window.index.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            for window in session.windows.values() {
                lines.push(format!(
                    "  window {}:{} ({}) active_pane={}",
                    window.index,
                    window.id,
                    window.name,
                    window
                        .active_pane_id
                        .and_then(|id| window.panes.get(&id))
                        .map(|pane| pane.index.to_string())
                        .unwrap_or_else(|| "-".to_string())
                ));
                for pane in window.panes.values() {
                    lines.push(format!(
                        "    pane {}:{} ({}) shell={}",
                        pane.index, pane.id, pane.name, pane.shell
                    ));
                }
            }
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

    async fn terminate_session(&self, session: Session) {
        for (_, window) in session.windows {
            self.terminate_window(window).await;
        }
    }

    async fn terminate_window(&self, window: Window) {
        for (_, mut pane) in window.panes {
            let _ = pane.pty.terminate().await;
            let _ = pane.pty.wait().await;
        }
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
                "Starting Caterm server.\nSocket: {}\nUse `caterm new-session`, `caterm new-window`, `caterm new-pane`, and related commands to manage the hierarchy.\n",
                socket_path.display()
            )
            .as_bytes(),
        )
        .await?;
    stdout.flush().await?;
    Ok(())
}
