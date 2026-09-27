use super::runner::ExecSpec;

/// Error markers for opencode runs (safety net). Narrow high-signal set from
/// the T5 live smoke on 1.18.32: a bad model (`nosuchprovider/nosuch-model`)
/// exits 1 with empty stdout and prints a masked `UnknownError` whose message
/// is `Unexpected server error. Check server logs for details.` on stderr
/// (~11–20s; unknown provider reaches the network — NOT the sub-4s local
/// catalog miss an unknown *model* on a known provider produces).
/// `Provider not found` comes from `opencode models <bad>` (exit 1).
/// Do NOT widen to generic markers like `Error:`. Smoke-verified: these two
/// patterns drive bridge → TASK_STATE_FAILED with
/// `error pattern matched: "Unexpected server error"`.
pub const OPENCODE_ERROR_PATTERNS: &[&str] = &["Unexpected server error", "Provider not found"];

/// Build the opencode one-shot invocation: `opencode run <prompt>`.
///
/// - prompt goes via argv (variadic positional); never the literal `-`
///   placeholder (upstream #28407 treats `-` as a positional message).
/// - workdir rides the runner's `current_dir` — opencode has no `-C` flag
///   (project discovery follows cwd).
/// - no approval/sandbox flags: opencode permissions allow-all by default,
///   and `--dangerously-skip-permissions` alias status is unconfirmed (🟡).
/// - model/provider are NOT hardcoded: opencode reads its own config
///   (deployment points it at the gateway via `OPENCODE_CONFIG`), keeping
///   deployment values out of this open repo.
/// - `require_output: true` guards upstream #36413: a permission-rejected run
///   can exit 0 with empty stdout; Completed must then be demoted to Failed.
pub fn opencode_exec(prompt: &str, workdir: Option<String>) -> ExecSpec {
    ExecSpec {
        program: "opencode".into(),
        args: vec!["run".into(), prompt.into()],
        workdir,
        error_patterns: OPENCODE_ERROR_PATTERNS,
        require_output: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_shot_command_with_cwd() {
        let spec = opencode_exec("add tests", Some("/tmp/w".into()));
        assert_eq!(spec.program, "opencode");
        assert_eq!(spec.args, vec!["run", "add tests"]);
        // workdir rides current_dir — no -C flag exists on opencode
        assert_eq!(spec.workdir.as_deref(), Some("/tmp/w"));
        assert_eq!(spec.error_patterns, OPENCODE_ERROR_PATTERNS);
        assert!(spec.require_output);
    }

    #[test]
    fn builds_one_shot_command_without_cwd() {
        let spec = opencode_exec("add tests", None);
        assert_eq!(spec.args, vec!["run", "add tests"]);
        assert!(spec.workdir.is_none());
        // never a `-` prompt placeholder (upstream #28407)
        assert!(!spec.args.contains(&"-".to_string()));
        // no provider/model/key or approval flags
        assert_eq!(spec.args.len(), 2);
    }
}
