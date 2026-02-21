# Artifact Keeper Example Plugin

A fully working example of a custom format handler plugin for [Artifact Keeper](https://github.com/artifact-keeper/artifact-keeper). This plugin handles **Unity `.unitypackage`** files (gzipped tarballs), demonstrating real-world format validation, metadata extraction, and index generation.

Use this repo as a starting point for building your own plugins. Fork it, change the format key, and implement your logic.

## What this plugin does

| Capability | Description |
|------------|-------------|
| **Validate** | Checks gzip magic bytes and correct file extension |
| **Parse metadata** | Extracts version from path/filename, detects content type |
| **Generate index** | Creates a `unity-index.json` listing all packages in a repository |

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- The `wasm32-wasip2` target (installed automatically via `rust-toolchain.toml`)

## Build

```bash
# Clone this repo
git clone https://github.com/artifact-keeper/artifact-keeper-example-plugin.git
cd artifact-keeper-example-plugin

# Build the WASM component
cargo build --release

# Output: target/wasm32-wasip2/release/unity_format_plugin.wasm
```

## Test

Unit tests run on the host target (not WASM):

```bash
cargo test --target $(rustc -vV | grep host | awk '{print $2}')
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

Download the ZIP from the [Releases](https://github.com/artifact-keeper/artifact-keeper-example-plugin/releases) page, then:

```bash
curl -X POST https://your-registry/api/v1/plugins/install/zip \
  -H "Authorization: Bearer $TOKEN" \
  -F "file=@unity-format-plugin-v0.1.0.zip"
```

### From local path

```bash
curl -X POST https://your-registry/api/v1/plugins/install/local \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"path": "/path/to/artifact-keeper-example-plugin"}'
```

## Create your own plugin

1. **Fork this repo** or use it as a template
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
├── .cargo/config.toml      # Default WASM target
├── .github/workflows/
│   ├── ci.yml              # Lint + test + build on push/PR
│   └── release.yml         # Build + package + GitHub Release on tag
├── src/lib.rs              # Plugin implementation
├── wit/format-plugin.wit   # WIT contract (from Artifact Keeper)
├── plugin.toml             # Plugin manifest
├── Cargo.toml              # Rust project config
└── rust-toolchain.toml     # Rust toolchain + WASM target
```

## WIT Interface

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
