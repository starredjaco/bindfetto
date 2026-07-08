// Standalone Node smoke test for the wasm decoder (no VS Code required).
// Run: node scripts/smoke.mjs   (needs a modern Node with WebAssembly, i.e. >= 12)

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.dirname(fileURLToPath(import.meta.url));
const wasmBytes = fs.readFileSync(path.join(dir, "..", "media", "bindfetto_decode_wasm.wasm"));
const catalog = JSON.stringify({ "android.app.IActivityManager": { "7": "startActivity" } });

const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const ex = instance.exports;

function withCString(s, fn) {
  const bytes = new TextEncoder().encode(s);
  const len = bytes.length + 1;
  const ptr = ex.bf_alloc(len);
  const mem = new Uint8Array(ex.memory.buffer);
  mem.set(bytes, ptr);
  mem[ptr + bytes.length] = 0;
  try {
    return fn(ptr);
  } finally {
    ex.bf_free(ptr, len);
  }
}

function readCString(ptr) {
  const mem = new Uint8Array(ex.memory.buffer);
  let end = ptr;
  while (mem[end] !== 0) end++;
  return new TextDecoder().decode(mem.subarray(ptr, end));
}

const handle = withCString(catalog, (p) => ex.bf_decoder_new(p));
if (handle === 0) {
  console.error("bf_decoder_new failed");
  process.exit(1);
}

function decode(line) {
  return withCString(line, (p) => {
    const out = ex.bf_decode_line(handle, p);
    if (out === 0) return line;
    const s = readCString(out);
    ex.bf_string_free(out);
    return s;
  });
}

const input = "com.x (1) -> system_server (658): android.app.IActivityManager.[code:7], 180B";
const got = decode(input);
console.log("in :", input);
console.log("out:", got);

const pass =
  got.includes("android.app.IActivityManager.startActivity") &&
  !got.includes("[code:7]") &&
  decode("unrelated line") === "unrelated line";

ex.bf_decoder_free(handle);
console.log(pass ? "WASM SMOKE OK" : "WASM SMOKE FAIL");
process.exit(pass ? 0 : 1);
