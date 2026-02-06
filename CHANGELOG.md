# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0-a1] - 2026-02-06

First public alpha release of the Unity format handler plugin for Artifact Keeper.

### Added

- WIT contract implementation with all required exports:
  - `format-key`: Returns "unity" format identifier
  - `parse-metadata`: Extracts version and metadata from Unity packages
  - `validate`: Validates gzip magic bytes and .unitypackage extension
  - `generate-index`: Creates JSON index for repository browsing
- Real format validation using gzip magic bytes (1f 8b) and extension checking
- Version extraction from file paths and filenames
- JSON index generation for Unity package repository browsing
- GitHub Actions CI workflow with lint, test, and WASM build steps
- GitHub Actions release workflow that builds ZIP installable via plugin install API
- Comprehensive unit test suite (12 tests)
- WIT interface definition for format handler contract
- Example plugin.toml configuration
- MIT license
