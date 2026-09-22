use crate::commands::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub struct QueueWorker {
    state: AppState,
    shutdown_requested: Arc<AtomicBool>,
    thread_exited: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl QueueWorker {
    pub fn start(state: AppState) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let thread_exited = Arc::new(AtomicBool::new(false));
        let thread_exited_clone = Arc::clone(&thread_exited);
        let worker_state = state.clone();

        let handle = std::thread::Builder::new()
            .name("resvera-queue-worker".to_string())
            .spawn(move || {
                info!("Resvera background queue worker started");
                while !shutdown_clone.load(Ordering::Relaxed) {
                    if worker_state.orchestrator.is_paused() {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }

                    match worker_state.orchestrator.process_next_job() {
                        Ok(Some(completed_job)) => {
                            if let Some(out_path) = &completed_job.output_path {
                                worker_state
                                    .preview_scope
                                    .allow_file(std::path::Path::new(out_path));
                            }
                            info!(
                                job_id = %completed_job.id,
                                state = %completed_job.state,
                                "Background worker processed job"
                            );
                        }
                        Ok(None) => {
                            // Queue is empty or paused, back off briefly
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(err) => {
                            error!(error = %err, "Background worker encountered error");
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
                thread_exited_clone.store(true, Ordering::SeqCst);
                info!("Resvera background queue worker stopped");
            })
            .expect("Failed to spawn background queue worker thread");

        Self {
            state,
            shutdown_requested: shutdown,
            thread_exited,
            handle: Some(handle),
        }
    }

    pub fn stop_with_timeout(&mut self, timeout: Duration) -> bool {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        let clean = self.state.orchestrator.shutdown(timeout).unwrap_or(false);

        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.thread_exited.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }

        if self.thread_exited.load(Ordering::SeqCst) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            true
        } else {
            warn!("Queue worker thread did not terminate within bounded timeout; proceeding with shutdown persistence");
            let _ = self.state.orchestrator.db.run_crash_recovery_sweep();
            clean
        }
    }

    pub fn stop(&mut self) {
        self.stop_with_timeout(Duration::from_secs(3));
    }
}

impl Drop for QueueWorker {
    fn drop(&mut self) {
        self.stop();
    }
}
