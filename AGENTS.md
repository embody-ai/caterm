# AGENTS.md — Caterm

## What is Caterm?

Caterm is a **persistent, shareable remote terminal workspace**. Think tmux with a native macOS GUI, accessible from anywhere via web or mobile, with real-time session sharing for collaborative debugging.

The killer features are:
- **Remote access** — run the daemon on your Mac Mini, access your terminal sessions from iPhone, browser, or another Mac, anywhere in the world
- **Session sharing** — share a live terminal session with a collaborator via a single link. They open a browser, join instantly, and can watch or type in real time
- **Persistent sessions** — sessions survive client disconnects, exactly like tmux. Close the app, come back later, everything is still running

This is NOT another terminal emulator. It is a **persistent terminal multiplexer with a cloud relay layer and native GUI clients**.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│              Caterm Daemon (Rust)                    │
│  The single source of truth. Long-running process.  │
│  Owns all PTYs, sessions, windows, panes.            │
│  Listens on Unix socket (local) + TCP/WS (remote).  │
└────────────────────┬────────────────────────────────┘
                     │ WebSocket + TLS + JWT
              ┌──────▼──────┐
              │ Relay Server │  (cloud, NAT traversal, auth, share links)
              └──────┬───────┘
         ┌───────────┼──────────────┐
         ▼           ▼              ▼
   SwiftUI macOS   Web browser    iOS app
   (Unix socket    (xterm.js)     (SwiftUI)
    local fast)
```

### Component Hierarchy (tmux-equivalent)

```
Server (daemon process)
└── Session          — top-level named workspace, survives detach
    └── Window       — like a browser tab, full screen, one visible at a time
        └── Pane     — a PTY split region, runs one process each
```

---

## Repository Structure

```
caterm/
├── AGENTS.md                  ← you are here
├── daemon/                    ← Rust: core engine
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs            — entry point, daemonize, start listeners
│       ├── daemon.rs          — top-level state: sessions, clients
│       ├── session.rs         — Session, Window structs
│       ├── pane.rs            — Pane, PTY ownership, scrollback buffer
│       ├── server/
│       │   ├── local.rs       — Unix socket listener (local clients)
│       │   └── remote.rs      — TCP/WebSocket listener (relay, remote clients)
│       ├── protocol.rs        — all message types (commands + events), serde
│       └── relay.rs           — outbound persistent WebSocket to relay server
│
├── relay/                     ← Rust: cloud relay server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── auth.rs            — JWT validation, share token issuance
│       ├── routing.rs         — session_id → daemon connection table
│       └── bridge.rs          — byte forwarding between daemon and clients
│
├── macos-app/                 ← Swift: SwiftUI macOS client
│   ├── CatermApp.xcodeproj
│   └── Caterm/
│       ├── App.swift
│       ├── SessionManager.swift    — WebSocket client, protocol decoding
│       ├── DaemonLauncher.swift    — spawn/detect daemon process
│       ├── Views/
│       │   ├── MainLayout.swift    — HSplitView / VSplitView panel layout
│       │   ├── TerminalView.swift  — WKWebView hosting xterm.js
│       │   └── SessionSidebar.swift
│       └── Resources/
│           └── xterm/             — bundled xterm.js + addons
│
└── ios-app/                   ← Swift: SwiftUI iOS client
    └── Caterm/
        ├── App.swift
        ├── RelayClient.swift      — WebSocket to relay
        └── Views/
            └── TerminalView.swift
```

---

## Tech Stack

### Daemon (Rust)
| Crate | Purpose |
|---|---|
| `tokio` | Async runtime |
| `axum` | WebSocket server (both local and remote listeners) |
| `portable-pty` | PTY creation and management per pane |
| `serde` / `serde_json` | Protocol message serialization |
| `toml` | Config file parsing |
| `jsonwebtoken` | JWT auth for remote connections |
| `rustls` / `tokio-rustls` | TLS for remote listener |
| `uuid` | Session/window/pane IDs |

### Relay Server (Rust)
Same async stack as daemon (`tokio` + `axum`), focused purely on routing and auth. No PTY involvement.

### macOS App (Swift)
| Component | Approach |
|---|---|
| UI framework | SwiftUI |
| Terminal rendering | `WKWebView` embedding `xterm.js` |
| WebSocket client | `URLSessionWebSocketTask` (native, zero deps) |
| Daemon launch | `Foundation.Process` |
| Panel layout | `HSplitView` / `VSplitView` |

### iOS App (Swift)
Same as macOS but connects to relay only (no Unix socket access). SwiftUI + `URLSessionWebSocketTask`.

---

## Protocol

All communication between clients and the daemon (local or remote) uses **JSON messages over WebSocket**.

### Client → Daemon commands

```json
{ "cmd": "create_session", "name": "work" }
{ "cmd": "attach_session", "session_id": "abc-123" }
{ "cmd": "detach_session", "session_id": "abc-123" }
{ "cmd": "list_sessions" }

