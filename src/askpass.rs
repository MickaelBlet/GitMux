//! GIT_ASKPASS / SSH_ASKPASS bridge: git spawns `gitmux <prompt>`, which hands the
//! prompt to the running TUI through files in a private temp dir and prints the answer.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const ENV_DIR: &str = "GITMUX_ASKPASS_DIR";
pub const CANCEL: &str = "\u{18}";

/// Called from main() when running as askpass helper. Returns exit code.
pub fn helper(dir: &Path, prompt: &str) -> i32 {
    let id = std::process::id();
    let req = dir.join(format!("req-{id}"));
    let tmp = dir.join(format!(".req-{id}"));
    if std::fs::write(&tmp, prompt).is_err() || std::fs::rename(&tmp, &req).is_err() {
        return 1;
    }
    let ans = dir.join(format!("ans-{id}"));
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(600) {
        if let Ok(s) = std::fs::read_to_string(&ans) {
            let _ = std::fs::remove_file(&ans);
            if s == CANCEL {
                return 1;
            }
            println!("{s}");
            return 0;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::fs::remove_file(&req);
    1
}

pub struct Bridge {
    pub dir: PathBuf,
}

impl Bridge {
    pub fn new() -> Option<Bridge> {
        let dir = std::env::temp_dir().join(format!("gitmux-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
        }
        Some(Bridge { dir })
    }

    pub fn env(&self) -> Vec<(String, String)> {
        let exe = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        vec![
            ("GIT_ASKPASS".into(), exe.clone()),
            ("SSH_ASKPASS".into(), exe),
            ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
            (
                "DISPLAY".into(),
                std::env::var("DISPLAY").unwrap_or_else(|_| ":0".into()),
            ),
            (ENV_DIR.into(), self.dir.to_string_lossy().into_owned()),
        ]
    }

    /// Pending prompts: (id, prompt). The request file is consumed.
    pub fn poll(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_prefix("req-")
                    && let Ok(p) = std::fs::read_to_string(e.path())
                {
                    let _ = std::fs::remove_file(e.path());
                    out.push((id.to_string(), p));
                }
            }
        }
        out
    }

    pub fn answer(&self, id: &str, answer: &str) {
        let tmp = self.dir.join(format!(".ans-{id}"));
        if std::fs::write(&tmp, answer).is_ok() {
            let _ = std::fs::rename(&tmp, self.dir.join(format!("ans-{id}")));
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
