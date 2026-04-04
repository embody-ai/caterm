mod auth;
mod routing;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use http::StatusCode;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{ErrorResponse, Request, Response},
    },
};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::auth::{ConnectionRole, RelayAuth};
use crate::routing::{MessageTx, RoutingTable};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let config = RelayConfig::from_env();
    let state = Arc::new(AppState {
        auth: RelayAuth::from_env(),
        routing: RoutingTable::default(),
    });

    let listener = TcpListener::bind(config.listen_addr).await?;
    info!(listen_addr = %config.listen_addr, "starting Caterm relay server");

    loop {
        let (stream, remote_addr) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(state, stream).await {
                warn!(%remote_addr, ?error, "relay connection failed");
            }
        });
    }
}

#[derive(Clone)]
struct AppState {
    auth: RelayAuth,
    routing: RoutingTable,
}

#[derive(Debug, Clone)]
struct RelayConfig {
    listen_addr: SocketAddr,
}

impl RelayConfig {
    fn from_env() -> Self {
        let listen_addr = std::env::var("CATERM_RELAY_LISTEN_ADDR")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 8080)));

        Self { listen_addr }
    }
}

#[derive(Debug, Clone)]
struct HandshakeContext {
    session_id: String,
    role: ConnectionRole,
}

async fn handle_connection(state: Arc<AppState>, stream: TcpStream) -> Result<()> {
    let context = Arc::new(Mutex::new(None::<HandshakeContext>));
    let callback_context = context.clone();
    let callback_state = state.clone();

    let socket = accept_hdr_async(stream, move |request: &Request, response: Response| {
        let parsed = parse_request(request, &callback_state.auth).map_err(to_error_response)?;
        *callback_context.lock().expect("handshake context poisoned") = Some(parsed);
        Ok(response)
    })
    .await?;

    let context = context
        .lock()
        .expect("handshake context poisoned")
        .clone()
        .context("missing handshake context")?;

    run_socket(
        socket,
        state.routing.clone(),
        context.role,
        context.session_id,
    )
    .await
}

async fn run_socket(
    socket: tokio_tungstenite::WebSocketStream<TcpStream>,
    routing: RoutingTable,
    role: ConnectionRole,
    session_id: String,
) -> Result<()> {
    let (mut writer, mut reader) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let client_id = register(&routing, role, &session_id, tx).await;

    let writer_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if writer.send(message).await.is_err() {
                break;
            }
        }
    });

    while let Some(frame) = reader.next().await {
        let message = match frame {
            Ok(message) => message,
            Err(error) => {
                warn!(%session_id, ?role, ?error, "relay websocket receive failed");
                break;
            }
        };

        if matches!(message, Message::Close(_)) {
            break;
        }

        let recipients = match role {
            ConnectionRole::Daemon => routing.client_txs(&session_id).await,
            ConnectionRole::Client => routing.daemon_tx(&session_id).await.into_iter().collect(),
        };

        debug!(%session_id, ?role, recipients = recipients.len(), "forwarding relay frame");
        for tx in recipients {
            let _ = tx.send(message.clone());
        }
    }

    unregister(&routing, role, &session_id, client_id).await;
    writer_task.abort();
    Ok(())
}

async fn register(
    routing: &RoutingTable,
    role: ConnectionRole,
    session_id: &str,
    tx: MessageTx,
) -> Option<Uuid> {
    match role {
        ConnectionRole::Daemon => {
            routing.attach_daemon(session_id, tx).await;
            None
        }
        ConnectionRole::Client => Some(routing.attach_client(session_id, tx).await),
    }
}

async fn unregister(
    routing: &RoutingTable,
    role: ConnectionRole,
    session_id: &str,
    client_id: Option<Uuid>,
) {
    match role {
        ConnectionRole::Daemon => routing.detach_daemon(session_id).await,
        ConnectionRole::Client => {
            if let Some(client_id) = client_id {
                routing.detach_client(session_id, client_id).await;
            }
        }
    }
}

fn parse_request(
    request: &Request,
    auth: &RelayAuth,
) -> Result<HandshakeContext, (StatusCode, String)> {
    let path = request.uri().path();
    let session_id = path
        .strip_prefix("/ws/")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                "expected /ws/{session_id}".to_string(),
            )
        })?
        .to_string();

    let mut role = None;
    let mut token = None;

    if let Some(query) = request.uri().query() {
        for pair in query.split('&').filter(|entry| !entry.is_empty()) {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next().unwrap_or_default();
            let value = parts.next().unwrap_or_default().to_string();

            match key {
                "role" => role = Some(value),
                "token" => token = Some(value),
                _ => {}
            }
        }
    }

    let role = match role.as_deref() {
        Some("daemon") => ConnectionRole::Daemon,
        Some("client") => ConnectionRole::Client,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "role must be either 'daemon' or 'client'".to_string(),
            ));
        }
    };

    auth.authorize(role, token.as_deref())
        .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;

    Ok(HandshakeContext { session_id, role })
}

fn to_error_response((status, message): (StatusCode, String)) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(message));
    *response.status_mut() = status;
    response
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}
