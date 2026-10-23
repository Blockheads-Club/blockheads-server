//! `NSRunLoop` emulation: timers, `performSelector:afterDelay:`, the stdin source, the main-thread queue and the
//! TryCatch boundary re-export.
//!

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;

use crate::app::stdin::StdinHandle;
use crate::foundation::clock;
use crate::foundation::exception::catch_ns;

pub use crate::net::bhmatch::{TimerId, TimerScheduler};

/// `+[TryCatchHandler performSelector:target:]`; foundation already owns it (CRITIC note,).
pub use crate::foundation::exception::try_catch_handler_perform as try_catch_perform;

/// Callback of one timer. Repeating callbacks live behind an `Rc<RefCell>` so they can be called with no borrow
/// of the timer table held.
enum TimerCb {
    Repeating(Rc<RefCell<Box<dyn FnMut()>>>),
    Once(Option<Box<dyn FnOnce()>>),
}

struct Timer {
    id: TimerId,
    fire_date: f64,
    interval: f64,
    repeats: bool,
    valid: bool,
    cb: TimerCb,
}

type QueueItem = Box<dyn FnOnce() + Send>;

struct State {
    timers: Vec<Timer>,
    next_id: TimerId,
    stdin: Option<Rc<StdinHandle>>,
}

/// `[NSRunLoop currentRunLoop]`.
pub struct RunLoop {
    state: RefCell<State>,
    tx: mpsc::Sender<QueueItem>,
    rx: RefCell<mpsc::Receiver<QueueItem>>,
    /// Set once `main_thread_sender` was handed out: the poll then wakes every 10 ms to look at the queue
    /// (the public API returns a plain `mpsc::Sender`, which cannot signal a poll fd).
    queue_used: Cell<bool>,
}

thread_local! {
    static CURRENT: Rc<RunLoop> = {
        let (tx, rx) = mpsc::channel();
        Rc::new(RunLoop {
            state: RefCell::new(State { timers: Vec::new(), next_id: 1, stdin: None }),
            tx,
            rx: RefCell::new(rx),
            queue_used: Cell::new(false),
        })
    };
}

/// `performSelector:withObject:afterDelay:` on the current run loop.
pub fn perform_after_delay(delay: f64, cb: Box<dyn FnOnce()>) -> TimerId {
    RunLoop::current().perform_after_delay(delay, cb)
}

/// `-[NSTimer invalidate]` on the current run loop.
pub fn invalidate(id: TimerId) {
    RunLoop::current().invalidate(id)
}

/// Uncaught `NsException` in a non-boundary callback (no catch site).
/// replace its inferred text once observed; has the observed behaviour on the reference:
/// `<argv0>: Uncaught exception <name>, reason: <reason>\n` without NSLog prefix, exit status 1.
fn uncaught_guard(f: impl FnOnce()) {
    if let Err(ex) = catch_ns(f) {
        let argv0 = std::env::args_os().next().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
        crate::foundation::exception::uncaught_exception_exit(&ex, &argv0);
    }
}

impl RunLoop {
    /// `[NSRunLoop currentRunLoop]`: thread-local singleton.
    pub fn current() -> Rc<RunLoop> {
        CURRENT.with(|c| c.clone())
    }

    /// Insert keeping the array sorted by `fire_date`, after existing equal dates.
    fn insert(&self, t: Timer) {
        let mut st = self.state.borrow_mut();
        let pos = st.timers.partition_point(|x| x.fire_date <= t.fire_date);
        st.timers.insert(pos, t);
    }

    fn alloc_id(&self) -> TimerId {
        let mut st = self.state.borrow_mut();
        let id = st.next_id;
        st.next_id += 1;
        id
    }

    /// `initWithFireDate:interval:target:selector:userInfo:repeats:` + `addTimer:forMode:`.
    pub fn add_timer(&self, fire_date: f64, interval: f64, repeats: bool, cb: Box<dyn FnMut()>) -> TimerId {
        let id = self.alloc_id();
        self.insert(Timer {
            id,
            fire_date,
            interval,
            repeats,
            valid: true,
            cb: TimerCb::Repeating(Rc::new(RefCell::new(cb))),
        });
        id
    }

