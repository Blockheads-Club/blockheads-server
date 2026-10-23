//! `NSFileHandle fileHandleWithStandardInput` + `readInBackgroundAndNotify` emulation.
//!
//! notification observer).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};

use crate::foundation::exception::catch_ns;
use crate::nslog;

/// Size of one read: 0xA000 bytes (observed, app runtime 5; 's 4096 is wrong).
pub const READ_CHUNK: usize = 40960;

/// Observer type: receives the bytes of one `NSFileHandleNotificationDataItem` (empty slice at EOF/error).
pub type ReadObserver = Rc<dyn Fn(&[u8])>;

/// Where the bytes come from.
enum Source {
    /// A raw descriptor (fd 0 on Unix), waited on with `poll(2)`.
    #[cfg(unix)]
    Fd(i32),
    /// A channel of chunks (an empty chunk, or a disconnect, is EOF). Used for the console on Windows and for
    /// embedded hosts that inject commands.
    Channel(Receiver<Vec<u8>>),
}

/// `NSFileHandle` on standard input (or, for hosts and tests, any other source).
pub struct StdinHandle {
    source: Source,
    /// A chunk taken from a channel while waiting; delivered by `service_readable`.
    pending: RefCell<Option<Vec<u8>>>,
    armed: Cell<bool>,
    eof: Cell<bool>,
    observer: RefCell<Option<ReadObserver>>,
}

impl StdinHandle {
    fn new(source: Source) -> Rc<StdinHandle> {
        Rc::new(StdinHandle {
            source,
            pending: RefCell::new(None),
            armed: Cell::new(false),
            eof: Cell::new(false),
            observer: RefCell::new(None),
        })
    }

    /// `[NSFileHandle fileHandleWithStandardInput]` (fd 0 on Unix, a reader thread on other platforms).
    pub fn file_handle_with_standard_input() -> Rc<StdinHandle> {
        #[cfg(unix)]
        {
            Self::with_fd(0)
        }
        #[cfg(not(unix))]
        {
            let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut stdin = std::io::stdin().lock();
                let mut buf = vec![0u8; READ_CHUNK];
                loop {
                    match stdin.read(&mut buf) {
                        Ok(n) if n > 0 => {
                            if tx.send(buf[..n].to_vec()).is_err() {
                                return;
                            }
                        }
                        _ => {
                            let _ = tx.send(Vec::new());
                            return;
                        }
                    }
                }
            });
            Self::from_channel(rx)
        }
    }

    /// Same on an arbitrary descriptor (unit tests use pipes). Not an original selector.
    #[cfg(unix)]
    pub fn with_fd(fd: i32) -> Rc<StdinHandle> {
        Self::new(Source::Fd(fd))
    }

    /// A handle fed by a channel of chunks (hosts inject console commands this way). Not an original selector.
    pub fn from_channel(rx: Receiver<Vec<u8>>) -> Rc<StdinHandle> {
        Self::new(Source::Channel(rx))
    }

    /// `addObserver:selector:name:NSFileHandleReadCompletionNotification object:`. Exactly one observer.
    pub fn add_read_completion_observer(&self, f: ReadObserver) {
        *self.observer.borrow_mut() = Some(f);
    }

    /// `-readInBackgroundAndNotify`: only arms one read; nothing is read now.
    pub fn read_in_background_and_notify(&self) {
        self.armed.set(true);
    }

    fn listening(&self) -> bool {
        self.armed.get() && !self.eof.get()
    }

    /// Descriptor the run loop must poll for `POLLIN`, `None` when disarmed, after EOF or for channel sources
    /// (EOF decision,).
    #[cfg(unix)]
    pub fn poll_fd(&self) -> Option<i32> {
        match &self.source {
            Source::Fd(fd) if self.listening() => Some(*fd),
            _ => None,
        }
    }

    /// True once EOF / a hard read error was delivered.
    pub fn at_eof(&self) -> bool {
        self.eof.get()
    }

    /// Run-loop phase 2 wait: block up to `timeout_ms` until input is ready. A disarmed handle just sleeps
    /// (like `poll` with fd -1). Returns true when `service_readable` has something to deliver.
    pub fn wait_readable(&self, timeout_ms: i32) -> bool {
        let timeout_ms = timeout_ms.max(0);
        if !self.listening() {
            std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
            return false;
        }
        match &self.source {
            #[cfg(unix)]
            Source::Fd(fd) => {
                let mut pfd = [libc::pollfd { fd: *fd, events: libc::POLLIN, revents: 0 }];
                // SAFETY: one valid pollfd.
                let r = unsafe { libc::poll(pfd.as_mut_ptr(), 1, timeout_ms) };
                r > 0 && pfd[0].revents != 0
            }
            Source::Channel(rx) => {
                if self.pending.borrow().is_some() {
                    return true;
                }
                match rx.recv_timeout(std::time::Duration::from_millis(timeout_ms as u64)) {
                    Ok(v) => {
                        *self.pending.borrow_mut() = Some(v);
                        true
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => {
                        *self.pending.borrow_mut() = Some(Vec::new());
                        true
                    }
                }
            }
        }
    }

    /// Called by the run loop when `wait_readable` reported readiness: ONE `read`, ONE notification
    /// The observer runs after the read, with no borrow held.
    pub fn service_readable(&self) {
        if !self.listening() {
            return;
        }
        let data: Vec<u8> = match &self.source {
            #[cfg(unix)]
            Source::Fd(fd) => {
                let mut buf = vec![0u8; READ_CHUNK];
                // SAFETY: buf is valid for READ_CHUNK bytes.
                let n = unsafe { libc::read(*fd, buf.as_mut_ptr() as *mut libc::c_void, READ_CHUNK) };
                if n < 0 {
                    let e = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                    if e == libc::EINTR || e == libc::EAGAIN {
                        return; // try again next iteration, still armed
                    }
                }
                buf.truncate(n.max(0) as usize);
                buf
            }
            Source::Channel(rx) => match self.pending.borrow_mut().take() {
                Some(v) => v,
                None => match rx.try_recv() {
                    Ok(v) => v,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
                },
            },
        };
        self.armed.set(false);
        if !data.is_empty() {
            self.notify(&data);
        } else {
            // EOF or hard error: exactly one empty notification, then never polled again.
            self.eof.set(true);
            self.notify(&[]);
        }
    }

    /// Observer dispatch with GNUstep's `NSNotificationCenter` exception catch.
    fn notify(&self, data: &[u8]) {
        let obs = self.observer.borrow().clone();
        if let Some(o) = obs {
            if let Err(ex) = catch_ns(|| o(data)) {
                // The pointer value is not reproducible; the diff harness masks it.
                nslog!(
                    "Problem posting notification: <NSException: 0x{:x}> NAME:{} REASON:{} INFO:(null)",
                    0x5555_0000_0000u64,
                    ex.name,
                    ex.reason_or_null()
                );
            }
        }
    }
}

