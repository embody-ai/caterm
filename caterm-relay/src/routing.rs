use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

pub type MessageTx = mpsc::UnboundedSender<Message>;

#[derive(Clone, Default)]
pub struct RoutingTable {
    sessions: Arc<RwLock<HashMap<String, SessionRoute>>>,
}

#[derive(Default)]
struct SessionRoute {
    daemon: Option<MessageTx>,
    clients: HashMap<Uuid, MessageTx>,
}

impl RoutingTable {
    pub async fn attach_daemon(&self, session_id: &str, tx: MessageTx) {
        let mut sessions = self.sessions.write().await;
        let entry = sessions.entry(session_id.to_owned()).or_default();
        entry.daemon = Some(tx);
    }

    pub async fn detach_daemon(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        let remove = match sessions.get_mut(session_id) {
            Some(entry) => {
                entry.daemon = None;
                entry.clients.is_empty()
            }
            None => false,
        };

        if remove {
            sessions.remove(session_id);
        }
    }

    pub async fn attach_client(&self, session_id: &str, tx: MessageTx) -> Uuid {
        let client_id = Uuid::new_v4();
        let mut sessions = self.sessions.write().await;
        let entry = sessions.entry(session_id.to_owned()).or_default();
        entry.clients.insert(client_id, tx);
        client_id
    }

    pub async fn detach_client(&self, session_id: &str, client_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        let remove = match sessions.get_mut(session_id) {
            Some(entry) => {
                entry.clients.remove(&client_id);
                entry.daemon.is_none() && entry.clients.is_empty()
            }
            None => false,
        };

        if remove {
            sessions.remove(session_id);
        }
    }

    pub async fn daemon_tx(&self, session_id: &str) -> Option<MessageTx> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .and_then(|entry| entry.daemon.clone())
    }

    pub async fn client_txs(&self, session_id: &str) -> Vec<MessageTx> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .map(|entry| entry.clients.values().cloned().collect())
            .unwrap_or_default()
    }
}
