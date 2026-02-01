//! Ethereum Vanity Address Generator CLI
//!
//! Usage:
//!   eth_vanity -p dead           # Find address starting with "dead"
//!   eth_vanity -p beef -t suffix # Find address ending with "beef"
//!   eth_vanity -p cafe -t contains -n 5 # Find 5 addresses containing "cafe"

use std::process;
use std::time::Duration;

use clap::Parser;

use eth_vanity::{Config, Pattern, WorkerPool};

fn main() {
    let config = Config::parse();

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Configuration error: {}", e);
        process::exit(1);
    }

    // Create the pattern
    let pattern = Pattern::new(
        config.normalized_pattern(),
        config.pattern_type,
        config.case_sensitive,
    );

    // Print startup info
    println!("Ethereum Vanity Address Generator");
    println!("==================================");
    println!("Pattern:    {} ({})", pattern.pattern(), pattern.pattern_type());
    println!("Difficulty: {}", pattern.difficulty_description());
    println!("Workers:    {}", config.worker_count());
    println!("Target:     {} address(es)", config.count);
    println!();

    // Create worker pool
    let pool = WorkerPool::new(config.worker_count(), pattern);

    // Set up ctrl-c handler
    let stop_flag = pool.stop_flag_clone();
    ctrlc_handler(stop_flag);

    println!("Searching... (Press Ctrl+C to stop)\n");

    let mut found = 0;
    let report_interval = Duration::from_secs(config.report_interval);

    loop {
        // Wait for result or timeout for progress report
        match pool.wait_for_result(report_interval) {
            Some(result) => {
                found += 1;
                print_result(&result, found);

                if config.count > 0 && found >= config.count {
                    println!("\nTarget reached! Found {} address(es).", found);
                    break;
                }
            }
            None => {
                // Timeout - print progress
                print_progress(&pool);
            }
        }

        // Check if we should stop (ctrl-c was pressed)
        if pool.is_stopped() {
            println!("\nStopped by user.");
            break;
        }
    }

    // Print final stats
    println!("\n--- Final Statistics ---");
    println!("Total keys generated: {}", format_number(pool.total_keys()));
    println!("Total matches found:  {}", pool.total_matches());
    println!("Time elapsed:         {:.2}s", pool.elapsed().as_secs_f64());
    println!(
        "Average speed:        {}/s",
        format_number(pool.keys_per_second() as u64)
    );

    pool.join();
}

fn print_result(result: &eth_vanity::VanityResult, index: usize) {
    println!("=== Match #{} ===", index);
    println!("Address:     {}", result.address);
    println!("Private Key: {}", result.private_key);
    println!("Worker:      {}", result.worker_id);
    println!();
}

fn print_progress(pool: &WorkerPool) {
    let keys = pool.total_keys();
    let rate = pool.keys_per_second();
    let elapsed = pool.elapsed().as_secs();

    println!(
        "[{:>4}s] Generated {} keys ({}/s)",
        elapsed,
        format_number(keys),
        format_number(rate as u64)
    );
}

fn format_number(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.2}B", n as f64 / 1_000_000_000.0)
    } else if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn ctrlc_handler(stop_flag: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    ctrlc::set_handler(move || {
        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .expect("Error setting Ctrl-C handler");
}
