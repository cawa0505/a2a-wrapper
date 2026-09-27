use super::runner::ExecSpec;

/// Error markers for codex exec runs (safety net). codex can surface stream /
/// API errors from an OpenAI-compatible proxy while still exiting 0, so these
/// mirror the aider/zero net. Conservative high-signal set — refine against a
/// real smoke run.
pub const CODEX_ERROR_PATTERNS: &[&str] = &[
    "stream error",
    "Unauthorized",
    "invalid api key",
    "Rate limit",
    "error sending request",
];

/// Build the codex one-shot invocation: `codex exec <prompt>
/// --dangerously-bypass-approvals-and-sandbox --skip-git-repo-check` with
/// explicit cwd via `-C`.
///
/// `--dangerously-bypass-approvals-and-sandbox` runs headless with no approval
/// prompts (the invoking orchestrator already supplies the controlled worker
/// environment), mirror-
/// ing zero's `--auto high --skip-permissions-unsafe`. `--skip-git-repo-check`
/// lets a task run outside a git repo.
///
/// Model/provider are NOT hardcoded: codex reads its own `~/.codex/config.toml`
/// (deployment points it at the gateway via `CODEX_HOME`), keeping deployment
/// values out of this open repo.
pub fn codex_exec(prompt: &str, workdir: Option<String>) -> ExecSpec {
    let mut args = vec![
        "exec".into(),
        prompt.into(),
        "--dangerously-bypass-approvals-and-sandbox".into(),
        "--skip-git-repo-check".into(),
    ];
    if let Some(dir) = &workdir {
        args.push("-C".into());
        args.push(dir.into());
    }
    ExecSpec {
        program: "codex".into(),
        args,
        workdir,
        error_patterns: CODEX_ERROR_PATTERNS,
        require_output: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_shot_command_with_cwd() {
        let spec = codex_exec("add tests", Some("/tmp/w".into()));
        assert_eq!(spec.program, "codex");
        assert_eq!(
            spec.args,
            vec![
                "exec",
                "add tests",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check",
                "-C",
                "/tmp/w"
            ]
        );
        assert_eq!(spec.workdir.as_deref(), Some("/tmp/w"));
        assert_eq!(spec.error_patterns, CODEX_ERROR_PATTERNS);
    }

    #[test]
    fn builds_one_shot_command_without_cwd() {
        let spec = codex_exec("add tests", None);
        assert_eq!(
            spec.args,
            vec![
                "exec",
                "add tests",
                "--dangerously-bypass-approvals-and-sandbox",
                "--skip-git-repo-check"
            ]
        );
    }
}
