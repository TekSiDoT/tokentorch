use crate::usage::ApiUsageResponse;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, REFERER, USER_AGENT};

const BASE_URL: &str = "https://claude.ai";

pub struct ClaudeClient {
    client: reqwest::Client,
    session_key: String,
    org_id: String,
}

#[derive(Debug)]
pub struct ApiResult {
    pub usage: ApiUsageResponse,
    pub refreshed_session_key: Option<String>,
}

impl ClaudeClient {
    pub fn new(session_key: &str, org_id: &str) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            session_key: session_key.to_string(),
            org_id: org_id.to_string(),
        }
    }

    pub fn session_key(&self) -> &str {
        &self.session_key
    }

    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    pub fn update_session_key(&mut self, key: String) {
        self.session_key = key;
    }

    fn headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:147.0) Gecko/20100101 Firefox/147.0",
            ),
        );
        headers.insert(
            REFERER,
            HeaderValue::from_static("https://claude.ai/settings/usage"),
        );
        headers.insert(
            "anthropic-client-platform",
            HeaderValue::from_static("web_claude_ai"),
        );
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );

        let cookie_value = format!("sessionKey={}", self.session_key);
        if let Ok(val) = HeaderValue::from_str(&cookie_value) {
            headers.insert(COOKIE, val);
        }

        headers
    }

    pub async fn fetch_usage(&self) -> Result<ApiResult, String> {
        let url = format!(
            "{}/api/organizations/{}/usage",
            BASE_URL, self.org_id
        );

        let response = self
            .client
            .get(&url)
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if response.status() == 401 || response.status() == 403 {
            return Err("Session expired. Please update your session key.".to_string());
        }

        if !response.status().is_success() {
            return Err(format!("API error: HTTP {}", response.status()));
        }

        // Check for refreshed session key in Set-Cookie header
        let refreshed_session_key = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .find_map(|val| {
                let s = val.to_str().ok()?;
                if s.starts_with("sessionKey=") {
                    let key = s
                        .split(';')
                        .next()?
                        .strip_prefix("sessionKey=")?;
                    Some(key.to_string())
                } else {
                    None
                }
            });

        let usage: ApiUsageResponse = response
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        Ok(ApiResult {
            usage,
            refreshed_session_key,
        })
    }
}
