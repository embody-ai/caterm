use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration, sleep};
use tracing::{info, warn};

use crate::config::DaemonConfig;
use crate::pty::PtySession;

use super::event::ServerEvent;
use super::pane::Pane;
use super::request::{ClientOptions, ServerResponse, SessionRequest};
use super::session::Session;
use super::snapshot::ServerSnapshot;
use super::window::Window;

pub(crate) type RequestTx = mpsc::UnboundedSender<RequestEnvelope>;

pub(crate) struct RequestEnvelope {
    pub kind: RequestKind,
}

pub(crate) enum RequestKind {
    Command {
        request: SessionRequest,
        response_tx: oneshot::Sender<ServerResponse>,
    },
    Attach {
        attached_tx: oneshot::Sender<AttachedClient>,
    },
    Detach {
        subscriber_id: u64,
    },
}

pub(crate) struct AttachedClient {
    pub subscriber_id: u64,
    pub events_rx: mpsc::UnboundedReceiver<ServerEvent>,
}

pub struct SessionManagerServer {
    config: DaemonConfig,
    socket_path: PathBuf,
    sessions: BTreeMap<u64, Session>,
    next_session_id: u64,
    next_window_id: u64,
    next_pane_id: u64,
    next_subscriber_id: u64,
    subscribers: BTreeMap<u64, mpsc::UnboundedSender<ServerEvent>>,
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
            next_subscriber_id: 1,
            subscribers: BTreeMap::new(),
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

        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let listener_task = spawn_local_listener(listener, request_tx.clone());
        let relay_task = self
            .config
            .relay
            .clone()
            .map(|config| crate::relay::spawn_relay_client(config, request_tx.clone()));
        let mut runtime_tick = time::interval(Duration::from_millis(100));

        loop {
            tokio::select! {
                request = request_rx.recv() => {
                    let Some(request) = request else {
                        break;
                    };

                    match request.kind {
                        RequestKind::Command { request, response_tx } => {
                            let should_stop = matches!(request, SessionRequest::Stop);
                            let response = match self.handle_request(request).await {
                                Ok(events) => {
                                    self.broadcast_events(&events);
                                    ServerResponse { ok: true, events }
                                }
                                Err(error) => ServerResponse {
                                    ok: false,
                                    events: vec![ServerEvent::Error {
                                        message: error.to_string(),
                                    }],
                                },
                            };

                            let _ = response_tx.send(response);
                            if should_stop {
                                break;
                            }
                        }
                        RequestKind::Attach { attached_tx } => {
                            let attached = self.attach_client();
                            let _ = attached_tx.send(attached);
                        }
                        RequestKind::Detach { subscriber_id } => {
                            self.subscribers.remove(&subscriber_id);
                        }
                    }
                }
                _ = runtime_tick.tick() => {
                    let events = self.collect_runtime_events()?;
                    if !events.is_empty() {
                        self.broadcast_events(&events);
                    }
                }
                signal_result = tokio::signal::ctrl_c() => {
                    signal_result?;
                    info!("received shutdown signal");
                    break;
                }
            }
        }

        listener_task.abort();
        if let Some(task) = relay_task {
            task.abort();
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

    async fn handle_request(&mut self, request: SessionRequest) -> Result<Vec<ServerEvent>> {
        let mut events = self.collect_runtime_events()?;

        match request {
            SessionRequest::Attach => {
                bail!("attach requires a streaming local connection");
            }
            SessionRequest::CreateSession { name } => {
                events.extend(self.create_session(name).await?)
            }
            SessionRequest::CreateWindow { session, name } => {
                events.extend(self.create_window(&session, name).await?)
            }
            SessionRequest::CreatePane {
                session,
                window,
                name,
            } => events.extend(self.create_pane(&session, &window, name).await?),
            SessionRequest::DeleteSession { target } => {
                events.extend(self.delete_session(&target).await?)
            }
            SessionRequest::DeleteWindow { session, target } => {
                events.extend(self.delete_window(&session, &target).await?)
            }
            SessionRequest::DeletePane {
                session,
                window,
                target,
            } => events.extend(self.delete_pane(&session, &window, &target).await?),
            SessionRequest::SendInput {
                session,
                window,
                pane,
                data,
            } => events.extend(self.send_input(&session, &window, &pane, &data).await?),
            SessionRequest::List => {
                let snapshot = self.snapshot();
                events.push(ServerEvent::SessionList {
                    sessions: snapshot.sessions.clone(),
                });
                events.push(ServerEvent::Snapshot { snapshot });
            }
            SessionRequest::Stop => events.push(ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            }),
            SessionRequest::Ping => events.push(ServerEvent::Pong),
        }

        Ok(events)
    }

    fn attach_client(&mut self) -> AttachedClient {
        let subscriber_id = self.next_subscriber_id;
        self.next_subscriber_id += 1;
        let (events_tx, mut events_rx) = mpsc::unbounded_channel();
        let snapshot = self.snapshot();
        let _ = events_tx.send(ServerEvent::Snapshot { snapshot });
        self.subscribers.insert(subscriber_id, events_tx);

        AttachedClient {
            subscriber_id,
            events_rx: {
                let (bridge_tx, bridge_rx) = mpsc::unbounded_channel();
                tokio::spawn(async move {
                    while let Some(event) = events_rx.recv().await {
                        if bridge_tx.send(event).is_err() {
                            break;
                        }
                    }
                });
                bridge_rx
            },
        }
    }

