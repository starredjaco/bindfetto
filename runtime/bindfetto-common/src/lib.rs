#![no_std]

//! Shared data contract between the eBPF probe and the userspace consumer.
//!
//! This is the wire format that crosses the ring buffer. Keep it `#[repr(C)]`,
//! `Copy`, and free of pointers/padding surprises so both sides agree byte-for-byte.

/// `TF_ONE_WAY` — set in [`TxEvent::flags`] for async (oneway) transactions.
pub const TF_ONE_WAY: u32 = 0x01;

/// One captured Binder transaction.
///
/// Milestone 1 fills only pids/code/flags. `data_size` and the raw interface
/// descriptor bytes are added in later milestones (see ROADMAP).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TxEvent {
    /// Kernel monotonic timestamp (ns) when the transaction was observed.
    pub ts_ns: u64,
    /// Sender process id (tgid).
    pub src_pid: u32,
    /// Sender thread id.
    pub src_tid: u32,
    /// Target process id (`to_proc` from the tracepoint).
    pub dst_pid: u32,
    /// Raw transaction code (method selector; decoded offline via the catalog).
    pub code: u32,
    /// Transaction flags; test against [`TF_ONE_WAY`] for async.
    pub flags: u32,
    /// Parcel payload size in bytes. 0 until the buffer read lands (M3).
    pub data_size: u32,
}

impl TxEvent {
    /// True if this is an async (oneway) transaction.
    #[inline]
    pub fn is_oneway(&self) -> bool {
        self.flags & TF_ONE_WAY != 0
    }
}

#[cfg(feature = "user")]
unsafe impl aya::Pod for TxEvent {}
