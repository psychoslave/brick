# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-09

### Added
- Initial BRICK stack proof of concept implementation
- Kernel queue (kqueue) event loop for non-blocking I/O
- Unix domain socket listener at `/tmp/brick_poc.sock`
- FastCGI protocol parsing and framing
- Embedded SQLite database with cart table
- Non-blocking client registration in kqueue loop
- Fallback HTTP responder for netcat validation
- AGENTS.md with AI assistance guidelines
- README.md with project documentation
- .gitignore for target/ and .idea/ directories

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
