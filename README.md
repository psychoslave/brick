# brick-poc

## The BRICK Architectural Stack

| **B** | Berkeley Software Distribution (BSD) or Darwin as the host operating system |
| **R** | Rust for compiled, memory-safe backend logic |
| **I** | In-process database (SQLite) for direct embedded persistence without network overhead |
| **C** | Common Gateway Interface (CGI), specifically Fast Common Gateway Interface (FastCGI) record protocol |
| **K** | Kernel queue (kqueue) for native event notifications and non-blocking socket multiplexing |

`brick-poc` is a zero-dependency, ultra-minimalist web service proof of concept that demonstrates a complete vertical stack from kernel event handling to HTTP response generation, entirely within a single native process.

## Core Mechanics

### Kernel Queue Event Loop

The service uses BSD/Darwin's native `kqueue()` system call to register interest in read events on the Unix domain socket. This provides:

- **Native efficiency**: Direct kernel integration without polling or third-party event loop libraries
- **Non-blocking I/O**: File descriptors are set to non-blocking mode, allowing the single-threaded loop to handle multiple concurrent connections
- **Scalable multiplexing**: The kernel efficiently tracks and reports ready descriptors, even with thousands of connections

### FastCGI Protocol Parsing

FastCGI provides a binary protocol for efficient request/response exchange between web servers and applications. This POC:

- Parses FastCGI record headers and body content
- Handles request multiplexing over a single connection
- Maintains request state for proper response routing

### Embedded SQLite

SQLite provides zero-configuration, serverless database functionality:

- **Single-file persistence**: Complete database stored in a local file
- **No network overhead**: Direct function calls, no client-server communication
- **ACID compliance**: Full transaction support with atomic commits

### Unix Domain Socket

Communication occurs over a Unix domain socket at `/tmp/brick_poc.sock`:

- **Local-only**: No network exposure, accessible only to local processes
- **Filesystem permissions**: Standard Unix permission model for access control
- **High performance**: Avoids TCP/IP stack overhead for local communication

## Prerequisites

- **Operating System**: Darwin (macOS) or Berkeley Software Distribution (FreeBSD/NetBSD/OpenBSD)
- **Compiler**: Rust 1.70+ (with stable toolchain)
- **Package Manager**: Cargo (included with Rust)
- **Dependencies**: libc (provided by the OS)

## Installation

```bash
# Clone the repository
git clone git@github.com:psychoslave/brick-poc.git
cd brick-poc

# Build the project
cargo build --release
```

## Execution

### Starting the Service

```bash
cargo run --release
```

The service will create a Unix domain socket at `/tmp/brick_poc.sock` and begin listening for connections.

### Verification

Use `nc` (netcat) or `socat` to test the service:

```bash
# Using netcat (if it supports Unix sockets)
nc -U /tmp/brick_poc.sock

# Using socat
socat - UNIX-CONNECT:/tmp/brick_poc.sock

# Send a test request
curl --unix-socket /tmp/brick_poc.sock http://localhost/
```

### Expected Response

```
HTTP/1.1 200 OK
Content-Type: text/plain; charset=utf-8
Connection: close

Proof of concept: native kernel queue listener executed successfully.
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         brick-poc                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────┐  │
│  │  kqueue()   │    │ FastCGI     │    │    SQLite           │  │
│  │  Event Loop │────▶│ Protocol    │────▶│ Embedded Database   │  │
│  │             │    │ Parser      │    │                     │  │
│  └─────────────┘    └─────────────┘    └─────────────────────┘  │
│           │                    │                     │            │
│           ▼                    ▼                     ▼            │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                    Unix Domain Socket                    │    │
│  │                    /tmp/brick_poc.sock                    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Project Structure

```
brick-poc/
├── Cargo.toml          # Rust package manifest
├── Cargo.lock          # Dependency lock file
├── README.md           # This file
├── .gitignore          # Git ignore patterns
└── src/
    └── main.rs         # Main application entry point
```

## License

This project is provided as-is for proof-of-concept purposes. See the repository for licensing details.

## Technical Notes

- The service currently handles a single request at a time in the event loop
- FastCGI protocol support is minimal and focused on request/response basics
- SQLite integration is planned for future iterations
- Error handling is basic but covers critical paths

## Future Enhancements

- [ ] Full FastCGI protocol implementation
- [ ] SQLite database integration for request logging
- [ ] Concurrent request handling
- [ ] HTTPS support via Unix domain socket forwarding
- [ ] Configuration file support
- [ ] Graceful shutdown handling
