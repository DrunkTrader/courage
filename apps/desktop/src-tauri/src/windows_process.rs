//! Windows 10+ child creation with atomic job assignment and an explicit handle list.
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::mem::{size_of, size_of_val};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    SetHandleInformation, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::Threading::*;

pub struct Child {
    process: OwnedHandle,
    job: OwnedHandle,
    pid: u32,
    pub stdin: Option<File>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
}

fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn pipe(parent_reads: bool) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let (mut read, mut write) = (null_mut(), null_mut());
    // SAFETY: writable out parameters, valid attributes. Each returned handle gets one owner.
    unsafe {
        checked(CreatePipe(&mut read, &mut write, &attributes, 4096))?;
        let read = OwnedHandle::from_raw_handle(read);
        let write = OwnedHandle::from_raw_handle(write);
        let (parent, child) = if parent_reads { (read, write) } else { (write, read) };
        checked(SetHandleInformation(parent.as_raw_handle(), HANDLE_FLAG_INHERIT, 0))?;
        Ok((parent, child))
    }
}

struct Attributes {
    _storage: Vec<usize>,
    pointer: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl Attributes {
    fn new() -> io::Result<Self> {
        let mut bytes = 0;
        // SAFETY: first call requests size; usize storage provides pointer alignment.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
            let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
            let pointer = storage.as_mut_ptr().cast();
            checked(InitializeProcThreadAttributeList(pointer, 2, 0, &mut bytes))?;
            Ok(Self { _storage: storage, pointer })
        }
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: pointer stays valid until storage is dropped after this method.
        unsafe { DeleteProcThreadAttributeList(self.pointer) }
    }
}

// MSVCRT argv quoting, including trailing backslashes and embedded quotes. No shell.
fn quote(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut result = vec![b'"' as u16];
    let mut slashes = 0;
    for ch in value.encode_wide() {
        if ch == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "NUL in argument"));
        }
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        result.push(ch);
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
    Ok(result)
}

impl Child {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        // Callers pass configuration through argv/stdin, never shell interpolation.
        if command.get_envs().next().is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "per-child environment overrides are unsupported",
            ));
        }
        let mut line = quote(command.get_program())?;
        for arg in command.get_args() {
            line.push(b' ' as u16);
            line.extend(quote(arg)?);
        }
        line.push(0);
        let cwd = command
            .get_current_dir()
            .map(|path| path.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>());
        let (stdin, child_in) = pipe(false)?;
        let (stdout, child_out) = pipe(true)?;
        let (stderr, child_err) = pipe(true)?;
        // SAFETY: Win32 structs have correct sizes; all pointer backing allocations/handles
        // outlive CreateProcessW. Job handle is non-inheritable and absent from HANDLE_LIST.
        unsafe {
            let raw_job = CreateJobObjectW(null(), null());
            if raw_job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = OwnedHandle::from_raw_handle(raw_job);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            checked(SetInformationJobObject(
                raw_job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ))?;
            let attributes = Attributes::new()?;
            let handles =
                [child_in.as_raw_handle(), child_out.as_raw_handle(), child_err.as_raw_handle()];
            checked(UpdateProcThreadAttribute(
                attributes.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(&handles),
                null_mut(),
                null(),
            ))?;
            let jobs = [raw_job];
            checked(UpdateProcThreadAttribute(
                attributes.pointer,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                jobs.as_ptr().cast(),
                size_of_val(&jobs),
                null_mut(),
                null(),
            ))?;
            let mut startup = STARTUPINFOEXW::default();
            startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
            startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            startup.StartupInfo.hStdInput = handles[0];
            startup.StartupInfo.hStdOutput = handles[1];
            startup.StartupInfo.hStdError = handles[2];
            startup.lpAttributeList = attributes.pointer;
            let mut info = PROCESS_INFORMATION::default();
            checked(CreateProcessW(
                null(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                null(),
                cwd.as_ref().map_or(null(), |value| value.as_ptr()),
                &startup.StartupInfo,
                &mut info,
            ))?;
            let process = OwnedHandle::from_raw_handle(info.hProcess);
            drop(OwnedHandle::from_raw_handle(info.hThread));
            Ok(Self {
                process,
                job,
                pid: info.dwProcessId,
                stdin: Some(stdin.into()),
                stdout: Some(stdout.into()),
                stderr: Some(stderr.into()),
            })
        }
    }

    pub fn id(&self) -> u32 {
        self.pid
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        // SAFETY: process is an owned, live kernel handle, including after child exit.
        unsafe {
            match WaitForSingleObject(self.process.as_raw_handle(), 0) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    checked(GetExitCodeProcess(self.process.as_raw_handle(), &mut code))?;
                    Ok(Some(ExitStatus::from_raw(code)))
                }
                _ => Err(io::Error::last_os_error()),
            }
        }
    }

    pub fn kill(&mut self) -> io::Result<()> {
        // SAFETY: only this owner's dedicated job (and its descendants) is terminated.
        unsafe { checked(TerminateJobObject(self.job.as_raw_handle(), 1)) }
    }

    pub fn tree_empty(&self) -> io::Result<bool> {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: correct writable buffer type/size and owned job handle.
        unsafe {
            checked(QueryInformationJobObject(
                self.job.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of_val(&info) as u32,
                null_mut(),
            ))?;
        }
        Ok(info.ActiveProcesses == 0)
    }
}

pub fn locking_processes(path: &std::path::Path) -> String {
    use windows_sys::Win32::System::RestartManager::*;
    let mut session = 0;
    let mut key = [0u16; 33];
    let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: valid buffers, each RM session ended below; this is diagnostic only.
    unsafe {
        let code = RmStartSession(&mut session, 0, key.as_mut_ptr());
        if code != 0 {
            return format!("Restart Manager unavailable ({code})");
        }
        let result = (|| {
            let code = RmRegisterResources(session, 1, &file.as_ptr(), 0, null(), 0, null());
            if code != 0 {
                return format!("resource query unavailable ({code})");
            }
            let (mut needed, mut count, mut reason) = (0, 0, 0);
            let code = RmGetList(session, &mut needed, &mut count, null_mut(), &mut reason);
            if code == 0 {
                return "none reported (not proof of no lock)".into();
            }
            if code != 234 || needed > 256 {
                return format!("owner query unavailable ({code})");
            }
            let mut owners = vec![RM_PROCESS_INFO::default(); needed as usize];
            count = needed;
            let code =
                RmGetList(session, &mut needed, &mut count, owners.as_mut_ptr(), &mut reason);
            if code != 0 {
                return format!("owner list changed/unavailable ({code})");
            }
            owners[..count as usize]
                .iter()
                .map(|owner| format!("pid={}", owner.Process.dwProcessId))
                .collect::<Vec<_>>()
                .join(", ")
        })();
        RmEndSession(session);
        result
    }
}
