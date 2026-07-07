#![no_std]
#![no_main]

//! Milestone 1 probe: attach to the `binder:binder_transaction` tracepoint and
//! push a compact [`TxEvent`] into a ring buffer for every transaction.
//!
//! Later milestones add: parcel-head copy + descriptor hash (M3/M4), an error
//! attach point on the binder return path (M5), and an in-kernel filter map (M4).

use aya_ebpf::{
    helpers::{bpf_get_current_pid_tgid, bpf_ktime_get_ns},
    macros::{map, tracepoint},
    maps::RingBuf,
    programs::TracePointContext,
};
use bindfetto_common::TxEvent;

/// Ring buffer to userspace. 256 KiB; size is tunable once we see real volume.
#[map]
static EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

// Field offsets inside the binder_transaction tracepoint record (absolute from the
// record start, as `read_at` expects). Taken from the target kernel's format file:
//
//   adb shell cat /sys/kernel/tracing/events/binder/binder_transaction/format
//
// Verified on the API-level AVD (kernel tracepoint ID 900): to_proc@16, code@28,
// flags@32. Re-check if targeting a different kernel.
const OFF_TO_PROC: usize = 16;
const OFF_CODE: usize = 28;
const OFF_FLAGS: usize = 32;

#[tracepoint(category = "binder", name = "binder_transaction")]
pub fn binder_transaction(ctx: TracePointContext) -> u32 {
    match try_binder_transaction(&ctx) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn try_binder_transaction(ctx: &TracePointContext) -> Result<(), i64> {
    let pid_tgid = bpf_get_current_pid_tgid();
    let src_pid = (pid_tgid >> 32) as u32;
    let src_tid = pid_tgid as u32;

    // The tracepoint gives us the target proc and the transaction metadata.
    let dst_pid = unsafe { ctx.read_at::<i32>(OFF_TO_PROC) }? as u32;
    let code = unsafe { ctx.read_at::<u32>(OFF_CODE) }?;
    let flags = unsafe { ctx.read_at::<u32>(OFF_FLAGS) }?;

    let Some(mut entry) = EVENTS.reserve::<TxEvent>(0) else {
        // Ring buffer full — drop this event. Counting drops comes later.
        return Ok(());
    };
    entry.write(TxEvent {
        ts_ns: unsafe { bpf_ktime_get_ns() },
        src_pid,
        src_tid,
        dst_pid,
        code,
        flags,
        data_size: 0, // filled once we read the buffer (M3)
    });
    entry.submit(0);
    Ok(())
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
