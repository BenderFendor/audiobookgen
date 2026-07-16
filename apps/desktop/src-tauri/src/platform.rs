use std::process::{Child, Command, Stdio};

pub struct SleepGuard {
    child: Option<Child>,
}

impl SleepGuard {
    pub fn acquire() -> Self {
        let child = platform_command().and_then(|mut command| {
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        });
        Self { child }
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(target_os = "linux")]
fn platform_command() -> Option<Command> {
    let mut command = Command::new("systemd-inhibit");
    command.args([
        "--what=sleep",
        "--why=AudiobookGen is generating narration",
        "--mode=block",
        "sleep",
        "infinity",
    ]);
    Some(command)
}

#[cfg(target_os = "macos")]
fn platform_command() -> Option<Command> {
    let mut command = Command::new("caffeinate");
    command.args(["-dims"]);
    Some(command)
}

#[cfg(target_os = "windows")]
fn platform_command() -> Option<Command> {
    let script = r#"Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class Native { [DllImport("kernel32.dll")] public static extern uint SetThreadExecutionState(uint esFlags); }'; while ($true) { [Native]::SetThreadExecutionState(0x80000003) | Out-Null; Start-Sleep -Seconds 20 }"#;
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    Some(command)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn platform_command() -> Option<Command> {
    None
}
