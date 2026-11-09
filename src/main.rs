use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use rusqlite::{params, Connection};

const SOCKET_PATH: &str = "/tmp/brick_poc.sock";
const EVENT_CAPACITY: usize = 64;

// Fast Common Gateway Interface record type constants
const FCGI_BEGIN_REQUEST: u8 = 1;
const FCGI_PARAMS: u8 = 4;
const FCGI_STDIN: u8 = 5;
const FCGI_STDOUT: u8 = 6;
const FCGI_END_REQUEST: u8 = 3;
const FCGI_RESPONDER: u16 = 1;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Connection::open_in_memory()?;
    db.execute(
        "CREATE TABLE cart (
            product_id TEXT PRIMARY KEY,
            quantity INTEGER NOT NULL
        )",
        [],
    )?;
    db.execute(
        "INSERT INTO cart (product_id, quantity) VALUES (?1, ?2)",
        params!["flyer-a5-135g", 2500],
    )?;

    if fs::metadata(SOCKET_PATH).is_ok() {
        fs::remove_file(SOCKET_PATH)?;
    }

    let listener = UnixListener::bind(SOCKET_PATH)?;
    listener.set_nonblocking(true)?;
    fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o777))?;
    let listener_fd = listener.as_raw_fd();

    let kq = unsafe { libc::kqueue() };
    if kq < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    register_kevent(kq, listener_fd, libc::EVFILT_READ, libc::EV_ADD | libc::EV_ENABLE)?;

    println!("BRICK daemon initialized on socket: {}", SOCKET_PATH);

    let mut event_buffer: [libc::kevent; EVENT_CAPACITY] = unsafe { std::mem::zeroed() };
    let mut client_streams: HashMap<i32, UnixStream> = HashMap::new();

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

            if (event.flags & libc::EV_EOF) != 0 {
                unregister_client(kq, target_fd, &mut client_streams);
                continue;
            }

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
            } else if event.filter == libc::EVFILT_READ {
                if let Some(stream) = client_streams.get_mut(&target_fd) {
                    process_client_input(target_fd, stream, &db);
                }
            }
        }
    }
}

fn register_kevent(kq: i32, fd: i32, filter: i16, flags: u16) -> Result<(), std::io::Error> {
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

fn unregister_client(kq: i32, fd: i32, clients: &mut HashMap<i32, UnixStream>) {
    let _ = register_kevent(kq, fd, libc::EVFILT_READ, libc::EV_DELETE);
    clients.remove(&fd);
    unsafe { libc::close(fd) };
}

fn process_client_input(fd: i32, stream: &mut UnixStream, db: &Connection) {
    let mut buffer = [0u8; 4096];
    match stream.read(&mut buffer) {
        Ok(read_bytes) if read_bytes >= 8 => {
            let version = buffer[0];
            let record_type = buffer[1];
            let request_id = u16::from_be_bytes([buffer[2], buffer[3]]);

            if version == 1 && (record_type == FCGI_BEGIN_REQUEST || record_type == FCGI_PARAMS || record_type == FCGI_STDIN) {
                let mut quantity: i64 = 0;
                let mut product: String = String::new();

                let _ = db.query_row(
                    "SELECT product_id, quantity FROM cart LIMIT 1",
                    [],
                    |row| {
                        product = row.get(0)?;
                        quantity = row.get(1)?;
                        Ok(())
                    },
                );

                let body = format!(
                    "<!DOCTYPE html><html><body><h1>BRICK Cart</h1><p>Item: {}</p><p>Quantity: {}</p></body></html>",
                    product, quantity
                );

                let headers = format!(
                    "Status: 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );

                let content = format!("{}{}", headers, body);
                send_fcgi_record(stream, FCGI_STDOUT, request_id, content.as_bytes());
                send_fcgi_record(stream, FCGI_STDOUT, request_id, &[]);
                send_fcgi_end_request(stream, request_id);
            } else {
                // Fallback direct Hypertext Transfer Protocol responder for local netcat validation
                let fallback_body = "BRICK protocol layer operational.\n";
                let raw_response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    fallback_body.len(),
                    fallback_body
                );
                let _ = stream.write_all(raw_response.as_bytes());
            }
        }
        _ => {}
    }
}

fn send_fcgi_record(stream: &mut UnixStream, record_type: u8, request_id: u16, data: &[u8]) {
    let length = data.len() as u16;
    let header = [
        1u8,
        record_type,
        (request_id >> 8) as u8,
        (request_id & 0xFF) as u8,
        (length >> 8) as u8,
        (length & 0xFF) as u8,
        0u8,
        0u8,
    ];
    let _ = stream.write_all(&header);
    if !data.is_empty() {
        let _ = stream.write_all(data);
    }
}

fn send_fcgi_end_request(stream: &mut UnixStream, request_id: u16) {
    let app_status: u32 = 0;
    let body = [
        (app_status >> 24) as u8,
        (app_status >> 16) as u8,
        (app_status >> 8) as u8,
        (app_status & 0xFF) as u8,
        0u8, // FCGI_REQUEST_COMPLETE
        0u8,
        0u8,
        0u8,
    ];
    send_fcgi_record(stream, FCGI_END_REQUEST, request_id, &body);
}
