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
    /// A2A v1 stdio node: dispatch tasks to `--adapter <name>` over NDJSON.
    Serve {
        adapter: String,
        cwd: Option<String>,
    },
}

pub fn parse() -> Mode {
    let mut debug_grid = false;
    let mut agent_args = Vec::new();
    let mut task = None;
    let mut serve = false;
    let mut adapter = None;
    let mut cwd = None;
    let mut after_double_dash = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if after_double_dash {
            agent_args.push(arg);
        } else {
            match arg.as_str() {
                "--" => after_double_dash = true,
                "--debug-grid" => debug_grid = true,
                "--serve" => serve = true,
                "--adapter" => {
                    adapter = Some(args.next().unwrap_or_else(|| {
                        eprintln!("--adapter requires a name");
                        usage();
                        process::exit(2);
                    }));
                }
                "--cwd" => {
                    cwd = Some(args.next().unwrap_or_else(|| {
                        eprintln!("--cwd requires a directory");
                        usage();
                        process::exit(2);
                    }));
                }
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

    if serve {
        return Mode::Serve {
            adapter: adapter.unwrap_or_else(|| {
                eprintln!("--serve requires --adapter <name>");
                usage();
                process::exit(2);
            }),
            cwd,
        };
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
    eprintln!(
        "usage: a2a-bridge [--serve --adapter <name> [--cwd <dir>] | --exec <task> | --debug-grid] [-- <agent> <args...>]"
    );
}
