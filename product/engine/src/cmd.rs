use std::ffi::OsStr;
use std::io::Read;
use std::process::Command;
use std::process::Stdio;
use std::time::{Duration, Instant};

pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    configure_for_background(&mut cmd);
    cmd
}

/// Terminate a VoxVulgi-owned child and its descendants, then reap the direct child.
/// Callers must only pass children they started themselves.
pub fn terminate_child_process_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        if let Ok(mut taskkill) = command("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                match taskkill.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    _ => {
                        let _ = taskkill.kill();
                        let _ = taskkill.wait();
                        break;
                    }
                }
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

/// Run an owned child with bounded lifetime, cooperative cancellation, complete pipe draining,
/// descendant-tree termination, and direct-child reaping.
pub fn run_owned_output<F>(
    command: &mut Command,
    timeout: Duration,
    should_cancel: F,
) -> std::io::Result<std::process::Output>
where
    F: FnMut() -> bool,
{
    run_owned_output_with_pid(command, timeout, should_cancel).map(|(output, _)| output)
}

/// Variant of [`run_owned_output`] that returns the direct child PID for diagnostics.
pub fn run_owned_output_with_pid<F>(
    command: &mut Command,
    timeout: Duration,
    mut should_cancel: F,
) -> std::io::Result<(std::process::Output, u32)>
where
    F: FnMut() -> bool,
{
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_owned_launch(command);
    let mut child = command.spawn()?;
    let child_pid = child.id();
    #[cfg(windows)]
    let lifecycle_job = match WindowsChildLifecycleJob::new().and_then(|job| {
        job.assign(&child)?;
        Ok(job)
    }) {
        Ok(job) => job,
        Err(error) => {
            terminate_child_process_tree(&mut child);
            return Err(std::io::Error::other(format!(
                "failed to bind owned child pid {child_pid} to its lifecycle job: {error}"
            )));
        }
    };
    #[cfg(windows)]
    if let Err(error) = resume_owned_child(&child) {
        let _ = lifecycle_job.terminate_all();
        terminate_child_process_tree(&mut child);
        return Err(std::io::Error::other(format!(
            "failed to resume owned child pid {child_pid} after lifecycle assignment: {error}"
        )));
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("owned child stdout pipe missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("owned child stderr pipe missing"))?;
    let (stdout_sender, stdout_receiver) = std::sync::mpsc::sync_channel(1);
    let (stderr_sender, stderr_receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stdout).read_to_end(&mut bytes);
        let _ = stdout_sender.send(bytes);
    });
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::BufReader::new(stderr).read_to_end(&mut bytes);
        let _ = stderr_sender.send(bytes);
    });
    let started = Instant::now();
    loop {
        let cancellation = should_cancel();
        let timed_out = timeout > Duration::ZERO && started.elapsed() >= timeout;
        if cancellation || timed_out {
            #[cfg(windows)]
            let _ = lifecycle_job.terminate_all();
            terminate_child_process_tree(&mut child);
            let pipe_deadline = Instant::now() + Duration::from_secs(2);
            let _ = stdout_receiver
                .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()));
            let _ = stderr_receiver
                .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()));
            return Err(std::io::Error::new(
                if timed_out {
                    std::io::ErrorKind::TimedOut
                } else {
                    std::io::ErrorKind::Interrupted
                },
                if timed_out {
                    format!(
                        "owned child pid {child_pid} timed out after {} ms and was terminated",
                        timeout.as_millis()
                    )
                } else {
                    format!("owned child pid {child_pid} was canceled and terminated")
                },
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // A direct child can exit after spawning an inheriting descendant. Terminate the
                // per-command job before joining pipe readers so an inherited pipe cannot hold the
                // worker forever after its nominal command has completed.
                #[cfg(windows)]
                let _ = lifecycle_job.terminate_all();
                let pipe_deadline = Instant::now() + Duration::from_secs(5);
                let stdout = stdout_receiver
                    .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!(
                                "owned child pid {child_pid} exited but its stdout pipe did not close after descendant termination"
                            ),
                        )
                    })?;
                let stderr = stderr_receiver
                    .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()))
                    .map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!(
                                "owned child pid {child_pid} exited but its stderr pipe did not close after descendant termination"
                            ),
                        )
                    })?;
                return Ok((
                    std::process::Output {
                        status,
                        stdout,
                        stderr,
                    },
                    child_pid,
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                #[cfg(windows)]
                let _ = lifecycle_job.terminate_all();
                terminate_child_process_tree(&mut child);
                let pipe_deadline = Instant::now() + Duration::from_secs(2);
                let _ = stdout_receiver
                    .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()));
                let _ = stderr_receiver
                    .recv_timeout(pipe_deadline.saturating_duration_since(Instant::now()));
                return Err(error);
            }
        }
    }
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
fn configure_owned_launch(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
}

#[cfg(not(windows))]
fn configure_owned_launch(_cmd: &mut Command) {}

#[cfg(windows)]
fn resume_owned_child(child: &std::process::Child) -> std::io::Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut found_thread_id = None;
        if Thread32First(snapshot, &mut entry) != 0 {
            loop {
                if entry.th32OwnerProcessID == child.id() {
                    found_thread_id = Some(entry.th32ThreadID);
                    break;
                }
                if Thread32Next(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
        let thread_id = found_thread_id.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "suspended primary thread for pid {} was not found",
                    child.id()
                ),
            )
        })?;
        let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id);
        if thread.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let resume_result = ResumeThread(thread);
        let resume_error = (resume_result == u32::MAX).then(std::io::Error::last_os_error);
        let _ = CloseHandle(thread);
        match resume_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

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

