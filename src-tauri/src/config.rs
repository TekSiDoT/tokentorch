use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub session_key: String,
    pub org_id: String,
    pub poll_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            session_key: String::new(),
            org_id: String::new(),
            poll_interval_secs: 300, // 5 minutes
        }
    }
}

impl AppConfig {
    pub fn is_configured(&self) -> bool {
        !self.session_key.is_empty() && !self.org_id.is_empty()
    }
}
