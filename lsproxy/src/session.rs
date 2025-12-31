// ABOUTME: Session and request ID management for debug logging.
// ABOUTME: Generates UUIDs for session tracking and per-request correlation.

use std::sync::OnceLock;
use uuid::Uuid;

/// Global session ID, generated once at server startup.
static SESSION_ID: OnceLock<Uuid> = OnceLock::new();

/// Initialize the global session ID.
///
/// Should be called once at server startup. If called multiple times,
/// returns the already-initialized session ID.
pub fn init_session() -> Uuid {
    *SESSION_ID.get_or_init(Uuid::new_v4)
}

/// Get the current session ID.
///
/// # Panics
/// Panics if called before `init_session()`.
pub fn session_id() -> Uuid {
    *SESSION_ID.get().expect("Session not initialized - call init_session() first")
}

/// Try to get the session ID without panicking.
///
/// Returns None if `init_session()` hasn't been called yet.
pub fn try_session_id() -> Option<Uuid> {
    SESSION_ID.get().copied()
}

/// Generate a new unique request ID for a tool invocation.
pub fn new_request_id() -> Uuid {
    Uuid::new_v4()
}

/// Format a request ID as an HTML comment header for markdown output.
///
/// Returns a string like `<!-- request: a1b2c3d4-e5f6-7890-abcd-ef1234567890 -->\n`
pub fn request_id_header(request_id: Uuid) -> String {
    format!("<!-- request: {} -->\n", request_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_session_returns_valid_uuid() {
        // Note: This test modifies global state, so it should run in isolation
        let id = init_session();
        assert!(!id.is_nil());
    }

    #[test]
    fn init_session_is_idempotent() {
        let id1 = init_session();
        let id2 = init_session();
        assert_eq!(id1, id2, "Multiple calls to init_session should return same ID");
    }

    #[test]
    fn new_request_id_generates_unique_ids() {
        let id1 = new_request_id();
        let id2 = new_request_id();
        assert_ne!(id1, id2, "Each request should have a unique ID");
    }

    #[test]
    fn request_id_header_formats_correctly() {
        let id = Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef1234567890").unwrap();
        let header = request_id_header(id);
        assert_eq!(header, "<!-- request: a1b2c3d4-e5f6-7890-abcd-ef1234567890 -->\n");
    }

    #[test]
    fn request_id_header_ends_with_newline() {
        let id = new_request_id();
        let header = request_id_header(id);
        assert!(header.ends_with('\n'));
    }

    #[test]
    fn request_id_header_contains_uuid() {
        let id = new_request_id();
        let header = request_id_header(id);
        assert!(header.contains(&id.to_string()));
    }
}
