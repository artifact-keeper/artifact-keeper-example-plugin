# WASM Plugin Test Plan

## Overview

The artifact-keeper example plugins are Rust WASM plugin templates using wit-bindgen. They compile to wasm32-wasip2 and implement the FormatHandler WIT contract. The workspace contains three plugins: Unity, RPM, and PyPI.

## Test Inventory

| Test Type | Framework | Count | CI Job | Status |
|-----------|-----------|-------|--------|--------|
| Format | cargo fmt | Full | CI | Active |
| Lint | cargo clippy | Full | CI | Active |
| Unit - Unity | cargo test | 12 | CI | Active |
| Unit - RPM | cargo test | 18 | CI | Active |
| Unit - PyPI | cargo test | 27 | CI | Active |
| WASM build | cargo build --release | 3 plugins | CI | Active |
| Integration | (none) | 0 | - | Missing |

## How to Run

### Lint
```bash
cargo fmt --check
cargo clippy --target wasm32-wasip2 --workspace -- -D warnings
```

### Unit Tests (must run on host, not WASM target)
```bash
cargo test --target $(rustc -vV | grep host | awk '{print $2}') --workspace
```

### Build WASM
```bash
cargo build --release --workspace
# Output: target/wasm32-wasip2/release/{unity,rpm,pypi}_format_plugin.wasm
```

## Plugin Test Coverage

### Unity Format (12 tests)
- Format key identity
- Gzip magic byte detection
- Non-gzip content type fallback
- Validation: accepts valid gzip, rejects empty, rejects wrong extension, rejects bad magic
- Version extraction from path component and filename
- Index generation: empty returns None, produces valid JSON

### RPM Format (18 tests)
- Format key identity
- Filename parsing: simple, hyphens in name, noarch, no extension
- Version extraction from paths
- Metadata: RPM magic detection, non-RPM fallback, empty error
- Validation: accepts valid RPM, rejects empty, wrong extension, too small, bad magic, empty path
- Index generation: empty returns None, produces JSON with name/arch/release fields

### PyPI Format (27 tests)
- Format key identity
- PEP 503 name normalization: simple, underscores, dots, consecutive separators, mixed, leading/trailing
- Wheel filename parsing: name extraction, version extraction, build tag handling
- Source distribution parsing: name from sdist, name with hyphens, version from tar.gz/zip
- Metadata: wheel content type, sdist content type, empty error
- Validation: accepts wheel, accepts sdist, rejects empty, wrong extension, bad wheel filename, sdist without version, empty path
- Index generation: empty returns None, produces HTML + JSON, normalizes package names

## Gaps and Roadmap

| Gap | Recommendation | Priority |
|-----|---------------|----------|
| No integration test | Add test that loads WASM in wasmtime and calls FormatHandler methods | P2 |
| No plugin lifecycle test | Test register, upload, download, list cycle | P3 |
| No cross-plugin test | Verify all three plugins can coexist in the same Artifact Keeper instance | P3 |
