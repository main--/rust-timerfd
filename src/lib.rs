//! A rust interface to the Linux kernel's timerfd API.
//!
//! # Example
//!
//! ```
//! use timerfd::{TimerFd, TimerState};
//! use std::time::Duration;
//!
//! // Create a new timerfd
//! // (unwrap is actually fine here for most usecases)
//! let mut tfd = TimerFd::new().unwrap();
//!
//! // The timer is initially disarmed
//! assert_eq!(tfd.get_state(), TimerState::Disarmed);
//!
//! // Set the timer
//! tfd.set_state(TimerState::Oneshot(Duration::new(1, 0)));
//!
//! // Observe that the timer is now set
//! match tfd.get_state() {
//!     TimerState::Oneshot(d) => println!("Remaining: {:?}", d),
//!     _ => unreachable!(),
//! }
//!
//! // Wait for the remaining time
//! tfd.read();
//!
//! // It was a oneshot timer, so it's now disarmed
//! assert_eq!(tfd.get_state(), TimerState::Disarmed);
//! ```
//!
//! # Usage
//!
//! Unfortunately, this example can't show why you would use
//! timerfd in the first place: Because it creates a file descriptor
//! that you can monitor with `select(2)`, `poll(2)` and `epoll(2)`.
//!
//! In other words, the only advantage this offers over any other
//! timer implementation is that it implements the `AsRawFd` trait.
//!
//! The file descriptor becomes ready/readable whenever the timer expires.


extern crate libc;

use std::os::unix::prelude::*;
use std::time::Duration;
use std::io::Result as IoResult;
use std::io::ErrorKind;
use std::fmt;

extern "C" {
    fn timerfd_create(clockid: libc::c_int, flags: libc::c_int) -> RawFd;
    fn timerfd_settime(fd: RawFd, flags: libc::c_int,
                       new_value: *const itimerspec, old_value: *mut itimerspec) -> libc::c_int;
    fn timerfd_gettime(fd: RawFd, curr_value: *mut itimerspec) -> libc::c_int;
}

#[derive(Clone, PartialEq, Eq)]
pub enum ClockId {
    /// Available clocks:
    ///
    /// A settable system-wide real-time clock.
    Realtime       = libc::CLOCK_REALTIME       as isize,

    /// This clock is like CLOCK_REALTIME, but will wake the system if it is suspended. The
    /// caller must have the CAP_WAKE_ALARM capability in order to set a timer against this
    /// clock.
    RealtimeAlarm  = libc::CLOCK_REALTIME_ALARM as isize,

    /// A nonsettable monotonically increasing clock that measures time from some unspecified
    /// point in the past that does not change after system startup.
    Monotonic      = libc::CLOCK_MONOTONIC      as isize,

    /// Like CLOCK_MONOTONIC, this is a monotonically increasing clock. However, whereas the
    /// CLOCK_MONOTONIC clock does not measure the time while a system is suspended, the
    /// CLOCK_BOOTTIME clock does include the time during which the system is suspended. This
    /// is useful for applications that need to be suspend-aware. CLOCK_REALTIME is not
    /// suitable for such applications, since that clock is affected by discon‐ tinuous
    /// changes to the system clock.
    Boottime       = libc::CLOCK_BOOTTIME       as isize,

    /// This clock is like CLOCK_BOOTTIME, but will wake the system if it is suspended. The
    /// caller must have the CAP_WAKE_ALARM capability in order to set a timer against this
    /// clock.
    BoottimeAlarm  = libc::CLOCK_BOOTTIME_ALARM as isize,
}

fn clock_name (clock: &ClockId) -> &'static str {
    match *clock {
        ClockId::Realtime       => "CLOCK_REALTIME",
        ClockId::RealtimeAlarm  => "CLOCK_REALTIME_ALARM",
        ClockId::Monotonic      => "CLOCK_MONOTONIC",
        ClockId::Boottime       => "CLOCK_BOOTTIME",
        ClockId::BoottimeAlarm  => "CLOCK_BOOTTIME_ALARM",
    }
}

