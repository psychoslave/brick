//! brick_server - Reference FastCGI server using the brick engine
//!
//! This is a minimal reference implementation that demonstrates how to use
//! the brick library to build a FastCGI service.

use brick::{Server, PingHandler};
use rusqlite::Connection;

const SOCKET_PATH: &str = "/tmp/brick.sock";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize SQLite database (in-memory for reference)
    let db = Connection::open_in_memory()?;

    // Create and start the server
    let server = Server::bind(SOCKET_PATH)?;
    server.serve(&db, PingHandler)?;

    Ok(())
}
