use std::{
    collections::HashSet,
    process::Child,
    sync::{LazyLock, Mutex, MutexGuard},
};

use pontia_core::{Error, Result};

// An unreaped child retains its PID. Use the same lock for signalling and reaping
// so direct children need no task inspection privileges, including on macOS 13.5.
static CHILDREN: LazyLock<Mutex<HashSet<libc::pid_t>>> = LazyLock::new(Default::default);

pub(super) fn reap(mut child: Child) {
    let pid = child.id() as libc::pid_t;
    CHILDREN.lock().unwrap().insert(pid);
    std::thread::spawn(move || {
        let mut info = std::mem::MaybeUninit::uninit();
        loop {
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    info.as_mut_ptr(),
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if result == 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                break;
            }
        }
        let mut children = CHILDREN.lock().unwrap();
        children.remove(&pid);
        // No termination can acquire the old PID after it is removed from the set.
        drop(children);
        let _ = child.wait();
    });
}

// Newer Darwin kernels accept an audit token for SIGTERM. macOS 13.5 also
// supports terminating through a Mach task port. Both name the process instance.
type AuditSignal = unsafe extern "C" fn(*const [i32; 8], libc::c_int) -> libc::c_int;

unsafe extern "C" {
    fn task_name_for_pid(host: u32, pid: libc::pid_t, task: *mut u32) -> i32;
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
}

pub(super) enum SignalTarget {
    Child {
        pid: libc::pid_t,
        _guard: MutexGuard<'static, HashSet<libc::pid_t>>,
    },
    Audit([i32; 8], AuditSignal),
    Task(TaskPort),
}

pub(super) struct TaskPort(u32);

#[allow(deprecated)]
impl Drop for TaskPort {
    fn drop(&mut self) {
        unsafe { mach_port_deallocate(libc::mach_task_self(), self.0) };
    }
}

impl SignalTarget {
    #[allow(deprecated)]
    pub(super) fn open(pid: libc::pid_t) -> Result<Self> {
        let children = CHILDREN.lock().unwrap();
        if children.contains(&pid) {
            return Ok(Self::Child {
                pid,
                _guard: children,
            });
        }
        drop(children);
        let host = unsafe { libc::mach_task_self() };
        let mut port = 0;
        let signal =
            unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"proc_signal_with_audittoken".as_ptr()) };
        if !signal.is_null()
            && unsafe { task_name_for_pid(host, pid, &mut port) } == libc::KERN_SUCCESS
        {
            let port = TaskPort(port);
            let mut token = [0; 8];
            let mut count = 8;
            const TASK_AUDIT_TOKEN: u32 = 15;
            if unsafe { libc::task_info(port.0, TASK_AUDIT_TOKEN, token.as_mut_ptr(), &mut count) }
                == libc::KERN_SUCCESS
                && count == 8
            {
                let signal =
                    unsafe { std::mem::transmute::<*mut libc::c_void, AuditSignal>(signal) };
                return Ok(Self::Audit(token, signal));
            }
        }
        let status = unsafe { libc::task_for_pid(host, pid, &mut port) };
        if status != libc::KERN_SUCCESS {
            return Err(Error::ControlUnknown(format!(
                "cannot acquire a Mach task port for process {pid}: {status}"
            )));
        }
        Ok(Self::Task(TaskPort(port)))
    }

    pub(super) fn signal(self, signo: i32) -> Result<()> {
        match self {
            Self::Child { pid, _guard } => {
                let status = unsafe { libc::kill(pid, signo) };
                let error = std::io::Error::last_os_error();
                if status != 0 && error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error.into());
                }
                Ok(())
            }
            Self::Audit(token, signal) => match unsafe { signal(&token, signo) } {
                0 | libc::ESRCH => Ok(()),
                status => Err(std::io::Error::from_raw_os_error(status).into()),
            },
            Self::Task(port) => {
                let status = unsafe { libc::task_terminate(port.0) };
                if status != libc::KERN_SUCCESS {
                    return Err(Error::ControlUnknown(format!(
                        "cannot terminate Mach task: {status}"
                    )));
                }
                Ok(())
            }
        }
    }
}
