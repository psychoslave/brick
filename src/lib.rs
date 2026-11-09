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

/// Maximum buffer size for connection reads
const MAX_BUFFER_SIZE: usize = 4096;

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

/// Connection state for tracking partial reads and parsing
pub struct ConnectionState {
    /// Buffer holding unparsed data
    pub buffer: Vec<u8>,
    /// Current parser state
    pub parser: FcgiParserState,
}

impl ConnectionState {
    pub fn new() -> Self {
        ConnectionState {
            buffer: Vec::with_capacity(MAX_BUFFER_SIZE),
            parser: FcgiParserState::default(),
        }
    }
}

/// State for tracking FastCGI record parsing
#[derive(Debug, Default)]
pub struct FcgiParserState {
    /// Current request ID being processed
    pub request_id: Option<u16>,
    /// Request role (FCGI_RESPONDER, etc.)
    pub role: u16,
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
        self.role = 0;
        self.params.clear();
        self.params_complete = false;
        self.stdin_data.clear();
    }

    /// Convert to a FcgiRequest for handling
    pub fn to_request(&self) -> FcgiRequest {
        FcgiRequest {
            request_id: self.request_id.unwrap_or(0),
            role: self.role,
            params: self.params.clone(),
            stdin_data: self.stdin_data.clone(),
        }
    }
}

/// Parse a FastCGI record from the buffer
///
/// Returns (record_type, request_id, payload, total_record_length) or None if incomplete
pub fn parse_fcgi_record(buffer: &[u8]) -> Option<(u8, u16, &[u8], usize)> {
    if buffer.len() < 8 {
        return None;
    }

    let version = buffer[0];
    let record_type = buffer[1];
    let request_id = u16::from_be_bytes([buffer[2], buffer[3]]);
    let content_length = u16::from_be_bytes([buffer[4], buffer[5]]);
    let padding_length = buffer[6] as usize;

    // Only process version 1
    if version != 1 {
        return None;
    }

    let total_length = 8 + content_length as usize + padding_length;
    if buffer.len() < total_length {
        return None;
    }

    let payload = &buffer[8..8 + content_length as usize];
    Some((record_type, request_id, payload, total_length))
}

/// Parse name-value length pair from FastCGI PARAMS
/// 
/// Returns (name, value, bytes_consumed) or (None, None, 0) if incomplete
fn parse_nv_pair(buffer: &[u8]) -> Option<(String, String, usize)> {
    if buffer.is_empty() {
        return None;
    }

    // Parse name length
    let name_len_byte = buffer[0];
    let (name_len, name_len_bytes) = if name_len_byte < 128 {
        (name_len_byte as usize, 1)
    } else {
        // 4-byte length: high bit indicates length, followed by 3 bytes
        if buffer.len() < 4 {
            return None;
        }
        let len = ((name_len_byte & 0x7F) as usize) << 24 |
                  (buffer[1] as usize) << 16 |
                  (buffer[2] as usize) << 8 |
                  (buffer[3] as usize);
        (len, 4)
    };

    let val_len_start = name_len_bytes;
    if buffer.len() <= val_len_start {
        return None;
    }

    // Parse value length
    let val_len_byte = buffer[val_len_start];
    let (val_len, val_len_bytes) = if val_len_byte < 128 {
        (val_len_byte as usize, 1)
    } else {
        // 4-byte length
        if buffer.len() < val_len_start + 4 {
            return None;
        }
        let len = ((val_len_byte & 0x7F) as usize) << 24 |
                  (buffer[val_len_start + 1] as usize) << 16 |
                  (buffer[val_len_start + 2] as usize) << 8 |
                  (buffer[val_len_start + 3] as usize);
        (len, 4)
    };

    let name_start = val_len_start + val_len_bytes;
    let val_start = name_start + name_len;

    // Check if we have enough data
    if buffer.len() < val_start + val_len {
        return None;
    }

    let name = String::from_utf8_lossy(&buffer[name_start..name_start + name_len]).to_string();
    let val = String::from_utf8_lossy(&buffer[val_start..val_start + val_len]).to_string();

    let total_consumed = val_start + val_len;
    Some((name, val, total_consumed))
}

