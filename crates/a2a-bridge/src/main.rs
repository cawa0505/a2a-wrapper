mod adapter;
mod args;
mod pty;
mod transport;

use std::io;
use std::process::ExitCode;

use anyhow::Result;
use tracing::{info, warn};

use adapter::{Adapter, adapter_or_bail};
use args::Mode;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_target(false)
        .init();

    match run() {
        Ok(code) => ExitCode::from(code),
        Err(err) => {
            warn!("fatal: {err:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<u8> {
    match args::parse() {
        Mode::Exec { task } => exec(&task),
        Mode::Serve { adapter, cwd } => serve(&adapter, cwd),
        Mode::Harness {
            debug_grid,
            agent_args,
        } => harness(debug_grid, &agent_args),
    }
}

/// Engine A: headless one-shot task (aider -m), print outcome, exit by status.
fn exec(task: &str) -> Result<u8> {
    info!("exec: aider -m {task:?}");
    let spec = adapter::aider_exec(task, None);
    match adapter::execute(&spec)? {
        adapter::TaskOutcome::Completed { artifacts } => {
            println!("completed");
            for diff in &artifacts {
                println!("--- artifact ---");
                print!("{diff}");
            }
            Ok(0)
        }
        adapter::TaskOutcome::Failed { reason } => {
            eprintln!("failed: {reason}");
            Ok(1)
        }
    }
}

/// A2A v1 stdio node: run the serve loop for the whitelisted `--adapter`.
fn serve(adapter: &str, cwd: Option<String>) -> Result<u8> {
    let selected: Adapter = adapter_or_bail(adapter)?;
    info!("serve: adapter={} cwd={:?}", selected.label(), cwd);
    transport::serve(selected, cwd)?;
    Ok(0)
}

/// Engine B: interactive PTY + VT100 grid harness.
fn harness(debug_grid: bool, agent_args: &[String]) -> Result<u8> {
    let agent = agent_args.first().map(String::as_str).unwrap_or("aider");
    info!("spawning agent: {agent} (debug_grid={debug_grid})");

    let session = pty::Session::spawn(agent_args, debug_grid)?;
    let status = session.wait()?;
    info!("agent exited with code {}", status.exit_code());
    Ok(status.exit_code() as u8)
}
