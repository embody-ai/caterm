# Caterm

**Caterm** is a modern, open-source remote terminal system that lets you securely access and control your machines from anywhere.

It is built around a simple but powerful idea:
**execution stays local, control happens remotely.**

---

## ✨ Overview

Caterm consists of three main components:

1. **`caterm` package (Rust)** – runs on macOS and Linux, manages PTY and shell
2. **`caterm-relay` package (Rust)** – handles authentication and real-time communication
3. **Clients** – web, mobile, or native apps for interacting with terminals

Additionally, a **macOS SwiftUI app** provides a native GUI by connecting to the local CLI agent.

---

## 🧱 Architecture

```
[ CLI Agent (Mac/Linux) ]
    ├── PTY (shell runtime)
    ├── bash / zsh
    ├── WebSocket client
    └── reconnect logic
            │
            ▼
       [ Web Server ]
    ├── auth
    ├── session routing
    └── real-time relay
            │
     ┌──────┴────────┐
     ▼               ▼
[ Web Client ]   [ Mobile App ]
 (xterm.js)        (iOS)

[ macOS App (SwiftUI) ]
        │
        ▼
[ Local CLI Agent ]
```

---

## 🚀 Features

* 🔐 Secure remote terminal access via WebSocket (WSS)
* 🌍 Access your machines from anywhere (no port forwarding required)
* 💻 Cross-platform agent (macOS + Linux)
* 📱 Web and mobile clients
* 🧠 Session-based architecture with reconnect support
* 🔄 Automatic reconnection with exponential backoff
* 👥 Multi-client support (read-only or control modes)
* 🧩 Clean separation between execution and rendering

---

## 🧠 Core Design

Caterm strictly separates responsibilities:

### CLI Agent (Execution Layer)

* Creates and manages PTY sessions
* Launches shell processes (`bash`, `zsh`)
* Handles stdin/stdout streams
* Connects to server via WebSocket
* Forwards **raw byte streams only**

> The agent does NOT implement terminal rendering or ANSI parsing.

---

### Clients (Rendering Layer)

* Web: uses terminal emulator (e.g. xterm.js)
* Mobile: WebView or native UI
* macOS App: SwiftUI + optional integration with a high-performance terminal engine

Clients are responsible for:

* Rendering terminal output
* Handling user input
* Managing UI/UX

---

### Server (Relay Layer)

* Authenticates users and agents
* Routes data between clients and agents
* Maintains session mappings
* Stays as stateless as possible

---

## 🖥 macOS Native App

Caterm includes a native macOS application built with SwiftUI.

* Connects to the **local CLI agent**
* Provides a native terminal GUI
* Can integrate advanced rendering engines for high performance
* Works offline (local mode) or with remote sessions

---

## 🔌 How It Works

1. Start the CLI agent on your machine
2. The agent connects to the Caterm server
3. Authenticate and register the machine
4. Open a web or mobile client
5. Connect to the session and start interacting

---

## 📦 Installation

```bash
cargo build -p caterm
cargo build -p caterm-relay
```

## ▶️ Running The Relay

```bash
cargo run -p caterm-relay

# Optional configuration
# CATERM_RELAY_LISTEN_ADDR=0.0.0.0:8080
# CATERM_RELAY_API_KEY=secret-for-daemons
# CATERM_RELAY_SHARE_TOKEN=secret-for-clients
```

---

## 🔐 Security

* Token-based authentication
* Encrypted communication (WSS)
* No inbound ports required
* Session isolation per user

---

## ♻️ Reliability

* Heartbeat (ping/pong)
* Automatic reconnect with exponential backoff
* Session resume support
* Optional output buffering for recovery

---

## 🛠 Tech Stack

* **Rust** – `caterm`, `caterm-relay`
* **WebSocket** – real-time communication
* **Swift / SwiftUI** – macOS app
* **Web (xterm.js)** – browser client

---

## 📌 Roadmap

* [ ] Multi-user collaboration (shared sessions)
* [ ] Read-only / control permissions
* [ ] Session recording & replay
* [ ] File transfer support
* [ ] Plugin system

---

## 🤝 Contributing

Contributions are welcome!
Feel free to open issues or submit pull requests.

---

## 📄 License

This project is licensed under the Apache 2.0 License.

---

## 💡 Philosophy

> Keep execution local.
> Make access global.
> Keep the system simple.

---

## ⭐️

If you find this project useful, consider giving it a star on GitHub!
