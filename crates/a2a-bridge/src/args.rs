use std::env;
use std::process;

/// Parses `--debug-grid` and agent args (everything after `--`).
///
/// Returns `(debug_grid, agent_args)`; an empty agent list means the
/// default agent (`aider`).
pub fn parse() -> (bool, Vec<String>) {
    let mut debug_grid = false;
    let mut agent_args = Vec::new();
    let mut after_double_dash = false;

    for arg in env::args().skip(1) {
        if after_double_dash {
            agent_args.push(arg);
        } else if arg == "--" {
            after_double_dash = true;
        } else if arg == "--debug-grid" {
            debug_grid = true;
        } else {
            eprintln!("unknown argument: {arg}");
            eprintln!("usage: a2a-bridge [--debug-grid] [-- <agent> <args...>]");
            process::exit(2);
        }
    }

    (debug_grid, agent_args)
}
