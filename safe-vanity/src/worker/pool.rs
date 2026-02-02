//! Worker pool for Safe vanity mining.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver};

use crate::matcher::Pattern;

use super::cpu::{CpuWorker, WorkerStats};

/// Result of a successful Safe vanity match.
#[derive(Debug, Clone)]
pub struct SafeVanityResult {
    /// The salt nonce (32 bytes) to use with SafeProxyFactory.createProxyWithNonce.
    pub salt_nonce: [u8; 32],
    /// The predicted Safe proxy address (20 bytes).
    pub address: [u8; 20],
    /// Worker ID that found it.
    pub worker_id: usize,
}

impl SafeVanityResult {
    /// Salt nonce as hex (no 0x).
    pub fn salt_nonce_hex(&self) -> String {
        hex::encode(self.salt_nonce)
    }

    /// Salt nonce as decimal string (for Safe SDK `safeDeploymentConfig.saltNonce`).
    pub fn salt_nonce_decimal(&self) -> String {
        bytes_to_decimal(&self.salt_nonce)
    }

    /// Address as checksummed hex (0x...).
    pub fn address_checksum(&self) -> String {
        crate::matcher::Address::from_bytes(self.address).to_checksum()
    }
}

/// Convert a big-endian byte array to decimal string without bigint crate.
fn bytes_to_decimal(bytes: &[u8; 32]) -> String {
    // Skip leading zeros
    let first_nonzero = bytes.iter().position(|&b| b != 0);
    let Some(start) = first_nonzero else {
        return "0".to_string();
    };

    // Build decimal digits by repeated base-256 to base-10 conversion.
    // Work with the significant bytes only.
    let significant = &bytes[start..];
    let mut digits: Vec<u8> = vec![0]; // decimal digits (least significant first)

    for &byte in significant {
        // Multiply existing digits by 256 and add current byte
        let mut carry = byte as u32;
        for d in digits.iter_mut() {
            let val = (*d as u32) * 256 + carry;
            *d = (val % 10) as u8;
            carry = val / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }

    digits.iter().rev().map(|d| (b'0' + d) as char).collect()
}

pub struct WorkerPool {
    num_workers: usize,
    pattern: Pattern,
    handles: Option<Vec<JoinHandle<()>>>,
    result_rx: Receiver<SafeVanityResult>,
    stop_flag: Arc<AtomicBool>,
    stats: Arc<WorkerStats>,
    start_time: Instant,
}

impl WorkerPool {
    pub fn new(
        num_workers: usize,
        pattern: Pattern,
        factory: [u8; 20],
        init_code_hash: [u8; 32],
        initializer_hash: [u8; 32],
    ) -> Self {
        let (result_tx, result_rx) = bounded(100);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(WorkerStats::new());

        let handles = (0..num_workers)
            .map(|id| {
                let pattern = pattern.clone();
                let result_tx = result_tx.clone();
                let stop_flag = stop_flag.clone();
                let stats = stats.clone();

                thread::Builder::new()
                    .name(format!("safe-vanity-worker-{}", id))
                    .spawn(move || {
                        let worker = CpuWorker::new(
                            id,
                            pattern,
                            factory,
                            init_code_hash,
                            initializer_hash,
                            result_tx,
                            stop_flag,
                            stats,
                        );
                        worker.run();
                    })
                    .expect("spawn worker")
            })
            .collect();

        drop(result_tx);

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

    pub fn wait_for_result(&self, timeout: Duration) -> Option<SafeVanityResult> {
        self.result_rx.recv_timeout(timeout).ok()
    }

    pub fn try_recv(&self) -> Option<SafeVanityResult> {
        self.result_rx.try_recv().ok()
    }

    pub fn results(&self) -> impl Iterator<Item = SafeVanityResult> + '_ {
        self.result_rx.iter()
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn join(mut self) {
        self.stop();
        if let Some(h) = self.handles.take() {
            for handle in h {
                let _ = handle.join();
            }
        }
    }

    pub fn num_workers(&self) -> usize {
        self.num_workers
    }
    pub fn pattern(&self) -> &Pattern {
        &self.pattern
    }
    pub fn total_salts(&self) -> u64 {
        self.stats.total_salts()
    }
    pub fn total_matches(&self) -> u64 {
        self.stats.total_matches()
    }
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
    pub fn salts_per_second(&self) -> f64 {
        let t = self.elapsed().as_secs_f64();
        if t > 0.0 {
            self.total_salts() as f64 / t
        } else {
            0.0
        }
    }
    pub fn stop_flag_clone(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }
    pub fn is_stopped(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.stop();
        if let Some(h) = self.handles.take() {
            for handle in h {
                let _ = handle.join();
            }
        }
    }
}