impl fmt::Display for ClockId {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", clock_name(self))
    }
}

impl fmt::Debug for ClockId {
    fn fmt (&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ({})", self.clone() as libc::c_int, clock_name(self))
    }
}

static TFD_CLOEXEC: libc::c_int = 0o2000000;
static TFD_NONBLOCK: libc::c_int = 0o0004000;

mod structs;
use structs::itimerspec;

/// Holds the state of a `TimerFd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerState {
    /// The timer is disarmed and will not fire.
    Disarmed,

    /// The timer will fire once after the specified duration
    /// and then disarm.
    Oneshot(Duration),

    /// The timer will fire once after `current` and then
    /// automatically rearm with `interval` as its duration.
    Periodic {
        current: Duration,
        interval: Duration,
    }
}

/// Represents a timerfd.
///
/// See also [`timerfd_create(2)`].
///
/// [`timerfd_create(2)`]: http://man7.org/linux/man-pages/man2/timerfd_create.2.html
pub struct TimerFd(RawFd);

fn neg_is_err(i: libc::c_int) -> IoResult<libc::c_int> {
    if i >= 0 {
        Ok(i)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

impl TimerFd {
    /// Creates a new `TimerFd`.
    ///
    /// By default, it uses the monotonic clock, is blocking and does not close on exec.
    /// The parameters allow you to change that.
    ///
    /// # Errors
    ///
    /// On Linux 2.6.26 and earlier, nonblocking and cloexec are not supported and setting them
    /// will return an error of kind `ErrorKind::InvalidInput`.
    ///
    /// This can also fail in various cases of resource exhaustion. Please check
    /// `timerfd_create(2)` for details.
    pub fn new_custom(clock: ClockId, nonblocking: bool, cloexec: bool) -> IoResult<TimerFd> {

        let mut flags = 0;
        if nonblocking {
            flags |= TFD_NONBLOCK;
        }
        if cloexec {
            flags |= TFD_CLOEXEC;
        }

        let fd = neg_is_err(unsafe { timerfd_create(clock as libc::c_int, flags) })?;
        Ok(TimerFd(fd))
    }

    /// Creates a new `TimerFd` with default settings.
    ///
    /// Use `new_custom` to specify custom settings.
    pub fn new() -> IoResult<TimerFd> {
        TimerFd::new_custom(ClockId::Monotonic, false, false)
    }

    /// Sets this timerfd to a given `TimerState` and returns the old state.
    pub fn set_state(&mut self, state: TimerState) -> TimerState {
        let mut old = itimerspec::null();
        let new: itimerspec = state.into();
        neg_is_err(unsafe { timerfd_settime(self.0, 0, &new, &mut old) })
            .expect("Looks like timerfd_settime failed in some undocumented way");
        old.into()
    }

    /// Returns the current `TimerState`.
    pub fn get_state(&self) -> TimerState {
        let mut state = itimerspec::null();
        neg_is_err(unsafe { timerfd_gettime(self.0, &mut state) })
            .expect("Looks like timerfd_gettime failed in some undocumented way");
        state.into()
    }

    /// Read from this timerfd.
    ///
    /// Returns the number of timer expirations since the last read.
    /// If this timerfd is operating in blocking mode (the default), it will
    /// not return zero but instead block until the timer has expired at least once.
    pub fn read(&mut self) -> u64 {
        const BUFSIZE: usize = 8;
        
        let mut buffer: u64 = 0;
        let bufptr: *mut _ = &mut buffer;
        loop {
            let res = unsafe { libc::read(self.0, bufptr as *mut libc::c_void, BUFSIZE) };
            match res {
                8 => {
                    assert!(buffer != 0);
                    return buffer;
                }
                -1 => {
                    let err = std::io::Error::last_os_error();
                    match err.kind() {
                        ErrorKind::WouldBlock => return 0,
                        ErrorKind::Interrupted => (),
                        _ => panic!("Unexpected read error: {}", err),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

impl AsRawFd for TimerFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for TimerFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}
