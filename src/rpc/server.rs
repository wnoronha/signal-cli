use crate::cli::Command;
use crate::output::{AttachmentEnvelope, DataMessageEnvelope, ReceiveEnvelope};
use crate::rpc::handler;
use crate::rpc::models::*;
use crate::utils;
use futures::{StreamExt, pin_mut};
use presage::Manager;
use presage::libsignal_service::content::ContentBody;
use presage::model::messages::Received;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::{Mutex, broadcast};
use tracing::{error, info, warn};

use presage::libsignal_service::prelude::Uuid;

pub async fn run_json_rpc(
    config_dir: PathBuf,
    active_account: Option<String>,
    command: Command,
) -> anyhow::Result<()> {
    let account = active_account.ok_or_else(|| anyhow::anyhow!("Account not specified."))?;
    let store = utils::open_sqlite_store(&config_dir, &account).await?;
    let manager = Manager::load_registered(store).await?;
    let manager = Arc::new(Mutex::new(manager));

    let (notification_tx, _) = broadcast::channel::<String>(100);

    // Background receive task
    let manager_clone = manager.clone();
    let n_tx = notification_tx.clone();
    tokio::task::spawn_local(async move {
        let mut manager = manager_clone.lock().await;
        if let Ok(messages) = manager.receive_messages().await {
            drop(manager); // Release lock while waiting for stream
            pin_mut!(messages);
            while let Some(received) = messages.next().await {
                if let Received::Content(content) = received {
                    let source_uuid = match content.metadata.sender {
                        presage::libsignal_service::protocol::ServiceId::Aci(u) => {
                            Uuid::from(u).to_string()
                        }
                        presage::libsignal_service::protocol::ServiceId::Pni(u) => {
                            Uuid::from(u).to_string()
                        }
                    };

                    let envelope = ReceiveEnvelope {
                        source: None,
                        source_uuid: Some(source_uuid),
                        source_name: None,
                        source_device: Some(content.metadata.sender_device.into()),
                        timestamp: content.metadata.timestamp,
                        data_message: if let ContentBody::DataMessage(dm) = content.body {
                            Some(DataMessageEnvelope {
                                message: dm.body,
                                attachments: Some(
                                    dm.attachments
                                        .into_iter()
                                        .map(|a| AttachmentEnvelope {
                                            id: a
                                                .attachment_identifier
                                                .map(|i| format!("{:?}", i))
                                                .unwrap_or_default(),
                                            content_type: a.content_type.unwrap_or_default(),
                                            filename: a.file_name,
                                        })
                                        .collect(),
                                ),
                            })
                        } else {
                            None
                        },
                    };

                    let notification = JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "receive".to_string(),
                        params: Some(serde_json::to_value(envelope).unwrap()),
                    };

                    if let Ok(json) = serde_json::to_string(&notification) {
                        let _ = n_tx.send(json);
                    }
                }
            }
        }
    });

    match command {
        Command::JsonRpc { .. } => {
            info!("Starting JSON-RPC on stdin/stdout");
            handle_client(
                manager,
                notification_tx.subscribe(),
                io::stdin(),
                io::stdout(),
            )
            .await?;
        }
        Command::Daemon { socket, tcp, .. } => {
            if let Some(path) = socket {
                info!("Starting JSON-RPC on unix socket: {}", path);
                let _ = std::fs::remove_file(&path);
                let listener = UnixListener::bind(&path)?;
                loop {
                    let (stream, _) = listener.accept().await?;
                    let (reader, writer) = stream.into_split();
                    let m = manager.clone();
                    let n_rx = notification_tx.subscribe();
                    tokio::task::spawn_local(async move {
                        if let Err(e) = handle_client(m, n_rx, reader, writer).await {
                            error!("Client error: {:?}", e);
                        }
                    });
                }
            } else if let Some(addr) = tcp {
                info!("Starting JSON-RPC on TCP: {}", addr);
                let listener = TcpListener::bind(&addr).await?;
                loop {
                    let (stream, _) = listener.accept().await?;
                    let (reader, writer) = stream.into_split();
                    let m = manager.clone();
                    let n_rx = notification_tx.subscribe();
                    tokio::task::spawn_local(async move {
                        if let Err(e) = handle_client(m, n_rx, reader, writer).await {
                            error!("Client error: {:?}", e);
                        }
                    });
                }
            } else {
                warn!(
                    "No daemon interface specified (socket/tcp/http). Defaulting to stdin/stdout."
                );
                handle_client(
                    manager,
                    notification_tx.subscribe(),
                    io::stdin(),
                    io::stdout(),
                )
                .await?;
            }
        }
        _ => unreachable!(),
    }

    Ok(())
}

async fn handle_client<R, W>(
    manager: Arc<
        Mutex<Manager<impl presage::store::Store + 'static, presage::manager::Registered>>,
    >,
    mut notification_rx: broadcast::Receiver<String>,

    reader: R,
    mut writer: W,
) -> anyhow::Result<()>
where
    R: io::AsyncRead + Unpin,
    W: io::AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(reader).lines();

    loop {
        tokio::select! {
            line_res = lines.next_line() => {
                let line = match line_res {
                    Ok(Some(l)) => l,
                    Ok(None) => return Ok(()), // Connection closed
                    Err(e) => return Err(e.into()),
                };

                if line.trim().is_empty() {
                    continue;
                }

                let request: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(req) => req,
                    Err(e) => {
                        let error_res = JsonRpcResponse {
                            jsonrpc: "2.0".to_string(),
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32700, // Parse error
                                message: format!("Parse error: {:?}", e),
                                data: None,
                            }),
                            id: serde_json::Value::Null,
                        };
                        let res_json = serde_json::to_string(&error_res)?;
                        writer.write_all(res_json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                        continue;
                    }
                };

                let mut manager_lock = manager.lock().await;
                let response = handler::handle_request(request, &mut manager_lock).await;
                drop(manager_lock);

                let res_json = serde_json::to_string(&response)?;
                writer.write_all(res_json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
            }
            notification_res = notification_rx.recv() => {
                match notification_res {
                    Ok(notification_json) => {
                        writer.write_all(notification_json.as_bytes()).await?;
                        writer.write_all(b"\n").await?;
                        writer.flush().await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        warn!("Notification receiver lagged");
                    }
                }
            }
        }
    }
}
