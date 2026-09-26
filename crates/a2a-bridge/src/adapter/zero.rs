use super::runner::ExecSpec;

/// Error markers for zero exec runs (analogous to aider's).
pub const ZERO_ERROR_PATTERNS: &[&str] = &[
    "Authentication Error",
    "Invalid API key",
    "Rate limit",
    "Connection error",
];

/// Build the zero one-shot invocation: `zero exec <prompt> --auto high
/// --skip-permissions-unsafe` with explicit cwd via `-C`.
/// `--auto high` prevents approval-gated tools from blocking headless runs.
/// Note: `-m` on zero is `--model`, NOT message — verified 2026-08-08.
pub fn zero_exec(prompt: &str, workdir: Option<String>) -> ExecSpec {
    let mut args = vec![
        "exec".into(),
        prompt.into(),
        "--auto".into(),
        "high".into(),
        "--skip-permissions-unsafe".into(),
    ];
    if let Some(dir) = &workdir {
        args.push("-C".into());
        args.push(dir.into());
    }
    ExecSpec {
        program: "zero".into(),
        args,
        workdir,
        error_patterns: ZERO_ERROR_PATTERNS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_shot_command_with_cwd() {
        let spec = zero_exec("add tests", Some("/tmp/w".into()));
        assert_eq!(spec.program, "zero");
        assert_eq!(
            spec.args,
            vec![
                "exec",
                "add tests",
                "--auto",
                "high",
                "--skip-permissions-unsafe",
                "-C",
                "/tmp/w"
            ]
        );
        assert_eq!(spec.workdir.as_deref(), Some("/tmp/w"));
        assert_eq!(spec.error_patterns, ZERO_ERROR_PATTERNS);
    }

    #[test]
    fn builds_one_shot_command_without_cwd() {
        let spec = zero_exec("add tests", None);
        assert_eq!(
            spec.args,
            vec![
                "exec",
                "add tests",
                "--auto",
                "high",
                "--skip-permissions-unsafe"
            ]
        );
    }
}
