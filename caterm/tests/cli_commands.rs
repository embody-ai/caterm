use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
use serde_json::json;
use tempfile::TempDir;

fn bin_path() -> PathBuf {
    cargo_bin("caterm")
}

struct TestDaemon {
    _tempdir: TempDir,
    socket_path: PathBuf,
    child: Child,
}

impl TestDaemon {
    fn start() -> Self {
        let tempdir = TempDir::new().expect("create tempdir");
        let socket_path = tempdir.path().join("caterm.sock");

        let child = Command::new(bin_path())
            .arg("start")
            .arg("--socket")
            .arg(&socket_path)
            .env("RUST_LOG", "error")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start daemon");

        wait_for_socket(&socket_path);

        Self {
            _tempdir: tempdir,
            socket_path,
            child,
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        let mut command = Command::new(bin_path());
        command.arg("--socket").arg(&self.socket_path);
        command.args(args);
        command.output().expect("run cli command")
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = Command::new(bin_path())
            .arg("--socket")
            .arg(&self.socket_path)
            .arg("stop")
            .output();

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }

        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait_for_socket(socket_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);

    while Instant::now() < deadline {
        if socket_path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }

    panic!("daemon socket was not created at {}", socket_path.display());
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is valid utf8")
}

fn metadata_path(socket_path: &Path) -> PathBuf {
    let file_name = socket_path
        .file_name()
        .map(|name| format!("{}.meta.json", name.to_string_lossy()))
        .unwrap_or_else(|| "caterm.meta.json".to_string());
    socket_path.with_file_name(file_name)
}

#[test]
fn new_session_and_list_show_hierarchy() {
    let daemon = TestDaemon::start();

    let create = daemon.run(&["new-session", "work"]);
    assert!(create.status.success(), "{}", stdout_text(&create));
    let create_stdout = stdout_text(&create);
    assert!(create_stdout.contains("Created session 1 (work)"));
    assert!(create_stdout.contains("Created pane 0:1 (pane-1)"));

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let list_stdout = stdout_text(&list);
    assert!(list_stdout.contains("Sessions:"));
    assert!(list_stdout.contains("session 1 (work)"));
    assert!(list_stdout.contains("window 0:1 (window-0)"));
    assert!(list_stdout.contains("pane 0:1 (pane-1)"));
}

#[test]
fn new_window_and_new_pane_are_visible_in_list() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );
    assert!(
        daemon
            .run(&["new-pane", "work", "editor", "logs"])
            .status
            .success()
    );

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let list_stdout = stdout_text(&list);
    assert!(list_stdout.contains("window 1:2 (editor) layout=tiled"));
    assert!(list_stdout.contains("pane 0:2 (pane-2)"));
    assert!(list_stdout.contains("pane 1:3 (logs)"));
}

#[test]
fn select_commands_update_active_targets() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );
    assert!(
        daemon
            .run(&["new-pane", "work", "editor", "logs"])
            .status
            .success()
    );

    let select_window = daemon.run(&["select-window", "work", "editor"]);
    assert!(
        select_window.status.success(),
        "{}",
        stdout_text(&select_window)
    );
    assert!(stdout_text(&select_window).contains("Selected window 1:2 (editor) in session 1"));

    let select_pane = daemon.run(&["select-pane", "work", "editor", "logs"]);
    assert!(
        select_pane.status.success(),
        "{}",
        stdout_text(&select_pane)
    );
    assert!(stdout_text(&select_pane).contains("Selected pane 1:3 (logs) in session 1, window 2"));

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let stdout = stdout_text(&list);
    assert!(
        stdout.contains("session 1 (work) active_window=1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("window 1:2 (editor) layout=tiled active_pane=1"),
        "{stdout}"
    );
}

#[test]
fn list_shows_single_and_tiled_layout_state() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let initial = daemon.run(&["list"]);
    assert!(initial.status.success(), "{}", stdout_text(&initial));
    assert!(stdout_text(&initial).contains("window 0:1 (window-0) layout=single"));

    assert!(
        daemon
            .run(&["new-pane", "work", "0", "logs"])
            .status
            .success()
    );

    let updated = daemon.run(&["list"]);
    assert!(updated.status.success(), "{}", stdout_text(&updated));
    assert!(stdout_text(&updated).contains("window 0:1 (window-0) layout=tiled"));
}

