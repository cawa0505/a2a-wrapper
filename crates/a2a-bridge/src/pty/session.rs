use std::io::{self, Read, Write};
use std::sync::Arc;

use anyhow::Result;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use crate::pty::grid::Grid;
use crate::pty::input::spawn_input_thread;

const GRID_ROWS: u16 = 24;
const GRID_COLS: u16 = 80;

/// PTY session supervising one CLI agent (PoC: aider).
pub struct Session {
    child: Box<dyn Child + Send + Sync>,
}

impl Session {
    /// Spawns the agent in a fresh PTY, forwards its output to the host
    /// terminal while feeding the VT100 grid, and forwards host input back.
    ///
    /// `agent_args` is the agent command; empty means the default (`aider`).
    pub fn spawn(agent_args: &[String], debug_grid: bool) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: GRID_ROWS,
            cols: GRID_COLS,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(if agent_args.is_empty() {
            "aider"
        } else {
            &agent_args[0]
        });
        if !agent_args.is_empty() {
            cmd.args(&agent_args[1..]);
        }
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let grid = Arc::new(Grid::new());
        spawn_reader_thread(pair.master.try_clone_reader()?, Arc::clone(&grid));
        spawn_input_thread(pair.master.take_writer()?, grid, debug_grid);

        Ok(Self { child })
    }

    /// Blocks until the child exits, returning its exit status.
    pub fn wait(mut self) -> Result<portable_pty::ExitStatus> {
        Ok(self.child.wait()?)
    }
}

/// Forwards raw child bytes to the host terminal and feeds the grid parser.
fn spawn_reader_thread(mut reader: Box<dyn Read + Send>, grid: Arc<Grid>) {
    std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let mut buf = [0u8; 4096];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(n) if n > 0 => n,
                _ => break,
            };
            let chunk = &buf[..n];
            if out.write_all(chunk).is_err() {
                break;
            }
            let _ = out.flush();
            grid.process(chunk);
        }
    });
}
