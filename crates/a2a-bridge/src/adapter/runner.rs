use std::process::Command;

use anyhow::Result;

use super::outcome::{TaskOutcome, classify};

/// A headless one-shot invocation of an agent (Engine A).
pub struct ExecSpec {
    pub program: String,
    pub args: Vec<String>,
    pub workdir: Option<String>,
    pub error_patterns: &'static [&'static str],
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
    Ok(with_artifact(outcome, spec.workdir.as_deref()))
}

fn with_artifact(outcome: TaskOutcome, workdir: Option<&str>) -> TaskOutcome {
    match outcome {
        TaskOutcome::Completed { artifact: None } => TaskOutcome::Completed {
            artifact: git_diff(workdir).ok().flatten(),
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
        }
    }

    #[test]
    fn fake_agent_completed() {
        let o = execute(&fake_agent("exit 0")).unwrap();
        assert!(matches!(o, TaskOutcome::Completed { .. }));
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
