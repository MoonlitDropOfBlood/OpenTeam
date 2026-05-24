use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RateLimiter {
    rpm: u32,
    state: Arc<Mutex<RateLimiterState>>,
}

struct RateLimiterState {
    timestamps: Vec<Instant>,
}

impl RateLimiter {
    pub fn new(rpm: u32) -> Self {
        Self {
            rpm,
            state: Arc::new(Mutex::new(RateLimiterState {
                timestamps: Vec::new(),
            })),
        }
    }

    pub async fn acquire(&self) {
        let mut state = self.state.lock().await;
        let now = Instant::now();

        // Prune timestamps older than 1 minute
        state.timestamps.retain(|t| now.duration_since(*t).as_secs() < 60);

        if state.timestamps.len() >= self.rpm as usize {
            let oldest = state.timestamps[0];
            let wait = 60u64.saturating_sub(now.duration_since(oldest).as_secs());
            if wait > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            }
        }

        state.timestamps.push(now);
    }
}
