mod args;
mod pty;

use std::io;
use std::process::ExitCode;

use anyhow::Result;
use tracing::{info, warn};

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
    let (debug_grid, agent_args) = args::parse();
    let agent = agent_args.first().map(String::as_str).unwrap_or("aider");
    info!("spawning agent: {agent} (debug_grid={debug_grid})");

    let session = pty::Session::spawn(&agent_args, debug_grid)?;
    let status = session.wait()?;
    info!("agent exited with code {}", status.exit_code());
    Ok(status.exit_code() as u8)
}
