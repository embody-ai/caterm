mod command;
mod daemon_state;
mod error;
mod event;
mod pane;
mod protocol;
mod request;
mod server;
mod session;
mod snapshot;
mod window;

pub use command::{CommandResponse, CommandResult};
pub use event::{EventEnvelope, ServerEvent};
pub use protocol::PROTOCOL_VERSION;
pub use request::{ClientOptions, SessionRequest};
pub use server::{
    DaemonStatus, SessionManagerServer, attach_client_stream, cleanup_stale_socket, daemon_status,
    default_socket_path, is_server_running, send_client_request,
};
pub(crate) use server::{RequestEnvelope, RequestKind, RequestTx};
pub use snapshot::ServerSnapshot;