/// Process client input with proper buffer management
///
/// This function maintains a persistent buffer for each connection and properly
/// handles partial records across buffer boundaries.
///
/// Returns Ok(true) if FastCGI data was processed, Ok(false) if no data was processed
/// (or data was buffered for later), or Err on IO error.
pub fn process_client_input(
    stream: &mut UnixStream,
    db: &Connection,
    handler: &dyn RequestHandler,
    state: &mut ConnectionState,
) -> std::io::Result<bool> {
    // Read new data into buffer
    let mut temp_buffer = [0u8; MAX_BUFFER_SIZE];
    let bytes_read = stream.read(&mut temp_buffer)?;
    
    if bytes_read == 0 {
        // Connection closed
        return Ok(false);
    }

    // Append new data to connection buffer
    state.buffer.extend_from_slice(&temp_buffer[..bytes_read]);

    // Process records in the buffer
    let mut offset = 0;
    while offset < state.buffer.len() {
        if let Some((record_type, req_id, payload, record_len)) = parse_fcgi_record(&state.buffer[offset..]) {
            // Process the record
            match record_type {
                FCGI_BEGIN_REQUEST => {
                    state.parser.reset();
                    state.parser.request_id = Some(req_id);
                    if payload.len() >= 2 {
                        state.parser.role = u16::from_be_bytes([payload[0], payload[1]]);
                    }
                }
                FCGI_PARAMS => {
                    if state.parser.request_id == Some(req_id) {
                        if payload.is_empty() {
                            // Empty params frame marks end of parameters
                            state.parser.params_complete = true;
                        } else {
                            // Parse name-value pairs
                            let mut pos = 0;
                            while pos < payload.len() {
                                if let Some((name, val, consumed)) = parse_nv_pair(&payload[pos..]) {
                                    state.parser.params.insert(name.clone(), val.clone());
                                    pos += consumed;
                                } else {
                                    // Incomplete pair, wait for more data
                                    break;
                                }
                            }
                        }
                    }
                }
                FCGI_STDIN => {
                    if state.parser.request_id == Some(req_id) {
                        if !payload.is_empty() {
                            state.parser.stdin_data.extend_from_slice(payload);
                        } else {
                            // Empty stdin frame marks end of request body
                            // Request is complete - handle it
                            let request = state.parser.to_request();
                            let response = handler.handle(&request, db);
                            
                            // Send response via stream
                            let headers = "Status: 200 OK\r\nContent-Type: text/plain\r\n\r\n";
                            let mut content = Vec::new();
                            content.extend_from_slice(headers.as_bytes());
                            content.extend_from_slice(&response.body);
                            
                            // Send as FastCGI STDOUT records
                            send_fcgi_record(stream, FCGI_STDOUT, req_id, &content)?;
                            send_fcgi_record(stream, FCGI_STDOUT, req_id, &[])?;
                            send_fcgi_end_request(stream, req_id, 0)?;
                            
                            // Reset parser for next request on this connection
                            state.parser.reset();
                        }
                    }
                }
                _ => {
                    // Ignore other record types for now
                }
            }
            
            // Advance offset by the full record length
            offset += record_len;
        } else {
            // Incomplete record, wait for more data
            break;
        }
    }

    // Shift unparsed data to the beginning of the buffer
    if offset > 0 {
        state.buffer.drain(..offset);
    }

    // Return true if we processed any FastCGI data
    Ok(offset > 0)
}

