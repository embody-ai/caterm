pub fn classify_error(message: &str) -> &'static str {
    if message.starts_with("session not found:")
        || (message.starts_with("session ") && message.ends_with(" not found"))
    {
        "session_not_found"
    } else if message.starts_with("window not found")
        || (message.starts_with("window ") && message.ends_with(" not found"))
    {
        "window_not_found"
    } else if message.starts_with("pane not found")
        || (message.starts_with("pane ") && message.ends_with(" not found"))
    {
        "pane_not_found"
    } else if message.starts_with("session name already exists:") {
        "duplicate_session_name"
    } else if message.starts_with("window name already exists in session") {
        "duplicate_window_name"
    } else if message.starts_with("pane name already exists in window") {
        "duplicate_pane_name"
    } else if message.starts_with("cannot delete the last window in session") {
        "last_window"
    } else if message.starts_with("cannot delete the last pane in window") {
        "last_pane"
    } else if message.starts_with("cannot resize a window with fewer than 2 panes") {
        "resize_single_pane"
    } else if message.starts_with("resize would exceed the minimum pane size") {
        "resize_limit"
    } else if message.starts_with("pane ") && message.ends_with(" has already exited") {
        "pane_exited"
    } else if message.starts_with("protocol version mismatch:") {
        "protocol_mismatch"
    } else if message.contains("failed to connect to Caterm server") {
        "server_unreachable"
    } else {
        "internal_error"
    }
}

#[cfg(test)]
mod tests {
    use super::classify_error;

    #[test]
    fn classifies_known_errors() {
        assert_eq!(
            classify_error("session not found: work"),
            "session_not_found"
        );
        assert_eq!(
            classify_error("window name already exists in session 1: editor"),
            "duplicate_window_name"
        );
        assert_eq!(
            classify_error("cannot resize a window with fewer than 2 panes"),
            "resize_single_pane"
        );
        assert_eq!(classify_error("something unexpected"), "internal_error");
    }
}
