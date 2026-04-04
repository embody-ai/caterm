use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

use crate::config::RelayClientConfig;
use crate::session_manager::{
    CommandResponse, RequestEnvelope, RequestKind, RequestTx, SessionRequest,
};

pub fn spawn_relay_client(config: RelayClientConfig, request_tx: RequestTx) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);

        loop {
            match run_connection(&config, request_tx.clone()).await {
                Ok(()) => backoff = Duration::from_secs(1),
                Err(error) => {
                    warn!(
                        session_id = %config.session_id,
                        relay_url = %config.url,
                        ?error,
                        "relay client disconnected"
                    );
                    sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, Duration::from_secs(30));
                }
            }
        }
    })
}

async fn run_connection(config: &RelayClientConfig, request_tx: RequestTx) -> Result<()> {
    let url = build_ws_url(config);
    let (stream, _) = connect_async(&url)
        .await
        .with_context(|| format!("failed to connect to relay at {url}"))?;
    info!(session_id = %config.session_id, relay_url = %config.url, "connected to relay");

    let (mut writer, mut reader) = stream.split();

    while let Some(frame) = reader.next().await {
        let message = frame?;

        match message {
            Message::Text(text) => {
                let request = serde_json::from_str::<SessionRequest>(text.as_ref())
                    .context("failed to decode relay request")?;
                let response = dispatch_request(&request_tx, request).await?;
                let payload = serde_json::to_string(&response)?;
                writer.send(Message::Text(payload.into())).await?;
            }
            Message::Ping(payload) => {
                writer.send(Message::Pong(payload)).await?;
            }
            Message::Close(frame) => {
                writer.send(Message::Close(frame)).await.ok();
                break;
            }
            Message::Binary(_) | Message::Pong(_) | Message::Frame(_) => {}
        }
    }

    Ok(())
}

async fn dispatch_request(
    request_tx: &RequestTx,
    request: SessionRequest,
) -> Result<CommandResponse> {
    let (response_tx, response_rx) = oneshot::channel();
    request_tx
        .send(RequestEnvelope {
            kind: RequestKind::Command {
                request,
                response_tx,
            },
        })
        .map_err(|_| anyhow!("daemon request loop is not running"))?;

    response_rx
        .await
        .map_err(|_| anyhow!("daemon request loop dropped relay response"))
}

fn build_ws_url(config: &RelayClientConfig) -> String {
    let base = config.url.trim_end_matches('/');
    let mut url = format!("{base}/ws/{}?role=daemon", config.session_id);
    if let Some(api_key) = &config.api_key {
        url.push_str("&token=");
        url.push_str(api_key);
    }
    url
}
