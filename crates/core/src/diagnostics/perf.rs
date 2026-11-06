//! Own-process CPU and memory (NFR-PERF-001, NFR-PERF-002). Reads only this process; no other
//! process and no game data is touched.

use super::PerfSample;
use std::sync::Mutex;
use std::time::Instant;

/// Previous reading: when, and total CPU seconds used so far.
static LAST: Mutex<Option<(Instant, f64)>> = Mutex::new(None);

/// CPU % of one core since the previous call (so it can pass 100 on a busy multi-thread app)
/// and resident memory in MB. The first call only sets the baseline and reports 0.0 CPU.
/// `None` when the platform gives no reading.
pub fn sample_self() -> Option<PerfSample> {
    let (cpu_s, rss_mb) = read_now()?;
    let now = Instant::now();
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let cpu_pct = match *last {
        Some((t, c)) => {
            let wall = now.duration_since(t).as_secs_f64();
            if wall > 0.0 {
                (((cpu_s - c) / wall) * 100.0).max(0.0) as f32
            } else {
                0.0
            }
        }
        None => 0.0,
    };
    *last = Some((now, cpu_s));
    Some(PerfSample { cpu_pct, rss_mb })
}

#[cfg(target_os = "linux")]
fn read_now() -> Option<(f64, u32)> {
    // /proc/self/stat: utime and stime are fields 14 and 15, in clock ticks (USER_HZ is 100 on
    // every Linux ABI Rust supports). The command name in field 2 may hold spaces, so count
    // from the closing parenthesis.
    const USER_HZ: f64 = 100.0;
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ticks: f64 = f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?;
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb: u64 = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((ticks / USER_HZ, (kb / 1024) as u32))
}

#[cfg(windows)]
fn read_now() -> Option<(f64, u32)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    // SAFETY: plain Win32 calls on the pseudo-handle of this process, with correctly sized
    // out-structures.
    unsafe {
        let h = GetCurrentProcess();
        let z = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) == 0 {
            return None;
        }
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64;
        let cpu_s = (ft(k) + ft(u)) / 10_000_000.0; // 100 ns units
        let mut m: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(h, &mut m, m.cb) == 0 {
            return None;
        }
        Some((cpu_s, (m.WorkingSetSize / (1024 * 1024)) as u32))
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn read_now() -> Option<(f64, u32)> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn sample_self_reads_this_process() {
        let a = super::sample_self().expect("reading on a supported platform");
        assert!(a.rss_mb > 0);
        let b = super::sample_self().unwrap();
        assert!(b.cpu_pct >= 0.0);
    }
}
