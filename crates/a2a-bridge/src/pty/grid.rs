use std::sync::Mutex;

use vt100::Parser;

const GRID_ROWS: u16 = 24;
const GRID_COLS: u16 = 80;

/// VT100 grid view over the child's escape stream.
///
/// The reader thread feeds raw bytes via [`Grid::process`]; UI code reads the
/// rendered screen via [`Grid::rows`] / [`Grid::text`]. [`Grid::dump`] prints
/// the numbered grid to stderr for calibration (debug harness).
pub struct Grid {
    parser: Mutex<Parser>,
}

impl Grid {
    pub fn new() -> Self {
        Self {
            parser: Mutex::new(Parser::new(GRID_ROWS, GRID_COLS, 0)),
        }
    }

    /// Feed raw bytes from the child's output into the terminal emulator.
    pub fn process(&self, bytes: &[u8]) {
        self.parser
            .lock()
            .expect("grid mutex poisoned")
            .process(bytes);
    }

    /// All 24 rows as text, trailing whitespace trimmed (blank rows stay blank).
    pub fn rows(&self) -> Vec<String> {
        let parser = self.parser.lock().expect("grid mutex poisoned");
        let screen = parser.screen();
        screen
            .rows(0, GRID_COLS)
            .map(|row| row.trim_end().to_string())
            .collect()
    }

    /// Numbered row dump to stderr (calibration aid).
    pub fn dump(&self) {
        eprintln!("--- grid dump (24 rows) ---");
        for (i, row) in self.rows().iter().enumerate() {
            eprintln!("[{i:02}] '{row}'");
        }
        eprintln!("---------------------------");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_rows_from_escape_stream() {
        let grid = Grid::new();
        grid.process(b"\x1b[2J"); // clear screen
        grid.process(b"\x1b[H"); // cursor home
        grid.process(b"hello");
        grid.process(b"\x1b[2;1H"); // row 2, col 1
        grid.process(b"world");

        let rows = grid.rows();
        assert_eq!(rows.len(), 24);
        assert_eq!(rows[0], "hello");
        assert_eq!(rows[1], "world");
        assert!(rows[2..].iter().all(|row| row.is_empty()));
    }

    #[test]
    fn trims_trailing_whitespace_per_row() {
        let grid = Grid::new();
        grid.process(b"pad     ");
        grid.process(b"\x1b[2;1H");
        grid.process(b"indent   ");

        let rows = grid.rows();
        assert_eq!(rows[0], "pad");
        assert_eq!(rows[1], "indent");
    }
}