{ "cmd": "create_window", "session_id": "abc-123", "name": "editor" }
{ "cmd": "create_pane",   "window_id": "win-1", "split": "vertical" }
{ "cmd": "resize_pane",   "pane_id": "pane-1", "rows": 40, "cols": 120 }
{ "cmd": "send_input",    "pane_id": "pane-1", "data": "ls -la\n" }
{ "cmd": "kill_pane",     "pane_id": "pane-1" }
{ "cmd": "kill_session",  "session_id": "abc-123" }
```

### Daemon → Client events

```json
{ "event": "session_list",   "sessions": [...] }
{ "event": "session_created","session": { "id": "...", "name": "work", "windows": [] } }
{ "event": "pane_created",   "pane": { "id": "...", "window_id": "..." } }
{ "event": "pty_output",     "pane_id": "pane-1", "data": "<base64-encoded bytes>" }
{ "event": "pane_exited",    "pane_id": "pane-1", "exit_code": 0 }
{ "event": "snapshot",       "pane_id": "pane-1", "screen": "...", "scrollback": "..." }
```

### Share link flow

```json
// Client requests a share token
{ "cmd": "create_share_link", "session_id": "abc-123", "permission": "read-only", "expires_in_secs": 3600 }

// Daemon asks relay to mint a token, returns:
{ "event": "share_link_created", "url": "https://caterm.app/join/xK92mQpL7", "token": "xK92mQpL7", "expires_at": 1234567890 }
```

---

## Core Data Model

```rust
struct Daemon {
    sessions: HashMap<SessionId, Session>,
    clients: HashMap<ClientId, ConnectedClient>,
    config: Config,
}

struct Session {
    id: SessionId,          // uuid
    name: String,
    windows: Vec<Window>,
    active_window: WindowId,
    attached_clients: Vec<ClientId>,
}

struct Window {
    id: WindowId,
    name: String,
    panes: Vec<Pane>,
    layout: Layout,         // how panes are spatially arranged
}

struct Pane {
    id: PaneId,
    pty: PtyPair,           // portable-pty handle
    child_pid: u32,
    scrollback: VecDeque<Bytes>,   // ring buffer, configurable size
    screen: TerminalScreen,        // current visible grid state
    size: (u16, u16),              // rows, cols
}
```

---

## How Session Persistence Works

The daemon is a long-running background process. Persistence is **not magic** — it works because:

1. The daemon holds all PTY file descriptors open
2. When a client disconnects, the daemon does NOT close the PTY
3. The child process (shell) never receives SIGHUP and keeps running
4. Screen state and scrollback are stored in daemon memory
5. When a client reconnects, the daemon sends the current screen snapshot + scrollback

The daemon should write its PID to `/tmp/caterm.pid` and its Unix socket to `/tmp/caterm.sock`. New client invocations check for the socket first — if present, they connect as a client rather than spawning a new daemon.

---

## Development Guide

### Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# Xcode (for macOS/iOS apps)
# Install from Mac App Store, minimum Xcode 15

# For testing PTY behavior
brew install tmux   # useful reference implementation
```

### Running the daemon locally

```bash
cd daemon
cargo run

# Daemon listens on:
#   /tmp/caterm.sock  (Unix socket, local clients)
#   0.0.0.0:7890      (TCP/WebSocket, remote/relay)
```

### Running the relay server

```bash
cd relay
cargo run

# Relay listens on:
#   0.0.0.0:443  (production)
#   0.0.0.0:8080 (dev)
```

