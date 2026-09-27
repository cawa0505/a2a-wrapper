mod aider;
mod codex;
mod opencode;
mod outcome;
mod rho;
mod runner;
mod zero;

use anyhow::{Result, bail};

pub use aider::aider_exec;
pub use outcome::TaskOutcome;
pub use runner::execute;

/// The Engine-A adapters the bridge can dispatch. This enum IS the whitelist —
/// the dispatch `match` is compiler-exhaustive and the lock test asserts
/// `Adapter::ALL`'s labels, so adding/removing a variant without syncing turns
/// a test red (mirror-locked with schedulers of partner deployments via the
/// same test name; see AGENTS.md boundary notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adapter {
    Aider,
    Codex,
    Opencode,
    Rho,
    Zero,
}

impl Adapter {
    pub const ALL: &'static [Adapter] = &[
        Adapter::Aider,
        Adapter::Codex,
        Adapter::Opencode,
        Adapter::Rho,
        Adapter::Zero,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Adapter::Aider => "aider",
            Adapter::Codex => "codex",
            Adapter::Opencode => "opencode",
            Adapter::Rho => "rho",
            Adapter::Zero => "zero",
        }
    }

    /// Parse a `--adapter` label against the whitelist; unknown → `None`.
    pub fn parse(s: &str) -> Option<Adapter> {
        Adapter::ALL.iter().copied().find(|a| a.label() == s)
    }

    /// Run the selected adapter headless (Engine A) and classify the outcome.
    pub fn run(self, prompt: &str, workdir: Option<String>) -> Result<TaskOutcome> {
        match self {
            Adapter::Aider => execute(&aider::aider_exec(prompt, workdir)),
            Adapter::Codex => execute(&codex::codex_exec(prompt, workdir)),
            Adapter::Opencode => execute(&opencode::opencode_exec(prompt, workdir)),
            Adapter::Zero => execute(&zero::zero_exec(prompt, workdir)),
            Adapter::Rho => rho::rho_run(prompt, workdir),
        }
    }
}

/// Parse an adapter label or fail with a message listing the whitelist.
pub fn adapter_or_bail(name: &str) -> Result<Adapter> {
    match Adapter::parse(name) {
        Some(a) => Ok(a),
        None => {
            let labels: Vec<&str> = Adapter::ALL.iter().map(|a| a.label()).collect();
            bail!("unknown adapter {name:?}; expected one of {labels:?}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_whitelist_is_exactly_aider_codex_opencode_rho_zero() {
        let mut labels: Vec<&str> = Adapter::ALL.iter().map(|a| a.label()).collect();
        labels.sort_unstable();
        assert_eq!(labels, ["aider", "codex", "opencode", "rho", "zero"]);
    }

    #[test]
    fn parse_round_trips_every_label() {
        for a in Adapter::ALL {
            assert_eq!(Adapter::parse(a.label()), Some(*a));
        }
        assert_eq!(Adapter::parse("nope"), None);
    }
}
