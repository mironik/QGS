#![forbid(unsafe_code)]

use qgs_core::SessionManager;
use qgs_protocol::{handle_hello, HelloRequest, CURRENT_PROTOCOL_VERSION};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sessions = SessionManager::new();
    let session = sessions.create_session()?;

    let hello = HelloRequest::current();
    let welcome = handle_hello(&hello, session.id())?;

    assert_eq!(welcome.version, CURRENT_PROTOCOL_VERSION);
    assert_eq!(welcome.session_id, session.id());

    println!(
        "verified HELLO -> WELCOME version={} session_id={}",
        welcome.version,
        welcome.session_id.get()
    );

    Ok(())
}
