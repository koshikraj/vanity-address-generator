//! GPU-based worker for vanity address generation using OpenCL.
//!
//! Uses the incremental key approach:
//! 1. CPU generates random base private key
//! 2. CPU computes base public key Q = k * G
//! 3. GPU computes Q + i*G for millions of offsets in parallel
//! 4. GPU runs keccak256 and pattern matching
//! 5. CPU reconstructs private key for any matches

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use opencl3::command_queue::{CommandQueue, CL_QUEUE_PROFILING_ENABLE};
use opencl3::context::Context;
use opencl3::device::{get_all_devices, Device, CL_DEVICE_TYPE_GPU};
use opencl3::kernel::{ExecuteKernel, Kernel};
use opencl3::memory::{Buffer, CL_MEM_READ_ONLY, CL_MEM_READ_WRITE, CL_MEM_WRITE_ONLY};
use opencl3::program::Program;
use opencl3::types::{cl_uchar, cl_uint, CL_BLOCKING};
use secp256k1::{PublicKey, Secp256k1, SecretKey};

use crate::crypto::Keypair;
use crate::matcher::Pattern;

use super::cpu::WorkerStats;
use super::VanityResult;

/// OpenCL kernel source
const KERNEL_SOURCE: &str = include_str!("../../kernels/vanity.cl");

/// Maximum number of results per batch
const MAX_RESULTS_PER_BATCH: u32 = 256;

/// Errors that can occur during GPU operations.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("No GPU device found")]
    DeviceNotFound,

    #[error("GPU initialization failed: {0}")]
    InitFailed(String),

    #[error("Kernel compilation failed: {0}")]
    KernelCompile(String),

    #[error("Buffer operation failed: {0}")]
    BufferError(String),

    #[error("Kernel execution failed: {0}")]
    KernelExec(String),
}

/// Pattern configuration matching the GPU kernel's `gpu_pattern_config_t`.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct GpuPatternConfig {
    pattern_type: u32,       // 0=prefix, 1=suffix, 2=contains, 3=prefix+suffix
    pattern_len: u32,        // prefix pattern length in nibbles
    suffix_len: u32,         // suffix pattern length in nibbles
    _pad: u32,
    pattern_nibbles: [u8; 40],
    suffix_nibbles: [u8; 40],
}

/// Result entry matching the GPU kernel's `gpu_result_t`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuResult {
    found: u32,
    offset: u32,
    addr: [u8; 20],
}

/// Lists available OpenCL GPU devices.
pub fn list_devices() -> Vec<String> {
    match get_all_devices(CL_DEVICE_TYPE_GPU) {
        Ok(device_ids) => device_ids
            .iter()
            .filter_map(|&id| {
                let dev = Device::new(id);
                dev.name().ok()
            })
            .collect(),
        Err(_) => vec![],
    }
}

/// GPU worker that uses OpenCL for parallel vanity address generation.
pub struct GpuWorker {
    /// Worker ID
    id: usize,
    /// Pattern to match
    pattern: Pattern,
    /// Channel to send results
    result_tx: Sender<VanityResult>,
    /// Shared stop flag
    stop_flag: Arc<AtomicBool>,
    /// Shared statistics
    stats: Arc<WorkerStats>,
    /// OpenCL context
    context: Context,
    /// OpenCL command queue
    queue: CommandQueue,
    /// Compiled kernel
    kernel: Kernel,
    /// Work size (number of keys per batch)
    work_size: usize,
    /// Precomputed G table (32 entries of 2^k * G, each 64 bytes)
    g_table: Vec<u8>,
}

