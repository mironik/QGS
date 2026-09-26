#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};

use qgs_protocol::{ProtocolError, SessionId};

#[derive(Debug)]
pub struct SessionManager {
    next_session_id: AtomicU64,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
        }
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_session(&self) -> Result<Session, ProtocolError> {
        let raw = self
            .next_session_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1).filter(|_| next != u64::MAX)
            })
            .map_err(|_| ProtocolError::SessionIdsExhausted)?;
        let id = SessionId::new(raw)?;
        Ok(Session { id })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Session {
    id: SessionId,
}

impl Session {
    pub const fn id(self) -> SessionId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_unique_sessions() {
        let sessions = SessionManager::new();

        let first = sessions.create_session().expect("first session");
        let second = sessions.create_session().expect("second session");

        assert_ne!(first.id(), second.id());
        assert_eq!(first.id().get(), 1);
        assert_eq!(second.id().get(), 2);
    }

    #[test]
    fn refuses_to_wrap_session_ids_to_zero() {
        let sessions = SessionManager {
            next_session_id: AtomicU64::new(u64::MAX),
        };

        let err = sessions
            .create_session()
            .expect_err("session ids should be exhausted before wrapping");

        assert_eq!(err, ProtocolError::SessionIdsExhausted);
        assert_eq!(sessions.next_session_id.load(Ordering::Relaxed), u64::MAX);
    }
}