    async fn create_session(&mut self, name: Option<String>) -> Result<Vec<ServerEvent>> {
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
            name: session_name,
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

        let mut window = Window {
            id: window_id,
            index: window_index,
            name: window_name,
            panes: BTreeMap::new(),
            active_pane_id: Some(pane_id),
        };
        window.panes.insert(pane.id, pane);
        session.windows.insert(window.id, window);
        session.active_window_id = Some(window_id);

        self.sessions.insert(id, session);

        let session_snapshot = self
            .sessions
            .get(&id)
            .expect("created session missing")
            .snapshot();
        let pane_snapshot = self
            .sessions
            .get(&id)
            .and_then(|session| session.windows.get(&window_id))
            .and_then(|window| window.panes.get(&pane_id))
            .expect("created pane missing")
            .snapshot();

        Ok(vec![
            ServerEvent::SessionCreated {
                session: session_snapshot,
            },
            ServerEvent::PaneCreated {
                session_id: id,
                window_id,
                pane: pane_snapshot,
            },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn create_window(
        &mut self,
        session_target: &str,
        name: Option<String>,
    ) -> Result<Vec<ServerEvent>> {
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

        let mut window = Window {
            id: window_id,
            index: window_index,
            name: window_name,
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

        let window_snapshot = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.windows.get(&window_id))
            .expect("created window missing")
            .snapshot();
        let pane_snapshot = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.windows.get(&window_id))
            .and_then(|window| window.panes.get(&pane_id))
            .expect("created pane missing")
            .snapshot();

        Ok(vec![
            ServerEvent::WindowCreated {
                session_id,
                window: window_snapshot,
            },
            ServerEvent::PaneCreated {
                session_id,
                window_id,
                pane: pane_snapshot,
            },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn create_pane(
        &mut self,
        session_target: &str,
        window_target: &str,
        name: Option<String>,
    ) -> Result<Vec<ServerEvent>> {
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

        let pane = self.spawn_pane(pane_name, pane_index).await?;
        let window = self
            .sessions
            .get_mut(&session_id)
            .and_then(|session| session.windows.get_mut(&window_id))
            .ok_or_else(|| anyhow!("window {window_id} not found"))?;
        window.active_pane_id = Some(pane.id);
        window.panes.insert(pane.id, pane);

        let pane_snapshot = self
            .sessions
            .get(&session_id)
            .and_then(|session| session.windows.get(&window_id))
            .and_then(|window| window.panes.get(&pane_id))
            .expect("created pane missing")
            .snapshot();

        Ok(vec![
            ServerEvent::PaneCreated {
                session_id,
                window_id,
                pane: pane_snapshot,
            },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn spawn_pane(&mut self, name: String, index: u32) -> Result<Pane> {
        let id = self.next_pane_id;
        self.next_pane_id += 1;

        let mut pty = PtySession::spawn(&self.config.shell, self.config.cols, self.config.rows)?;
        let (output_tx, output_rx) = mpsc::unbounded_channel();
        pty.start_output_pump(output_tx).await?;

        Ok(Pane {
            id,
            index,
            name,
            shell: self.config.shell.clone(),
            pty,
            output_rx,
            exit_code: None,
        })
    }

    async fn delete_session(&mut self, target: &str) -> Result<Vec<ServerEvent>> {
        let id = self.resolve_session_id(target)?;
        let session = self
            .sessions
            .remove(&id)
            .ok_or_else(|| anyhow!("session {id} not found"))?;

        self.terminate_session(session).await;

        Ok(vec![
            ServerEvent::SessionDeleted { session_id: id },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn delete_window(
        &mut self,
        session_target: &str,
        target: &str,
    ) -> Result<Vec<ServerEvent>> {
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

        Ok(vec![
            ServerEvent::WindowDeleted {
                session_id,
                window_id,
            },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn delete_pane(
        &mut self,
        session_target: &str,
        window_target: &str,
        target: &str,
    ) -> Result<Vec<ServerEvent>> {
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

        Ok(vec![
            ServerEvent::PaneDeleted {
                session_id,
                window_id,
                pane_id,
            },
            ServerEvent::Snapshot {
                snapshot: self.snapshot(),
            },
        ])
    }

    async fn send_input(
        &mut self,
        session_target: &str,
        window_target: &str,
        pane_target: &str,
        data: &str,
    ) -> Result<Vec<ServerEvent>> {
        let session_id = self.resolve_session_id(session_target)?;
        let window_id = self.resolve_window_id(session_id, window_target)?;
        let pane_id = self.resolve_pane_id(session_id, window_id, pane_target)?;

        let pane = self
            .sessions
            .get_mut(&session_id)
            .and_then(|session| session.windows.get_mut(&window_id))
            .and_then(|window| window.panes.get_mut(&pane_id))
            .ok_or_else(|| anyhow!("pane {pane_id} not found"))?;
        if pane.exit_code.is_some() {
            bail!("pane {pane_id} has already exited");
        }

        pane.pty.write_all(data.as_bytes()).await?;
        sleep(Duration::from_millis(50)).await;

        let mut events = self.collect_runtime_events()?;
        events.push(ServerEvent::Snapshot {
            snapshot: self.snapshot(),
        });
        Ok(events)
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

    fn snapshot(&self) -> ServerSnapshot {
        ServerSnapshot {
            sessions: self.sessions.values().map(Session::snapshot).collect(),
        }
    }

    fn collect_runtime_events(&mut self) -> Result<Vec<ServerEvent>> {
        let mut events = Vec::new();

        for (session_id, session) in &mut self.sessions {
            for (window_id, window) in &mut session.windows {
                for (pane_id, pane) in &mut window.panes {
                    while let Ok(chunk) = pane.output_rx.try_recv() {
                        if !chunk.is_empty() {
                            events.push(ServerEvent::PtyOutput {
                                session_id: *session_id,
                                window_id: *window_id,
                                pane_id: *pane_id,
                                data: String::from_utf8_lossy(&chunk).into_owned(),
                            });
                        }
                    }

                    if pane.exit_code.is_none() {
                        if let Some(exit_code) = pane.pty.try_wait()? {
                            pane.exit_code = Some(exit_code);
                            events.push(ServerEvent::PaneExited {
                                session_id: *session_id,
                                window_id: *window_id,
                                pane_id: *pane_id,
                                exit_code,
                            });
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    fn broadcast_events(&mut self, events: &[ServerEvent]) {
        self.subscribers.retain(|_, tx| {
            for event in events {
                if tx.send(event.clone()).is_err() {
                    return false;
                }
            }
            true
        });
    }
}

fn spawn_local_listener(listener: UnixListener, request_tx: RequestTx) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(result) => result,
                Err(error) => {
                    warn!(%error, "failed to accept local connection");
                    break;
                }
            };

            let request_tx = request_tx.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_local_connection(stream, request_tx).await {
                    warn!(?error, "local client request failed");
                }
            });
        }
    })
}

async fn handle_local_connection(stream: UnixStream, request_tx: RequestTx) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    let Some(line) = lines.next_line().await? else {
        return Ok(());
    };

    let request = serde_json::from_str::<SessionRequest>(&line)
        .with_context(|| "failed to decode session request")?;

    if matches!(request, SessionRequest::Attach) {
        let (attached_tx, attached_rx) = oneshot::channel();
        request_tx
            .send(RequestEnvelope {
                kind: RequestKind::Attach { attached_tx },
            })
            .map_err(|_| anyhow!("server request loop has stopped"))?;

        let AttachedClient {
            subscriber_id,
            mut events_rx,
        } = attached_rx
            .await
            .map_err(|_| anyhow!("server request loop dropped attach response"))?;

        while let Some(event) = events_rx.recv().await {
            let payload = serde_json::to_string(&event)?;
            if writer.write_all(payload.as_bytes()).await.is_err() {
                break;
            }
            if writer.write_all(b"\n").await.is_err() {
                break;
            }
            if writer.flush().await.is_err() {
                break;
            }
        }

        let _ = request_tx.send(RequestEnvelope {
            kind: RequestKind::Detach { subscriber_id },
        });
        return Ok(());
    }

    let (response_tx, response_rx) = oneshot::channel();
    request_tx
        .send(RequestEnvelope {
            kind: RequestKind::Command {
                request,
                response_tx,
            },
        })
        .map_err(|_| anyhow!("server request loop has stopped"))?;

    let response = response_rx
        .await
        .map_err(|_| anyhow!("server request loop dropped the response"))?;

    let payload = serde_json::to_string(&response)?;
    writer.write_all(payload.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    Ok(())
}

pub async fn send_client_request(
    options: &ClientOptions,
    request: SessionRequest,
) -> Result<ServerResponse> {
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
    let response = serde_json::from_str::<ServerResponse>(&response_line)?;

    Ok(response)
}

pub async fn attach_client_stream(
    options: &ClientOptions,
) -> Result<tokio::io::Lines<BufReader<tokio::net::unix::OwnedReadHalf>>> {
    let stream = UnixStream::connect(&options.socket_path)
        .await
        .with_context(|| {
            format!(
                "failed to connect to Caterm server at {}",
                options.socket_path.display()
            )
        })?;
    let (reader, mut writer) = stream.into_split();
    let payload = serde_json::to_string(&SessionRequest::Attach)?;

    writer.write_all(payload.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    Ok(BufReader::new(reader).lines())
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
                "Starting Caterm server.\nSocket: {}\nUse `caterm new-session`, `caterm new-window`, `caterm new-pane`, `caterm send-input`, and related commands to manage the hierarchy.\n",
                socket_path.display()
            )
            .as_bytes(),
        )
        .await?;
    stdout.flush().await?;
    Ok(())
}
