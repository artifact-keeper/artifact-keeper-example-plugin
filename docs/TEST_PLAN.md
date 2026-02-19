# WASM Plugin Test Plan

## Overview

The artifact-keeper example plugin is a Rust WASM plugin template using wit-bindgen. It compiles to wasm32-wasip1 and implements the FormatHandler WIT contract.

## Test Inventory

| Test Type | Framework | Count | CI Job | Status |
|-----------|-----------|-------|--------|--------|
| Check | cargo check | Full | `check` | Active |
| Format | cargo fmt | Full | CI | Active |
| Lint | cargo clippy | Full | CI | Active |
| Unit | cargo test | Minimal | `test` | Active |
| WASM build | cargo build --release | Full | `build` | Active |
| Integration | (none) | 0 | - | Missing |

## How to Run

### Check and Lint
```bash
cargo check --workspace
cargo fmt --check
cargo clippy --workspace -- -D warnings
```

### Unit Tests (must run on host, not WASM target)
```bash
cargo test --target $(rustc -vV | grep host | awk '{print $2}')
```

### Build WASM
```bash
cargo build --release
# Output: target/wasm32-wasip1/release/unity_format_plugin.wasm
```

## Gaps and Roadmap

| Gap | Recommendation | Priority |
|-----|---------------|----------|
| No integration test | Add test that loads WASM in wasmtime and calls FormatHandler methods | P2 |
| No plugin lifecycle test | Test register, upload, download, list cycle | P3 |
