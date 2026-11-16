use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    ffi::OsStr,
    io,
    path::Path,
    process::{Command, Stdio},
};

const LATEST_RELEASE_URL: &str = "https://github.com/Paryx-games/roblox-manager/releases/latest";
const UPDATE_WORKER_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$appPath = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('@@APP_PATH@@'))
$installer = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('@@INSTALLER_PATH@@'))
$checksum = '@@CHECKSUM@@'
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        $running = Get-Process -Name 'rm_tauri' -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -and $_.Path -ieq $appPath }
        if (-not $running) { break }
        if ([DateTime]::UtcNow -ge $deadline) { throw 'RM is still running.' }
        Start-Sleep -Milliseconds 200
    } while ($true)

    $actualChecksum = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualChecksum -ne $checksum) { throw 'The staged installer failed verification.' }
    $process = Start-Process -FilePath $installer -ArgumentList @('/S', '/UPDATE', '/R') -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw 'The installer returned an error.' }
} catch {
    try { Start-Process -FilePath 'explorer.exe' -ArgumentList '@@LATEST_RELEASE_URL@@' } catch {}
    try {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show('RM could not finish installing the update. The GitHub release page is open for manual installer repair.', 'Roblox Manager update') | Out-Null
    } catch {}
} finally {
    Remove-Item -LiteralPath $installer -Force -ErrorAction SilentlyContinue
}
"#;

fn is_staged_installer(path: &Path) -> bool {
    let Some(filename) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(id) = filename
        .strip_prefix("rm-update-")
        .and_then(|name| name.strip_suffix(".exe"))
    else {
        return false;
    };
    if uuid::Uuid::parse_str(id).is_err() {
        return false;
    }
    let Some(parent) = path.parent().and_then(|parent| parent.canonicalize().ok()) else {
        return false;
    };
    std::env::temp_dir()
        .canonicalize()
        .is_ok_and(|temp| parent == temp)
        && path.is_file()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn encode_os_str(value: &OsStr) -> String {
    use std::os::windows::ffi::OsStrExt;

    let bytes = value
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    STANDARD.encode(bytes)
}

fn encode_utf16(value: &str) -> String {
    STANDARD.encode(
        value
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

fn worker_script(executable: &Path, installer: &Path, checksum: &str) -> String {
    UPDATE_WORKER_SCRIPT
        .replace("@@APP_PATH@@", &encode_os_str(executable.as_os_str()))
        .replace("@@INSTALLER_PATH@@", &encode_os_str(installer.as_os_str()))
        .replace("@@CHECKSUM@@", &checksum.to_ascii_lowercase())
        .replace("@@LATEST_RELEASE_URL@@", LATEST_RELEASE_URL)
}

pub fn spawn_update_worker(executable: &Path, installer: &Path, checksum: &str) -> io::Result<()> {
    if !is_staged_installer(installer) || !valid_sha256(checksum) {
        return Err(io::Error::other("invalid staged update"));
    }

    let script = worker_script(executable, installer, checksum);
    let encoded_script = encode_utf16(&script);
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &encoded_script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_installer_requires_the_expected_temporary_file_name() {
        use std::fs;

        let path = std::env::temp_dir().join(format!("rm-update-{}.exe", uuid::Uuid::new_v4()));
        fs::write(&path, b"synthetic").unwrap();
        assert!(is_staged_installer(&path));
        let unrelated = std::env::temp_dir().join("not-an-rm-update.exe");
        assert!(!is_staged_installer(&unrelated));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn update_worker_encodes_paths_and_waits_for_the_app_to_exit() {
        let executable = Path::new(r"C:\Users\O'Connor\RM\rm_tauri.exe");
        let installer = Path::new(r"C:\Users\O'Connor\AppData\Local\Temp\rm-update-abc.exe");
        let checksum = "ab".repeat(32);
        let script = worker_script(executable, installer, &checksum);

        assert!(script.contains(&encode_os_str(executable.as_os_str())));
        assert!(script.contains(&encode_os_str(installer.as_os_str())));
        assert!(!script.contains(r"C:\Users\O'Connor"));
        assert!(script.contains("Get-Process -Name 'rm_tauri'"));
        assert!(script.contains("Get-FileHash -LiteralPath $installer -Algorithm SHA256"));
        assert!(script.contains("Start-Process -FilePath $installer"));
        assert!(script.contains(LATEST_RELEASE_URL));
    }

    #[test]
    fn update_worker_script_parses_as_powershell() {
        let script = worker_script(
            Path::new(r"C:\RM\rm_tauri.exe"),
            Path::new(r"C:\Temp\rm-update-abc.exe"),
            &"ab".repeat(32),
        );
        let parser = format!(
            "$source = [Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('{}')); \
             $tokens = $null; $errors = $null; \
             [System.Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors) | Out-Null; \
             if ($errors.Count -gt 0) {{ $errors | ForEach-Object {{ Write-Error $_ }}; exit 1 }}",
            encode_utf16(&script)
        );
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &parser])
            .output()
            .expect("Windows PowerShell should be available");

        assert!(
            result.status.success(),
            "PowerShell parser rejected update worker script: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