    /// `+scheduledTimerWithTimeInterval:...`: fire date = now + interval.
    pub fn scheduled_timer(&self, interval: f64, repeats: bool, cb: Box<dyn FnMut()>) -> TimerId {
        self.add_timer(clock::reference_time() + interval, interval, repeats, cb)
    }

    /// `performSelector:withObject:afterDelay:`: a non-repeating timer; the closure owns its target.
    /// Callback exceptions are uncaught.
    pub fn perform_after_delay(&self, delay: f64, cb: Box<dyn FnOnce()>) -> TimerId {
        let id = self.alloc_id();
        self.insert(Timer {
            id,
            fire_date: clock::reference_time() + delay,
            interval: 0.0,
            repeats: false,
            valid: true,
            cb: TimerCb::Once(Some(cb)),
        });
        id
    }

    /// `-[NSTimer invalidate]`: idempotent; safe from inside any callback.
    pub fn invalidate(&self, id: TimerId) {
        let mut st = self.state.borrow_mut();
        if let Some(t) = st.timers.iter_mut().find(|t| t.id == id) {
            t.valid = false;
            // A pending one-shot closure is dropped now (releases its target).
            if let TimerCb::Once(c) = &mut t.cb {
                let dropped = c.take();
                drop(st);
                drop(dropped);
            }
        }
    }

    /// Register the process stdin source.
    pub fn set_stdin_source(&self, h: Rc<StdinHandle>) {
        self.state.borrow_mut().stdin = Some(h);
    }

    /// Sender for `performSelectorOnMainThread:...waitUntilDone:NO`; drained after stdin each iteration
    pub fn main_thread_sender(&self) -> mpsc::Sender<Box<dyn FnOnce() + Send>> {
        self.queue_used.set(true);
        self.tx.clone()
    }

    /// Fire one timer per the late-timer rule. Table borrow is released before the callback.
    fn fire(&self, id: TimerId, now: f64) {
        enum Call {
            Rep(Rc<RefCell<Box<dyn FnMut()>>>),
            Once(Option<Box<dyn FnOnce()>>),
        }
        let call = {
            let mut st = self.state.borrow_mut();
            let Some(t) = st.timers.iter_mut().find(|t| t.id == id) else { return };
            if !t.valid || t.fire_date > now {
                return;
            }
            if t.repeats {
                let mut nxt = t.fire_date + t.interval;
                if nxt <= now {
                    let add = t.interval * ((now - nxt) / t.interval).floor();
                    nxt += add;
                    if nxt <= now {
                        nxt += t.interval;
                    }
                }
                t.fire_date = nxt; // BEFORE the callback
                match &t.cb {
                    TimerCb::Repeating(c) => Call::Rep(c.clone()),
                    TimerCb::Once(_) => return,
                }
            } else {
                t.valid = false;
                match &mut t.cb {
                    TimerCb::Once(c) => Call::Once(c.take()),
                    // Non-repeating timer created through `add_timer`.
                    TimerCb::Repeating(c) => Call::Rep(c.clone()),
                }
            }
        };
        match call {
            Call::Rep(c) => (c.borrow_mut())(),
            Call::Once(Some(f)) => uncaught_guard(f),
            Call::Once(None) => {}
        }
    }

