pub mod role;
pub mod log_redact;
pub mod log_tail;
pub mod session_health;
pub mod singleton;
pub mod recovery;
pub mod file_classification;
pub mod git_command;
pub mod membrane;
pub mod state_paths;
pub mod wake; // #4339 — the one line a nudge may type

pub mod reap; // #4951 — spawn a fire-and-forget child and still reap it
pub mod roles_api; // #4077 — pulse role rows from /api/chorus/context/roles
