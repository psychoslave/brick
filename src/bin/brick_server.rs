//! brick_server - Reference FastCGI server using the brick engine
//!
//! This is a minimal reference implementation that demonstrates how to use
//! the brick library to build a FastCGI service.

use brick::{process_client_input, register_kevent, FcgiRequest, FcgiResponse, RequestHandler};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};

const SOCKET_PATH: &str = "/tmp/brick.sock";
const EVENT_CAPACITY: usize = 64;

/// Reference handler that returns a simple ping response
struct PingHandler;

impl RequestHandler for PingHandler {
    fn handle(&self, _request: &FcgiRequest, _db: &Connection) -> FcgiResponse {
        FcgiResponse {
            request_id: 0, // Will be set by caller
            status: 0,
            body: b"pong\n".to_vec(),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize SQLite database (in-memory for reference)
    let db = Connection::open_in_memory()?;

    // Clean up any existing socket
    if fs::metadata(SOCKET_PATH).is_ok() {
        fs::remove_file(SOCKET_PATH)?;
    }

    // Create and configure Unix domain socket
    let listener = UnixListener::bind(SOCKET_PATH)?;
    listener.set_nonblocking(true)?;
    fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o777))?;
    let listener_fd = listener.as_raw_fd();

    // Initialize kqueue
    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    // Register listener with kqueue
    register_kevent(kq, listener_fd, libc::EVFILT_READ, libc::EV_ADD | libc::EV_ENABLE)?;

    println!("BRICK server listening on: {}", SOCKET_PATH);

    let mut event_buffer: [libc::kevent; EVENT_CAPACITY] = unsafe { std::mem::zeroed() };
    let mut client_streams: HashMap<i32, UnixStream> = HashMap::new();
    let handler = PingHandler;

    loop {
        let event_count = unsafe {
            libc::kevent(
                kq,
                std::ptr::null(),
                0,
                event_buffer.as_mut_ptr(),
                EVENT_CAPACITY as i32,
                std::ptr::null(),
            )
        };

        if event_count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }

        for i in 0..event_count as usize {
            let event = event_buffer[i];
            let target_fd = event.ident as i32;

            // Handle connection close
            if (event.flags & libc::EV_EOF) != 0 {
                client_streams.remove(&target_fd);
                continue;
            }

            // Handle new connection
            if target_fd == listener_fd {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            stream.set_nonblocking(true)?;
                            let client_fd = stream.as_raw_fd();
                            if register_kevent(kq, client_fd, libc::EVFILT_READ, libc::EV_ADD | libc::EV_ENABLE).is_ok() {
                                client_streams.insert(client_fd, stream);
                            }
                        }
                        Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(error) => {
                            eprintln!("Socket accept error: {}", error);
                            break;
                        }
                    }
                }
            }
            // Handle client data
            else if event.filter == libc::EVFILT_READ {
                if let Some(stream) = client_streams.remove(&target_fd) {
                    let mut tmp_stream = stream;
                    let mut buffer = [0u8; 4096];
                    match tmp_stream.read(&mut buffer) {
                        Ok(bytes_read) if bytes_read > 0 => {
                            // Check if this looks like FastCGI (version 1)
                            if bytes_read >= 8 && buffer[0] == 1 {
                                // Process as FastCGI
                                let _ = process_client_input(&mut tmp_stream, &db, &handler);
                            } else {
                                // Fallback: raw HTTP responder for testing
                                let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\npong\n";
                                let _ = tmp_stream.write_all(response);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!("Error reading from client: {}", error);
                        }
                    }
                    // Stream is dropped here, which closes the fd
                    // kqueue will automatically detect EOF on next event
                }
            }
        }
    }
}
