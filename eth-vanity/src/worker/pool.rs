//! Worker pool management.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, Sender};

use crate::matcher::Pattern;

#[cfg(feature = "gpu")]
use super::gpu::GpuWorker;
use super::cpu::{CpuWorker, WorkerStats};

/// Result of a successful vanity address generation.
#[derive(Debug, Clone)]
pub struct VanityResult {
    /// The private key (hex encoded, no 0x prefix)
    pub private_key: String,
    /// The Ethereum address (checksummed with 0x prefix)
    pub address: String,
    /// The ID of the worker that found this result
    pub worker_id: usize,
}

/// Manages a pool of workers for parallel vanity address generation.
pub struct WorkerPool {
    /// Number of workers
    num_workers: usize,
    /// The pattern to search for
    pattern: Pattern,
    /// Worker thread handles (Option to allow taking during join)
    handles: Option<Vec<JoinHandle<()>>>,
    /// Channel receiver for results
    result_rx: Receiver<VanityResult>,
    /// Shared stop flag
    stop_flag: Arc<AtomicBool>,
    /// Shared statistics
    stats: Arc<WorkerStats>,
    /// Start time
    start_time: Instant,
}

impl WorkerPool {
    /// Creates a new worker pool with the specified number of workers.
    pub fn new(num_workers: usize, pattern: Pattern) -> Self {
        let (result_tx, result_rx) = bounded(100);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(WorkerStats::new());

        let handles = Self::spawn_workers(
            num_workers,
            pattern.clone(),
            result_tx,
            stop_flag.clone(),
            stats.clone(),
        );

        Self {
            num_workers,
            pattern,
            handles: Some(handles),
            result_rx,
            stop_flag,
            stats,
            start_time: Instant::now(),
        }
    }

    /// Creates a new worker pool with optional GPU acceleration.
    #[cfg(feature = "gpu")]
    pub fn new_with_gpu(
        num_cpu_workers: usize,
        pattern: Pattern,
        enable_gpu: bool,
        gpu_device: usize,
        gpu_work_size: usize,
    ) -> Self {
        let (result_tx, result_rx) = bounded(100);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(WorkerStats::new());

        let mut handles = Self::spawn_workers(
            num_cpu_workers,
            pattern.clone(),
            result_tx.clone(),
            stop_flag.clone(),
            stats.clone(),
        );

        let mut gpu_active = false;

        if enable_gpu {
            let gpu_pattern = pattern.clone();
            let gpu_tx = result_tx.clone();
            let gpu_stop = stop_flag.clone();
            let gpu_stats = stats.clone();
            let gpu_id = num_cpu_workers; // GPU worker gets next ID

            match GpuWorker::new(
                gpu_id,
                gpu_pattern.clone(),
                gpu_tx.clone(),
                gpu_stop.clone(),
                gpu_stats.clone(),
                gpu_device,
                gpu_work_size,
            ) {
                Ok(gpu_worker) => {
                    let handle = thread::Builder::new()
                        .name("vanity-gpu-worker".into())
                        .spawn(move || {
                            gpu_worker.run();
                        })
                        .expect("Failed to spawn GPU worker thread");
                    handles.push(handle);
                    gpu_active = true;
                }
                Err(e) => {
                    eprintln!("Warning: GPU initialization failed: {}", e);
                    eprintln!("Continuing with CPU-only workers.");
                }
            }
        }

        // Drop the extra sender clone so the channel closes when all workers finish
        drop(result_tx);

        let total_workers = if gpu_active {
            num_cpu_workers + 1
        } else {
            num_cpu_workers
        };

        Self {
            num_workers: total_workers,
            pattern,
            handles: Some(handles),
            result_rx,
            stop_flag,
            stats,
            start_time: Instant::now(),
        }
    }

    /// Spawns worker threads.
    fn spawn_workers(
        num_workers: usize,
        pattern: Pattern,
        result_tx: Sender<VanityResult>,
        stop_flag: Arc<AtomicBool>,
        stats: Arc<WorkerStats>,
    ) -> Vec<JoinHandle<()>> {
        (0..num_workers)
            .map(|id| {
                let pattern = pattern.clone();
                let result_tx = result_tx.clone();
                let stop_flag = stop_flag.clone();
                let stats = stats.clone();

                thread::Builder::new()
                    .name(format!("vanity-worker-{}", id))
                    .spawn(move || {
                        let worker = CpuWorker::new(id, pattern, result_tx, stop_flag, stats);
                        worker.run();
                    })
                    .expect("Failed to spawn worker thread")
            })
            .collect()
    }

    /// Waits for a result with optional timeout.
    ///
    /// Returns `Some(result)` if a match is found, `None` if timeout expires.
    pub fn wait_for_result(&self, timeout: Duration) -> Option<VanityResult> {
        self.result_rx.recv_timeout(timeout).ok()
    }

    /// Attempts to receive a result without blocking.
    pub fn try_recv(&self) -> Option<VanityResult> {
        self.result_rx.try_recv().ok()
    }

    /// Returns an iterator over results (blocking).
    pub fn results(&self) -> impl Iterator<Item = VanityResult> + '_ {
        self.result_rx.iter()
    }

    /// Signals all workers to stop.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    /// Waits for all workers to complete.
    pub fn join(mut self) {
        self.stop();
        if let Some(handles) = self.handles.take() {
            for handle in handles {
                let _ = handle.join();
            }
        }
    }

    /// Returns the number of workers.
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }

    /// Returns the pattern being searched for.
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }

    /// Returns the total keys generated across all workers.
    pub fn total_keys(&self) -> u64 {
        self.stats.total_keys()
    }

    /// Returns the total matches found.
    pub fn total_matches(&self) -> u64 {
        self.stats.total_matches()
    }

    /// Returns the elapsed time since the pool was created.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Returns the current generation rate (keys per second).
    pub fn keys_per_second(&self) -> f64 {
        let elapsed = self.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.total_keys() as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Returns a clone of the stop flag for external use (e.g., signal handlers).
    pub fn stop_flag_clone(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Returns true if the pool has been signaled to stop.
    pub fn is_stopped(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.stop();
        // Wait for workers to finish if they haven't been joined
        if let Some(handles) = self.handles.take() {
            for handle in handles {
                let _ = handle.join();
            }
        }
    }
}