impl GpuWorker {
    /// Creates a new GPU worker.
    pub fn new(
        id: usize,
        pattern: Pattern,
        result_tx: Sender<VanityResult>,
        stop_flag: Arc<AtomicBool>,
        stats: Arc<WorkerStats>,
        device_index: usize,
        work_size: usize,
    ) -> Result<Self, GpuError> {
        // Get GPU devices
        let device_ids =
            get_all_devices(CL_DEVICE_TYPE_GPU).map_err(|e| GpuError::InitFailed(e.to_string()))?;

        if device_ids.is_empty() {
            return Err(GpuError::DeviceNotFound);
        }

        if device_index >= device_ids.len() {
            return Err(GpuError::DeviceNotFound);
        }

        let device = Device::new(device_ids[device_index]);
        let device_name = device.name().unwrap_or_else(|_| "Unknown".into());
        eprintln!("GPU Worker {}: Using device: {}", id, device_name);

        // Create context and queue
        let context = Context::from_device(&device)
            .map_err(|e| GpuError::InitFailed(e.to_string()))?;

        let queue =
            CommandQueue::create_default_with_properties(&context, CL_QUEUE_PROFILING_ENABLE, 0)
                .map_err(|e| GpuError::InitFailed(e.to_string()))?;

        // Compile kernel (NVIDIA's OpenCL compiler can be slow with complex math kernels)
        eprintln!("GPU Worker {}: Compiling OpenCL kernel...", id);
        let program = Program::create_and_build_from_source(&context, KERNEL_SOURCE, "-cl-mad-enable")
            .map_err(|e| GpuError::KernelCompile(e.to_string()))?;
        eprintln!("GPU Worker {}: Kernel compiled successfully", id);

        let kernel = Kernel::create(&program, "vanity_iterate_and_match")
            .map_err(|e| GpuError::KernelCompile(e.to_string()))?;

        // Precompute G table: 2^0 * G, 2^1 * G, ..., 2^31 * G
        let g_table = Self::compute_g_table();

        Ok(Self {
            id,
            pattern,
            result_tx,
            stop_flag,
            stats,
            context,
            queue,
            kernel,
            work_size,
            g_table,
        })
    }

    /// Precomputes the table of 2^k * G for k = 0..31.
    /// Each entry is 64 bytes: x (32 bytes BE) || y (32 bytes BE).
    fn compute_g_table() -> Vec<u8> {
        let secp = Secp256k1::new();
        let mut table = vec![0u8; 32 * 64];

        // Start with G (private key = 1)
        let mut scalar_bytes = [0u8; 32];
        scalar_bytes[31] = 1;
        let secret = SecretKey::from_slice(&scalar_bytes).unwrap();
        let mut point = PublicKey::from_secret_key(&secp, &secret);

        for k in 0..32 {
            let serialized = point.serialize_uncompressed();
            // serialized[0] = 0x04, serialized[1..33] = x, serialized[33..65] = y
            let offset = k * 64;
            table[offset..offset + 32].copy_from_slice(&serialized[1..33]);
            table[offset + 32..offset + 64].copy_from_slice(&serialized[33..65]);

            // Double the point for next iteration: 2^(k+1) * G
            if k < 31 {
                // Combine with itself: secret * 2
                // We need to double the point. Use the ec_combine approach:
                // point = point + point via a known private key trick.
                // Actually, we just track the scalar and recompute.
                // scalar = 2^k, so 2^(k+1) = 2 * 2^k
                let mut next_scalar = [0u8; 32];
                let bit_pos = k + 1;
                if bit_pos < 256 {
                    next_scalar[31 - bit_pos / 8] |= 1u8 << (bit_pos % 8);
                    let next_secret = SecretKey::from_slice(&next_scalar).unwrap();
                    point = PublicKey::from_secret_key(&secp, &next_secret);
                }
            }
        }

        table
    }

