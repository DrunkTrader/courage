//! One owner for each media/speech child, its pipes, readers and Windows job.
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(windows)]
use crate::windows_process::Child;
#[cfg(not(windows))]
use std::process::Child;

const OUTPUT_LIMIT: usize = 64 * 1024;
const LINE_LIMIT: usize = 8192;
const EVENT_LIMIT: usize = 32;
const POLL: Duration = Duration::from_millis(10);
const FORCE_TIMEOUT: Duration = Duration::from_secs(3);

pub struct ReceivedLine {
    pub text: String,
    pub received: Instant,
}

#[derive(Default)]
struct StreamData {
    tail: VecDeque<u8>,
    lines: VecDeque<ReceivedLine>,
}

struct Reader {
    task: Option<JoinHandle<io::Result<()>>>,
    data: Arc<Mutex<StreamData>>,
}

impl Reader {
    fn start(mut pipe: impl Read + Send + 'static, events: bool) -> io::Result<Self> {
        let data = Arc::new(Mutex::new(StreamData::default()));
        let shared = data.clone();
        let task = thread::Builder::new().name("courage-child-pipe".into()).spawn(move || {
            let mut buffer = [0u8; 4096];
            let mut line = Vec::new();
            loop {
                let count = pipe.read(&mut buffer)?;
                if count == 0 {
                    return Ok(());
                }
                let mut data =
                    shared.lock().map_err(|_| io::Error::other("pipe reader poisoned"))?;
                for &byte in &buffer[..count] {
                    if data.tail.len() == OUTPUT_LIMIT {
                        data.tail.pop_front();
                    }
                    data.tail.push_back(byte);
                    if events {
                        if byte == b'\n' {
                            if data.lines.len() == EVENT_LIMIT {
                                data.lines.pop_front();
                            }
                            data.lines.push_back(ReceivedLine {
                                text: String::from_utf8_lossy(&line).trim_end().to_owned(),
                                received: Instant::now(),
                            });
                            line.clear();
                        } else if line.len() < LINE_LIMIT {
                            line.push(byte);
                        }
                    }
                }
            }
        })?;
        Ok(Self { task: Some(task), data })
    }

    fn join(&mut self, deadline: Instant) -> io::Result<()> {
        if let Some(task) = self.task.as_ref() {
            while !task.is_finished() {
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "child pipe did not reach EOF",
                    ));
                }
                thread::sleep(POLL);
            }
        }
        if let Some(task) = self.task.take() {
            task.join().map_err(|_| io::Error::other("child pipe reader panicked"))??;
        }
        Ok(())
    }

    fn bytes(&self) -> Vec<u8> {
        self.data.lock().map(|data| data.tail.iter().copied().collect()).unwrap_or_default()
    }
}

pub struct ManagedChild {
    child: Option<Child>,
    stdout: Option<Reader>,
    stderr: Option<Reader>,
    pid: u32,
    status: Option<ExitStatus>,
    pub forced: bool,
    stdin_bytes: usize,
}

