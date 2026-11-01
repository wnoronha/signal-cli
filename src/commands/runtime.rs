//! Load a registered manager for one-shot CLI commands.
//!
//! One-shot commands must NOT spawn a background `receive_messages` loop:
//! it would race the foreground `Manager`, drain the server queue, and
//! discard messages (previously logged at `debug` only). Incoming mail is
//! handled explicitly by `receive` (CLI) or the daemon actor (RPC).

use crate::app::{RegisteredManager, RegisteredManager as ConcreteManager};
use presage_store_sqlite::SqliteStore;

/// Load a registered manager. Concretized to `SqliteStore`: the only store
/// this binary ever uses. The old generic `<S: Store>` leaked presage
/// internals through every `core_*` signature.
pub(crate) async fn load_registered(store: SqliteStore) -> anyhow::Result<RegisteredManager> {
    Ok(ConcreteManager::load_registered(store).await?)
}
