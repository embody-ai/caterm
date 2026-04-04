use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionRole {
    Daemon,
    Client,
}

#[derive(Debug, Clone)]
pub struct RelayAuth {
    daemon_api_key: Option<String>,
    client_share_token: Option<String>,
}

impl RelayAuth {
    pub fn from_env() -> Self {
        Self {
            daemon_api_key: read_secret("CATERM_RELAY_API_KEY"),
            client_share_token: read_secret("CATERM_RELAY_SHARE_TOKEN"),
        }
    }

    pub fn authorize(&self, role: ConnectionRole, token: Option<&str>) -> Result<()> {
        match role {
            ConnectionRole::Daemon => match self.daemon_api_key.as_deref() {
                Some(expected) => validate_token(expected, token, "daemon API key"),
                None => Ok(()),
            },
            ConnectionRole::Client => match self.client_share_token.as_deref() {
                Some(expected) => validate_token(expected, token, "share token"),
                None => Ok(()),
            },
        }
    }
}

fn read_secret(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn validate_token(expected: &str, provided: Option<&str>, label: &str) -> Result<()> {
    match provided {
        Some(candidate) if candidate == expected => Ok(()),
        Some(_) => bail!("invalid {label}"),
        None => bail!("missing {label}"),
    }
}
