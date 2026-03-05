# AGENTS.md

> Universal instructions for AI coding agents working on this repository.
> See [agents.md](https://agents.md) for the spec.

## Repository Overview

Rust ports of Windows driver samples from the official [Windows Driver Samples](https://github.com/microsoft/Windows-driver-samples), using crates from [windows-drivers-rs](https://github.com/microsoft/windows-drivers-rs). All driver crates target `#![no_std]` and compile as `cdylib` for the Windows kernel.

## Build Commands

Requires EWDK build environment, Clang (LLVM), and `cargo-make`.

```shell
# Build all workspace members
cargo build --locked

# Build and package drivers (stamps INF, creates CAT file in Package/)
cargo make

# Build a specific driver sample
cargo make --cwd general/echo/kmdf/driver/DriverSync

# Run the echo test app
cargo run --bin echoapp
cargo run --bin echoapp -- -Async
```

## Lint and Formatting

CI enforces all of these — run before submitting PRs:

```shell
# Clippy (treats warnings as errors)
cargo clippy --locked --all-targets -- -D warnings

# Clippy with nightly features
cargo +nightly clippy --locked --all-targets --features nightly -- -D warnings

# Rust formatting (requires nightly due to unstable rustfmt options)
cargo +nightly fmt --all -- --check

# TOML formatting
taplo fmt --check --diff

# Unused dependency detection
cargo machete

# Security audit
cargo audit --deny warnings

# Documentation build (warnings are errors via RUSTDOCFLAGS=-D warnings in CI)
cargo doc --locked
```

## Architecture

### Workspace Structure

The workspace (`Cargo.toml`) contains two categories of driver samples:

- **`general/`** — Functional driver samples (e.g., `echo/kmdf/driver/DriverSync` is a KMDF echo driver)
- **`tools/`** — Diagnostic/verification drivers (e.g., `dv/kmdf/fail_driver_pool_leak` intentionally leaks memory for Driver Verifier testing)
- **`general/echo/kmdf/exe`** — User-mode test app (`echoapp`) for the echo driver

### Driver Crate Anatomy

Each driver crate follows this pattern:

- **`lib.rs`** — Crate root. `#![no_std]`, clippy lints, module declarations, context structs, `WDF_*_SIZE` constants, and `wdf_declare_context_type!` macro invocations
- **`driver.rs`** — `DriverEntry` (exported as `#[export_name = "DriverEntry"]`, `extern "system"`) and `EvtDriverDeviceAdd` callback
- **`device.rs`** — Device creation, PnP/power callbacks
- **`queue.rs`** — I/O queue initialization and request handling callbacks
- **`build.rs`** — Calls `wdk_build::configure_wdk_binary_build()`
- **`*.inx`** — INF template file for driver installation

### Key Crate Dependencies (from windows-drivers-rs)

| Crate | Purpose |
|-------|---------|
| `wdk` | Safe Rust wrappers and macros (`println!`, `paged_code!`, `nt_success`, `wdf::Timer`, `wdf::SpinLock`) |
| `wdk-sys` | Raw FFI bindings to WDK. Use via `call_unsafe_wdf_function_binding!` macro |
| `wdk-alloc` | `WdkAllocator` — kernel-compatible global allocator |
| `wdk-panic` | Kernel-compatible panic handler |
| `wdk-build` | Build-time configuration for driver compilation and `cargo-make` integration |

## Key Conventions

### Driver Entry Points

- `DriverEntry` must use `#[export_name = "DriverEntry"]` and `extern "system"` calling convention
- Place init code in `#[link_section = "INIT"]`; place pageable code in `#[link_section = "PAGE"]` and invoke `paged_code!()` at function entry
- WDF callbacks use `extern "C"` calling convention

### WDF Function Calls

All WDF framework function calls go through the `call_unsafe_wdf_function_binding!` macro from `wdk-sys`. These are always `unsafe` blocks:

```rust
unsafe {
    call_unsafe_wdf_function_binding!(
        WdfDriverCreate,
        driver as PDRIVER_OBJECT,
        registry_path,
        WDF_NO_OBJECT_ATTRIBUTES,
        &raw mut driver_config,
        driver_handle_output,
    )
}
```

### WDF Structure Sizes

WDF structs require their `Size` field to be set. The codebase uses const-evaluated `WDF_*_SIZE` constants with compile-time truncation assertions:

```rust
#[allow(
    clippy::cast_possible_truncation,
    reason = "size_of::<WDF_DRIVER_CONFIG>() is known to fit in ULONG due to below const assert"
)]
const WDF_DRIVER_CONFIG_SIZE: ULONG = {
    const S: usize = core::mem::size_of::<WDF_DRIVER_CONFIG>();
    const { assert!(S <= ULONG::MAX as usize) };
    S as ULONG
};
```

### WDF Object Contexts

Context types are registered using macros from `wdf_object_context.rs`:

- `wdf_declare_context_type!(MyContext)` — generates `wdf_object_get_my_context` accessor
- `wdf_declare_context_type_with_name!(MyContext, custom_getter_name)` — generates a named accessor
- `wdf_get_context_type_info!(MyContext)` — retrieves the type info pointer for `WDF_OBJECT_ATTRIBUTES.ContextTypeInfo`

### Kernel Allocator and Panic Handler

Every driver crate must include both, gated behind `#[cfg(not(test))]`:

```rust
#[cfg(not(test))]
extern crate wdk_panic;

#[cfg(not(test))]
use wdk_alloc::WdkAllocator;

#[cfg(not(test))]
#[global_allocator]
static GLOBAL_ALLOCATOR: WdkAllocator = WdkAllocator;
```

### Clippy Configuration

All driver crates enable strict clippy lints:

```rust
#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![allow(clippy::missing_safety_doc)]
```

### Nightly Feature Gating

Crates expose an optional `nightly` feature for unstable Rust capabilities. The feature cascades through `wdk` and `wdk-sys`:

```toml
[features]
nightly = ["wdk/nightly", "wdk-sys/nightly"]
```

### Formatting

- Rust: Uses nightly `rustfmt` with unstable options (see `rustfmt.toml`): `imports_granularity = "Crate"`, `group_imports = "StdExternalCrate"`, `hex_literal_case = "Upper"`, among others
- TOML: Uses `taplo` with CRLF line endings (`taplo.toml`)

### Rust Flags

`-C target-feature=+crt-static` is set in `.cargo/config.toml` and must also be in CI `RUSTFLAGS`. CI also sets `-D warnings` to deny all warnings.

### Copyright Headers

Every `.rs` file starts with:

```rust
// Copyright (c) Microsoft Corporation.
// License: MIT OR Apache-2.0
```
