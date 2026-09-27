use std::process::Command;

use anyhow::Result;

use super::outcome::{TaskOutcome, classify};

/// A headless one-shot invocation of an agent (Engine A).
pub struct ExecSpec {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub error_patterns: &'static [&'static str],
    /// Demote `Completed` to `Failed` when exit 0 but stdout is empty —
    /// opencode's upstream #36413 guard (a permission-rejected run can exit 0
    /// with no output). Existing adapters keep `false`, so their exit-0
    /// semantics are unchanged.
    pub require_output: bool,
}

/// Run `spec` to completion, classify the outcome, and attach the git-diff
/// artifact on success.
///
/// Sync `std::process` on purpose: the tokio migration is deferred to the
/// transport step (`tokio::process::Command` is a near drop-in).
pub fn execute(spec: &ExecSpec) -> Result<TaskOutcome> {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    if let Some(dir) = &spec.workdir {
        cmd.current_dir(dir);
        // `current_dir` alone is not enough: agents that resolve their project
        // root from `$PWD` (opencode does) would ignore it and work in the
        // spawning shell's directory instead. Measured 1.18.32: with an
        // inherited `PWD`, `opencode run` wrote into the spawning repo.
        cmd.env("PWD", dir);
    }
    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let outcome = classify(
        output.status.success(),
        &stdout,
        &stderr,
        spec.error_patterns,
    );
    let outcome = if spec.require_output
        && matches!(outcome, TaskOutcome::Completed { .. })
        && stdout.trim().is_empty()
    {
        TaskOutcome::Failed {
            reason: "no output: exit 0 with empty stdout".into(),
        }
    } else {
        outcome
    };
    Ok(with_artifact(outcome, spec.workdir.as_deref()))
}

fn with_artifact(outcome: TaskOutcome, workdir: Option<&str>) -> TaskOutcome {
    match outcome {
        TaskOutcome::Completed { artifacts } if artifacts.is_empty() => TaskOutcome::Completed {
            artifacts: git_diff(workdir).ok().flatten().into_iter().collect(),
        },
        other => other,
    }
}

/// `git diff HEAD` in the agent's workdir (None → inherit cwd).
/// ponytail: fresh repos without HEAD fail and yield `None`; real usage
/// always has HEAD, plain `git diff` fallback added if it ever matters.
fn git_diff(workdir: Option<&str>) -> Result<Option<String>> {
    let mut cmd = Command::new("git");
    cmd.args(["diff", "HEAD"]);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        return Ok(None);
    }
    let diff = String::from_utf8_lossy(&out.stdout).to_string();
    if diff.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(diff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("a2a-bridge-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn extracts_git_diff_from_workdir() {
        let dir = temp_dir("diff");
        git(&dir, &["init", "-q"]);
        git(&dir, &["config", "user.email", "t@t"]);
        git(&dir, &["config", "user.name", "t"]);
        fs::write(dir.join("hello.txt"), "one\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "init"]);
        fs::write(dir.join("hello.txt"), "one\ntwo\n").unwrap();

        let diff = git_diff(Some(dir.to_str().unwrap())).unwrap().unwrap();
        assert!(diff.contains("+two"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_head_yields_none_artifact() {
        let dir = temp_dir("nohead");
        git(&dir, &["init", "-q"]);
        fs::write(dir.join("hello.txt"), "one\n").unwrap();
        fs::write(dir.join("hello.txt"), "one\ntwo\n").unwrap();

        assert_eq!(git_diff(Some(dir.to_str().unwrap())).unwrap(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    fn fake_agent(script: &str) -> ExecSpec {
        ExecSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            workdir: None,
            error_patterns: &["Authentication Error"],
            require_output: false,
        }
    }

    #[test]
    fn fake_agent_completed() {
        let o = execute(&fake_agent("exit 0")).unwrap();
        assert!(matches!(o, TaskOutcome::Completed { .. }));
    }

    /// Existing adapters' semantics stay put: exit 0 + empty stdout is still
    /// Completed unless the spec opts into `require_output`.
    #[test]
    fn require_output_demotes_exit0_empty_stdout_to_failed() {
        let mut spec = fake_agent("exit 0");
        spec.require_output = true;
        let o = execute(&spec).unwrap();
        assert!(
            matches!(&o, TaskOutcome::Failed { reason } if reason.contains("no output")),
            "{o:?}"
        );
    }

    #[test]
    fn require_output_keeps_exit0_nonempty_stdout_completed() {
        let mut spec = fake_agent("echo task done; exit 0");
        spec.require_output = true;
        let o = execute(&spec).unwrap();
        assert!(matches!(o, TaskOutcome::Completed { .. }), "{o:?}");
    }

    /// opencode resolves its project root from `$PWD`, not the real cwd, so
    /// `current_dir` alone silently ran it in the spawning repo. Lock the
    /// `PWD` export: the fake agent exits non-zero when it disagrees.
    #[test]
    fn workdir_is_exported_as_pwd() {
        let dir = temp_dir("pwd");
        let mut spec = fake_agent(&format!("[ \"$PWD\" = '{}' ]", dir.display()));
        spec.workdir = Some(dir.to_str().unwrap().into());
        let o = execute(&spec).unwrap();
        assert!(matches!(o, TaskOutcome::Completed { .. }), "{o:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fake_agent_failed_on_nonzero_exit() {
        let o = execute(&fake_agent("echo boom; exit 3")).unwrap();
        assert!(matches!(o, TaskOutcome::Failed { reason } if reason.contains("non-zero")));
    }

    #[test]
    fn fake_agent_failed_on_pattern_despite_zero_exit() {
        let o = execute(&fake_agent("echo 'Authentication Error: nope'; exit 0")).unwrap();
        assert!(
            matches!(o, TaskOutcome::Failed { reason } if reason.contains("Authentication Error"))
        );
    }
}
