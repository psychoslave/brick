use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};

const SOCKET_PATH: &str = "/tmp/brick_poc.sock";
const EVENT_CAPACITY: usize = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let register_event = libc::kevent {
        ident: listener_fd as usize,
        filter: libc::EVFILT_READ,
        flags: libc::EV_ADD | libc::EV_ENABLE,
        fflags: 0,
        data: 0,
        udata: std::ptr::null_mut(),
    };

    let change_status = unsafe {
        libc::kevent(
            kq,
            &register_event,
            1,
            std::ptr::null_mut(),
            0,
            std::ptr::null(),
        )
    };

    if change_status < 0 {
        return Err(std::io::Error::last_os_error().into());
    }

    println!("Listening on Unix domain socket: {}", SOCKET_PATH);

    let mut event_buffer: [libc::kevent; EVENT_CAPACITY] = unsafe { std::mem::zeroed() };

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

            if target_fd == listener_fd {
                loop {
                    match listener.accept() {
                        Ok((mut client_stream, _)) => {
                            handle_connection(&mut client_stream);
                        }
                        Err(ref error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            break;
                        }
                        Err(error) => {
                            eprintln!("Socket accept failure: {}", error);
                            break;
                        }
                    }
                }
            }
        }
    }
}

fn handle_connection(stream: &mut UnixStream) {
    let mut read_buffer = [0u8; 1024];

    match stream.read(&mut read_buffer) {
        Ok(bytes_read) if bytes_read > 0 => {
            let payload = String::from_utf8_lossy(&read_buffer[..bytes_read]);
            println!("Received payload:\n{}", payload.trim_end());

            let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "Connection: close\r\n",
            "\r\n",
            "Proof of concept: native kernel queue listener executed successfully.\n"
            );

            if let Err(error) = stream.write_all(response.as_bytes()) {
                eprintln!("Socket write failure: {}", error);
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("Socket read failure: {}", error),
    }
}