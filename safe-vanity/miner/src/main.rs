//! Safe Vanity Address Miner CLI
//!
//! Mines saltNonce until the CREATE2-derived Safe proxy address matches the pattern.
//! Use --factory, --init-code-hash, --initializer-hash from your Safe config
//! (e.g. from Safe SDK getAddress flow).

use std::process;
use std::time::Duration;

use clap::Parser;

use safe_vanity::{Config, Pattern, WorkerPool};

fn main() {
    let config = Config::parse();

    if let Err(e) = config.validate() {
        eprintln!("Configuration error: {}", e);
        process::exit(1);
    }

    let pattern = if let Some(ref suffix) = config.normalized_suffix() {
        Pattern::new_prefix_and_suffix(
            config.normalized_pattern(),
            suffix.clone(),
            config.case_sensitive,
        )
    } else {
        Pattern::new(
            config.normalized_pattern(),
            config.effective_pattern_type(),
            config.case_sensitive,
        )
    };

    println!("Safe Vanity Address Miner");
    println!("==========================");
    let pattern_display = if let Some(suffix) = pattern.suffix() {
        format!("{} ... {} ({})", pattern.pattern(), suffix, pattern.pattern_type())
    } else {
        format!("{} ({})", pattern.pattern(), pattern.pattern_type())
    };
    println!("Pattern:    {}", pattern_display);
    println!("Difficulty: {}", pattern.difficulty_description());
    println!("Workers:    {}", config.worker_count());
    println!("Target:     {} address(es)", config.count);
    println!();

    let factory = config.factory_bytes();
    let init_code_hash = config.init_code_hash_bytes();
    let initializer_hash = config.initializer_hash_bytes();

    let pool = WorkerPool::new(
        config.worker_count(),
        pattern,
        factory,
        init_code_hash,
        initializer_hash,
    );

    let stop_flag = pool.stop_flag_clone();
    ctrlc::set_handler(move || {
        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .expect("set Ctrl-C handler");

    println!("Searching... (Press Ctrl+C to stop)\n");

    let mut found = 0;
    let report_interval = Duration::from_secs(config.report_interval);

    loop {
        match pool.wait_for_result(report_interval) {
            Some(result) => {
                found += 1;
                print_result(&result, found);
                if config.count > 0 && found >= config.count {
                    println!("\nTarget reached! Found {} address(es).", found);
                    break;
                }
            }
            None => print_progress(&pool),
        }
        if pool.is_stopped() {
            println!("\nStopped by user.");
            break;
        }
    }

    println!("\n--- Final Statistics ---");
    println!("Total salts tried:  {}", format_number(pool.total_salts()));
    println!("Total matches:     {}", pool.total_matches());
    println!("Time elapsed:       {:.2}s", pool.elapsed().as_secs_f64());
    println!(
        "Average speed:      {}/s",
        format_number(pool.salts_per_second() as u64)
    );

    pool.join();
}

fn print_result(result: &safe_vanity::SafeVanityResult, index: usize) {
    println!("=== Match #{} ===", index);
    println!("Address:      {}", result.address_checksum());
    println!("Salt (hex):   0x{}", result.salt_nonce_hex());
    println!("Salt (dec):   {}", result.salt_nonce_decimal());
    println!("Worker:       {}", result.worker_id);
    println!();
}

fn print_progress(pool: &WorkerPool) {
    let salts = pool.total_salts();
    let rate = pool.salts_per_second();
    let elapsed = pool.elapsed().as_secs();
    println!(
        "[{:>4}s] Tried {} salts ({}/s)",
        elapsed,
        format_number(salts),
        format_number(rate as u64)
    );
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}
