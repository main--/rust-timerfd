use std::time::Duration;
use std::convert::TryInto;

use TimerState;
use libc;

// libc timespec is really awkward to work with (no traits etc)
// so we have our own
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct timespec {
    tv_sec: libc::time_t,
    tv_nsec: libc::c_long,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct itimerspec {
    it_interval: timespec,
    it_value: timespec,
}

impl itimerspec {
    pub fn null() -> itimerspec {
        itimerspec { it_interval: TS_NULL, it_value: TS_NULL }
    }
}

const TS_NULL: timespec = timespec { tv_sec: 0, tv_nsec: 0 };

impl From<Duration> for timespec {
    fn from(d: Duration) -> timespec {
        timespec {
            tv_sec: d.as_secs() as libc::time_t,
            tv_nsec: d.subsec_nanos().into(),
        }
    }
}

impl From<timespec> for Duration {
    fn from(ts: timespec) -> Duration {
        Duration::new(ts.tv_sec as u64, ts.tv_nsec.try_into()
            .expect("timespec overflow when converting to Duration"))
    }
}


impl From<TimerState> for itimerspec {
    fn from(ts: TimerState) -> itimerspec {
        match ts {
            TimerState::Disarmed => itimerspec {
                it_value: TS_NULL,
                it_interval: TS_NULL
            },
            TimerState::Oneshot(d) => itimerspec {
                it_value: d.into(),
                it_interval: TS_NULL,
            },
            TimerState::Periodic { current, interval } => itimerspec {
                it_value: current.into(),
                it_interval: interval.into(),
            },
        }
    }
}

impl From<itimerspec> for TimerState {
    fn from(its: itimerspec) -> TimerState {
        match its {
            itimerspec { it_value: TS_NULL, .. } => {
                TimerState::Disarmed
            }
            itimerspec { it_value, it_interval: TS_NULL } => {
                TimerState::Oneshot(it_value.into())
            }
            itimerspec { it_value, it_interval } => {
                TimerState::Periodic {
                    current: it_value.into(),
                    interval: it_interval.into(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use TimerState;
    use super::itimerspec;
    use std::time::Duration;

    #[test]
    fn convert_disarmed() {
        let start = TimerState::Disarmed;
        let clone = start.clone();
        assert_eq!(clone, start);
        let native: itimerspec = clone.into();
        assert!(native.it_value.tv_sec == 0);
        assert!(native.it_value.tv_nsec == 0);

        let target: TimerState = native.into();
        assert_eq!(target, start);
    }

    #[test]
    fn convert_oneshot() {
        let start = TimerState::Oneshot(Duration::new(1, 0));
        let clone = start.clone();
        assert_eq!(clone, start);
        let native: itimerspec = clone.into();
        assert!(native.it_interval.tv_sec == 0);
        assert!(native.it_interval.tv_nsec == 0);
        assert!(native.it_value.tv_sec == 1);
        assert!(native.it_value.tv_nsec == 0);

        let target: TimerState = native.into();
        assert_eq!(target, start);
    }

    #[test]
    fn convert_periodic() {
        let start = TimerState::Periodic {
            current: Duration::new(1, 0),
            interval: Duration::new(0, 1),
        };
        let clone = start.clone();
        assert_eq!(clone, start);
        let native: itimerspec = clone.into();
        assert!(native.it_interval.tv_sec == 0);
        assert!(native.it_interval.tv_nsec == 1);
        assert!(native.it_value.tv_sec == 1);
        assert!(native.it_value.tv_nsec == 0);

        let target: TimerState = native.into();
        assert_eq!(target, start);
    }
}
