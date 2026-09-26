use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::outcome::TaskOutcome;
use super::runner::{ExecSpec, execute};

/// Narrow safety net: rho exit codes are honest (client-side probes,
/// 2026-09-26), so only the two config-mistake markers that cannot
/// false-match streamed model output. Broad markers (`Error:`,
/// `Permission denied`) are deliberately omitted.
pub const RHO_ERROR_PATTERNS: &[&str] = &["unknown provider", "is not available for provider"];

/// Global flags + `run --timeout` pinned from the caller environment.
/// The bridge takes NO rho defaults — every value is explicit.
#[derive(Debug)]
struct RhoEnv {
    config: String,
    provider: String,
    model: String,
    timeout: String,
}

/// Run a rho one-shot task (Engine A) with pinned env + dual artifacts.
///
/// Env pinning: `RHO_CONFIG/PROVIDER/MODEL/TIMEOUT` all come from the caller;
/// any missing → `Failed{reason}` with NO process spawned and no silent
/// fallback to rho's own config. Assembly: global flags precede the `run`
/// subcommand; `--timeout` carries its unit verbatim; prompt on argv; workdir
/// via runner `current_dir`. On clean exit the `--output-file` status JSON is
/// merged next to the runner's `git diff HEAD` artifact (dual artifacts); a
/// missing/empty status file on a clean exit is a contract breach → `Failed`.
/// The temp status file is removed on every outcome.
pub fn rho_run(prompt: &str, workdir: Option<String>) -> Result<TaskOutcome> {
    let pinned = match read_env() {
        Ok(e) => e,
        Err(missing) => {
            return Ok(TaskOutcome::Failed {
                reason: format!("rho env not set: {missing}"),
            });
        }
    };

    let output_file = temp_status_path();
    let spec = build_spec(prompt, workdir, &pinned, &output_file);
    let outcome = execute(&spec)?;

    let merged = match outcome {
        TaskOutcome::Completed { artifacts } => match read_status(&output_file) {
            Some(status) => {
                let mut all = Vec::with_capacity(artifacts.len() + 1);
                all.push(status);
                all.extend(artifacts);
                TaskOutcome::Completed { artifacts: all }
            }
            None => TaskOutcome::Failed {
                reason: "rho completed but --output-file status is missing/empty".into(),
            },
        },
        other => other,
    };

    let _ = fs::remove_file(&output_file);
    Ok(merged)
}

fn read_env() -> Result<RhoEnv, &'static str> {
    read_env_from(|k| env::var(k).ok())
}

/// Env read split from the process for testing (edition 2024 makes
/// `set_var`/`remove_var` unsafe — inject the lookup instead of mutating).
fn read_env_from(get: impl Fn(&str) -> Option<String>) -> Result<RhoEnv, &'static str> {
    Ok(RhoEnv {
        config: get("RHO_CONFIG").ok_or("RHO_CONFIG")?,
        provider: get("RHO_PROVIDER").ok_or("RHO_PROVIDER")?,
        model: get("RHO_MODEL").ok_or("RHO_MODEL")?,
        timeout: get("RHO_TIMEOUT").ok_or("RHO_TIMEOUT")?,
    })
}

fn build_spec(
    prompt: &str,
    workdir: Option<String>,
    pinned: &RhoEnv,
    output_file: &Path,
) -> ExecSpec {
    ExecSpec {
        program: "rho".into(),
        args: vec![
            "--config".into(),
            pinned.config.clone(),
            "--provider".into(),
            pinned.provider.clone(),
            "--model".into(),
            pinned.model.clone(),
            "run".into(),
            prompt.into(),
            "--timeout".into(),
            pinned.timeout.clone(),
            "--output-file".into(),
            output_file.display().to_string(),
        ],
        workdir,
        error_patterns: RHO_ERROR_PATTERNS,
    }
}

fn temp_status_path() -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!(
        "a2a-rho-status-{}-{nanos}.json",
        std::process::id()
    ))
}

/// Read the finalized status JSON; missing or empty → `None` (breach signal).
fn read_status(path: &Path) -> Option<String> {
    let s = fs::read_to_string(path).ok()?;
    if s.trim().is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn pinned() -> RhoEnv {
        RhoEnv {
            config: "/etc/rho.toml".into(),
            provider: "gw".into(),
            model: "m1".into(),
            timeout: "180sec".into(),
        }
    }

    #[test]
    fn spec_puts_global_flags_before_run_and_no_dash_c() {
        let out = Path::new("/tmp/s.json");
        let spec = build_spec("do it", Some("/tmp/w".into()), &pinned(), out);
        assert_eq!(spec.program, "rho");
        assert_eq!(
            spec.args,
            vec![
                "--config",
                "/etc/rho.toml",
                "--provider",
                "gw",
                "--model",
                "m1",
                "run",
                "do it",
                "--timeout",
                "180sec",
                "--output-file",
                "/tmp/s.json",
            ]
        );
        // workdir rides runner current_dir, NOT a -C flag (rho differs from zero/codex).
        assert!(!spec.args.iter().any(|a| a == "-C"));
        assert_eq!(spec.workdir.as_deref(), Some("/tmp/w"));
        assert_eq!(spec.error_patterns, RHO_ERROR_PATTERNS);
    }

    #[test]
    fn timeout_unit_is_preserved_verbatim() {
        let spec = build_spec("x", None, &pinned(), Path::new("/tmp/s.json"));
        let i = spec.args.iter().position(|a| a == "--timeout").unwrap();
        assert_eq!(spec.args[i + 1], "180sec");
    }

    #[test]
    fn missing_any_env_var_names_the_missing_one() {
        let full: HashMap<&str, &str> = [
            ("RHO_CONFIG", "c"),
            ("RHO_PROVIDER", "p"),
            ("RHO_MODEL", "m"),
            ("RHO_TIMEOUT", "180sec"),
        ]
        .into_iter()
        .collect();
        for miss in ["RHO_CONFIG", "RHO_PROVIDER", "RHO_MODEL", "RHO_TIMEOUT"] {
            let err = read_env_from(|k| {
                if k == miss {
                    None
                } else {
                    full.get(k).map(|s| s.to_string())
                }
            })
            .unwrap_err();
            assert_eq!(err, miss);
        }
    }

    #[test]
    fn all_env_present_parses() {
        let e = read_env_from(|_| Some("v".into())).unwrap();
        assert_eq!(e.config, "v");
        assert_eq!(e.timeout, "v");
    }

    #[test]
    fn read_status_none_on_empty_or_missing() {
        assert_eq!(read_status(Path::new("/no/such/file.json")), None);
        let p = temp_status_path();
        fs::write(&p, "   \n").unwrap();
        assert_eq!(read_status(&p), None);
        fs::write(&p, "{\"ok\":true}").unwrap();
        assert_eq!(read_status(&p).as_deref(), Some("{\"ok\":true}"));
        let _ = fs::remove_file(&p);
    }
}