    /// One iteration of `runMode:NSDefaultRunLoopMode beforeDate:distantFuture`.
    pub fn run_mode_default_before_distant_future(&self) -> bool {
        self.drain_server_delays();
        // phase 1: -limitDateForMode: fires timers, array order, fresh clock read before each.
        let ids: Vec<TimerId> = self.state.borrow().timers.iter().map(|t| t.id).collect();
        for id in ids {
            let now = clock::reference_time();
            self.fire(id, now);
        }
        let limit = {
            let mut st = self.state.borrow_mut();
            st.timers.retain(|t| t.valid);
            st.timers.iter().map(|t| t.fire_date).fold(None, |m: Option<f64>, d| Some(m.map_or(d, |x| x.min(d))))
        };
        // phase 2: -acceptInputForMode:beforeDate:limit
        let wait = limit.map(|l| (l - clock::reference_time()).max(0.0));
        let virt = clock::is_virtual();
        let mut timeout_ms: i64 = match wait {
            Some(w) => trunc_timeout_ms(w),
            None => i32::MAX as i64,
        };
        if virt {
            timeout_ms = timeout_ms.min(1);
        } else if self.queue_used.get() {
            timeout_ms = timeout_ms.min(10);
        }
        let timeout_ms = timeout_ms.clamp(0, i32::MAX as i64) as i32;
        let stdin = self.state.borrow().stdin.clone();
        let ready = match stdin.as_ref() {
            Some(h) => h.wait_readable(timeout_ms),
            None => {
                std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
                false
            }
        };
        if virt {
            if let (Some(w), Some(l)) = (wait, limit) {
                clock::advance_virtual(w);
                nudge_virtual_clock_to(l);
            }
        }
        if ready {
            if let Some(h) = &stdin {
                h.service_readable();
            }
        }
        self.drain_queue();
        true
    }

    /// Move callbacks BHServer queued while no scheduler was installed into real timers.
    fn drain_server_delays(&self) {
        for (delay, cb) in crate::server::bhserver::take_pending_delays() {
            self.perform_after_delay(delay, cb);
        }
    }

    fn drain_queue(&self) {
        let mut items = Vec::new();
        {
            let rx = self.rx.borrow();
            while let Ok(i) = rx.try_recv() {
                items.push(i);
            }
        }
        for i in items {
            uncaught_guard(i);
        }
    }

    /// main's loop: iterate until `done()` is true (CommandLineDelegate `done`,). `done` is evaluated
    /// before EVERY iteration.
    pub fn run_until(&self, done: &dyn Fn() -> bool) {
        while !done() && self.run_mode_default_before_distant_future() {}
    }
}

impl TimerScheduler for RunLoop {
    fn schedule_repeating(&self, interval_secs: f64, callback: Box<dyn FnMut()>) -> TimerId {
        self.scheduled_timer(interval_secs, true, callback)
    }
    fn invalidate(&self, id: TimerId) {
        RunLoop::invalidate(self, id)
    }
}

/// f64 rounding of `start + elapsed` can leave the virtual clock an ulp short of a timer date, which would stall it
/// forever; nudge by sub-microsecond steps (unobservable).
fn nudge_virtual_clock_to(limit: f64) {
    let mut eps = 1e-9;
    while clock::reference_time() < limit && eps < 1e-3 {
        clock::advance_virtual(eps);
        eps *= 2.0;
    }
}

/// GNUstep `acceptInputForMode:beforeDate:` truncates the poll timeout (`(int)(wait*1000)`); the run loop then
/// re-polls with 0 until the timer date has passed. This keeps the idle-timer latency constant (r3_console_2).
fn trunc_timeout_ms(wait: f64) -> i64 {
    let ms = wait * 1000.0;
    if ms >= i32::MAX as f64 {
        i32::MAX as i64
    } else if ms <= 0.0 {
        0
    } else {
        ms as i64
    }
}