/// Spawn a yt-dlp process suspended, bind it to the process-wide lifecycle job, and only then
/// allow it to execute. The suspended launch closes the window in which yt-dlp could create an
/// inheriting descendant before Windows has assigned the direct child to the app-owned job.
pub fn spawn_yt_dlp_child_to_app_lifecycle(
    command: &mut Command,
) -> std::io::Result<std::process::Child> {
    configure_owned_launch(command);
    let mut child = command.spawn()?;
    let child_pid = child.id();
    if let Err(error) = bind_yt_dlp_child_to_app_lifecycle(&child) {
        terminate_child_process_tree(&mut child);
        return Err(std::io::Error::new(
            error.kind(),
            format!(
                "failed to bind suspended yt-dlp child pid {child_pid} to the VoxVulgi process lifecycle: {error}"
            ),
        ));
    }
    #[cfg(windows)]
    if let Err(error) = resume_owned_child(&child) {
        terminate_child_process_tree(&mut child);
        return Err(std::io::Error::new(
            error.kind(),
            format!(
                "failed to resume yt-dlp child pid {child_pid} after lifecycle assignment: {error}"
            ),
        ));
    }
    Ok(child)
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

    struct DescendantCleanup {
        pid_path: std::path::PathBuf,
    }

    impl Drop for DescendantCleanup {
        fn drop(&mut self) {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, TerminateProcess, PROCESS_TERMINATE,
            };

            if let Ok(pid_text) = std::fs::read_to_string(&self.pid_path) {
                if let Ok(pid) = pid_text.trim().parse::<u32>() {
                    unsafe {
                        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
                        if !handle.is_null() {
                            let _ = TerminateProcess(handle, 1);
                            let _ = CloseHandle(handle);
                        }
                    }
                }
            }
            let _ = std::fs::remove_file(&self.pid_path);
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
    fn immediate_inheriting_descendant_helper() {
        let Ok(pid_path) = std::env::var("VOXVULGI_IMMEDIATE_DESCENDANT_PID_PATH") else {
            return;
        };
        let descendant = command("ping.exe")
            .args(["-n", "60", "127.0.0.1"])
            .spawn()
            .expect("spawn immediate inheriting descendant");
        std::fs::write(pid_path, descendant.id().to_string())
            .expect("publish immediate descendant PID");
    }

    #[test]
    fn owned_output_terminates_inheriting_descendant_before_pipe_drain() {
        let pid_path = std::env::temp_dir().join(format!(
            "voxvulgi_owned_descendant_{}_{}.pid",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = DescendantCleanup {
            pid_path: pid_path.clone(),
        };
        let mut command = command(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "--exact",
                "cmd::tests::immediate_inheriting_descendant_helper",
                "--nocapture",
            ])
            .env("VOXVULGI_IMMEDIATE_DESCENDANT_PID_PATH", &pid_path);
        let started = Instant::now();
        let output = run_owned_output(&mut command, Duration::from_secs(10), || false)
            .expect("owned parent and inheriting descendant must settle within the bound");
        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_secs(15));
        let descendant_pid = std::fs::read_to_string(&pid_path)
            .expect("parent must publish descendant PID")
            .trim()
            .parse::<u32>()
            .expect("parse descendant PID");
        wait_for_pid_terminated(descendant_pid);
    }

    #[test]
    fn yt_dlp_app_lifecycle_immediate_descendant_helper() {
        let Ok(pid_path) = std::env::var("VOXVULGI_YTDLP_LIFECYCLE_DESCENDANT_PID_PATH") else {
            return;
        };
        let mut command = command(std::env::current_exe().expect("current test executable"));
        command
            .args([
                "--exact",
                "cmd::tests::immediate_inheriting_descendant_helper",
                "--nocapture",
            ])
            .env("VOXVULGI_IMMEDIATE_DESCENDANT_PID_PATH", &pid_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut child = spawn_yt_dlp_child_to_app_lifecycle(&mut command)
            .expect("spawn suspended lifecycle-bound yt-dlp stand-in");
        let status = child.wait().expect("wait for yt-dlp stand-in");
        assert!(
            status.success(),
            "yt-dlp stand-in must publish its descendant"
        );
        std::thread::sleep(Duration::from_secs(60));
        panic!("yt-dlp lifecycle owner helper was not terminated by its parent test");
    }

    #[test]
    fn yt_dlp_suspended_launch_contains_immediate_descendant_after_owner_abort() {
        let pid_path = std::env::temp_dir().join(format!(
            "voxvulgi_yt_dlp_descendant_{}_{}.pid",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _cleanup = DescendantCleanup {
            pid_path: pid_path.clone(),
        };
        let mut helper =
            std::process::Command::new(std::env::current_exe().expect("current test exe"))
                .args([
                    "--exact",
                    "cmd::tests::yt_dlp_app_lifecycle_immediate_descendant_helper",
                    "--nocapture",
                ])
                .env("VOXVULGI_YTDLP_LIFECYCLE_DESCENDANT_PID_PATH", &pid_path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("run yt-dlp lifecycle owner helper");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pid_path.is_file() {
            assert!(
                Instant::now() < deadline,
                "yt-dlp lifecycle helper did not publish its immediate descendant PID"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        let descendant_pid = std::fs::read_to_string(&pid_path)
            .expect("read yt-dlp descendant PID")
            .trim()
            .parse::<u32>()
            .expect("parse yt-dlp descendant PID");
        helper
            .kill()
            .expect("abruptly terminate yt-dlp lifecycle owner");
        let status = helper
            .wait()
            .expect("wait for terminated yt-dlp lifecycle owner");
        assert!(!status.success(), "lifecycle owner must terminate abruptly");
        wait_for_pid_terminated(descendant_pid);
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