#[test]
fn split_commands_create_directional_layouts() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let split_horizontal = daemon.run(&["split-horizontal", "work", "0", "top"]);
    assert!(
        split_horizontal.status.success(),
        "{}",
        stdout_text(&split_horizontal)
    );
    assert!(
        stdout_text(&split_horizontal).contains("with horizontal layout; created pane 1:2 (top)")
    );

    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );

    let split_vertical = daemon.run(&["split-vertical", "work", "editor", "side"]);
    assert!(
        split_vertical.status.success(),
        "{}",
        stdout_text(&split_vertical)
    );
    assert!(stdout_text(&split_vertical).contains("with vertical layout; created pane 1:4 (side)"));

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let stdout = stdout_text(&list);
    assert!(
        stdout.contains("window 0:1 (window-0) layout=horizontal"),
        "{stdout}"
    );
    assert!(
        stdout.contains("window 1:2 (editor) layout=vertical"),
        "{stdout}"
    );
}

#[test]
fn resize_pane_updates_reported_sizes() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["split-horizontal", "work", "0", "top"])
            .status
            .success()
    );

    let resize = daemon.run(&["resize-pane", "work", "0", "0", "10"]);
    assert!(resize.status.success(), "{}", stdout_text(&resize));
    assert!(
        stdout_text(&resize).contains("Resized pane 0:1 (pane-1) to 60% in session 1, window 1")
    );

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let stdout = stdout_text(&list);
    assert!(stdout.contains("pane 0:1 (pane-1) size=60"), "{stdout}");
    assert!(stdout.contains("pane 1:2 (top) size=40"), "{stdout}");
}

#[test]
fn resize_pane_rejects_single_pane_windows() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let resize = daemon.run(&["resize-pane", "work", "0", "0", "10"]);
    assert!(!resize.status.success());
    assert!(stdout_text(&resize).contains("cannot resize a window with fewer than 2 panes"));
}

#[test]
fn rename_commands_update_hierarchy_names() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );
    assert!(
        daemon
            .run(&["new-pane", "work", "editor", "logs"])
            .status
            .success()
    );

    let rename_session = daemon.run(&["rename-session", "work", "client-work"]);
    assert!(
        rename_session.status.success(),
        "{}",
        stdout_text(&rename_session)
    );
    assert!(stdout_text(&rename_session).contains("Renamed session 1 to client-work"));

    let rename_window = daemon.run(&["rename-window", "client-work", "editor", "shells"]);
    assert!(
        rename_window.status.success(),
        "{}",
        stdout_text(&rename_window)
    );
    assert!(stdout_text(&rename_window).contains("Renamed window 1:2 to shells in session 1"));

    let rename_pane = daemon.run(&["rename-pane", "client-work", "shells", "logs", "tail"]);
    assert!(
        rename_pane.status.success(),
        "{}",
        stdout_text(&rename_pane)
    );
    assert!(stdout_text(&rename_pane).contains("Renamed pane 1:3 to tail in session 1, window 2"));

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    let stdout = stdout_text(&list);
    assert!(stdout.contains("session 1 (client-work)"), "{stdout}");
    assert!(stdout.contains("window 1:2 (shells)"), "{stdout}");
    assert!(stdout.contains("pane 1:3 (tail)"), "{stdout}");
}

#[test]
fn send_input_acknowledges_delivery() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let send = daemon.run(&["send-input", "work", "0", "0", "echo hello from test\r"]);
    assert!(send.status.success(), "{}", stdout_text(&send));
    let stdout = stdout_text(&send);
    assert!(stdout.contains("Accepted input for pane 1 in session 1, window 1"));
}

