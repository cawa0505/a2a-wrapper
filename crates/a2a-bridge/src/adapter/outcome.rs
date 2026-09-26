/// Result of running a coding-agent task as a headless subprocess (Engine A).
#[derive(Debug, PartialEq, Eq)]
pub enum TaskOutcome {
    /// Exit 0 and no error pattern matched. `artifacts` carries the task
    /// artifacts as text (e.g. `git diff`; rho also carries its status JSON).
    Completed { artifacts: Vec<String> },
    /// Nonzero exit or an error pattern matched; reason carries the detail.
    Failed { reason: String },
}

/// Classify a subprocess result against the adapter's error patterns.
///
/// Invariant: `Completed` iff `exit_ok` AND no pattern matched in output.
/// Error patterns are the safety net because some agents (aider) exit 0 even
/// on auth/API failures — verified empirically 2026-08-08.
pub fn classify(exit_ok: bool, stdout: &str, stderr: &str, patterns: &[&str]) -> TaskOutcome {
    if let Some(reason) = match_pattern(stdout, stderr, patterns) {
        return TaskOutcome::Failed { reason };
    }
    if !exit_ok {
        return TaskOutcome::Failed {
            reason: format!("non-zero exit; stderr tail: {}", tail(stderr, 500)),
        };
    }
    TaskOutcome::Completed {
        artifacts: Vec::new(),
    }
}

fn match_pattern(stdout: &str, stderr: &str, patterns: &[&str]) -> Option<String> {
    patterns
        .iter()
        .find(|p| stderr.contains(**p) || stdout.contains(**p))
        .map(|p| format!("error pattern matched: {p:?}"))
}

fn tail(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.trim().to_string()
    } else {
        let cut = chars.len() - max_chars;
        format!("...{}", chars[cut..].iter().collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATTERNS: &[&str] = &["Authentication Error", "Rate limit"];

    #[test]
    fn completed_when_clean_and_zero_exit() {
        assert_eq!(
            classify(true, "ok", "", PATTERNS),
            TaskOutcome::Completed {
                artifacts: Vec::new()
            }
        );
    }

    #[test]
    fn failed_when_pattern_in_stderr_despite_zero_exit() {
        let o = classify(true, "", "Authentication Error: invalid token", PATTERNS);
        assert!(
            matches!(o, TaskOutcome::Failed { reason } if reason.contains("Authentication Error"))
        );
    }

    #[test]
    fn failed_when_pattern_in_stdout() {
        let o = classify(true, "Rate limit reached", "", PATTERNS);
        assert!(matches!(o, TaskOutcome::Failed { .. }));
    }

    #[test]
    fn failed_on_nonzero_exit_even_when_output_clean() {
        let o = classify(false, "some output", "boom", PATTERNS);
        assert!(matches!(o, TaskOutcome::Failed { reason } if reason.contains("non-zero")));
    }

    #[test]
    fn tail_truncates_on_char_boundary() {
        let s = "中文content".repeat(100);
        let t = tail(&s, 50);
        assert!(t.starts_with("..."));
        assert_eq!(t.chars().count(), 53);
    }
}
