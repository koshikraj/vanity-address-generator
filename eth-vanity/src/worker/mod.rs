//! Worker pool for parallel vanity address generation.
//!
//! This module provides:
//! - Multi-threaded CPU workers
//! - GPU workers (OpenCL, behind `gpu` feature flag)
//! - Coordinated work distribution
//! - Progress tracking and reporting

mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;
mod pool;

pub use cpu::CpuWorker;
#[cfg(feature = "gpu")]
pub use gpu::GpuWorker;
pub use pool::{VanityResult, WorkerPool};
