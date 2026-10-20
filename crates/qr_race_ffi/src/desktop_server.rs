//! Desktop server supervision from the bridge.
//!
//! The GUI calls `qr_desktop_server_start`; Rust spawns `qr_race_server` and puts
//! it in a Windows job object with `KILL_ON_JOB_CLOSE`. Closing the GUI process
//! therefore closes the job handle and kills the server, so a crashed window can
//! never leave the port occupied.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};
    use std::ptr::null;
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    struct Guard {
        child: Child,
        job: HANDLE,
    }

    // The raw handle is owned by the guard and only touched while it is locked.
    unsafe impl Send for Guard {}

    static GUARD: Mutex<Option<Guard>> = Mutex::new(None);

    pub fn start(binary: &Path, config: &Path) -> Result<(), String> {
        stop();
        let child = Command::new(binary)
            .arg("serve")
            .arg("--config")
            .arg(config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn {}: {error}", binary.display()))?;

        unsafe {
            let job = CreateJobObjectW(null(), null());
            if job.is_null() {
                let mut child = child;
                let _ = child.kill();
                return Err("CreateJobObjectW failed".into());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                let _ = CloseHandle(job);
                let mut child = child;
                let _ = child.kill();
                return Err("SetInformationJobObject failed".into());
            }
            let process = child.as_raw_handle() as HANDLE;
            if AssignProcessToJobObject(job, process) == 0 {
                let _ = CloseHandle(job);
                let mut child = child;
                let _ = child.kill();
                return Err("AssignProcessToJobObject failed".into());
            }
            if let Ok(mut slot) = GUARD.lock() {
                *slot = Some(Guard { child, job });
            }
        }
        Ok(())
    }

    pub fn stop() {
        let guard = GUARD.lock().ok().and_then(|mut slot| slot.take());
        if let Some(mut guard) = guard {
            let _ = guard.child.kill();
            let _ = guard.child.wait();
            unsafe {
                let _ = CloseHandle(guard.job);
            }
        }
    }

    pub fn status() -> i32 {
        let Ok(mut slot) = GUARD.lock() else {
            return 0;
        };
        let Some(guard) = slot.as_mut() else {
            return 0;
        };
        match guard.child.try_wait() {
            Ok(Some(_)) => {
                *slot = None;
                0
            }
            Ok(None) => 1,
            Err(_) => -1,
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::Path;
    pub fn start(_binary: &Path, _config: &Path) -> Result<(), String> {
        Err("desktop server bridge is currently Windows-only".into())
    }
    pub fn stop() {}
    pub fn status() -> i32 {
        0
    }
}

pub use imp::{start, status, stop};
