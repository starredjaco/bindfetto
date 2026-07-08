//! Bindfetto userspace consumer (M1–M3).
//!
//! Loads the probe, attaches the kprobe + tracepoint, drains the ring buffer, and
//! emits one line per transaction to the selected sink:
//!
//!   <name> (<pid>) -> <name> (<pid>): <interface>.[code:N], <size>B [oneway]
//!
//! Process names come from `/proc/<pid>/cmdline` (cached). The interface
//! descriptor is decoded from the UTF-16LE bytes captured by the probe; the method
//! name itself is resolved offline against the AIDL catalog (later milestone).
//!
//! Sink: `--sink console|logcat|both` (default console). Logcat lines use tag
//! `bindfetto` and carry the `BINDFETTO` marker so the offline decoder can select
//! them.

use std::collections::HashMap;
use std::fs;

use anyhow::Context as _;
use aya::{
    maps::RingBuf,
    programs::{KProbe, TracePoint},
    Ebpf,
};
use bindfetto_common::TxEvent;
use tokio::io::unix::AsyncFd;

// eBPF object built by build.rs (aya-build).
static EBPF_OBJ: &[u8] = aya::include_bytes_aligned!(concat!(env!("OUT_DIR"), "/bindfetto"));

/// Logcat tag; the decoder can select bindfetto lines with `logcat -s bindfetto`.
const LOG_TAG: &str = "bindfetto";
/// In-message marker so bindfetto lines are identifiable even in a merged/DLT log
/// where the tag may be flattened.
const LOG_MARKER: &str = "BINDFETTO";

/// Output destination for formatted transaction lines.
#[derive(Clone, Copy)]
enum Sink {
    Console,
    Logcat,
    Both,
}

impl Sink {
    fn console(self) -> bool {
        matches!(self, Sink::Console | Sink::Both)
    }
    fn logcat(self) -> bool {
        matches!(self, Sink::Logcat | Sink::Both)
    }

    fn parse(args: &[String]) -> Self {
        match args
            .iter()
            .position(|a| a == "--sink")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str)
        {
            Some("logcat") => Sink::Logcat,
            Some("both") => Sink::Both,
            _ => Sink::Console,
        }
    }
}

/// Minimal binding to Android's liblog for the logcat sink.
mod logcat {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int};

    const ANDROID_LOG_INFO: c_int = 4;

    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }

    pub fn write(tag: &str, msg: &str) {
        if let (Ok(tag), Ok(msg)) = (CString::new(tag), CString::new(msg)) {
            unsafe { __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), msg.as_ptr()) };
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut ebpf = Ebpf::load(EBPF_OBJ).context("load eBPF object")?;

    let kp: &mut KProbe = ebpf
        .program_mut("binder_transaction_enter")
        .context("program `binder_transaction_enter` missing")?
        .try_into()?;
    kp.load()?;
    kp.attach("binder_transaction", 0)
        .context("attach kprobe binder_transaction")?;

    let tp: &mut TracePoint = ebpf
        .program_mut("binder_transaction")
        .context("program `binder_transaction` missing")?
        .try_into()?;
    tp.load()?;
    tp.attach("binder", "binder_transaction")
        .context("attach binder:binder_transaction (need root + BPF-permissive SELinux)")?;

    let sink = Sink::parse(&std::env::args().collect::<Vec<_>>());
    let ring = RingBuf::try_from(ebpf.take_map("EVENTS").context("EVENTS map missing")?)?;
    let mut async_ring = AsyncFd::new(ring)?;
    let mut names = NameCache::default();
    // Kernel events carry CLOCK_MONOTONIC ns; this offset maps them to wall-clock.
    let boot_offset_ns = monotonic_to_realtime_offset_ns();

    println!("bindfetto: capturing binder transactions (Ctrl-C to stop)");

    // Reused across every event so the drain loop allocates nothing per line.
    let mut core = String::new();
    let mut scratch = String::new();

    loop {
        let mut guard = async_ring.readable_mut().await?;
        let ring = guard.get_inner_mut();
        while let Some(item) = ring.next() {
            let ev: &TxEvent = unsafe { &*(item.as_ptr() as *const TxEvent) };
            emit(ev, &mut names, boot_offset_ns, sink, &mut core, &mut scratch);
        }
        guard.clear_ready();
    }
}

/// Emit one transaction to the configured sink(s).
///
/// `core` and `scratch` are caller-owned buffers reused across every event so the
/// hot path allocates nothing on the heap (their capacity is retained between calls).
fn emit(
    ev: &TxEvent,
    names: &mut NameCache,
    boot_offset_ns: i128,
    sink: Sink,
    core: &mut String,
    scratch: &mut String,
) {
    core.clear();
    format_core(core, ev, names);
    if sink.console() {
        scratch.clear();
        write_timestamp(scratch, ev.ts_ns, boot_offset_ns);
        scratch.push(' ');
        scratch.push_str(core.as_str());
        println!("{scratch}");
    }
    if sink.logcat() {
        // Logcat records its own timestamp, so the message carries only the marker
        // and the core line. (liblog's C API copies the string, so one alloc here
        // is unavoidable.)
        scratch.clear();
        scratch.push_str(LOG_MARKER);
        scratch.push(' ');
        scratch.push_str(core.as_str());
        logcat::write(LOG_TAG, scratch.as_str());
    }
}