    /// Converts a Pattern to GPU pattern config.
    fn pattern_to_gpu_config(pattern: &Pattern) -> GpuPatternConfig {
        let mut config = GpuPatternConfig {
            pattern_type: match pattern.pattern_type() {
                crate::matcher::PatternType::Prefix => 0,
                crate::matcher::PatternType::Suffix => 1,
                crate::matcher::PatternType::Contains => 2,
                crate::matcher::PatternType::PrefixAndSuffix => 3,
            },
            pattern_len: 0,
            suffix_len: 0,
            _pad: 0,
            pattern_nibbles: [0u8; 40],
            suffix_nibbles: [0u8; 40],
        };

        // Convert hex pattern string to nibbles
        let pat = pattern.pattern();
        config.pattern_len = pat.len() as u32;
        for (i, ch) in pat.chars().enumerate() {
            if i < 40 {
                config.pattern_nibbles[i] = ch.to_digit(16).unwrap_or(0) as u8;
            }
        }

        // For suffix match type, put the pattern in suffix_nibbles instead
        if config.pattern_type == 1 {
            config.suffix_len = config.pattern_len;
            config.pattern_len = 0;
            config.suffix_nibbles[..pat.len()].copy_from_slice(&config.pattern_nibbles[..pat.len()]);
            config.pattern_nibbles = [0u8; 40];
        }

        // Handle prefix+suffix
        if let Some(suffix) = pattern.suffix() {
            config.suffix_len = suffix.len() as u32;
            for (i, ch) in suffix.chars().enumerate() {
                if i < 40 {
                    config.suffix_nibbles[i] = ch.to_digit(16).unwrap_or(0) as u8;
                }
            }
        }

        config
    }

    /// Adds a scalar offset to a base private key modulo the secp256k1 curve order.
    fn add_scalar_mod_n(base_key: &[u8; 32], offset: u32) -> [u8; 32] {
        // secp256k1 curve order n
        let n: [u64; 4] = [
            0xBFD25E8CD0364141,
            0xBAAEDCE6AF48A03B,
            0xFFFFFFFFFFFFFFFE,
            0xFFFFFFFFFFFFFFFF,
        ];

        // Convert base_key (big-endian) to u64 limbs (little-endian limb order)
        let mut key = [0u64; 4];
        for i in 0..4 {
            let off = (3 - i) * 8;
            key[i] = u64::from_be_bytes([
                base_key[off],
                base_key[off + 1],
                base_key[off + 2],
                base_key[off + 3],
                base_key[off + 4],
                base_key[off + 5],
                base_key[off + 6],
                base_key[off + 7],
            ]);
        }

        // Add offset
        let mut carry = offset as u128;
        for limb in key.iter_mut() {
            let sum = *limb as u128 + carry;
            *limb = sum as u64;
            carry = sum >> 64;
        }

        // Reduce mod n if needed
        let mut gte_n = carry > 0;
        if !gte_n {
            // Compare key >= n
            for i in (0..4).rev() {
                if key[i] > n[i] {
                    gte_n = true;
                    break;
                }
                if key[i] < n[i] {
                    break;
                }
            }
        }

        if gte_n {
            let mut borrow: u128 = 0;
            for i in 0..4 {
                let diff = key[i] as u128 + (1u128 << 64) - n[i] as u128 - borrow;
                key[i] = diff as u64;
                borrow = 1 - (diff >> 64);
            }
        }

        // Convert back to big-endian bytes
        let mut result = [0u8; 32];
        for i in 0..4 {
            let off = (3 - i) * 8;
            let bytes = key[i].to_be_bytes();
            result[off..off + 8].copy_from_slice(&bytes);
        }

        result
    }

