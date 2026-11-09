# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-09

### Added
- Library crate structure with `src/lib.rs`
- Reference server binary at `src/bin/brick_server.rs`
- `RequestHandler` trait for extensible request processing
- `FcgiRequest` and `FcgiResponse` structs for type-safe FastCGI handling
- Proper FastCGI state machine for record aggregation
- Mermaid architecture diagram in README
- Socket path changed to `/tmp/brick.sock`

### Changed
- Project renamed from `brick-poc` to `brick`
- Separated generic engine from domain-specific code
- Refactored to support library usage in downstream projects
- Removed domain-specific code (flyer-a5-135g, cart table)
- Improved FastCGI parsing to handle multi-frame requests
- Updated README with library usage documentation

### Fixed
- Non-blocking I/O: proper handling of read/write events
- IO safety: avoid double-close of file descriptors
- Removed unused imports (FromRawFd, IntoRawFd)
- FastCGI connection lifecycle: proper EOF handling

### Removed
- Domain-specific artifacts from the engine
- ASCII architecture diagram (replaced with Mermaid)

### Security
- N/A

## [0.1.0] - 2026-11-09

### Added
- Initial BRICK stack proof of concept implementation
- Kernel queue (kqueue) event loop for non-blocking I/O
- Unix domain socket listener at `/tmp/brick_poc.sock`
- FastCGI protocol parsing and framing
- Embedded SQLite database integration
- Non-blocking client registration in kqueue loop
- Fallback HTTP responder for netcat validation
- AGENTS.md with AI assistance guidelines
- README.md with project documentation
- .gitignore for target/ and .idea/ directories
- CHANGELOG.md following Keep a Changelog format

### Changed
- N/A

### Fixed
- N/A

### Removed
- N/A

### Security
- N/A

---

## Template for Future Entries

### Added
- for new features

### Changed
- for changes in existing functionality

### Fixed
- for bug fixes

### Removed
- for deprecated features removal

### Security
- for vulnerability fixes
