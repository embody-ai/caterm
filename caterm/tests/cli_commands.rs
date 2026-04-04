use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use assert_cmd::cargo::cargo_bin;
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
    assert!(list_stdout.contains("window 1:2 (editor)"));
    assert!(list_stdout.contains("pane 0:2 (pane-2)"));
    assert!(list_stdout.contains("pane 1:3 (logs)"));
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
    assert!(running_stdout.contains(daemon.socket_path.to_string_lossy().as_ref()));

    let stop = daemon.run(&["stop"]);
    assert!(stop.status.success(), "{}", stdout_text(&stop));

    let stopped = daemon.run(&["status"]);
    assert!(stopped.status.success(), "{}", stdout_text(&stopped));
    assert!(stdout_text(&stopped).contains("Caterm daemon is not running"));
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
