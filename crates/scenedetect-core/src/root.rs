// Keep the established crate root intact while the browser-facing incremental
// detector API lives in a focused module. This wrapper can disappear once the
// root module is next reorganized without changing the public API.
include!("lib.rs");

mod session;
pub use session::DetectionSession;
