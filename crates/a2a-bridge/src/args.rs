use std::env;
use std::process;

/// Which top-level mode the bridge runs in.
pub enum Mode {
    /// Interactive harness: PTY + VT100 grid (Engine B), default agent aider.
    Harness {
        debug_grid: bool,
        agent_args: Vec<String>,
    },
    /// Headless one-shot task execution (Engine A).
    Exec { task: String },
}

pub fn parse() -> Mode {
    let mut debug_grid = false;
    let mut agent_args = Vec::new();
    let mut task = None;
    let mut after_double_dash = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if after_double_dash {
            agent_args.push(arg);
        } else {
            match arg.as_str() {
                "--" => after_double_dash = true,
                "--debug-grid" => debug_grid = true,
                "--exec" => {
                    task = Some(args.next().unwrap_or_else(|| {
                        eprintln!("--exec requires a task message");
                        usage();
                        process::exit(2);
                    }));
                }
                _ => {
                    eprintln!("unknown argument: {arg}");
                    usage();
                    process::exit(2);
                }
            }
        }
    }

    match task {
        Some(task) => Mode::Exec { task },
        None => Mode::Harness {
            debug_grid,
            agent_args,
        },
    }
}

fn usage() {
    eprintln!("usage: a2a-bridge [--exec <task> | --debug-grid] [-- <agent> <args...>]");
}
