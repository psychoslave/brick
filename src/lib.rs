//! # brick - BRICK Stack Engine
//!
//! A zero-dependency, ultra-minimalist web service engine built on the BRICK stack:
//! - **B**erkeley Software Distribution (BSD) / Darwin
//! - **R**ust for memory-safe backend logic
//! - **I**n-process SQLite database
//! - **C**ommon Gateway Interface (FastCGI)
//! - **K**ernel queue (kqueue)
//!
//! This crate provides a reusable library for building FastCGI services with
//! non-blocking I/O using kernel queue on BSD/Darwin systems.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use rusqlite::Connection;

/// FastCGI record type constants
pub const FCGI_BEGIN_REQUEST: u8 = 1;
pub const FCGI_PARAMS: u8 = 4;
pub const FCGI_STDIN: u8 = 5;
pub const FCGI_STDOUT: u8 = 6;
pub const FCGI_END_REQUEST: u8 = 3;

/// FastCGI request type constants
pub const FCGI_RESPONDER: u16 = 1;

/// Represents a parsed FastCGI request
#[derive(Debug, Default)]
pub struct FcgiRequest {
    /// Request ID from FCGI_BEGIN_REQUEST
    pub request_id: u16,
    /// Request role (typically FCGI_RESPONDER)
    pub role: u16,
    /// Environment variables from FCGI_PARAMS
    pub params: HashMap<String, String>,
    /// Request body from FCGI_STDIN
    pub stdin_data: Vec<u8>,
}

/// Represents a FastCGI response to be sent
#[derive(Debug, Default)]
pub struct FcgiResponse {
    /// Request ID to respond to
    pub request_id: u16,
    /// Response status code
    pub status: u32,
    /// Response body
    pub body: Vec<u8>,
}

/// Trait for handling FastCGI requests
/// 
/// Implement this trait to provide custom request handling logic.
/// The handler receives a parsed request and a database connection,
/// and returns a response to be framed and sent back to the client.
pub trait RequestHandler: Send + Sync {
    /// Handle a FastCGI request
    ///
    /// # Arguments
    /// * `request` - The parsed FastCGI request
    /// * `db` - SQLite database connection for persistence
    ///
    /// # Returns
    /// A FastCGI response to be sent to the client
    fn handle(&self, request: &FcgiRequest, db: &Connection) -> FcgiResponse;
}

/// Default request handler that returns a simple pong response
pub struct PingHandler;

impl RequestHandler for PingHandler {
    fn handle(&self, _request: &FcgiRequest, _db: &Connection) -> FcgiResponse {
        FcgiResponse {
            request_id: 0,
            status: 0,
            body: b"pong\n".to_vec(),
        }
    }
}

/// State for tracking FastCGI record parsing
#[derive(Debug, Default)]
pub struct FcgiParserState {
    /// Current request ID being processed
    pub request_id: Option<u16>,
    /// Accumulated parameters
    pub params: HashMap<String, String>,
    /// Whether we've seen the empty params frame
    pub params_complete: bool,
    /// Accumulated stdin data
    pub stdin_data: Vec<u8>,
}

impl FcgiParserState {
    /// Reset state for a new request
    pub fn reset(&mut self) {
        self.request_id = None;
        self.params.clear();
        self.params_complete = false;
        self.stdin_data.clear();
    }
}

/// Parse a FastCGI record from the buffer
///
/// Returns the record type and payload, or None if more data is needed
pub fn parse_fcgi_record(buffer: &[u8]) -> Option<(u8, u16, &[u8])> {
    if buffer.len() < 8 {
        return None;
    }

    let version = buffer[0];
    let record_type = buffer[1];
    let request_id = u16::from_be_bytes([buffer[2], buffer[3]]);
    let content_length = u16::from_be_bytes([buffer[4], buffer[5]]);
    let padding_length = buffer[6];

    // Only process version 1
    if version != 1 {
        return None;
    }

    let total_length = 8 + content_length as usize + padding_length as usize;
    if buffer.len() < total_length {
        return None;
    }

    let payload = &buffer[8..8 + content_length as usize];
    Some((record_type, request_id, payload))
}

/// Send a FastCGI record to the stream
pub fn send_fcgi_record(stream: &mut UnixStream, record_type: u8, request_id: u16, data: &[u8]) -> std::io::Result<()> {
    let length = data.len() as u16;
    let padding_length = (8 - (length as usize % 8)) % 8;
    let total_length = 8 + length as usize + padding_length;

    let mut buffer = Vec::with_capacity(total_length);
    
    // Header: version, type, request_id, content_length, padding_length
    buffer.push(1); // version
    buffer.push(record_type);
    buffer.extend_from_slice(&request_id.to_be_bytes());
    buffer.extend_from_slice(&length.to_be_bytes());
    buffer.push(padding_length as u8);
    buffer.push(0); // reserved
    
    // Content
    buffer.extend_from_slice(data);
    
    // Padding
    for _ in 0..padding_length {
        buffer.push(0);
    }
    
    stream.write_all(&buffer)
}

/// Send FCGI_END_REQUEST record
pub fn send_fcgi_end_request(stream: &mut UnixStream, request_id: u16, app_status: u32) -> std::io::Result<()> {
    let body = [
        (app_status >> 24) as u8,
        (app_status >> 16) as u8,
        (app_status >> 8) as u8,
        app_status as u8,
        0u8, // FCGI_REQUEST_COMPLETE
        0u8,
        0u8,
        0u8,
    ];
    send_fcgi_record(stream, FCGI_END_REQUEST, request_id, &body)
}

