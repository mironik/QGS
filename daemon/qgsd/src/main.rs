#![forbid(unsafe_code)]

use qgs_core::SessionManager;
use qgs_protocol::{handle_hello, HelloRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sessions = SessionManager::new();
    let session = sessions.create_session()?;

    let hello = HelloRequest::current();
    let welcome = handle_hello(&hello, session.id())?;

    println!(
        "HELLO -> WELCOME version={} session_id={}",
        welcome.version,
        welcome.session_id.get()
    );

    Ok(())
}
