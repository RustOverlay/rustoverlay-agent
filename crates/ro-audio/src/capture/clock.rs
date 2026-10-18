use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;

/// 100-ns ticks from the FILETIME epoch (1601) to the Unix epoch.
const FILETIME_TO_UNIX: i64 = 116_444_736_000_000_000;

/// One simultaneous reading of QPC and the system UTC clock, both in 100-ns ticks. UTC counts from
/// the Unix epoch. `bracket_100ns` is the QPC time the UTC read took, an upper bound on the error.
#[derive(Clone, Copy, Debug)]
pub struct ClockPair {
    pub qpc_100ns: i64,
    pub utc_100ns: i64,
    pub bracket_100ns: i64,
}

impl ClockPair {
    pub fn utc_of(&self, qpc_100ns: i64) -> i64 {
        self.utc_100ns + (qpc_100ns - self.qpc_100ns)
    }
}

pub struct Clock {
    frequency: i64,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock {
    pub fn new() -> Self {
        let mut frequency = 0;
        // Cannot fail on XP or later.
        let _ = unsafe { QueryPerformanceFrequency(&mut frequency) };
        Self { frequency }
    }

    pub fn qpc_100ns(&self) -> i64 {
        let mut count = 0;
        let _ = unsafe { QueryPerformanceCounter(&mut count) };
        (i128::from(count) * 10_000_000 / i128::from(self.frequency)) as i64
    }

    /// Brackets a UTC read between two QPC reads and keeps the tightest of a few attempts, so a
    /// preemption mid-read cannot skew the pair.
    pub fn sample(&self) -> ClockPair {
        (0..8)
            .map(|_| {
                let before = self.qpc_100ns();
                let utc = filetime_to_unix_100ns(unsafe { GetSystemTimePreciseAsFileTime() });
                let after = self.qpc_100ns();
                ClockPair { qpc_100ns: before + (after - before) / 2, utc_100ns: utc, bracket_100ns: after - before }
            })
            .min_by_key(|pair| pair.bracket_100ns)
            .expect("non-empty range")
    }
}

fn filetime_to_unix_100ns(time: windows::Win32::Foundation::FILETIME) -> i64 {
    ((i64::from(time.dwHighDateTime) << 32) | i64::from(time.dwLowDateTime)) - FILETIME_TO_UNIX
}