#[cfg(test)]
mod trunc_tests {
    use super::trunc_timeout_ms;
    #[test]
    fn truncates_not_ceils() {
        assert_eq!(trunc_timeout_ms(0.0014), 1);
        assert_eq!(trunc_timeout_ms(0.0009), 0);
        assert_eq!(trunc_timeout_ms(0.0), 0);
        assert_eq!(trunc_timeout_ms(1e12), i32::MAX as i64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    static CLOCK_LOCK: &Mutex<()> = &crate::foundation::clock::TEST_CLOCK_LOCK;
    const START: f64 = 1_000_000_000.0;

    fn setup() -> (std::sync::MutexGuard<'static, ()>, Rc<RunLoop>) {
        let g = CLOCK_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clock::set_virtual(Some(START));
        // Fresh loop per test: the test thread is new, so the thread-local is fresh.
        (g, RunLoop::current())
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// T3 (experiment A): late repeating timer fires once immediately after a stall, then keeps its grid.
    #[test]
    fn t3_late_timer_keeps_grid() {
        let (_g, rl) = setup();
        let t0 = clock::reference_time();
        let log: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(vec![]));
        let l2 = log.clone();
        rl.add_timer(
            t0 + 0.3,
            0.2,
            true,
            Box::new(move || {
                let n = l2.borrow().len();
                l2.borrow_mut().push(clock::reference_time() - t0);
                if n == 0 {
                    clock::advance_virtual(0.755);
                }
            }),
        );
        for _ in 0..8 {
            rl.run_mode_default_before_distant_future();
        }
        let l = log.borrow();
        assert!(near(l[0], 0.3), "{:?}", l);
        assert!(near(l[1], 1.055), "{:?}", l); // immediate extra fire
        assert!(near(l[2], 1.1), "{:?}", l); // first grid point > now
        assert!(near(l[3], 1.3), "{:?}", l);
        assert!(near(l[4], 1.5), "{:?}", l);
    }

    /// T4 (experiment B): array order within a pass, no catch-up burst.
    #[test]
    fn t4_poll_idle_stall_order() {
        let (_g, rl) = setup();
        let t0 = clock::reference_time();
        let log: Rc<RefCell<Vec<(&'static str, f64)>>> = Rc::new(RefCell::new(vec![]));
        let (la, lb) = (log.clone(), log.clone());
        let p = 0.016666666666666666;
        let stalled = Rc::new(Cell::new(false));
        let st = stalled.clone();
        rl.add_timer(
            t0 + p,
            p,
            true,
            Box::new(move || {
                la.borrow_mut().push(("poll", clock::reference_time() - t0));
                if !st.get() && clock::reference_time() - t0 > 0.5 {
                    st.set(true);
                    clock::advance_virtual(0.75);
                }
            }),
        );
        rl.add_timer(
            t0 + 0.2,
            0.2,
            true,
            Box::new(move || lb.borrow_mut().push(("idle", clock::reference_time() - t0))),
        );
        for _ in 0..200 {
            rl.run_mode_default_before_distant_future();
            if stalled.get() && log.borrow().len() > 0 {
                let n = log.borrow().iter().filter(|e| e.1 > 1.2).count();
                if n >= 4 {
                    break;
                }
            }
        }
        let l = log.borrow();
        let i = l.iter().position(|e| e.1 > 1.2 || (stalled.get() && e.1 > 0.5 && e.0 == "idle")).unwrap();
        // The stall happened in a poll; the next entry after it must be idle (array position after poll), then poll.
        let s = l.iter().rposition(|e| e.0 == "poll" && e.1 > 0.5 && e.1 < 0.6).unwrap();
        assert_eq!(l[s + 1].0, "idle", "{:?}", &l[s.saturating_sub(1)..(s + 5).min(l.len())]);
        assert_eq!(l[s + 2].0, "poll");
        assert!(l[s + 3].1 - l[s + 2].1 < 0.03, "no catch-up burst");
        let _ = i;
    }

    /// T5: one-shot fires once at now+delay; invalidate cancels; closure holding the only Rc still runs.
    #[test]
    fn t5_one_shot() {
        let (_g, rl) = setup();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        rl.perform_after_delay(2.0, Box::new(move || h.set(h.get() + 1)));
        let h = hits.clone();
        let id = rl.perform_after_delay(1.0, Box::new(move || h.set(h.get() + 100)));
        rl.invalidate(id);
        rl.invalidate(id);
        let owned = Rc::new(Cell::new(0));
        let o = owned.clone();
        rl.perform_after_delay(0.5, Box::new(move || o.set(7)));
        drop(owned);
        let t0 = clock::reference_time();
        // Heartbeat so the virtual clock keeps advancing once the one-shots are gone.
        rl.add_timer(t0 + 0.1, 0.1, true, Box::new(|| {}));
        while clock::reference_time() - t0 < 3.0 {
            rl.run_mode_default_before_distant_future();
        }
        assert_eq!(hits.get(), 1);
    }

    /// T6: equal fire dates fire in creation order.
    #[test]
    fn t6_ties_in_creation_order() {
        let (_g, rl) = setup();
        let t0 = clock::reference_time();
        let log: Rc<RefCell<Vec<u8>>> = Rc::new(RefCell::new(vec![]));
        for k in 0..3u8 {
            let l = log.clone();
            rl.add_timer(t0 + 0.1, 1.0, false, Box::new(move || l.borrow_mut().push(k)));
        }
        for _ in 0..3 {
            rl.run_mode_default_before_distant_future();
        }
        assert_eq!(*log.borrow(), vec![0, 1, 2]);
    }

    /// A callback may invalidate its own repeating timer.
    #[test]
    fn self_invalidate() {
        let (_g, rl) = setup();
        let t0 = clock::reference_time();
        let hits = Rc::new(Cell::new(0));
        let id: Rc<Cell<TimerId>> = Rc::new(Cell::new(0));
        let (h, i, r2) = (hits.clone(), id.clone(), rl.clone());
        id.set(rl.add_timer(
            t0 + 0.1,
            0.1,
            true,
            Box::new(move || {
                h.set(h.get() + 1);
                r2.invalidate(i.get());
            }),
        ));
        for _ in 0..6 {
            rl.run_mode_default_before_distant_future();
        }
        assert_eq!(hits.get(), 1);
    }

    /// The free helpers and the `TimerScheduler` face go through the current loop.
    #[test]
    fn free_functions_and_scheduler_face() {
        let (_g, rl) = setup();
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let id = perform_after_delay(0.0, Box::new(move || h.set(h.get() + 1)));
        invalidate(id);
        let h = hits.clone();
        perform_after_delay(0.0, Box::new(move || h.set(h.get() + 10)));
        let h = hits.clone();
        let st = rl.scheduled_timer(0.1, false, Box::new(move || h.set(h.get() + 100)));
        let sched: &dyn TimerScheduler = &*rl;
        let h = hits.clone();
        let rep = sched.schedule_repeating(0.1, Box::new(move || h.set(h.get() + 1000)));
        sched.invalidate(rep);
        let _ = st;
        for _ in 0..4 {
            rl.run_mode_default_before_distant_future();
        }
        assert_eq!(hits.get(), 110);
        rl.run_until(&|| true);
        rl.invalidate(424_242); // unknown ids are ignored
    }

    #[test]
    fn nudging_stops_at_the_limit_or_after_the_step_cap() {
        let (_g, _rl) = setup();
        let now = clock::reference_time();
        nudge_virtual_clock_to(now + 3e-6);
        assert!(clock::reference_time() >= now + 3e-6);
        let before = clock::reference_time();
        nudge_virtual_clock_to(before + 10.0);
        assert!(clock::reference_time() < before + 0.01); // gives up after the steps reach 1 ms
        nudge_virtual_clock_to(0.0);
    }

    /// `fire` guards that the iteration itself never trips.
    #[test]
    fn fire_ignores_what_cannot_fire() {
        let (_g, rl) = setup();
        let now = clock::reference_time();
        rl.fire(999, now); // unknown id
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        let id = rl.add_timer(now + 10.0, 1.0, true, Box::new(move || h.set(h.get() + 1)));
        rl.fire(id, now); // not due yet
        rl.invalidate(id);
        rl.fire(id, now + 20.0); // invalidated
        assert_eq!(hits.get(), 0);
        // A repeating entry that carries a one-shot closure, and a spent one-shot: hand-made, the API never builds them.
        for (repeats, cb) in [(true, TimerCb::Once(Some(Box::new(|| ())))), (false, TimerCb::Once(None))] {
            rl.insert(Timer { id: 500, fire_date: now, interval: 1.0, repeats, valid: true, cb });
            rl.fire(500, now + 1.0);
            rl.state.borrow_mut().timers.retain(|t| t.id != 500);
        }
        // Rescheduling after a long stall lands on the grid, strictly after now.
        let h = hits.clone();
        let id = rl.add_timer(now + 1.0, 1.0, true, Box::new(move || h.set(h.get() + 1)));
        for t in [now + 1.0, now + 3.5, now + 5.0] {
            rl.fire(id, t);
        }
        assert_eq!(hits.get(), 3);
        let next = rl.state.borrow().timers.iter().find(|t| t.id == id).unwrap().fire_date;
        assert!(near(next, now + 6.0), "{next}");
    }

    /// Stdin, the main-thread queue and the server's pending delays are serviced once per iteration.
    #[test]
    fn iteration_services_input_queue_and_server_delays() {
        let (_g, rl) = setup();
        let got = Rc::new(RefCell::new(Vec::<usize>::new()));
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let handle = StdinHandle::from_channel(rx);
        let g2 = got.clone();
        handle.add_read_completion_observer(Rc::new(move |d| g2.borrow_mut().push(d.len())));
        handle.read_in_background_and_notify();
        rl.set_stdin_source(handle);
        tx.send(b"abc".to_vec()).unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let r2 = ran.clone();
        rl.main_thread_sender().send(Box::new(move || r2.store(true, Ordering::SeqCst))).unwrap();
        let h = crate::server::bhserver::test_support::Harness::new("rl_delays");
        h.srv.set_delay_scheduler(None);
        let delayed = Rc::new(Cell::new(false));
        let d2 = delayed.clone();
        h.srv.perform_after_delay(0.0, Box::new(move || d2.set(true)));
        // No timers at all: the virtual clock has nothing to advance to, the queue is polled quickly.
        rl.run_mode_default_before_distant_future();
        rl.run_mode_default_before_distant_future();
        assert_eq!(*got.borrow(), vec![3]);
        assert!(ran.load(Ordering::SeqCst) && delayed.get());
        // Real clock, nothing to wait for: sleeps in short slices because the queue is in use.
        clock::set_virtual(None);
        rl.run_mode_default_before_distant_future();
        clock::set_virtual(Some(START));
        // No stdin source at all.
        let bare = std::thread::spawn(|| {
            let rl = RunLoop::current();
            rl.add_timer(clock::reference_time() + 0.001, 1.0, false, Box::new(|| ()));
            rl.run_mode_default_before_distant_future()
        });
        assert!(bare.join().unwrap());
    }

    /// An exception escaping a one-shot or a queued call ends the process like an uncaught one.
    #[test]
    fn escaping_exceptions_end_the_process() {
        for kind in ["delay", "queue"] {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "app::run_loop::tests::child", "--nocapture", "--test-threads=1"])
                .env("BH_TEST_CHILD", kind)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(1), "{kind}: {}", String::from_utf8_lossy(&out.stderr));
            assert!(String::from_utf8_lossy(&out.stderr).contains("Uncaught exception NSRangeException"), "{kind}");
        }
    }

    /// Does nothing in the test run; the check above starts it with `BH_TEST_CHILD` set.
    #[test]
    fn child() {
        let rl = RunLoop::current();
        match std::env::var("BH_TEST_CHILD").as_deref() {
            Ok("delay") => {
                rl.perform_after_delay(
                    0.0,
                    Box::new(|| crate::foundation::exception::ns_raise("NSRangeException", "r")),
                );
                rl.run_mode_default_before_distant_future();
            }
            Ok("queue") => {
                rl.main_thread_sender()
                    .send(Box::new(|| crate::foundation::exception::ns_raise("NSRangeException", "r")))
                    .unwrap();
                rl.run_mode_default_before_distant_future();
            }
            _ => {}
        }
    }
}
