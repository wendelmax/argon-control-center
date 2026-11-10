use std::fs;
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
    let output = if is_root() {
        Command::new("/bin/bash")
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|e| format!("Falha ao executar bash: {e}"))?
    } else if Path::new("/usr/bin/pkexec").exists() {
        Command::new("/usr/bin/pkexec")
            .arg("/bin/bash")
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|e| format!("Falha ao executar pkexec: {e}"))?
    } else {
        Command::new("sudo")
            .arg("-n")
            .arg("/bin/bash")
            .arg("-c")
            .arg(script)
            .output()
            .map_err(|e| format!("Falha ao executar sudo: {e}"))?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(if stdout.is_empty() {
            "Operação concluída com sucesso.".to_string()
        } else {
            stdout
        })
    } else {
        Err(if stderr.is_empty() {
            format!("Comando falhou com código {:?}", output.status.code())
        } else {
            stderr
        })
    }
}

/// Writes content to a privileged file atomically by staging to `/tmp` first,
/// then moving/copying with root permissions (and optionally running a post-command).
pub fn write_privileged_file(
    target_path: &str,
    content: &str,
    post_cmd: Option<&str>,
) -> Result<String, String> {
    if is_root() {
        fs::write(target_path, content)
            .map_err(|e| format!("Erro ao gravar {target_path}: {e}"))?;
        if let Some(cmd) = post_cmd {
            return run_root_script(cmd);
        }
        return Ok(format!("Arquivo {target_path} atualizado com sucesso."));
    }

    let safe_name = target_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>();
    let tmp_path = format!("/tmp/argon_cc_{}_{}", std::process::id(), safe_name);

    fs::write(&tmp_path, content)
        .map_err(|e| format!("Erro ao criar arquivo temporário {tmp_path}: {e}"))?;

    let script = match post_cmd {
        Some(cmd) => format!(
            "cp -f '{tmp_path}' '{target_path}' && chmod 644 '{target_path}' && rm -f '{tmp_path}' && ({cmd})"
        ),
        None => format!(
            "cp -f '{tmp_path}' '{target_path}' && chmod 644 '{target_path}' && rm -f '{tmp_path}'"
        ),
    };

    let res = run_root_script(&script);
    let _ = fs::remove_file(&tmp_path);
    res
}
