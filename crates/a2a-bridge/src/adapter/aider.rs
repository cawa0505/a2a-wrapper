use super::runner::ExecSpec;

/// Error markers that indicate a failed aider run even on exit code 0.
/// Verified: aider -m exits 0 on auth failure (smoke test 2026-08-08).
pub const AIDER_ERROR_PATTERNS: &[&str] = &[
    "Authentication Error",
    "Invalid API key",
    "Rate limit",
    "Connection error",
    "Error communicating with",
];

/// Build the aider one-shot invocation: `aider -m <task> --yes --no-auto-commits`.
/// `--yes` answers prompts headlessly; `--no-auto-commits` keeps the working
/// tree dirty so the git-diff artifact captures the change.
pub fn aider_exec(task: &str, workdir: Option<String>) -> ExecSpec {
    ExecSpec {
        program: "aider".into(),
        args: vec![
            "-m".into(),
            task.into(),
            "--yes".into(),
            "--no-auto-commits".into(),
        ],
        workdir,
        error_patterns: AIDER_ERROR_PATTERNS,
        require_output: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_shot_command() {
        let spec = aider_exec("fix it", Some("/tmp/w".into()));
        assert_eq!(spec.program, "aider");
        assert_eq!(
            spec.args,
            vec!["-m", "fix it", "--yes", "--no-auto-commits"]
        );
        assert_eq!(spec.workdir.as_deref(), Some("/tmp/w"));
        assert_eq!(spec.error_patterns, AIDER_ERROR_PATTERNS);
    }
}
