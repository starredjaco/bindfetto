//! Bindfetto userspace consumer — Milestone 1.
//!
//! Loads the eBPF probe, attaches it to `binder:binder_transaction`, drains the
//! ring buffer, and prints one line per transaction to the console:
//!
//!   <src_pid> -> <dst_pid>: code=<n> flags=<n> size=<n> [oneway]
//!
//! Process-name resolution (M2), descriptor decoding (M3), in-kernel filtering
//! (M4), errors (M5), and the logcat/file/JSONL sinks come later.

use anyhow::Context as _;
use aya::{maps::RingBuf, programs::TracePoint, Ebpf};
use bindfetto_common::TxEvent;
use tokio::io::unix::AsyncFd;

// The eBPF object built by build.rs (aya-build). Path/macro may differ by aya
// version; confirm against the template when finalizing the build.
static EBPF_OBJ: &[u8] =
    aya::include_bytes_aligned!(concat!(env!("OUT_DIR"), "/bindfetto"));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut ebpf = Ebpf::load(EBPF_OBJ).context("load eBPF object")?;

    let program: &mut TracePoint = ebpf
        .program_mut("binder_transaction")
        .context("program `binder_transaction` missing")?
        .try_into()?;
    program.load()?;
    program
        .attach("binder", "binder_transaction")
        .context("attach binder:binder_transaction (need root + BPF-permissive SELinux)")?;

    let ring = RingBuf::try_from(
        ebpf.take_map("EVENTS").context("EVENTS map missing")?,
    )?;
    let mut async_ring = AsyncFd::new(ring)?;

    println!("bindfetto: capturing binder transactions (Ctrl-C to stop)");

    loop {
        let mut guard = async_ring.readable_mut().await?;
        let ring = guard.get_inner_mut();
        while let Some(item) = ring.next() {
            let ev: &TxEvent = unsafe { &*(item.as_ptr() as *const TxEvent) };
            print_event(ev);
        }
        guard.clear_ready();
    }
}

fn print_event(ev: &TxEvent) {
    let oneway = if ev.is_oneway() { " oneway" } else { "" };
    println!(
        "{} -> {}: code={} flags={:#x} size={}{}",
        ev.src_pid, ev.dst_pid, ev.code, ev.flags, ev.data_size, oneway
    );
}
