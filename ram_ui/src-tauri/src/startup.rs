use std::io;

#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> io::Result<()> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    let current_exe = std::env::current_exe()?;
    let (run_key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        KEY_SET_VALUE,
    )?;

    if enabled {
        let command = format!("\"{}\"", current_exe.display());
        run_key.set_value("RobloxManager", &command)
    } else {
        match run_key.delete_value("RobloxManager") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Windows startup is only available on Windows",
    ))
}
