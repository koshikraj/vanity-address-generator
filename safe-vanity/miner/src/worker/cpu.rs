//! CPU worker for Safe vanity address mining.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use rand::RngCore;

use crate::crypto::create2::{safe_address, safe_salt};
use crate::matcher::{Address, Pattern};

use super::SafeVanityResult;

#[derive(Debug, Default)]
pub struct WorkerStats {
    pub salts_tried: AtomicU64,
    pub matches_found: AtomicU64,
}

impl WorkerStats {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn total_salts(&self) -> u64 {
        self.salts_tried.load(Ordering::Relaxed)
    }
    pub fn total_matches(&self) -> u64 {
        self.matches_found.load(Ordering::Relaxed)
    }
}

pub struct CpuWorker {
    id: usize,
    pattern: Pattern,
    factory: [u8; 20],
    init_code_hash: [u8; 32],
    initializer_hash: [u8; 32],
    result_tx: Sender<SafeVanityResult>,
    stop_flag: Arc<AtomicBool>,
    stats: Arc<WorkerStats>,
}

impl CpuWorker {
    pub fn new(
        id: usize,
        pattern: Pattern,
        factory: [u8; 20],
        init_code_hash: [u8; 32],
        initializer_hash: [u8; 32],
        result_tx: Sender<SafeVanityResult>,
        stop_flag: Arc<AtomicBool>,
        stats: Arc<WorkerStats>,
    ) -> Self {
        Self {
            id,
            pattern,
            factory,
            init_code_hash,
            initializer_hash,
            result_tx,
            stop_flag,
            stats,
        }
    }

    pub fn run(&self) {
        const BATCH_SIZE: u64 = 1000;

        // Each worker starts from a random nonce and increments sequentially.
        // This avoids per-iteration RNG overhead while ensuring workers explore
        // different regions of the 256-bit nonce space.
        let mut salt_nonce = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut salt_nonce);

        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            for _ in 0..BATCH_SIZE {
                let salt = safe_salt(&self.initializer_hash, &salt_nonce);
                let addr = safe_address(&self.factory, &self.init_code_hash, &salt);
                let address = Address::from_bytes(addr);

                if self.pattern.matches(&address).is_match() {
                    self.stats.matches_found.fetch_add(1, Ordering::Relaxed);
                    let result = SafeVanityResult {
                        salt_nonce,
                        address: addr,
                        worker_id: self.id,
                    };
                    let _ = self.result_tx.send(result);
                }

                // Increment nonce as a 256-bit big-endian counter
                increment_nonce(&mut salt_nonce);
            }

            self.stats.salts_tried.fetch_add(BATCH_SIZE, Ordering::Relaxed);
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

/// Increment a 32-byte big-endian integer by 1 (with wrapping).
#[inline]
fn increment_nonce(nonce: &mut [u8; 32]) {
    for byte in nonce.iter_mut().rev() {
        let (val, overflow) = byte.overflowing_add(1);
        *byte = val;
        if !overflow {
            return;
        }
    }
}