/// Write the shared, sink-independent line into `out`:
/// `src (pid) -> dst (pid): <label>, <size>B`.
fn format_core(out: &mut String, ev: &TxEvent, names: &mut NameCache) {
    use std::fmt::Write as _;
    names.ensure(ev.src_pid);
    names.ensure(ev.dst_pid);
    let src = names.lookup(ev.src_pid);
    let dst = names.lookup(ev.dst_pid);
    let oneway = if ev.is_oneway() { " oneway" } else { "" };
    let _ = write!(out, "{src} ({}) -> {dst} ({}): ", ev.src_pid, ev.dst_pid);
    // When there's no AIDL interface token: a reply carries none by design; anything
    // else is likely HIDL/hwbinder or a special transaction, not an AIDL call.
    if write_iface(out, ev) {
        let _ = write!(out, ".[code:{}]", ev.code);
    } else if ev.reply != 0 {
        let _ = write!(out, "<reply code:{}>", ev.code);
    } else {
        let _ = write!(out, "<non-aidl code:{}>", ev.code);
    }
    let _ = write!(out, ", {}B{oneway}", ev.data_size);
}

/// Nanoseconds to add to a `CLOCK_MONOTONIC` timestamp to get `CLOCK_REALTIME`
/// (Unix epoch) nanoseconds. Sampled once; good enough for display.
fn monotonic_to_realtime_offset_ns() -> i128 {
    fn clock_ns(clk: libc::clockid_t) -> i128 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe { libc::clock_gettime(clk, &mut ts) };
        ts.tv_sec as i128 * 1_000_000_000 + ts.tv_nsec as i128
    }
    clock_ns(libc::CLOCK_REALTIME) - clock_ns(libc::CLOCK_MONOTONIC)
}

/// Write a kernel monotonic timestamp into `out` as local wall-clock `HH:MM:SS.mmm`.
fn write_timestamp(out: &mut String, ts_ns: u64, boot_offset_ns: i128) {
    use std::fmt::Write as _;
    let wall_ns = ts_ns as i128 + boot_offset_ns;
    let secs = (wall_ns / 1_000_000_000) as i64;
    let nsec = (wall_ns % 1_000_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nsec) {
        // `format(..)` yields a Display adapter; writing it borrows-and-formats in
        // place with no intermediate String.
        Some(dt) => {
            let _ = write!(out, "{}", dt.with_timezone(&chrono::Local).format("%H:%M:%S%.3f"));
        }
        None => out.push_str("--:--:--.---"),
    }
}

/// Decode the event's UTF-16LE interface descriptor and append it to `out`.
/// Returns false (writing nothing) when the event carries no usable descriptor.
fn write_iface(out: &mut String, ev: &TxEvent) -> bool {
    let len = ev.iface_byte_len as usize;
    if len == 0 || len > ev.iface.len() {
        return false;
    }
    let units = ev.iface[..len]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]));
    let start = out.len();
    for ch in char::decode_utf16(units) {
        match ch {
            Ok('\0') => break, // NUL-terminated descriptor: stop at the first NUL
            Ok(c) => out.push(c),
            Err(_) => out.push('\u{FFFD}'),
        }
    }
    out.len() != start
}

/// pid -> process name, cached (a pid's name is stable for its lifetime).
#[derive(Default)]
struct NameCache(HashMap<u32, String>);

impl NameCache {
    /// Resolve and cache `pid`'s name if not already known.
    fn ensure(&mut self, pid: u32) {
        self.0.entry(pid).or_insert_with(|| resolve_name(pid));
    }

    /// Look up a name already cached by [`ensure`]. Splitting resolution (`&mut`)
    /// from lookup (`&`) lets a caller hold `&str`s for two pids at once without
    /// cloning them out of the map.
    fn lookup(&self, pid: u32) -> &str {
        self.0.get(&pid).map(String::as_str).unwrap_or("?")
    }
}

fn resolve_name(pid: u32) -> String {
    // /proc/<pid>/cmdline: NUL-separated argv; the first field is the process name.
    if let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        if end > 0 {
            return String::from_utf8_lossy(&bytes[..end]).into_owned();
        }
    }
    // Fallback: /proc/<pid>/comm (truncated to 15 chars by the kernel).
    if let Ok(s) = fs::read_to_string(format!("/proc/{pid}/comm")) {
        let t = s.trim_end();
        if !t.is_empty() {
            return t.to_owned();
        }
    }
    format!("pid:{pid}")
}
