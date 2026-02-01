//! Worker pool for parallel vanity address generation.
//!
//! This module provides:
//! - Multi-threaded CPU workers
//! - Coordinated work distribution
//! - Progress tracking and reporting
//!
//! ## Future Extensions
//! - GPU workers (OpenCL/CUDA)
//! - Distributed workers (network)

mod cpu;
mod pool;

pub use cpu::CpuWorker;
pub use pool::{VanityResult, WorkerPool};