#[test]
fn send_input_can_exit_pane() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let exit = daemon.run(&["send-input", "work", "0", "0", "exit\r"]);
    assert!(exit.status.success(), "{}", stdout_text(&exit));

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let list = daemon.run(&["list"]);
        assert!(list.status.success(), "{}", stdout_text(&list));
        let stdout = stdout_text(&list);

        if stdout.contains("exit=0") {
            break;
        }

        if Instant::now() >= deadline {
            panic!("pane did not report exit code in time:\n{stdout}");
        }

        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn kill_commands_remove_resources() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );
    assert!(
        daemon
            .run(&["new-pane", "work", "editor", "logs"])
            .status
            .success()
    );

    let kill_pane = daemon.run(&["kill-pane", "work", "editor", "logs"]);
    assert!(kill_pane.status.success(), "{}", stdout_text(&kill_pane));
    assert!(stdout_text(&kill_pane).contains("Deleted pane 3"));

    let kill_window = daemon.run(&["kill-window", "work", "editor"]);
    assert!(
        kill_window.status.success(),
        "{}",
        stdout_text(&kill_window)
    );
    assert!(stdout_text(&kill_window).contains("Deleted window 2"));

    let kill_session = daemon.run(&["kill-session", "work"]);
    assert!(
        kill_session.status.success(),
        "{}",
        stdout_text(&kill_session)
    );
    assert!(stdout_text(&kill_session).contains("Deleted session 1"));

    let list = daemon.run(&["list"]);
    assert!(list.status.success(), "{}", stdout_text(&list));
    assert!(stdout_text(&list).contains("No active sessions"));
}

#[test]
fn kill_commands_reject_last_window_and_last_pane() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());

    let kill_last_pane = daemon.run(&["kill-pane", "work", "0", "0"]);
    assert!(!kill_last_pane.status.success());
    assert!(stdout_text(&kill_last_pane).contains("cannot delete the last pane in window 1"));
    let stderr = String::from_utf8(kill_last_pane.stderr).expect("stderr is valid utf8");
    assert!(
        stderr.contains("cannot delete the last pane in window 1"),
        "{stderr}"
    );

    let kill_last_window = daemon.run(&["kill-window", "work", "0"]);
    assert!(!kill_last_window.status.success());
    assert!(stdout_text(&kill_last_window).contains("cannot delete the last window in session 1"));
    let stderr = String::from_utf8(kill_last_window.stderr).expect("stderr is valid utf8");
    assert!(
        stderr.contains("cannot delete the last window in session 1"),
        "{stderr}"
    );
}

#[test]
fn stop_command_succeeds() {
    let daemon = TestDaemon::start();

    let stop = daemon.run(&["stop"]);
    assert!(stop.status.success(), "{}", stdout_text(&stop));
}

#[test]
fn status_command_reports_running_and_stopped_states() {
    let daemon = TestDaemon::start();

    let running = daemon.run(&["status"]);
    assert!(running.status.success(), "{}", stdout_text(&running));
    let running_stdout = stdout_text(&running);
    assert!(running_stdout.contains("Caterm daemon is running"));
    assert!(running_stdout.contains("pid"));
    assert!(running_stdout.contains(daemon.socket_path.to_string_lossy().as_ref()));

    let stop = daemon.run(&["stop"]);
    assert!(stop.status.success(), "{}", stdout_text(&stop));

    let stopped = daemon.run(&["status"]);
    assert!(stopped.status.success(), "{}", stdout_text(&stopped));
    assert!(stdout_text(&stopped).contains("Caterm daemon is not running"));
}

#[test]
fn start_recovers_from_stale_socket_and_metadata() {
    let tempdir = TempDir::new().expect("create tempdir");
    let socket_path = tempdir.path().join("caterm.sock");
    let metadata_path = metadata_path(&socket_path);

    std::fs::write(&socket_path, b"stale").expect("write stale socket placeholder");
    std::fs::write(
        &metadata_path,
        serde_json::to_vec(&json!({
            "pid": 999_999u32,
            "socket_path": socket_path,
        }))
        .expect("serialize metadata"),
    )
    .expect("write stale metadata");

    let mut child = Command::new(bin_path())
        .arg("start")
        .arg("--socket")
        .arg(&socket_path)
        .env("RUST_LOG", "error")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start daemon over stale socket");

    wait_for_socket(&socket_path);

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let status = Command::new(bin_path())
            .arg("--socket")
            .arg(&socket_path)
            .arg("status")
            .output()
            .expect("run status");
        assert!(status.status.success(), "{}", stdout_text(&status));
        if stdout_text(&status).contains("Caterm daemon is running") {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "daemon did not become healthy in time:\n{}",
                stdout_text(&status)
            );
        }
        thread::sleep(Duration::from_millis(50));
    }

    let stop = Command::new(bin_path())
        .arg("--socket")
        .arg(&socket_path)
        .arg("stop")
        .output()
        .expect("stop daemon");
    assert!(stop.status.success(), "{}", stdout_text(&stop));

    let _ = child.wait();
}