impl ManagedChild {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)]
        let child = Child::spawn(command)?;
        #[cfg(not(windows))]
        let child = command.spawn()?;
        let mut owned = Self {
            pid: child.id(),
            child: Some(child),
            stdout: None,
            stderr: None,
            status: None,
            forced: false,
            stdin_bytes: 0,
        };
        let child = owned.child.as_mut().expect("just assigned");
        if let Some(pipe) = child.stdout.take() {
            owned.stdout = Some(Reader::start(pipe, true)?);
        }
        if let Some(pipe) = child.stderr.take() {
            owned.stderr = Some(Reader::start(pipe, false)?);
        }
        Ok(owned)
    }

    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn released(&self) -> bool {
        self.child.is_none()
    }

    pub fn send(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Total writes stay below the pipe capacity even if the child never reads.
        if self.stdin_bytes + bytes.len() > 2048 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "child control input exceeds 2 KiB",
            ));
        }
        self.stdin_bytes += bytes.len();
        let stdin =
            self.child.as_mut().and_then(|child| child.stdin.as_mut()).ok_or_else(|| {
                io::Error::new(io::ErrorKind::BrokenPipe, "child stdin is closed")
            })?;
        stdin.write_all(bytes)?;
        stdin.flush()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if self.status.is_none() {
            if let Some(child) = self.child.as_mut() {
                self.status = child.try_wait()?;
            }
        }
        Ok(self.status)
    }

    pub fn lines(&mut self) -> Vec<ReceivedLine> {
        self.stdout
            .as_ref()
            .and_then(|reader| reader.data.lock().ok())
            .map(|mut data| data.lines.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn stderr(&self) -> String {
        self.stderr
            .as_ref()
            .map(|reader| String::from_utf8_lossy(&reader.bytes()).into_owned())
            .unwrap_or_default()
    }

    fn wait_until(&mut self, deadline: Instant) -> io::Result<bool> {
        loop {
            let root_exited = self.try_wait()?.is_some();
            #[cfg(windows)]
            let tree_empty = self.child.as_ref().map_or(Ok(true), Child::tree_empty)?;
            #[cfg(not(windows))]
            let tree_empty = root_exited;
            if root_exited && tree_empty {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            // Deadline-based polling of actual exit/job state, never a cleanup delay.
            thread::sleep(POLL);
        }
    }

    fn release(&mut self) -> io::Result<()> {
        let deadline = Instant::now() + FORCE_TIMEOUT;
        if let Some(reader) = &mut self.stdout {
            reader.join(deadline)?;
        }
        if let Some(reader) = &mut self.stderr {
            reader.join(deadline)?;
        }
        // Root reaped, entire Windows job empty, readers joined: now close process/job handles.
        self.child = None;
        Ok(())
    }

    pub fn shutdown(&mut self, request: &[u8], grace: Duration) -> io::Result<()> {
        if self.released() {
            return Ok(());
        }
        if self.try_wait()?.is_none() && !request.is_empty() {
            // Small one-shot q/stop request. A closed pipe means the child may already be exiting.
            if let Err(error) = self.send(request) {
                if error.kind() != io::ErrorKind::BrokenPipe {
                    eprintln!("child {} quit request: {error}", self.pid);
                }
            }
        }
        if let Some(child) = &mut self.child {
            child.stdin.take();
        }
        if !self.wait_until(Instant::now() + grace)? {
            self.forced = true;
            if let Err(error) = self.child.as_mut().expect("owned until release").kill() {
                // The root can finish between the deadline check and termination.
                if self.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            if !self.wait_until(Instant::now() + FORCE_TIMEOUT)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("owned process/job {} did not exit after termination", self.pid),
                ));
            }
        }
        self.release()
    }

    pub fn output(
        command: &mut Command,
        timeout: Duration,
        cancel: Option<&AtomicBool>,
    ) -> io::Result<Output> {
        let mut process = Self::spawn(command)?;
        if let Some(child) = &mut process.child {
            child.stdin.take();
        }
        let deadline = Instant::now() + timeout;
        while process.try_wait()?.is_none() {
            if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) || Instant::now() >= deadline
            {
                process.shutdown(&[], Duration::ZERO)?;
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "child operation cancelled or timed out",
                ));
            }
            thread::sleep(POLL);
        }
        process.shutdown(&[], Duration::ZERO)?;
        Ok(Output {
            status: process.status.expect("waited"),
            stdout: process.stdout.as_ref().map(Reader::bytes).unwrap_or_default(),
            stderr: process.stderr.as_ref().map(Reader::bytes).unwrap_or_default(),
        })
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown(&[], Duration::ZERO) {
            eprintln!("owned process {} drop cleanup: {error}", self.pid);
        }
        // On Windows the non-inherited kill-on-close job is also a crash/abort safeguard.
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn helper(name: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", &format!("process::tests::{name}"), "--ignored", "--nocapture"]);
        command
    }

    fn is_helper(name: &str) -> bool {
        std::env::args().any(|arg| arg == format!("process::tests::{name}"))
    }

    #[test]
    #[ignore = "subprocess fixture; invoked by ownership tests"]
    fn child_quit() {
        if !is_helper("child_quit") {
            return;
        }
        println!("\nframe=1");
        io::stdout().flush().unwrap();
        let mut line = String::new();
        io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "q");
        eprintln!("graceful acknowledgement");
    }

    #[test]
    #[ignore = "subprocess fixture; invoked by timeout tests"]
    fn child_hung() {
        if !is_helper("child_hung") {
            return;
        }
        println!("\nframe=1");
        io::stdout().flush().unwrap();
        loop {
            thread::park_timeout(Duration::from_secs(60));
        }
    }

    #[test]
    #[ignore = "subprocess fixture; invoked by initialization tests"]
    fn child_initializing() {
        if !is_helper("child_initializing") {
            return;
        }
        let mut line = String::new();
        io::stdin().read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "q");
    }

    #[test]
    fn graceful_exit_reaps_child_joins_readers_and_is_idempotent() {
        let mut child = ManagedChild::spawn(&mut helper("child_quit")).unwrap();
        assert!(child.try_wait().unwrap().is_none());
        child.shutdown(b"q\n", Duration::from_secs(3)).unwrap();
        assert!(child.try_wait().unwrap().unwrap().success());
        assert!(child.released());
        assert!(!child.forced);
        assert!(child.stderr().contains("graceful acknowledgement"));
        assert!(child.stdout.as_ref().unwrap().task.is_none());
        assert!(child.stderr.as_ref().unwrap().task.is_none());
        child.shutdown(b"q\n", Duration::ZERO).unwrap();
    }

    #[test]
    fn timeout_forces_only_owned_process_and_joins_pipes() {
        let mut child = ManagedChild::spawn(&mut helper("child_hung")).unwrap();
        let mut unrelated = ManagedChild::spawn(&mut helper("child_hung")).unwrap();
        child.shutdown(b"q\n", Duration::from_millis(40)).unwrap();
        assert!(child.forced && child.released());
        assert!(child.try_wait().unwrap().is_some());
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.shutdown(&[], Duration::ZERO).unwrap();
    }

    #[test]
    fn cancelled_output_is_reaped() {
        let error = ManagedChild::output(
            &mut helper("child_hung"),
            Duration::from_secs(30),
            Some(&AtomicBool::new(true)),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    #[ignore = "requires installed ffmpeg; run with the real_ffmpeg integration filter"]
    fn real_ffmpeg_forced_fallback_reaps_unresponsive_stdin() {
        let mut command = Command::new("ffmpeg");
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-re",
            "-f",
            "lavfi",
            "-i",
            "color=s=16x16:r=10",
            "-f",
            "null",
            "-",
        ]);
        let mut child = ManagedChild::spawn(&mut command).unwrap();
        child.shutdown(b"q\n", Duration::from_millis(100)).unwrap();
        assert!(child.forced && child.released());
        assert!(child.try_wait().unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "subprocess fixture; invoked by job tests"]
    fn child_tree() {
        if !is_helper("child_tree") {
            return;
        }
        let mut child = helper("child_hung").spawn().unwrap();
        println!("\ndescendant={}", child.id());
        io::stdout().flush().unwrap();
        let _ = child.wait();
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_terminates_descendants() {
        let mut child = ManagedChild::spawn(&mut helper("child_tree")).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Some(pid) = child.lines().iter().find_map(|line| {
                line.text.strip_prefix("descendant=").and_then(|pid| pid.parse::<u32>().ok())
            }) {
                break pid;
            }
            assert!(Instant::now() < deadline, "descendant never started");
            thread::sleep(POLL);
        };
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::*;
        // SAFETY: the PID was reported by our test fixture; retain a handle against PID reuse.
        unsafe {
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            assert!(!handle.is_null());
            child.shutdown(&[], Duration::ZERO).unwrap();
            assert_eq!(WaitForSingleObject(handle, 3000), WAIT_OBJECT_0);
            CloseHandle(handle);
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "subprocess fixture; invoked by owner crash test"]
    fn child_owner() {
        if !is_helper("child_owner") {
            return;
        }
        let child = ManagedChild::spawn(&mut helper("child_hung")).unwrap();
        println!("\nowned={}", child.id());
        io::stdout().flush().unwrap();
        loop {
            thread::park_timeout(Duration::from_secs(60));
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_owner_crash_closes_job_and_terminates_child() {
        use std::io::{BufRead, BufReader};
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::*;
        // Deliberately launch this fixture outside a job to exercise its own job's
        // kill-on-close behavior, rather than this test's TerminateJobObject path.
        let mut owner = helper("child_owner").stdout(Stdio::piped()).spawn().unwrap();
        let (send, recv) = std::sync::mpsc::channel();
        let pipe = owner.stdout.take().unwrap();
        let reader = thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if let Some(pid) =
                    line.strip_prefix("owned=").and_then(|pid| pid.parse::<u32>().ok())
                {
                    let _ = send.send(pid);
                }
            }
        });
        let reported = recv.recv_timeout(Duration::from_secs(5));
        // Always terminate our fixture, including on setup failure.
        let handle =
            reported.as_ref().ok().map(|pid| unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, *pid) });
        owner.kill().unwrap();
        owner.wait().unwrap();
        reader.join().unwrap();
        reported.unwrap();
        let handle = handle.unwrap();
        assert!(!handle.is_null());
        // SAFETY: retain the original child's handle so PID reuse cannot pass this test.
        unsafe {
            assert_eq!(WaitForSingleObject(handle, 3000), WAIT_OBJECT_0);
            CloseHandle(handle);
        }
    }
}
