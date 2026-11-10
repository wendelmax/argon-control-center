use std::path::Path;
use std::process::Command;

/// Returns true if the current process is running as root (euid == 0).
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

/// Runs a shell script with root privileges.
/// If already running as root, executes `/bin/bash -c <script>` directly.
/// Otherwise uses `pkexec /bin/bash -c <script>` (triggers native KDE/Polkit password dialog).
pub fn run_root_script(script: &str) -> Result<String, String> {
    run_root_script_with_stdin(script, None)
}

/// Same as [`run_root_script`], optionally feeding `stdin` to the script. Used to hand file
/// contents to root without staging them in a world-writable location.
pub fn run_root_script_with_stdin(script: &str, stdin: Option<&[u8]>) -> Result<String, String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut cmd = if is_root() {
        let mut c = Command::new("/bin/bash");
        c.arg("-c").arg(script);
        c
    } else if Path::new("/usr/bin/pkexec").exists() {
        let mut c = Command::new("/usr/bin/pkexec");
        c.arg("/bin/bash").arg("-c").arg(script);
        c
    } else {
        let mut c = Command::new("sudo");
        c.arg("-n").arg("/bin/bash").arg("-c").arg(script);
        c
    };
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run privileged command: {e}"))?;
    if let Some(data) = stdin {
        if let Some(mut pipe) = child.stdin.take() {
            pipe.write_all(data)
                .map_err(|e| format!("Failed to send content: {e}"))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed waiting for privileged command: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(if stdout.is_empty() {
            "Operation completed successfully.".to_string()
        } else {
            stdout
        })
    } else {
        Err(if stderr.is_empty() {
            format!("Command failed with code {:?}", output.status.code())
        } else {
            stderr
        })
    }
}

/// Single-quotes a path for bash (`'` → `'\''`).
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Root script that reads the new content from stdin, keeps a backup of the previous file
/// (`<file>.argoncc.bak`) and replaces it atomically (write `.argoncc.new`, then `mv`).
pub fn atomic_write_script(target_path: &str, post_cmd: Option<&str>) -> String {
    let t = shell_quote(target_path);
    let new = shell_quote(&format!("{target_path}.argoncc.new"));
    let bak = shell_quote(&format!("{target_path}.argoncc.bak"));
    let mut script = format!(
        "set -e; umask 022; \
         if [ -f {t} ]; then cp -a {t} {bak}; fi; \
         cat > {new}; chmod 644 {new}; sync {new} 2>/dev/null || true; mv -f {new} {t}"
    );
    if let Some(cmd) = post_cmd {
        script.push_str(&format!("; ({cmd})"));
    }
    script
}

/// Writes `content` to a root-owned file. The content goes to the privileged process through
/// stdin (never through a predictable `/tmp` file, which another local user could pre-create or
/// swap), the previous version is kept as `<file>.argoncc.bak`, and the replacement is atomic.
pub fn write_privileged_file(
    target_path: &str,
    content: &str,
    post_cmd: Option<&str>,
) -> Result<String, String> {
    let script = atomic_write_script(target_path, post_cmd);
    run_root_script_with_stdin(&script, Some(content.as_bytes()))
        .map(|out| {
            if out == "Operation completed successfully." {
                format!("{target_path} updated (backup: {target_path}.argoncc.bak).")
            } else {
                out
            }
        })
        .map_err(|e| format!("Failed to write {target_path}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn quote_escapes_single_quotes() {
        assert_eq!(shell_quote("/etc/a'b"), "'/etc/a'\\''b'");
    }

    #[test]
    fn atomic_write_roundtrip_in_tempdir() {
        // Runs the generated script with plain bash (no privileges) against a temp dir.
        let dir = std::env::temp_dir().join(format!("argoncc-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("conf's.txt");
        fs::write(&target, "old\n").unwrap();
        let script = atomic_write_script(target.to_str().unwrap(), None);
        let mut child = Command::new("/bin/bash")
            .arg("-c")
            .arg(&script)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(b"new\n").unwrap();
        }
        assert!(child.wait().unwrap().success());
        assert_eq!(fs::read_to_string(&target).unwrap(), "new\n");
        let bak = dir.join("conf's.txt.argoncc.bak");
        assert_eq!(fs::read_to_string(bak).unwrap(), "old\n");
        assert!(!dir.join("conf's.txt.argoncc.new").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
