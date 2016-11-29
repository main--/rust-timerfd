//! This crate implements the linux timerfd interface.
//!
//! # Example
//!
//! ```
//! use timerfd::{TimerFd, TimerState};
//! use std::time::Duration;
//!
//! let mut tfd = TimerFd::new().unwrap();
//! assert_eq!(tfd.get_state().unwrap(), TimerState::Disarmed);
//! tfd.set_state(TimerState::Oneshot(Duration::new(1, 0))).unwrap();
//! match tfd.get_state().unwrap() {
//!     TimerState::Oneshot(d) => println!("Remaining: {:?}", d),
//!     _ => unreachable!(),
//! }
//! tfd.read().unwrap();
//! assert_eq!(tfd.get_state().unwrap(), TimerState::Disarmed);
//! ```

extern crate libc;

use std::os::unix::prelude::*;
use std::time::Duration;
use std::io::Result as IoResult;

extern "C" {
    fn timerfd_create(clockid: libc::c_int, flags: libc::c_int) -> RawFd;
    fn timerfd_settime(fd: RawFd, flags: libc::c_int,
                       new_value: *const itimerspec, old_value: *mut itimerspec) -> libc::c_int;
    fn timerfd_gettime(fd: RawFd, curr_value: *mut itimerspec) -> libc::c_int;
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
    pub fn new_custom(realtime_clock: bool, nonblocking: bool, cloexec: bool) -> IoResult<TimerFd> {
        let clock = if realtime_clock { libc::CLOCK_REALTIME } else { libc::CLOCK_MONOTONIC };

        let mut flags = 0;
        if nonblocking {
            flags |= TFD_NONBLOCK;
        }
        if cloexec {
            flags |= TFD_CLOEXEC;
        }

        let fd = neg_is_err(unsafe { timerfd_create(clock, flags) })?;
        Ok(TimerFd(fd))
    }

    /// Creates a new `TimerFd` with default settings.
    ///
    /// Use `new_custom` to specify custom settings.
    pub fn new() -> IoResult<TimerFd> {
        TimerFd::new_custom(false, false, false)
    }

    /// Sets this timerfd to a given `TimerState` and returns the old state.
    pub fn set_state(&mut self, state: TimerState) -> IoResult<TimerState> {
        let mut old = itimerspec::null();
        let new: itimerspec = state.into();
        neg_is_err(unsafe { timerfd_settime(self.0, 0, &new, &mut old) })?;
        Ok(old.into())
    }

    /// Returns the current `TimerState`.
    pub fn get_state(&self) -> IoResult<TimerState> {
        let mut state = itimerspec::null();
        neg_is_err(unsafe { timerfd_gettime(self.0, &mut state) })?;
        Ok(state.into())
    }

    /// Read from this timerfd.
    ///
    /// Returns the number of timer expirations since the last read.
    /// If that number is zero, this function blocks until the timer expires
    /// (or returns an error if it's nonblocking).
    pub fn read(&mut self) -> IoResult<u64> {
        const BUFSIZE: usize = 8;
        
        let mut buffer: u64 = 0;
        let bufptr: *mut _ = &mut buffer;
        let res = unsafe { libc::read(self.0, bufptr as *mut libc::c_void, BUFSIZE) };
        neg_is_err(res as i32)?;
        assert!(res == BUFSIZE as isize);
        Ok(buffer)
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
