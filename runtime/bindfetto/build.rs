//! Build the eBPF crate for the bpf target and make its object available to the
//! consumer at compile time. See `aya-build` docs for the exact current API — this
//! is the standard aya-template build glue and is UNVERIFIED here (no toolchain).

use anyhow::Context as _;

fn main() -> anyhow::Result<()> {
    let ebpf_pkg = aya_build::cargo_metadata::MetadataCommand::new()
        .no_deps()
        .exec()
        .context("cargo metadata")?
        .packages
        .into_iter()
        .find(|p| p.name == "bindfetto-ebpf")
        .context("bindfetto-ebpf package not found")?;

    aya_build::build_ebpf([ebpf_pkg])?;
    Ok(())
}
