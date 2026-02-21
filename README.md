# Artifact Keeper Example Plugins

A collection of working example plugins for [Artifact Keeper](https://github.com/artifact-keeper/artifact-keeper). Each plugin implements a custom format handler using the WASM Component Model and the `artifact-keeper:format@1.0.0` WIT contract.

Use these as starting points for building your own plugins. Fork, change the format key, and implement your logic.

## Included plugins

| Plugin | Format Key | What it demonstrates |
|--------|-----------|---------------------|
| [Unity](plugins/unity-format/) | `unity` | Gzip magic byte validation, path-based version extraction, JSON index |
| [RPM](plugins/rpm-format/) | `rpm` | Binary format validation (RPM lead magic), right-to-left filename parsing, structured metadata |
| [PyPI](plugins/pypi-format/) | `pypi` | PEP 427 wheel parsing, PEP 503 name normalization, HTML + JSON index generation |

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- The `wasm32-wasip2` target (installed automatically via `rust-toolchain.toml`)

## Build

```bash
git clone https://github.com/artifact-keeper/artifact-keeper-example-plugin.git
cd artifact-keeper-example-plugin

# Build all plugins
cargo build --release

# Build a specific plugin
cargo build --release -p rpm-format-plugin

# Output: target/wasm32-wasip2/release/<plugin_name>.wasm
```

## Test

Unit tests run on the host target (not WASM):

```bash
# All plugins
cargo test --target $(rustc -vV | grep host | awk '{print $2}') --workspace

# Single plugin
cargo test --target $(rustc -vV | grep host | awk '{print $2}') -p pypi-format-plugin
```

## Install into Artifact Keeper

### From Git URL

```bash
curl -X POST https://your-registry/api/v1/plugins/install/git \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://github.com/artifact-keeper/artifact-keeper-example-plugin.git",
    "ref": "v0.1.0"
  }'
```

### From ZIP (release artifact)

Download a plugin ZIP from the [Releases](https://github.com/artifact-keeper/artifact-keeper-example-plugin/releases) page, then:

```bash
curl -X POST https://your-registry/api/v1/plugins/install/zip \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@rpm-format-plugin-v0.1.0.zip"
```

### From local path

```bash
curl -X POST https://your-registry/api/v1/plugins/install/local \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"path": "/path/to/artifact-keeper-example-plugin"}'
```

## Create your own plugin

1. **Copy one of the example plugins** as a starting point (the Unity plugin is the simplest)
2. Update `plugin.toml` with your format key, extensions, and description
3. Implement the four functions in `src/lib.rs`:
   - `format_key()` -- return your unique format identifier
   - `parse_metadata()` -- extract metadata from uploaded artifacts
   - `validate()` -- reject invalid artifacts before storage
   - `generate_index()` -- create repository index files (or return `None`)
4. The WIT contract in `wit/format-plugin.wit` defines the interface -- don't modify it
5. Push a tag to trigger the release workflow

## Project structure

```
.
├── Cargo.toml                 # Workspace root
├── .cargo/config.toml         # Default WASM target (wasm32-wasip2)
├── rust-toolchain.toml        # Rust stable + WASM target
├── wit/format-plugin.wit      # Shared WIT contract
├── plugins/
│   ├── unity-format/          # Unity .unitypackage handler
│   │   ├── Cargo.toml
│   │   ├── plugin.toml
│   │   └── src/lib.rs
│   ├── rpm-format/            # RPM package handler
│   │   ├── Cargo.toml
│   │   ├── plugin.toml
│   │   └── src/lib.rs
│   └── pypi-format/           # Python wheel/sdist handler
│       ├── Cargo.toml
│       ├── plugin.toml
│       └── src/lib.rs
└── .github/workflows/
    ├── ci.yml                 # Lint + test + build on push/PR
    └── release.yml            # Build + package + GitHub Release on tag
```

## WIT interface

Plugins implement the `artifact-keeper:format@1.0.0` interface:

```wit
interface handler {
    record metadata {
        path: string,
        version: option<string>,
        content-type: string,
        size-bytes: u64,
        checksum-sha256: option<string>,
    }

    format-key: func() -> string;
    parse-metadata: func(path: string, data: list<u8>) -> result<metadata, string>;
    validate: func(path: string, data: list<u8>) -> result<_, string>;
    generate-index: func(artifacts: list<metadata>) -> result<option<list<tuple<string, list<u8>>>>, string>;
}
```

## Resources

- [Plugin System Documentation](https://artifactkeeper.com/docs/advanced/plugins/)
- [Artifact Keeper](https://github.com/artifact-keeper/artifact-keeper)
- [WIT Specification](https://component-model.bytecodealliance.org/design/wit.html)
- [wit-bindgen](https://github.com/bytecodealliance/wit-bindgen)

## License

MIT
