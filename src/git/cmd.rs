//! Thin wrapper around the `git` executable, used for all write operations.

use anyhow::{Result, anyhow};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct LogLine {
    pub cmd: String,
    pub ok: bool,
}

pub type CmdLog = Arc<Mutex<Vec<LogLine>>>;

#[derive(Clone)]
pub struct Git {
    pub dir: PathBuf,
    pub env: Vec<(String, String)>,
    pub log: CmdLog,
}

pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Git {
    pub fn new(dir: &Path, env: Vec<(String, String)>, log: CmdLog) -> Self {
        Self {
            dir: dir.to_path_buf(),
            env,
            log,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new("git");
        c.current_dir(&self.dir)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_EDITOR", "true")
            .env("GIT_MERGE_AUTOEDIT", "no")
            .env("LC_ALL", "C");
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c
    }

    /// Run and return raw output regardless of exit status.
    pub fn exec(
        &self,
        args: &[&str],
        input: Option<&str>,
        envs: &[(&str, &str)],
    ) -> Result<Output> {
        let mut c = self.command(args);
        for (k, v) in envs {
            c.env(k, v);
        }
        c.stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let mut child = c.spawn().map_err(|e| anyhow!("failed to run git: {e}"))?;
        if let Some(inp) = input
            && let Some(mut stdin) = child.stdin.take()
        {
            stdin.write_all(inp.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        let o = Output {
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code().unwrap_or(-1),
        };
        if let Ok(mut l) = self.log.lock() {
            l.push(LogLine {
                cmd: format!("git {}", args.join(" ")),
                ok: o.code == 0,
            });
            let n = l.len();
            if n > 500 {
                l.drain(0..n - 500);
            }
        }
        Ok(o)
    }

    pub fn run_full(
        &self,
        args: &[&str],
        input: Option<&str>,
        envs: &[(&str, &str)],
    ) -> Result<String> {
        let o = self.exec(args, input, envs)?;
        if o.code != 0 {
            let msg = if o.stderr.trim().is_empty() {
                o.stdout
            } else {
                o.stderr
            };
            return Err(anyhow!("{}", msg.trim_end()));
        }
        Ok(o.stdout + &o.stderr)
    }

    pub fn run(&self, args: &[&str]) -> Result<String> {
        self.run_full(args, None, &[])
    }

    pub fn run_input(&self, args: &[&str], input: &str) -> Result<String> {
        self.run_full(args, Some(input), &[])
    }

    /// Read-only command, not recorded in the command log; stdout only.
    pub fn read(&self, args: &[&str]) -> Result<String> {
        let out = self
            .command(args)
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()?;
        let code = out.status.code().unwrap_or(-1);
        // `diff --no-index` exits 1 when files differ.
        if code != 0 && !(code == 1 && args.contains(&"--no-index")) {
            return Err(anyhow!(
                "{}",
                String::from_utf8_lossy(&out.stderr).trim_end()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

/// Split a command line into words, honoring single/double quotes and backslashes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut has = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, ' ' | '\t') => {
                if has {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, '\'' | '"') => {
                quote = Some(c);
                has = true;
            }
            (Some(q), c) if c == q => quote = None,
            (q, '\\') if q != Some('\'') => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                    has = true;
                }
            }
            (_, c) => {
                cur.push(c);
                has = true;
            }
        }
    }
    if has {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split() {
        assert_eq!(
            split_args(r#"commit -m "a b" 'c d' e\ f"#),
            vec!["commit", "-m", "a b", "c d", "e f"]
        );
    }
}
