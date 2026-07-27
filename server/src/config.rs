use anyhow::Result;
use keyring::Entry;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub folder_url: Option<String>,
    pub developer_token: Option<String>,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub mcp_server_url: Option<String>,
    pub mcp_connection_token: Option<String>,
    #[serde(default)]
    pub search_limit: Option<usize>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

pub const DEFAULT_PORT: u16 = 8410;

const KEYRING_SERVICE: &str = "kasugai_box";
const KEYRING_ACCOUNT: &str = "kasugai_box_config";

pub fn load_config() -> Config {
    match Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT) {
        Ok(entry) => match entry.get_password() {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        },
        Err(_) => Config::default(),
    }
}

pub fn save_config(config: &Config) -> Result<()> {
    let entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)?;
    let content = serde_json::to_string_pretty(config)?;
    entry.set_password(&content)?;
    Ok(())
}

/// UI へ返す設定ビュー。平文の機密情報は含めない（方針書 1.6）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub folder_url: Option<String>,
    pub mcp_server_url: Option<String>,
    pub search_limit: Option<usize>,
    pub port: u16,
    pub auto_update: bool,
    pub has_client_id: bool,
    pub has_client_secret: bool,
    pub has_developer_token: bool,
    pub has_mcp_connection_token: bool,
}

impl From<&Config> for ConfigView {
    fn from(c: &Config) -> Self {
        let has = |v: &Option<String>| v.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
        ConfigView {
            folder_url: c.folder_url.clone(),
            mcp_server_url: c.mcp_server_url.clone(),
            search_limit: c.search_limit,
            port: c.port.unwrap_or(DEFAULT_PORT),
            auto_update: c.auto_update,
            has_client_id: has(&c.client_id),
            has_client_secret: has(&c.client_secret),
            has_developer_token: has(&c.developer_token),
            has_mcp_connection_token: has(&c.mcp_connection_token),
        }
    }
}

/// 設定更新リクエスト。機密項目は「空でない値が来たときだけ」上書きする。
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigUpdate {
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub developer_token: Option<String>,
    pub folder_url: Option<String>,
    pub mcp_server_url: Option<String>,
    pub mcp_connection_token: Option<String>,
    pub search_limit: Option<usize>,
    pub port: Option<u16>,
    pub auto_update: Option<bool>,
}

pub fn apply_update(update: ConfigUpdate) -> Result<Config> {
    let mut config = load_config();
    let set_secret = |target: &mut Option<String>, value: Option<String>| {
        if let Some(v) = value {
            let v = v.trim().to_string();
            if !v.is_empty() {
                *target = Some(v);
            }
        }
    };
    set_secret(&mut config.client_id, update.client_id);
    set_secret(&mut config.client_secret, update.client_secret);
    set_secret(&mut config.developer_token, update.developer_token);
    set_secret(&mut config.mcp_connection_token, update.mcp_connection_token);
    if let Some(v) = update.folder_url {
        config.folder_url = Some(v);
    }
    if let Some(v) = update.mcp_server_url {
        config.mcp_server_url = if v.trim().is_empty() { None } else { Some(v) };
    }
    if let Some(v) = update.search_limit {
        config.search_limit = Some(v.clamp(1, 200));
    }
    if let Some(v) = update.port {
        config.port = Some(v.max(1));
    }
    if let Some(v) = update.auto_update {
        config.auto_update = v;
    }
    save_config(&config)?;
    Ok(config)
}
