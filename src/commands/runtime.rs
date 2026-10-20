//! Helpers to load a registered manager for one-shot CLI commands.
//!
//! One-shot commands must NOT spawn a background `receive_messages` loop:
//! it would race the foreground `Manager`, drain the server queue, and
//! discard messages (previously logged at `debug` only). Incoming mail is
//! handled explicitly by `receive` (CLI) or the daemon actor (RPC).

use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;

pub(crate) async fn load_registered<S: Store + Send>(
    store: S,
) -> anyhow::Result<Manager<S, Registered>> {
    Ok(Manager::load_registered(store).await?)
}

pub(crate) async fn load_registered_and_receive<S: Store + Send>(
    store: S,
) -> anyhow::Result<Manager<S, Registered>> {
    // Kept for call-site compatibility; intentionally no background task.
    load_registered(store).await
}
