use std::io::{self, Read, Write};
use std::sync::Arc;

use crate::pty::grid::Grid;

/// Ctrl+Q (0x11) in debug mode: dump the grid instead of forwarding.
const DEBUG_DUMP_KEY: u8 = 0x11;

/// Forwards host stdin to the PTY master writer.
///
/// Cooked-mode stdin line-buffers input (keys arrive on Enter), acceptable
/// for a calibration harness — the real sidecar is non-interactive.
pub fn spawn_input_thread(mut writer: Box<dyn Write + Send>, grid: Arc<Grid>, debug_grid: bool) {
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let mut buf = [0u8; 4096];
        loop {
            let n = match input.read(&mut buf) {
                Ok(n) if n > 0 => n,
                _ => break,
            };
            for &b in &buf[..n] {
                if debug_grid && b == DEBUG_DUMP_KEY {
                    grid.dump();
                    continue;
                }
                if writer.write_all(&[b]).is_err() {
                    return;
                }
            }
            let _ = writer.flush();
        }
    });
}
