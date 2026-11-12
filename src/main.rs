use chrono::Utc;
use std::env;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tyyoj_contracts::JudgerHeartbeat;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| "worker-local-1".into());
    let container_id = env::var("CONTAINER_ID").unwrap_or_else(|_| "standalone".into());

    tracing::info!("Starting persistent tyyoj-judger daemon: worker_id={}", worker_id);

    let heartbeat = JudgerHeartbeat {
        worker_id,
        container_id,
        timestamp: Utc::now(),
        is_busy: false,
        current_job_id: None,
    };

    tracing::info!("Worker initialized: {:?}", heartbeat);
    tokio::time::sleep(Duration::from_millis(50)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_creation() {
        let heartbeat = JudgerHeartbeat {
            worker_id: "test-worker".into(),
            container_id: "test-container".into(),
            timestamp: Utc::now(),
            is_busy: false,
            current_job_id: None,
        };
        assert_eq!(heartbeat.worker_id, "test-worker");
    }
}
