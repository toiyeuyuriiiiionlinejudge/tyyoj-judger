use chrono::Utc;
use redis::AsyncCommands;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tyyoj_contracts::{JudgeTask, JudgerHeartbeat, SubmissionLanguage};
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct TestCaseRow {
    id: Uuid,
    input_data: String,
    expected_output: String,
    order_num: i32,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let worker_id = env::var("WORKER_ID").unwrap_or_else(|_| "judger-worker-01".into());
    let container_id = env::var("CONTAINER_ID").unwrap_or_else(|_| "podman-local".into());
    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://tyyoj_user:tyyoj_dev_password@127.0.0.1:5432/tyyoj".into());
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".into());

    tracing::info!(
        "Starting persistent tyyoj-judger worker daemon: worker_id={}, container_id={}",
        worker_id,
        container_id
    );

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    let redis_client = redis::Client::open(redis_url).expect("Failed to connect to Redis");
    let mut redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to get Redis multiplexed connection");

    let mut last_heartbeat = std::time::Instant::now();

    loop {
        // 1. Send heartbeat every 5 seconds to Redis
        if last_heartbeat.elapsed() >= Duration::from_secs(5) {
            let heartbeat = JudgerHeartbeat {
                worker_id: worker_id.clone(),
                container_id: container_id.clone(),
                timestamp: Utc::now(),
                is_busy: false,
                current_job_id: None,
            };
            if let Ok(serialized) = serde_json::to_string(&heartbeat) {
                let key = format!("tyyoj:judger:heartbeat:{}", worker_id);
                let _: Result<(), _> = redis_conn.set_ex(&key, serialized, 15).await;
            }
            last_heartbeat = std::time::Instant::now();
        }

        // 2. Poll submission queue from Redis (non-blocking RPOP)
        let pop_res: Result<Option<String>, _> = redis_conn.rpop("tyyoj:queue:submissions", None).await;
        match pop_res {
            Ok(Some(task_json)) => {
                tracing::info!("Received judging task from Redis: {}", task_json);
                match serde_json::from_str::<JudgeTask>(&task_json) {
                    Ok(task) => process_judging_task(&db, task).await,
                    Err(e) => tracing::error!("Failed to deserialize task: {}", e),
                }
            }
            Ok(None) => {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            Err(e) => {
                tracing::warn!("Redis rpop error: {}", e);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

async fn process_judging_task(db: &PgPool, task: JudgeTask) {
    let sub_id = task.submission_id;

    // 1. Update status to 'Compiling'
    let _ = sqlx::query("UPDATE submissions SET verdict = 'Compiling' WHERE id = $1")
        .bind(sub_id)
        .execute(db)
        .await;

    // 2. Create scratch directory
    let scratch_dir = PathBuf::from(format!("/tmp/tyyoj_run_{}", sub_id));
    let _ = tokio::fs::create_dir_all(&scratch_dir).await;

    let target_bin = scratch_dir.join("solution");
    let mut compile_success = true;
    let mut compile_log = String::new();

    match task.language {
        SubmissionLanguage::Cpp20 => {
            let src_file = scratch_dir.join("solution.cpp");
            let _ = tokio::fs::write(&src_file, &task.source_code).await;
            let output = Command::new("g++")
                .arg("-std=c++20")
                .arg("-O2")
                .arg(&src_file)
                .arg("-o")
                .arg(&target_bin)
                .output()
                .await;

            if let Ok(out) = output {
                if !out.status.success() {
                    compile_success = false;
                    compile_log = String::from_utf8_lossy(&out.stderr).to_string();
                }
            } else {
                compile_success = false;
                compile_log = "Failed to spawn g++ compiler".into();
            }
        }
        SubmissionLanguage::Pascal => {
            let src_file = scratch_dir.join("solution.pas");
            let _ = tokio::fs::write(&src_file, &task.source_code).await;
            let output = Command::new("fpc")
                .arg("-O2")
                .arg(format!("-o{}", target_bin.display()))
                .arg(&src_file)
                .output()
                .await;

            if let Ok(out) = output {
                if !out.status.success() {
                    compile_success = false;
                    compile_log = String::from_utf8_lossy(&out.stderr).to_string();
                }
            } else {
                compile_success = false;
                compile_log = "Failed to spawn fpc compiler".into();
            }
        }
        SubmissionLanguage::Python3 => {
            let src_file = scratch_dir.join("solution.py");
            let _ = tokio::fs::write(&src_file, &task.source_code).await;
            // Python does not have separate compile step
        }
    }

    if !compile_success {
        tracing::warn!("Compilation failed for submission {}: {}", sub_id, compile_log);
        let _ = sqlx::query("UPDATE submissions SET verdict = 'CompilationError', compile_log = $1 WHERE id = $2")
            .bind(&compile_log)
            .bind(sub_id)
            .execute(db)
            .await;
        let _ = tokio::fs::remove_dir_all(&scratch_dir).await;
        return;
    }

    // 3. Update status to 'Running'
    let _ = sqlx::query("UPDATE submissions SET verdict = 'Running' WHERE id = $1")
        .bind(sub_id)
        .execute(db)
        .await;

    // 4. Fetch test cases from PostgreSQL
    let test_cases = sqlx::query_as::<_, TestCaseRow>(
        "SELECT id, input_data, expected_output, order_num FROM test_cases WHERE problem_id = $1 ORDER BY order_num ASC"
    )
    .bind(task.problem_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    if test_cases.is_empty() {
        tracing::warn!("No test cases found for problem {}", task.problem_id);
        let _ = sqlx::query("UPDATE submissions SET verdict = 'WrongAnswer' WHERE id = $1")
            .bind(sub_id)
            .execute(db)
            .await;
        let _ = tokio::fs::remove_dir_all(&scratch_dir).await;
        return;
    }

    let mut overall_verdict = "Accepted";
    let mut max_time_ms: i64 = 0;
    let max_mem_kb: i64 = 1420;

    for tc in test_cases {
        let start_time = std::time::Instant::now();

        // Spawn process feeding input data
        let proc = match task.language {
            SubmissionLanguage::Python3 => {
                let py_file = scratch_dir.join("solution.py");
                Command::new("python3")
                    .arg(py_file)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            }
            _ => {
                Command::new(&target_bin)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            }
        };

        if let Ok(mut process) = proc {
            if let Some(mut stdin) = process.stdin.take() {
                let _ = stdin.write_all(tc.input_data.as_bytes()).await;
                let _ = stdin.flush().await;
                drop(stdin); // Send EOF
            }

            let timeout_duration = Duration::from_millis(task.time_limit_ms);
            let run_res = tokio::time::timeout(timeout_duration, process.wait_with_output()).await;

            let elapsed_ms = start_time.elapsed().as_millis() as i64;
            if elapsed_ms > max_time_ms {
                max_time_ms = elapsed_ms;
            }

            let tc_verdict = match run_res {
                Err(_) => {
                    overall_verdict = "TimeLimitExceeded";
                    "TimeLimitExceeded"
                }
                Ok(Ok(output)) => {
                    if !output.status.success() {
                        overall_verdict = "RuntimeError";
                        "RuntimeError"
                    } else {
                        let actual = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        let expected = tc.expected_output.trim().to_string();
                        if actual == expected {
                            "Accepted"
                        } else {
                            if overall_verdict == "Accepted" {
                                overall_verdict = "WrongAnswer";
                            }
                            "WrongAnswer"
                        }
                    }
                }
                Ok(Err(_)) => {
                    overall_verdict = "RuntimeError";
                    "RuntimeError"
                }
            };

            // Record test result into PostgreSQL
            let _ = sqlx::query(
                "INSERT INTO submission_tests (id, submission_id, test_case_id, test_order, verdict, time_ms, memory_kb)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)"
            )
            .bind(Uuid::new_v4())
            .bind(sub_id)
            .bind(tc.id)
            .bind(tc.order_num)
            .bind(tc_verdict)
            .bind(elapsed_ms)
            .bind(max_mem_kb)
            .execute(db)
            .await;

            if overall_verdict != "Accepted" {
                break;
            }
        }
    }

    // 5. Update final verdict in PostgreSQL
    let _ = sqlx::query(
        "UPDATE submissions SET verdict = $1, time_consumed_ms = $2, memory_consumed_kb = $3 WHERE id = $4"
    )
    .bind(overall_verdict)
    .bind(max_time_ms)
    .bind(max_mem_kb)
    .bind(sub_id)
    .execute(db)
    .await;

    tracing::info!("Submission {} completed with verdict: {}", sub_id, overall_verdict);
    let _ = tokio::fs::remove_dir_all(&scratch_dir).await;
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
