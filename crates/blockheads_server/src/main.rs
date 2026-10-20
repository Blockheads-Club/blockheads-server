//! Process entry point of the dedicated server.
//!

fn main() {
    // Rust sets SIGPIPE to SIG_IGN; the original keeps the default disposition. No other handlers are installed:
    // SIGINT/SIGTERM/SIGHUP keep their defaults (exit 130/143/129, no output, no save).
    // SAFETY: resetting a signal disposition at process start, before any thread exists.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    blockheads_server::app::cli::main_entry()
}