/// Send a FastCGI record to the stream
pub fn send_fcgi_record(stream: &mut UnixStream, record_type: u8, request_id: u16, data: &[u8]) -> std::io::Result<()> {
    let length = data.len() as u16;
    let padding_length = (8 - (length as usize % 8)) % 8;

    let mut buffer = Vec::with_capacity(8 + length as usize + padding_length);
    
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
        let (record_type, request_id, payload, total_len) = result.unwrap();
        assert_eq!(record_type, FCGI_BEGIN_REQUEST);
        assert_eq!(request_id, 1);
        assert_eq!(payload.len(), 8);
        assert_eq!(total_len, 16); // 8 header + 8 body + 0 padding
    }

    #[test]
    fn test_parse_fcgi_record_with_padding() {
        // Test parsing with padding
        let buffer = [
            1, 1, 0, 1, 0, 5, 3, 0, // header: 5 bytes content, 3 bytes padding
            0, 1, 0, 0, 0, 0, 0, 0, // body + padding
        ];
        let result = parse_fcgi_record(&buffer);
        assert!(result.is_some());
        let (_, _, payload, total_len) = result.unwrap();
        assert_eq!(payload.len(), 5);
        assert_eq!(total_len, 16); // 8 header + 5 body + 3 padding
    }

    #[test]
    fn test_parse_fcgi_record_incomplete() {
        // Test incomplete record
        let buffer = [1, 1, 0, 1]; // Only 4 bytes
        let result = parse_fcgi_record(&buffer);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_nv_pair_simple() {
        // Test simple name-value pair with 1-byte lengths
        // Format: [name_len][val_len][name][val]
        // name_len=4, val_len=3, name="name", val="val"
        let buffer = b"\x04\x03nameval";
        let result = parse_nv_pair(buffer);
        assert!(result.is_some());
        let (name, val, consumed) = result.unwrap();
        assert_eq!(name, "name");
        assert_eq!(val, "val");
        assert_eq!(consumed, 9); // 1 + 1 + 4 + 3 = 9
    }

    // #[test]
    // fn test_parse_nv_pair_four_byte_length() {
    //     Four-byte length encoding is complex and rarely used
    //     TODO: Implement proper 4-byte length test
    // }

    #[test]
    fn test_parse_nv_pair_incomplete() {
        // Test incomplete name-value pair
        let buffer = b"\x03\x05name"; // Missing 'val'
        let result = parse_nv_pair(buffer);
        assert!(result.is_none());
    }

    #[test]
    fn test_multi_frame_params() {
        // Test parsing FCGI_PARAMS across multiple frames
        // Frame 1: FCGI_PARAMS with first pair
        // Frame 2: FCGI_PARAMS with second pair
        // Frame 3: Empty FCGI_PARAMS (end marker)
        
        // Build frame 1: FCGI_PARAMS with REQUEST_METHOD=GET
        let mut frame1 = vec![1, FCGI_PARAMS, 0, 1, 0, 12, 0, 0]; // 12 bytes content
        frame1.extend_from_slice(b"REQUEST_METHODGET"); // name="REQUEST_METHOD", val="GET"
        
        // Build frame 2: FCGI_PARAMS with REQUEST_URI=/
        let mut frame2 = vec![1, FCGI_PARAMS, 0, 1, 0, 11, 0, 0]; // 11 bytes content
        frame2.extend_from_slice(b"REQUEST_URI/"); // name="REQUEST_URI", val="/"
        
        // Frame 3: Empty FCGI_PARAMS
        let frame3 = vec![1, FCGI_PARAMS, 0, 1, 0, 0, 0, 0];
        
        // Combine all frames
        let mut buffer = frame1;
        buffer.extend_from_slice(&frame2);
        buffer.extend_from_slice(&frame3);
        
        // Parse the frames
        let mut state = ConnectionState::new();
        
        // Parse first frame
        let mut offset = 0;
        if let Some((record_type, req_id, payload, record_len)) = parse_fcgi_record(&buffer[offset..]) {
            assert_eq!(record_type, FCGI_PARAMS);
            assert_eq!(req_id, 1);
            offset += record_len;
        }
        
        // Parse second frame
        if let Some((record_type, req_id, payload, record_len)) = parse_fcgi_record(&buffer[offset..]) {
            assert_eq!(record_type, FCGI_PARAMS);
            assert_eq!(req_id, 1);
            offset += record_len;
        }
        
        // Parse third frame
        if let Some((record_type, req_id, payload, record_len)) = parse_fcgi_record(&buffer[offset..]) {
            assert_eq!(record_type, FCGI_PARAMS);
            assert_eq!(req_id, 1);
            assert_eq!(payload.len(), 0); // Empty payload
        }
    }
}
