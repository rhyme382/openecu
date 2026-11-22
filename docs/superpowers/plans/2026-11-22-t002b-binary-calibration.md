# T002-B Binary Calibration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read calibration values safely from an immutable in-memory BIN using explicit address mapping, A2L metadata, and lossless provenance.

**Architecture:** `openecu-firmware` owns checked ECU-address-to-BIN offset mapping and primitive byte decoding. `openecu-calibration` owns pure scalar and matrix calibration values, their physical conversion result, and the exact bytes/address that produced them. A2L adaptation is opt-in: it only creates a calibration definition when the referenced layout, byte order, data type, dimensions, and conversion are all unambiguous.

**Tech Stack:** Rust stable, existing workspace crates, Serde, thiserror.

**Spec:** User-approved T002-B design in this conversation.

## Global Constraints

- Work directly on `main`; do not create a worktree.
- Do not change `.oedef` v1 or modify firmware input bytes.
- No ECU access, flashing, network I/O, commits, or pushes.
- Support U8/S8/U16/S16/U32/S32 in both endian orders.
- Only `IDENTICAL` A2L conversion is executable; other conversions report unsupported metadata.

## Review Focus

- Address-to-offset arithmetic overflows or range ends beyond the image must fail before slicing.
- A scalar decoder must not reinterpret signed values or endian order.
- Matrix data must retain dimensions and its row-major storage declaration.
- A missing A2L layout, byte order, type, dimensions, or conversion must fail instead of being guessed.
- An A2L `COMPU_METHOD` other than `IDENTICAL` must be reported as unsupported, never silently applied.

### Task 1: Checked firmware mapping and decoding

**Files:** `crates/openecu-firmware/{Cargo.toml,src/lib.rs}`.

- [ ] Write failing unit tests for a mapped range, mapping underflow/overflow/out-of-bounds, every supported signed/unsigned type, and both endian orders.
- [ ] Implement immutable `EcuAddressMap`, byte-order/type enums, checked reads, and primitive decoding.
- [ ] Run the firmware tests green.

### Task 2: Pure calibration values and matrices

**Files:** `crates/openecu-calibration/{Cargo.toml,src/lib.rs}`.

- [ ] Write failing integration-style unit tests for scalar raw/physical/provenance separation and explicit row-major matrix shape/data.
- [ ] Implement pure calibration definitions, raw values, physical values, provenance, and row-major matrix validation.
- [ ] Run the calibration tests green.

### Task 3: Safe A2L adapter and documentation

**Files:** `crates/openecu-calibration/{Cargo.toml,src/lib.rs,Cargo.toml}`; `docs/a2l-import.md`.

- [ ] Write failing tests with synthetic A2L for complete `IDENTICAL` metadata and rejected missing/unsupported metadata.
- [ ] Implement the adapter using `RECORD_LAYOUT/FNC_VALUES`, module or item byte order, dimensions, and `COMPU_METHOD`; expose an explicit unsupported-conversion error.
- [ ] Document T002-B scope and limitations; run crate tests green.

### Task 4: Workspace verification

**Files:** no production files.

- [ ] Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `git diff --check`.
