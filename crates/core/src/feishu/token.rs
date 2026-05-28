use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use crate::CoreError;

/// Cache TAT for 90 minutes (token valid for 2h, refresh 30min before expiry)
const TOKEN_REFRESH_INTERVAL: Duration = Duration::from_secs(90 * 60);
/// Initial fetch timeout.
const TOKEN_FETCH_TIMEOUT: Duration = Duration::from_secs(10);

struct TokenCache {
    token: String,
    fetched_at: Instant,
}

/// Manages Feishu Tenant Access Token (TAT) lifecycle.
///
/// Fetches from `POST https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal`
/// using `FEISHU_APP_ID` / `FEISHU_APP_SECRET` env vars.
/// Caches and auto-refreshes before expiry.
pub struct FeishuTokenManager {
    cache: Arc<RwLock<Option<TokenCache>>>,
    app_id: String,
    app_secret: String,
}

impl FeishuTokenManager {
    pub fn new(app_id: String, app_secret: String) -> Self {
        Self {
            cache: Arc::new(RwLock::new(None)),
            app_id,
            app_secret,
        }
    }

    /// Get a valid tenant_access_token. Refreshes if cache is stale.
    pub async fn get_token(&self) -> Result<String, CoreError> {
        // Fast path: check if cache is still fresh
        {
            let guard = self.cache.read().await;
            if let Some(cache) = guard.as_ref() {
                if cache.fetched_at.elapsed() < TOKEN_REFRESH_INTERVAL {
                    return Ok(cache.token.clone());
                }
            }
        }

        // Slow path: fetch new token
        self.refresh_token().await
    }

    /// Force-refresh, even if current token is still valid.
    pub async fn refresh_token(&self) -> Result<String, CoreError> {
        let client = reqwest::Client::builder()
            .timeout(TOKEN_FETCH_TIMEOUT)
            .build()
            .map_err(|e| CoreError::Feishu(format!("HTTP client: {e}")))?;

        let resp = client
            .post("https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal")
            .json(&serde_json::json!({
                "app_id": self.app_id,
                "app_secret": self.app_secret,
            }))
            .send()
            .await
            .map_err(|e| CoreError::Feishu(format!("Fetch TAT: {e}")))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Feishu(format!("Parse TAT response: {e}")))?;

        let token = body["tenant_access_token"]
            .as_str()
            .ok_or_else(|| {
                let msg = body["msg"].as_str().unwrap_or("unknown error");
                CoreError::Feishu(format!("TAT fetch failed: {msg}"))
            })?
            .to_string();

        // Update cache
        let mut guard = self.cache.write().await;
        *guard = Some(TokenCache {
            token: token.clone(),
            fetched_at: Instant::now(),
        });

        tracing::info!("[TokenManager] TAT refreshed successfully");
        Ok(token)
    }
}