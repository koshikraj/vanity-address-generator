//! CPU-based worker for vanity address generation.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;

use crate::crypto::Keypair;
use crate::matcher::Pattern;

use super::VanityResult;

/// Statistics for a CPU worker.
#[derive(Debug, Default)]
pub struct WorkerStats {
    /// Total keys generated
    pub keys_generated: AtomicU64,
    /// Matches found
    pub matches_found: AtomicU64,
}

impl WorkerStats {
    /// Creates new worker stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the total keys generated.
    pub fn total_keys(&self) -> u64 {
        self.keys_generated.load(Ordering::Relaxed)
    }

    /// Returns the total matches found.
    pub fn total_matches(&self) -> u64 {
        self.matches_found.load(Ordering::Relaxed)
    }
}

/// A CPU worker that generates and tests keypairs.
pub struct CpuWorker {
    /// Worker ID
    id: usize,
    /// The pattern to match against
    pattern: Pattern,
    /// Channel to send results
    result_tx: Sender<VanityResult>,
    /// Shared stop flag
    stop_flag: Arc<AtomicBool>,
    /// Worker statistics
    stats: Arc<WorkerStats>,
}

impl CpuWorker {
    /// Creates a new CPU worker.
    pub fn new(
        id: usize,
        pattern: Pattern,
        result_tx: Sender<VanityResult>,
        stop_flag: Arc<AtomicBool>,
        stats: Arc<WorkerStats>,
    ) -> Self {
        Self {
            id,
            pattern,
            result_tx,
            stop_flag,
            stats,
        }
    }

    /// Runs the worker loop.
    ///
    /// Generates keypairs and tests them against the pattern until:
    /// - A match is found (sends result through channel)
    /// - Stop flag is set
    /// - Channel is closed
    pub fn run(&self) {
        // Process in batches to reduce atomic operation overhead
        const BATCH_SIZE: u64 = 1000;

        loop {
            // Check stop flag
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            // Generate and test a batch of keypairs
            for _ in 0..BATCH_SIZE {
                let keypair = Keypair::generate();

                if self.pattern.matches(keypair.address()).is_match() {
                    // Found a match!
                    self.stats.matches_found.fetch_add(1, Ordering::Relaxed);

                    let result = VanityResult {
                        private_key: keypair.private_key_hex(),
                        address: keypair.address().to_checksum(),
                        worker_id: self.id,
                    };

                    // Try to send result (ignore if channel closed)
                    let _ = self.result_tx.send(result);
                }
            }

            // Update stats
            self.stats.keys_generated.fetch_add(BATCH_SIZE, Ordering::Relaxed);
        }
    }

    /// Returns the worker ID.
    pub fn id(&self) -> usize {
        self.id
    }
}
