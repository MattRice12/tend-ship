//! tend-ship library entry. Both binaries (`tend-ship`, `tend-action-ship`)
//! are thin wrappers around [`run`]; tend's extension-discovery contract
//! resolves `tend-action-<name>` on `PATH`, so the second binary is the
//! shape tend expects.

mod dispatch;
mod encode;
mod help;
mod ship;

pub use dispatch::run;

/// Reset SIGPIPE to default behavior so writes to a closed stdout terminate
/// the process cleanly instead of panicking. Rust's default SIGPIPE handler
/// is `SIG_IGN`, which causes `println!`/`eprintln!` to return `EPIPE` and
/// panic — visible whenever tend-ship's output is piped through `head`,
/// `less`, etc. and the consumer exits early.
#[cfg(unix)]
pub fn reset_sigpipe() {
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    unsafe {
        // SIGPIPE = 13 on all Unixes; SIG_DFL = 0.
        signal(13, 0);
    }
}

#[cfg(not(unix))]
pub fn reset_sigpipe() {}
