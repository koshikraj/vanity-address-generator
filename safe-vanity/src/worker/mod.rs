//! Worker pool and CPU worker for Safe vanity mining.

mod cpu;
mod pool;

pub use cpu::{CpuWorker, WorkerStats};
pub use pool::{SafeVanityResult, WorkerPool};