/// Process client input and handle FastCGI requests
pub fn process_client_input(
    stream: &mut UnixStream,
    db: &Connection,
    handler: &dyn RequestHandler,
) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    let mut parser_state = FcgiParserState::default();
    let mut request = FcgiRequest::default();

    loop {
        let bytes_read = stream.read(&mut buffer)?;
        if bytes_read == 0 {
            // Connection closed
            break;
        }

        let mut offset = 0;
        while offset < bytes_read {
            let remaining = &buffer[offset..bytes_read];
            
            if let Some((record_type, req_id, payload)) = parse_fcgi_record(remaining) {
                match record_type {
                    FCGI_BEGIN_REQUEST => {
                        parser_state.reset();
                        parser_state.request_id = Some(req_id);
                        request.request_id = req_id;
                        if payload.len() >= 2 {
                            request.role = u16::from_be_bytes([payload[0], payload[1]]);
                        }
                    }
                    FCGI_PARAMS => {
                        if let Some(_req_id) = parser_state.request_id {
                            if payload.is_empty() {
                                // Empty params frame marks end of parameters
                                parser_state.params_complete = true;
                            } else {
                                // Parse name-value pairs
                                // FastCGI uses length-prefixed name-value pairs
                                // For simplicity, we parse basic format (1-byte lengths)
                                let mut pos = 0;
                                while pos < payload.len() {
                                    // Read name length
                                    let name_len_byte = payload[pos];
                                    let name_len = if name_len_byte < 128 {
                                        name_len_byte as usize
                                    } else {
                                        // 4-byte length (mask off high bit, then read 3 more bytes)
                                        ((name_len_byte & 0x7F) as usize) << 24 |
                                        (payload[pos + 1] as usize) << 16 |
                                        (payload[pos + 2] as usize) << 8 |
                                        (payload[pos + 3] as usize)
                                    };
                                    
                                    // Read value length
                                    let val_len_start = pos + if name_len_byte < 128 { 1 } else { 4 };
                                    if val_len_start >= payload.len() {
                                        break;
                                    }
                                    let val_len_byte = payload[val_len_start];
                                    let val_len = if val_len_byte < 128 {
                                        val_len_byte as usize
                                    } else {
                                        ((val_len_byte & 0x7F) as usize) << 24 |
                                        (payload[val_len_start + 1] as usize) << 16 |
                                        (payload[val_len_start + 2] as usize) << 8 |
                                        (payload[val_len_start + 3] as usize)
                                    };
                                    
                                    // Calculate start positions
                                    let name_start = val_len_start + if val_len_byte < 128 { 1 } else { 4 };
                                    let val_start = name_start + name_len;
                                    
                                    // Ensure we don't read past the buffer
                                    if val_start + val_len > payload.len() {
                                        break;
                                    }
                                    
                                    // Extract name and value
                                    let name = String::from_utf8_lossy(&payload[name_start..name_start + name_len]).to_string();
                                    let val = String::from_utf8_lossy(&payload[val_start..val_start + val_len]).to_string();
                                    
                                    parser_state.params.insert(name.clone(), val.clone());
                                    request.params.insert(name, val);
                                    
                                    // Move to next pair
                                    pos = val_start + val_len;
                                }
                            }
                        }
                    }
                    FCGI_STDIN => {
                        if let Some(_req_id) = parser_state.request_id {
                            if !payload.is_empty() {
                                parser_state.stdin_data.extend_from_slice(payload);
                                request.stdin_data.extend_from_slice(payload);
                            } else {
                                // Empty stdin frame marks end of request body
                                // Request is complete - handle it
                                let response = handler.handle(&request, db);
                                
                                // Send response
                                let headers = "Status: 200 OK\r\nContent-Type: text/plain\r\n\r\n";
                                let mut content = Vec::new();
                                content.extend_from_slice(headers.as_bytes());
                                content.extend_from_slice(&response.body);
                                send_fcgi_record(stream, FCGI_STDOUT, req_id, &content)?;
                                send_fcgi_record(stream, FCGI_STDOUT, req_id, &[])?;
                                send_fcgi_end_request(stream, req_id, 0)?;
                                
                                // Reset for next request on same connection
                                parser_state.reset();
                                request = FcgiRequest::default();
                            }
                        }
                    }
                    _ => {
                        // Ignore other record types
                    }
                }
                
                offset += 8 + payload.len();
            } else {
                // Incomplete record, wait for more data
                break;
            }
        }
    }

    Ok(())
}

/// Register a file descriptor with kqueue
pub fn register_kevent(kq: i32, fd: i32, filter: i16, flags: u16) -> std::io::Result<()> {
    let ev = libc::kevent {
        ident: fd as usize,
        filter,
        flags,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    };
    let ret = unsafe { libc::kevent(kq, &ev, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Unregister and close a client connection
pub fn unregister_client(kq: i32, fd: i32) -> std::io::Result<()> {
    let _ = register_kevent(kq, fd, libc::EVFILT_READ, libc::EV_DELETE);
    unsafe { libc::close(fd) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fcgi_record() {
        // Test parsing a simple FCGI_BEGIN_REQUEST
        let buffer = [
            1, 1, 0, 1, 0, 8, 0, 0, // header
            0, 1, 0, 0, 0, 0, 0, 0, // body (FCGI_RESPONDER = 1)
        ];
        let result = parse_fcgi_record(&buffer);
        assert!(result.is_some());
        let (record_type, request_id, payload) = result.unwrap();
        assert_eq!(record_type, FCGI_BEGIN_REQUEST);
        assert_eq!(request_id, 1);
        assert_eq!(payload.len(), 8);
    }
}