#[test]
fn attach_streams_live_events() {
    let daemon = TestDaemon::start();

    let mut attach = Command::new(bin_path())
        .arg("--socket")
        .arg(&daemon.socket_path)
        .arg("attach")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start attach client");

    thread::sleep(Duration::from_millis(100));

    let create = daemon.run(&["new-session", "work"]);
    assert!(create.status.success(), "{}", stdout_text(&create));
    let send = daemon.run(&["send-input", "work", "0", "0", "echo hello from attach\r"]);
    assert!(send.status.success(), "{}", stdout_text(&send));

    thread::sleep(Duration::from_millis(200));
    let _ = attach.kill();
    let output = attach.wait_with_output().expect("collect attach output");
    let stdout = stdout_text(&output);
    assert!(stdout.contains("Created session 1 (work)"), "{stdout}");
    assert!(stdout.contains("hello from attach"), "{stdout}");
}

#[test]
fn attach_replays_buffered_output_snapshots() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    let send = daemon.run(&["send-input", "work", "0", "0", "echo replay me\r"]);
    assert!(send.status.success(), "{}", stdout_text(&send));

    let mut attach = Command::new(bin_path())
        .arg("--socket")
        .arg(&daemon.socket_path)
        .arg("attach")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start attach client");

    thread::sleep(Duration::from_millis(200));
    let _ = attach.kill();
    let output = attach.wait_with_output().expect("collect attach output");
    let stdout = stdout_text(&output);
    assert!(stdout.contains("replay me"), "{stdout}");
}

#[test]
fn attach_target_filters_to_a_single_pane() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(daemon.run(&["new-session", "personal"]).status.success());

    let mut attach = Command::new(bin_path())
        .arg("--socket")
        .arg(&daemon.socket_path)
        .args(["attach", "work", "0", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start targeted attach client");

    thread::sleep(Duration::from_millis(100));

    let work_send = daemon.run(&["send-input", "work", "0", "0", "echo hello-work\r"]);
    assert!(work_send.status.success(), "{}", stdout_text(&work_send));
    let personal_send = daemon.run(&["send-input", "personal", "0", "0", "echo hello-personal\r"]);
    assert!(
        personal_send.status.success(),
        "{}",
        stdout_text(&personal_send)
    );

    thread::sleep(Duration::from_millis(250));
    let _ = attach.kill();
    let output = attach
        .wait_with_output()
        .expect("collect targeted attach output");
    let stdout = stdout_text(&output);

    assert!(
        stdout.contains("client_active session=1 window=1 pane=1"),
        "{stdout}"
    );
    assert!(stdout.contains("session 1 (work)"), "{stdout}");
    assert!(!stdout.contains("session 2 (personal)"), "{stdout}");
    assert!(stdout.contains("hello-work"), "{stdout}");
    assert!(!stdout.contains("hello-personal"), "{stdout}");
}

#[test]
fn attach_streams_active_selection_events() {
    let daemon = TestDaemon::start();

    assert!(daemon.run(&["new-session", "work"]).status.success());
    assert!(
        daemon
            .run(&["new-window", "work", "editor"])
            .status
            .success()
    );
    assert!(
        daemon
            .run(&["new-pane", "work", "editor", "logs"])
            .status
            .success()
    );

    let mut attach = Command::new(bin_path())
        .arg("--socket")
        .arg(&daemon.socket_path)
        .arg("attach")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start attach client");

    thread::sleep(Duration::from_millis(100));

    let select_window = daemon.run(&["select-window", "work", "editor"]);
    assert!(
        select_window.status.success(),
        "{}",
        stdout_text(&select_window)
    );
    let select_pane = daemon.run(&["select-pane", "work", "editor", "logs"]);
    assert!(
        select_pane.status.success(),
        "{}",
        stdout_text(&select_pane)
    );

    thread::sleep(Duration::from_millis(200));
    let _ = attach.kill();
    let output = attach.wait_with_output().expect("collect attach output");
    let stdout = stdout_text(&output);

    assert!(
        stdout.contains("Active window changed to 2 in session 1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Active pane changed to 3 in session 1, window 2"),
        "{stdout}"
    );
}
