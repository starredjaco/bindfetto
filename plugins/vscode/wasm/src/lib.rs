//! WebAssembly bindings for the decode core, for the VS Code extension.
//!
//! The decoder ABI (`bf_decoder_new` / `bf_decode_line` / `bf_decoder_free` /
//! `bf_string_free`) is the core's C ABI, re-exported here so the wasm cdylib keeps
//! those symbols. This crate only adds a byte allocator (`bf_alloc` / `bf_free`) so
//! JS can marshal UTF-8 strings through the module's linear memory. The JS side of
//! the contract lives in `plugins/vscode/src/decoder.ts`.
//!
//! String contract: inputs are NUL-terminated UTF-8 written into memory the caller
//! got from `bf_alloc` (and frees with `bf_free`). `bf_decode_line` returns a
//! NUL-terminated UTF-8 pointer the caller must free with `bf_string_free`.

use std::alloc::{alloc, dealloc, Layout};

// Re-export the core decoder C ABI so it is retained and exported by the wasm cdylib.
pub use bindfetto_decode::ffi::{
    bf_decode_line, bf_decoder_free, bf_decoder_new, bf_string_free,
};

/// Allocate `len` bytes of wasm linear memory; returns a pointer (0 on failure).
/// Free it with [`bf_free`] passing the same `len`.
#[no_mangle]
pub extern "C" fn bf_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    match Layout::from_size_align(len, 1) {
        Ok(layout) => unsafe { alloc(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a buffer from [`bf_alloc`]. `len` must match the original allocation.
///
/// # Safety
/// `ptr` must come from [`bf_alloc`] with the same `len`, not already freed.
#[no_mangle]
pub unsafe extern "C" fn bf_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len, 1) {
        dealloc(ptr, layout);
    }
}