#[cfg(test)]
mod chan_tests {
    use super::*;

    #[test]
    fn channel_source_delivers_chunks_then_eof() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let h = StdinHandle::from_channel(rx);
        let got: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
        let (g2, h2) = (got.clone(), h.clone());
        h.add_read_completion_observer(Rc::new(move |d| {
            g2.borrow_mut().push(d.len());
            h2.read_in_background_and_notify();
        }));
        assert!(!h.wait_readable(1));
        h.read_in_background_and_notify();
        assert!(!h.wait_readable(1));
        tx.send(b"abc".to_vec()).unwrap();
        assert!(h.wait_readable(100));
        h.service_readable();
        drop(tx);
        assert!(h.wait_readable(100));
        h.service_readable();
        assert_eq!(*got.borrow(), vec![3, 0]);
        assert!(h.at_eof());
        assert!(!h.wait_readable(1));
    }

    #[test]
    fn channel_pending_empty_and_dropped_sender_paths() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let h = StdinHandle::from_channel(rx);
        let got: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
        let g2 = got.clone();
        h.add_read_completion_observer(Rc::new(move |d| g2.borrow_mut().push(d.len())));
        h.service_readable(); // disarmed: nothing happens
        h.read_in_background_and_notify();
        h.service_readable(); // armed but nothing queued: still waiting
        assert!(got.borrow().is_empty() && !h.at_eof());
        tx.send(b"ab".to_vec()).unwrap();
        assert!(h.wait_readable(100));
        assert!(h.wait_readable(100)); // the chunk is already pending
        h.service_readable();
        assert_eq!(*got.borrow(), vec![2]);
        // Input that arrived without a wait is taken straight from the channel.
        h.read_in_background_and_notify();
        tx.send(b"cde".to_vec()).unwrap();
        h.service_readable();
        assert_eq!(*got.borrow(), vec![2, 3]);
        got.borrow_mut().pop();
        // A sender that is gone without a pending chunk is end of input as well.
        h.read_in_background_and_notify();
        drop(tx);
        h.service_readable();
        assert_eq!(*got.borrow(), vec![2, 0]);
        assert!(h.at_eof());
        // And so is one that vanishes while the loop waits.
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let h = StdinHandle::from_channel(rx);
        h.add_read_completion_observer(Rc::new(|_| ()));
        h.read_in_background_and_notify();
        drop(tx);
        assert!(h.wait_readable(100));
        h.service_readable();
        assert!(h.at_eof());
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn pipe() -> (i32, i32) {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        (fds[0], fds[1])
    }
    fn write_all(fd: i32, data: &[u8]) {
        let mut off = 0;
        while off < data.len() {
            let n = unsafe { libc::write(fd, data[off..].as_ptr() as *const _, data.len() - off) };
            assert!(n > 0);
            off += n as usize;
        }
    }

    /// T7 (small write): one 10 000-byte write is one observer call.
    #[test]
    fn single_write_is_one_chunk_and_rearm_works() {
        let (r, w) = pipe();
        let h = StdinHandle::with_fd(r);
        let got: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
        let (g2, h2) = (got.clone(), h.clone());
        h.add_read_completion_observer(Rc::new(move |d| {
            g2.borrow_mut().push(d.len());
            h2.read_in_background_and_notify();
        }));
        assert!(h.poll_fd().is_none());
        h.read_in_background_and_notify();
        assert_eq!(h.poll_fd(), Some(r));
        write_all(w, &vec![b'a'; 10_000]);
        h.service_readable();
        write_all(w, b"xyz");
        h.service_readable();
        assert_eq!(*got.borrow(), vec![10_000, 3]);
        unsafe { libc::close(w) };
        h.service_readable();
        assert_eq!(*got.borrow(), vec![10_000, 3, 0]);
        assert!(h.at_eof());
        assert!(h.poll_fd().is_none());
        h.read_in_background_and_notify();
        assert!(h.poll_fd().is_none());
        unsafe { libc::close(r) };
    }

    /// T7 (large write): chunks of 40 960.
    #[test]
    fn large_write_is_chunked_at_40960() {
        let (r, w) = pipe();
        let h = StdinHandle::with_fd(r);
        let got: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
        let (g2, h2) = (got.clone(), h.clone());
        h.add_read_completion_observer(Rc::new(move |d| {
            g2.borrow_mut().push(d.len());
            h2.read_in_background_and_notify();
        }));
        h.read_in_background_and_notify();
        // The pipe holds 65 536 bytes: write 60 000 from a helper-free sequence.
        write_all(w, &vec![b'b'; 60_000]);
        h.service_readable();
        h.service_readable();
        assert_eq!(*got.borrow(), vec![40_960, 19_040]);
        unsafe {
            libc::close(w);
            libc::close(r);
        }
    }

    /// item 4: an NsException in the observer is caught and stdin is not re-armed.
    #[test]
    fn observer_exception_is_caught_and_not_rearmed() {
        let (r, w) = pipe();
        let h = StdinHandle::with_fd(r);
        h.add_read_completion_observer(Rc::new(|_| crate::foundation::exception::ns_raise("NSRangeException", "boom")));
        h.read_in_background_and_notify();
        write_all(w, b"x");
        h.service_readable();
        assert!(h.poll_fd().is_none());
        unsafe {
            libc::close(w);
            libc::close(r);
        }
    }

    /// `wait_readable` on a descriptor: false on timeout, true with input, and a disarmed handle just sleeps.
    #[test]
    fn descriptor_waits_and_read_errors() {
        let (r, w) = pipe();
        let h = StdinHandle::with_fd(r);
        let got: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
        let g2 = got.clone();
        h.add_read_completion_observer(Rc::new(move |d| g2.borrow_mut().push(d.len())));
        assert!(!h.wait_readable(-5)); // disarmed: sleeps for the (clamped) timeout
        h.service_readable();
        h.read_in_background_and_notify();
        assert!(!h.wait_readable(1));
        write_all(w, b"hi");
        assert!(h.wait_readable(1000));
        h.service_readable();
        assert_eq!(*got.borrow(), vec![2]);
        // An interrupted or would-block read is retried on the next iteration, still armed.
        h.read_in_background_and_notify();
        unsafe { libc::fcntl(r, libc::F_SETFL, libc::O_NONBLOCK) };
        h.service_readable();
        assert_eq!(h.poll_fd(), Some(r));
        // A hard error is delivered as end of input.
        unsafe {
            libc::close(w);
            libc::close(r);
        }
        h.service_readable();
        assert!(h.at_eof());
        assert_eq!(*got.borrow(), vec![2, 0]);
    }

    #[test]
    fn standard_input_handle_is_not_read_until_armed() {
        let h = StdinHandle::file_handle_with_standard_input();
        assert!(h.poll_fd().is_none() && !h.at_eof());
    }

    /// An observer without a registered closure is fine.
    #[test]
    fn no_observer_means_no_notification() {
        let (r, w) = pipe();
        let h = StdinHandle::with_fd(r);
        h.read_in_background_and_notify();
        write_all(w, b"x");
        h.service_readable();
        assert!(h.poll_fd().is_none());
        unsafe {
            libc::close(w);
            libc::close(r);
        }
    }
}