    /// Runs the GPU worker main loop.
    pub fn run(&self) {
        loop {
            if self.stop_flag.load(Ordering::Relaxed) {
                break;
            }

            match self.run_batch() {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("GPU Worker {}: batch error: {}", self.id, e);
                    // Brief pause before retrying
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    }

    /// Runs a single batch: generates base key, dispatches GPU kernel, reads results.
    fn run_batch(&self) -> Result<(), GpuError> {
        let secp = Secp256k1::new();

        // Generate random base private key
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
        let base_key_bytes = secret_key.secret_bytes();

        // Get uncompressed public key (without 0x04 prefix)
        let pubkey_uncompressed = public_key.serialize_uncompressed();
        let base_pubkey_bytes: &[u8] = &pubkey_uncompressed[1..65]; // 64 bytes

        // Create GPU buffers
        let mut base_pubkey_buf = unsafe {
            Buffer::<cl_uchar>::create(&self.context, CL_MEM_READ_ONLY, 64, std::ptr::null_mut())
                .map_err(|e| GpuError::BufferError(e.to_string()))?
        };

        let mut g_table_buf = unsafe {
            Buffer::<cl_uchar>::create(
                &self.context,
                CL_MEM_READ_ONLY,
                32 * 64,
                std::ptr::null_mut(),
            )
            .map_err(|e| GpuError::BufferError(e.to_string()))?
        };

        let config = Self::pattern_to_gpu_config(&self.pattern);
        let config_bytes = unsafe {
            std::slice::from_raw_parts(
                &config as *const GpuPatternConfig as *const u8,
                std::mem::size_of::<GpuPatternConfig>(),
            )
        };

        let mut config_buf = unsafe {
            Buffer::<cl_uchar>::create(
                &self.context,
                CL_MEM_READ_ONLY,
                std::mem::size_of::<GpuPatternConfig>(),
                std::ptr::null_mut(),
            )
            .map_err(|e| GpuError::BufferError(e.to_string()))?
        };

        let mut result_buf = unsafe {
            Buffer::<cl_uchar>::create(
                &self.context,
                CL_MEM_WRITE_ONLY,
                MAX_RESULTS_PER_BATCH as usize * std::mem::size_of::<GpuResult>(),
                std::ptr::null_mut(),
            )
            .map_err(|e| GpuError::BufferError(e.to_string()))?
        };

        let mut result_count_buf = unsafe {
            Buffer::<cl_uint>::create(
                &self.context,
                CL_MEM_READ_WRITE,
                1,
                std::ptr::null_mut(),
            )
            .map_err(|e| GpuError::BufferError(e.to_string()))?
        };

        // Write data to buffers
        let zero_count: [u32; 1] = [0];

        unsafe {
            self.queue
                .enqueue_write_buffer(&mut base_pubkey_buf, CL_BLOCKING, 0, base_pubkey_bytes, &[])
                .map_err(|e| GpuError::BufferError(e.to_string()))?;

            self.queue
                .enqueue_write_buffer(&mut g_table_buf, CL_BLOCKING, 0, &self.g_table, &[])
                .map_err(|e| GpuError::BufferError(e.to_string()))?;

            self.queue
                .enqueue_write_buffer(&mut config_buf, CL_BLOCKING, 0, config_bytes, &[])
                .map_err(|e| GpuError::BufferError(e.to_string()))?;

            self.queue
                .enqueue_write_buffer(&mut result_count_buf, CL_BLOCKING, 0, &zero_count, &[])
                .map_err(|e| GpuError::BufferError(e.to_string()))?;
        }

        // Execute kernel
        let batch_offset: u32 = 0;
        let max_results: u32 = MAX_RESULTS_PER_BATCH;

        let kernel_event = unsafe {
            ExecuteKernel::new(&self.kernel)
                .set_arg(&base_pubkey_buf)
                .set_arg(&g_table_buf)
                .set_arg(&config_buf)
                .set_arg(&mut result_buf)
                .set_arg(&mut result_count_buf)
                .set_arg(&max_results)
                .set_arg(&batch_offset)
                .set_global_work_size(self.work_size)
                .enqueue_nd_range(&self.queue)
                .map_err(|e| GpuError::KernelExec(e.to_string()))?
        };

        // Wait for completion
        kernel_event
            .wait()
            .map_err(|e| GpuError::KernelExec(e.to_string()))?;

        // Read result count
        let mut count_out = [0u32; 1];
        unsafe {
            self.queue
                .enqueue_read_buffer(&result_count_buf, CL_BLOCKING, 0, &mut count_out, &[])
                .map_err(|e| GpuError::BufferError(e.to_string()))?;
        }

        let num_results = (count_out[0] as usize).min(MAX_RESULTS_PER_BATCH as usize);

        // Read results if any
        if num_results > 0 {
            let mut results_out =
                vec![GpuResult::default(); MAX_RESULTS_PER_BATCH as usize];
            let results_bytes = unsafe {
                std::slice::from_raw_parts_mut(
                    results_out.as_mut_ptr() as *mut u8,
                    results_out.len() * std::mem::size_of::<GpuResult>(),
                )
            };

            unsafe {
                self.queue
                    .enqueue_read_buffer(&result_buf, CL_BLOCKING, 0, results_bytes, &[])
                    .map_err(|e| GpuError::BufferError(e.to_string()))?;
            }

            for i in 0..num_results {
                let gpu_result = &results_out[i];
                if gpu_result.found == 0 {
                    continue;
                }

                // Reconstruct private key: base_key + offset mod n
                let derived_key = Self::add_scalar_mod_n(&base_key_bytes, gpu_result.offset);

                // Verify on CPU
                let keypair = Keypair::from_secret_key(derived_key);
                if self.pattern.matches(keypair.address()).is_match() {
                    self.stats.matches_found.fetch_add(1, Ordering::Relaxed);

                    let result = VanityResult {
                        private_key: keypair.private_key_hex(),
                        address: keypair.address().to_checksum(),
                        worker_id: self.id,
                    };

                    let _ = self.result_tx.send(result);
                }
            }
        }

        // Update stats
        self.stats
            .keys_generated
            .fetch_add(self.work_size as u64, Ordering::Relaxed);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_scalar_mod_n_basic() {
        let mut base = [0u8; 32];
        base[31] = 1; // key = 1
        let result = GpuWorker::add_scalar_mod_n(&base, 5);
        assert_eq!(result[31], 6); // 1 + 5 = 6
        // All other bytes should be 0
        for i in 0..31 {
            assert_eq!(result[i], 0);
        }
    }

    #[test]
    fn test_add_scalar_mod_n_carry() {
        let mut base = [0u8; 32];
        base[31] = 0xFF;
        let result = GpuWorker::add_scalar_mod_n(&base, 1);
        assert_eq!(result[31], 0);
        assert_eq!(result[30], 1); // carry propagated
    }

    #[test]
    fn test_pattern_to_gpu_config_prefix() {
        let pattern = Pattern::new("dead", crate::matcher::PatternType::Prefix, false);
        let config = GpuWorker::pattern_to_gpu_config(&pattern);
        let pt = config.pattern_type;
        let pl = config.pattern_len;
        assert_eq!(pt, 0);
        assert_eq!(pl, 4);
        assert_eq!(config.pattern_nibbles[0], 0xd);
        assert_eq!(config.pattern_nibbles[1], 0xe);
        assert_eq!(config.pattern_nibbles[2], 0xa);
        assert_eq!(config.pattern_nibbles[3], 0xd);
    }

    #[test]
    fn test_pattern_to_gpu_config_suffix() {
        let pattern = Pattern::new("beef", crate::matcher::PatternType::Suffix, false);
        let config = GpuWorker::pattern_to_gpu_config(&pattern);
        let pt = config.pattern_type;
        let sl = config.suffix_len;
        assert_eq!(pt, 1);
        assert_eq!(sl, 4);
        assert_eq!(config.suffix_nibbles[0], 0xb);
        assert_eq!(config.suffix_nibbles[1], 0xe);
        assert_eq!(config.suffix_nibbles[2], 0xe);
        assert_eq!(config.suffix_nibbles[3], 0xf);
    }

    #[test]
    fn test_g_table_size() {
        let table = GpuWorker::compute_g_table();
        assert_eq!(table.len(), 32 * 64);
        // First entry should be G (not all zeros)
        let first_64 = &table[0..64];
        assert!(first_64.iter().any(|&b| b != 0));
    }
}