### Running the macOS app

Open `macos-app/CatermApp.xcodeproj` in Xcode and hit Run. The app will:
1. Check if the daemon is running (ping `/tmp/caterm.sock`)
2. If not, spawn it automatically via `DaemonLauncher.swift`
3. Connect via Unix socket

### Build all Rust crates

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## Key Invariants — Never Violate These

1. **The daemon owns all PTYs.** Clients never create PTY processes. Only the daemon spawns shells.

2. **Protocol is symmetric.** The macOS app, iOS app, web browser, and relay all speak the same JSON-over-WebSocket protocol. There is no special local protocol — only the transport differs (Unix socket vs TCP).

3. **The relay never stores PTY content.** The relay is a pure byte router. It forwards encrypted WebSocket frames but never decrypts or persists terminal output. This is a privacy guarantee.

4. **Session IDs are UUIDs.** Never use sequential integers for session/window/pane IDs — they must be globally unique for the relay routing table to work correctly.

5. **Scrollback is bounded.** The scrollback ring buffer has a configurable maximum (default: 10,000 lines). Never grow it unboundedly.

6. **Daemonize properly.** On startup, if no existing socket is found, the process must `fork()`, `setsid()`, close stdio, and write its PID file before accepting connections.

---

## Development Phases

### Phase 1 — Daemon core (start here)
- [ ] PTY creation and management (`portable-pty`)
- [ ] Session / Window / Pane data model
- [ ] Unix socket listener + JSON protocol
- [ ] Basic commands: `create_session`, `create_pane`, `send_input`, `pty_output` events
- [ ] Attach / detach / re-attach flow
- [ ] Scrollback buffer and screen snapshot on attach

### Phase 2 — macOS app
- [ ] Daemon detection + auto-launch
- [ ] Unix socket WebSocket client in Swift
- [ ] `WKWebView` + xterm.js terminal view
- [ ] `HSplitView` / `VSplitView` pane layout
- [ ] Session sidebar

### Phase 3 — Relay + remote access (the killer feature)
- [ ] Relay server: routing table, WebSocket bridge
- [ ] JWT auth for remote clients
- [ ] Daemon outbound persistent connection to relay
- [ ] Share link minting + token validation
- [ ] Web client (xterm.js served from relay or CDN)

### Phase 4 — iOS app
- [ ] SwiftUI iOS terminal view
- [ ] Relay WebSocket client
- [ ] Read-only and read-write share modes

---

## Configuration File

Location: `~/.config/caterm/config.toml`

```toml
[daemon]
socket_path = "/tmp/caterm.sock"
tcp_port = 7890
scrollback_lines = 10000
default_shell = "/bin/zsh"

[relay]
url = "wss://relay.caterm.app"
api_key = ""   # set for authenticated relay access

[ui]
theme = "dark"
font_size = 14
```

---

## Testing Approach

- **Unit tests** — protocol serialization/deserialization, scrollback ring buffer, session state transitions
- **Integration tests** — spawn a daemon subprocess, connect via Unix socket, run commands, assert PTY output
- **Manual testing** — use `websocat` to connect to the daemon and send raw JSON commands:

```bash
# Install websocat
cargo install websocat

# Connect to daemon Unix socket
websocat unix:/tmp/caterm.sock

# Then type JSON commands:
{"cmd":"list_sessions"}
{"cmd":"create_session","name":"test"}
```

---

## Important Context for AI Agents

- This project is intentionally similar to **tmux** in architecture. When in doubt about session/window/pane semantics, refer to tmux's behavior as the reference implementation.
- The **relay server is the key differentiator** from tmux. It enables zero-config remote access and shareable sessions without port forwarding.
- PTY handling is subtle. Always use `portable-pty` — do not try to call `openpty()` directly. Handle `SIGWINCH` (terminal resize) properly by propagating resize events to the child process via `PtySize`.
- The macOS app is a **thin client**. No business logic belongs in Swift. If you find yourself implementing session state in Swift, move it to the daemon.
- xterm.js in `WKWebView` communicates with the Swift layer via `WKScriptMessageHandler` (JS → Swift) and `evaluateJavaScript` (Swift → JS). Keep this bridge minimal: only PTY bytes and resize events cross it.
