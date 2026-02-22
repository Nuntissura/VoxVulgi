use std::ffi::OsStr;
use std::process::Command;

pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    configure_for_background(&mut cmd);
    cmd
}

#[cfg(windows)]
fn configure_for_background(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;

    // Prevent console windows from stealing focus on Windows while running tools.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_for_background(_cmd: &mut Command) {}
