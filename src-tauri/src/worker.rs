use crate::commands::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

pub struct QueueWorker {
    shutdown_requested: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl QueueWorker {
    pub fn start(state: AppState) -> Self {
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);

        let handle = std::thread::Builder::new()
            .name("resvera-queue-worker".to_string())
            .spawn(move || {
                info!("Resvera background queue worker started");
                while !shutdown_clone.load(Ordering::Relaxed) {
                    if state.orchestrator.is_paused() {
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }

                    match state.orchestrator.process_next_job() {
                        Ok(Some(completed_job)) => {
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
                info!("Resvera background queue worker stopped");
            })
            .expect("Failed to spawn background queue worker thread");

        Self {
            shutdown_requested: shutdown,
            handle: Some(handle),
        }
    }

    pub fn stop(&mut self) {
        self.shutdown_requested.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for QueueWorker {
    fn drop(&mut self) {
        self.stop();
    }
}
