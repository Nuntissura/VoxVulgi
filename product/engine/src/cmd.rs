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

#[cfg(windows)]
struct WindowsChildLifecycleJob {
    handle: isize,
}

#[cfg(windows)]
impl WindowsChildLifecycleJob {
    fn new() -> std::io::Result<Self> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error());
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                let _ = CloseHandle(handle);
                return Err(error);
            }

            Ok(Self {
                handle: handle as isize,
            })
        }
    }

    fn assign(&self, child: &std::process::Child) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        if unsafe { AssignProcessToJobObject(self.handle as _, child.as_raw_handle() as _) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    fn terminate_all(&self) -> std::io::Result<()> {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;

        if unsafe { TerminateJobObject(self.handle as _, 1) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for WindowsChildLifecycleJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

#[cfg(windows)]
fn yt_dlp_lifecycle_job() -> &'static std::sync::OnceLock<WindowsChildLifecycleJob> {
    static JOB: std::sync::OnceLock<WindowsChildLifecycleJob> = std::sync::OnceLock::new();
    &JOB
}

#[cfg(windows)]
fn yt_dlp_lifecycle_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

#[cfg(windows)]
fn yt_dlp_shutdown_started() -> &'static std::sync::atomic::AtomicBool {
    static SHUTTING_DOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    &SHUTTING_DOWN
}

/// Bind a VoxVulgi-owned yt-dlp process to a Windows Job Object whose last handle is owned by
/// the VoxVulgi process. Windows closes that handle on both normal process exit and crashes, and
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` then terminates yt-dlp and its inherited descendants.
#[cfg(windows)]
pub fn bind_yt_dlp_child_to_app_lifecycle(child: &std::process::Child) -> std::io::Result<()> {
    use std::sync::atomic::Ordering;

    let _guard = yt_dlp_lifecycle_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if yt_dlp_shutdown_started().load(Ordering::Acquire) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "VoxVulgi is shutting down; refusing to start an untracked yt-dlp process",
        ));
    }

    if yt_dlp_lifecycle_job().get().is_none() {
        let job = WindowsChildLifecycleJob::new()?;
        let _ = yt_dlp_lifecycle_job().set(job);
    }
    yt_dlp_lifecycle_job()
        .get()
        .expect("yt-dlp lifecycle job initialized under lock")
        .assign(child)
}

#[cfg(not(windows))]
pub fn bind_yt_dlp_child_to_app_lifecycle(_child: &std::process::Child) -> std::io::Result<()> {
    Ok(())
}

/// Stop every yt-dlp process owned by this VoxVulgi instance and reject late shutdown-time
/// spawns. The Job Object handle remains valid until process exit so crash cleanup stays armed.
#[cfg(windows)]
pub fn shutdown_yt_dlp_children() -> std::io::Result<()> {
    use std::sync::atomic::Ordering;

    let _guard = yt_dlp_lifecycle_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    yt_dlp_shutdown_started().store(true, Ordering::Release);
    match yt_dlp_lifecycle_job().get() {
        Some(job) => job.terminate_all(),
        None => Ok(()),
    }
}

#[cfg(not(windows))]
pub fn shutdown_yt_dlp_children() -> std::io::Result<()> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    fn wait_for_terminated(child: &mut std::process::Child) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().expect("query child").is_some() {
                return;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("lifecycle job did not terminate its assigned child");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_pid_terminated(pid: u32) {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

        const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;

        unsafe {
            let handle = OpenProcess(SYNCHRONIZE_ACCESS, 0, pid);
            if handle.is_null() {
                return;
            }
            let wait = WaitForSingleObject(handle, 5_000);
            let _ = CloseHandle(handle);
            assert_eq!(
                wait, WAIT_OBJECT_0,
                "child PID {pid} survived owner-process abort"
            );
        }
    }

    fn sleeping_child() -> std::process::Child {
        command("ping.exe")
            .args(["-n", "60", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn lifecycle probe")
    }

    #[test]
    fn closing_lifecycle_job_terminates_assigned_child() {
        let job = WindowsChildLifecycleJob::new().expect("create lifecycle job");
        let mut child = sleeping_child();
        job.assign(&child).expect("assign lifecycle probe");
        drop(job);
        wait_for_terminated(&mut child);
    }

    #[test]
    fn explicit_lifecycle_shutdown_terminates_assigned_child() {
        let job = WindowsChildLifecycleJob::new().expect("create lifecycle job");
        let mut child = sleeping_child();
        job.assign(&child).expect("assign lifecycle probe");
        job.terminate_all().expect("terminate lifecycle job");
        wait_for_terminated(&mut child);
    }

    #[test]
    fn crash_lifecycle_helper() {
        let Ok(pid_path) = std::env::var("VOXVULGI_LIFECYCLE_CRASH_PID_PATH") else {
            return;
        };
        let job = WindowsChildLifecycleJob::new().expect("create crash lifecycle job");
        let child = sleeping_child();
        job.assign(&child).expect("assign crash lifecycle probe");
        std::fs::write(pid_path, child.id().to_string()).expect("publish crash child PID");
        std::thread::sleep(Duration::from_secs(60));
        panic!("crash lifecycle helper was not terminated by its parent test");
    }

    #[test]
    fn abrupt_owner_abort_terminates_assigned_child() {
        let pid_path = std::env::temp_dir().join(format!(
            "voxvulgi_lifecycle_crash_{}_{}.pid",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let mut helper =
            std::process::Command::new(std::env::current_exe().expect("current test exe"))
                .args([
                    "--exact",
                    "cmd::tests::crash_lifecycle_helper",
                    "--nocapture",
                ])
                .env("VOXVULGI_LIFECYCLE_CRASH_PID_PATH", &pid_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("run crash lifecycle helper");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !pid_path.is_file() {
            assert!(
                Instant::now() < deadline,
                "crash helper did not publish its child PID"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let child_pid = std::fs::read_to_string(&pid_path)
            .expect("read crash child PID")
            .trim()
            .parse::<u32>()
            .expect("parse crash child PID");
        helper.kill().expect("abruptly terminate lifecycle owner");
        let status = helper.wait().expect("wait for terminated lifecycle owner");
        assert!(!status.success(), "crash helper must terminate abruptly");
        let _ = std::fs::remove_file(&pid_path);
        wait_for_pid_terminated(child_pid);
    }
}
