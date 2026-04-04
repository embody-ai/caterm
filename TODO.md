# Caterm TODO

## Daemon Foundation

- [x] Add a `status` command to report whether the local daemon is running and which socket it uses
- [x] Add an attach/subscribe model so clients can maintain a persistent connection to the daemon
- [x] Split command/response control traffic from long-lived event streaming
- [x] Add explicit client attach targets for session/window/pane
- [x] Add per-client active session/window/pane tracking

## Session, Window, Pane Model

- [x] Add `select-window` and `select-pane` commands
- [ ] Add `rename-session`, `rename-window`, and `rename-pane` commands
- [ ] Define lifecycle rules for last-pane/last-window deletion
- [ ] Add automatic cleanup rules for empty windows and empty sessions
- [ ] Add layout state for panes inside a window
- [ ] Add horizontal/vertical split commands
- [ ] Add pane resize commands

## Protocol and Events

- [ ] Introduce stable protocol versioning
- [ ] Add structured error codes in daemon responses
- [ ] Add buffered PTY output snapshots for newly attached clients
- [ ] Add explicit state-change events for active window/pane selection
- [ ] Add backpressure/output buffering strategy for busy PTYs

## Daemon Runtime

- [ ] Add stale socket and PID handling beyond simple socket removal
- [ ] Add daemon metadata persistence and recovery strategy
- [ ] Add log file support for the daemon
- [ ] Add launchd/systemd-friendly foreground/background ergonomics

## Testing

- [ ] Add concurrent client integration tests
- [ ] Add stale socket and daemon restart tests
- [ ] Add protocol-level event stream tests once attach exists
- [ ] Add more invalid-target and error-path coverage
