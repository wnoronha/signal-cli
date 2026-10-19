//! Background receive loop used by one-shot CLI commands (not the daemon actor).

use futures::{StreamExt, pin_mut};
use presage::Manager;
use presage::manager::Registered;
use presage::store::Store;

pub(super) async fn load_registered_and_receive<S: Store + Send>(
    store: S,
) -> anyhow::Result<Manager<S, Registered>> {
    let manager = Manager::load_registered(store).await?;
    let manager_receive = manager.clone();
    tokio::task::spawn_local(receive_loop(manager_receive));
    Ok(manager)
}

async fn receive_loop<S: Store>(mut manager: Manager<S, Registered>) {
    use tracing::{debug, warn};
    debug!("Starting background receive loop");
    match manager.receive_messages().await {
        Ok(messages) => {
            pin_mut!(messages);
            while let Some(content) = messages.next().await {
                debug!("Received background message: {:?}", content);
            }
            debug!("Background receive loop finished (queue empty)");
        }
        Err(e) => {
            warn!("Background receive loop failed: {:?}", e);
        }
    }
}
