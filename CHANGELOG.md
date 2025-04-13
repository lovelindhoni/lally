# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)

## [0.2.0] - 2025-04-13

### Added

- Users can now customize the AOF (Append-Only File) flush interval via the configuration file. This improves write performance for specific workloads.
- Added Docker support to streamline builds and deployment.

### Changed

- Moved the `timestamp` module into the `utils` package for better modularity.

### Fixed

- Corrected a small typo in the README documentation.

### Deprecated

- N/A

### Removed

- N/A

### Security

- N/A

## [0.1.0] - 2025-01-20

### Added

- **Initial Release**: First public release of Lally, a distributed in-memory key-value database.
- **Core Features**:
  - Thread-safe, in-memory key-value store.
  - Configurable read and write quorums for consistency and availability.
  - Append-only file (AOF) logging for crash recovery and data persistence.
  - Extensible design with hooks for custom behavior during key-value operations.
  - Connection pooling for efficient inter-node communication using gRPC.
  - Read repair mechanism to resolve stale data during read operations.
  - Graceful shutdown for proper cluster exit and resource cleanup.
- **HTTP API**:
  - Endpoints for key-value operations (`/add`, `/get`, `/remove`).
  - Endpoint to list cluster nodes (`/nodes`).
- **Configuration**:
  - YAML-based configuration with CLI overrides.
  - Support for custom AOF replay logs and fresh starts.
- **Logging**:
  - Logging for debugging and tracing operations.
- **Documentation**:
  - README with setup instructions, configuration details, and API documentation.

### Fixed

- N/A (Initial release, no bug fixes yet).

### Changed

- N/A (Initial release, no changes yet).

### Deprecated

- N/A (Initial release, nothing deprecated).

### Removed

- N/A (Initial release, nothing removed).

### Security

- N/A (Initial release, no security-related updates yet).
