use crate::cli::Command;
use crate::output::{
    AttachmentEnvelope, CallMessageEnvelope, DataMessageEnvelope, ReactionEnvelope,
    ReceiveEnvelope, StickerEnvelope, TypingMessageEnvelope,
};
use crate::rpc::handler;
use crate::rpc::models::*;
use crate::utils;
use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
};
use futures::{StreamExt as _, pin_mut, stream::Stream};
use presage::Manager;
use presage::libsignal_service::content::ContentBody;
use presage::model::messages::Received;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};

use presage::libsignal_service::prelude::Uuid;

enum RpcRequestMessage {
    HandleRequest {
        request: JsonRpcRequest,
        response_tx: oneshot::Sender<JsonRpcResponse>,
    },
}

#[derive(Clone)]
struct AppState {
    rpc_tx: mpsc::Sender<RpcRequestMessage>,
    notification_tx: broadcast::Sender<String>,
}

async fn handle_rpc_post(
    State(state): State<Arc<AppState>>,
    Json(input): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    if input.is_array() {
        let requests: Vec<JsonRpcRequest> = match serde_json::from_value(input) {
            Ok(reqs) => reqs,
            Err(_) => {
                return Json(
                    serde_json::to_value(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32600,
                            message: "method field must be set".to_string(), // Simplified message to match official behavior
                            data: None,
                        }),
                        id: serde_json::Value::Null,
                    })
                    .unwrap(),
                );
            }
        };

        let mut responses = Vec::new();
        for req in requests {
            let (tx, rx) = oneshot::channel();
            let _ = state
                .rpc_tx
                .send(RpcRequestMessage::HandleRequest {
                    request: req,
                    response_tx: tx,
                })
                .await;
            if let Ok(res) = rx.await {
                responses.push(res);
            }
        }
        Json(serde_json::to_value(responses).unwrap())
    } else {
        let request: JsonRpcRequest = match serde_json::from_value(input.clone()) {
            Ok(req) => req,
            Err(_) => {
                return Json(
                    serde_json::to_value(JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32600,
                            message: "method field must be set".to_string(),
                            data: None,
                        }),
                        id: input.get("id").cloned().unwrap_or(serde_json::Value::Null),
                    })
                    .unwrap(),
                );
            }
        };

        let (tx, rx) = oneshot::channel();
        let _ = state
            .rpc_tx
            .send(RpcRequestMessage::HandleRequest {
                request,
                response_tx: tx,
            })
            .await;
        let response = rx.await.unwrap_or_else(|_| JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: "Internal error: actor died".to_string(),
                data: None,
            }),
            id: input.get("id").cloned().unwrap_or(serde_json::Value::Null),
        });
        Json(serde_json::to_value(response).unwrap())
    }
}

async fn handle_events_get(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.notification_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(msg) => {
                if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(&msg) {
                    let mut event = Event::default().event(notification.method);
                    if let Some(params) = notification.params {
                        if let Ok(data_str) = serde_json::to_string(&params) {
                            event = event.data(data_str);
                        } else {
                            event = event.data(msg);
                        }
                    } else {
                        event = event.data(msg);
                    }
                    Some(Ok(event))
                } else {
                    Some(Ok(Event::default().data(msg)))
                }
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::new())
}

async fn handle_check_get() -> &'static str {
    "OK"
}

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
    let (rpc_tx, mut rpc_rx) = mpsc::channel::<RpcRequestMessage>(32);

    // Unified RPC & Receive actor (handles non-Send futures from presage)
    let manager_clone = manager.clone();
    let n_tx = notification_tx.clone();
    let account_clone = account.clone();
    tokio::task::spawn_local(async move {
        let messages_stream = {
            let mut m = manager_clone.lock().await;
            m.receive_messages().await.ok()
        };

        if let Some(messages) = messages_stream {
            pin_mut!(messages);
            loop {
                tokio::select! {
                    received_res = messages.next() => {
                        if let Some(Received::Content(content)) = received_res {
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
                                thread_title: None,
                                group_id: None,
                                source_device: Some(content.metadata.sender_device.into()),
                                timestamp: content.metadata.server_timestamp.timestamp_millis() as u64,
                                server_received_timestamp: None, // Need to find where presage stores this
                                server_delivered_timestamp: None,
                                data_message: if let ContentBody::DataMessage(dm) = &content.body {
                                    Some(DataMessageEnvelope {
                                        message: dm.body.clone(),
                                        attachments: Some(
                                            dm.attachments
                                                .iter()
                                                .map(|a| AttachmentEnvelope {
                                                    id: a
                                                        .attachment_identifier
                                                        .as_ref()
                                                        .map(crate::utils::attachment_id_str)
                                                        .unwrap_or_default(),
                                                    content_type: a.content_type.clone().unwrap_or_default(),
                                                    filename: a.file_name.clone(),
                                                    size: a.size.map(|s| s as u64),
                                                    width: a.width,
                                                    height: a.height,
                                                    caption: a.caption.clone(),
                                                    upload_timestamp: a.upload_timestamp,
                                                })
                                                .collect(),
                                        ),
                                        sticker: dm.sticker.as_ref().map(|s| StickerEnvelope {
                                            pack_id: s.pack_id.as_ref().map(hex::encode).unwrap_or_default(),
                                            sticker_id: s.sticker_id.unwrap_or_default(),
                                        }),
                                        reaction: dm.reaction.as_ref().map(|r| ReactionEnvelope {
                                            emoji: r.emoji.clone().unwrap_or_default(),
                                            target_author: None,
                                            target_author_uuid: r.target_author_aci.clone(),
                                            target_sent_timestamp: r.target_sent_timestamp.unwrap_or_default(),
                                            is_remove: r.remove.unwrap_or_default(),
                                        }),
                                        expires_in_seconds: dm.expire_timer.unwrap_or_default(),
                                        is_expiration_update: false, // We'd need to check flags for this
                                        view_once: dm.is_view_once.unwrap_or_default(),
                                    })
                                } else {
                                    None
                                },
                                typing_message: if let ContentBody::TypingMessage(tm) = &content.body {
                                    Some(TypingMessageEnvelope {
                                        action: match tm.action {
                                            Some(a) if a == presage::libsignal_service::proto::typing_message::Action::Started as i32 => "STARTED".to_string(),
                                            Some(a) if a == presage::libsignal_service::proto::typing_message::Action::Stopped as i32 => "STOPPED".to_string(),
                                            _ => "UNKNOWN".to_string(),
                                        },
                                        timestamp: tm.timestamp.unwrap_or_default(),
                                    })
                                } else {
                                    None
                                },
                                call_message: if let ContentBody::CallMessage(cm) = &content.body {
                                    Some(CallMessageEnvelope {
                                        offer_message: cm.offer.as_ref().map(|o| serde_json::to_value(format!("{:?}", o)).unwrap()),
                                        ice_update_messages: Some(cm.ice_update.iter().map(|i| serde_json::to_value(format!("{:?}", i)).unwrap()).collect()),
                                        hangup_message: cm.hangup.as_ref().map(|h| serde_json::to_value(format!("{:?}", h)).unwrap()),
                                        busy_message: cm.busy.as_ref().map(|b| serde_json::to_value(format!("{:?}", b)).unwrap()),
                                    })
                                } else {
                                    None
                                },
                            };

                            let mut params = serde_json::Map::new();
                            params.insert("envelope".to_string(), serde_json::to_value(envelope).unwrap());
                            params.insert("account".to_string(), serde_json::Value::String(account_clone.clone()));

                            let notification = JsonRpcNotification {
                                jsonrpc: "2.0".to_string(),
                                method: "receive".to_string(),
                                params: Some(serde_json::Value::Object(params)),
                            };

                            if let Ok(json) = serde_json::to_string(&notification) {
                                let _ = n_tx.send(json);
                            }
                        } else if received_res.is_none() {
                            warn!("Message stream ended.");
                            break;
                        }
                    }
                    Some(rpc_msg) = rpc_rx.recv() => {
                        match rpc_msg {
                            RpcRequestMessage::HandleRequest { request, response_tx } => {
                                let mut m = manager_clone.lock().await;
                                let response = handler::handle_request(request, &mut *m).await;
                                let _ = response_tx.send(response);
                            }
                        }
                    }
                }
            }
        } else {
            // No messages stream, just handle RPC
            while let Some(rpc_msg) = rpc_rx.recv().await {
                match rpc_msg {
                    RpcRequestMessage::HandleRequest {
                        request,
                        response_tx,
                    } => {
                        let mut m = manager_clone.lock().await;
                        let response = handler::handle_request(request, &mut *m).await;
                        let _ = response_tx.send(response);
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
        Command::Daemon {
            socket, tcp, http, ..
        } => {
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
            } else if let Some(addr) = http {
                info!("Starting JSON-RPC on HTTP: {}", addr);
                let app_state = Arc::new(AppState {
                    rpc_tx: rpc_tx.clone(),
                    notification_tx: notification_tx.clone(),
                });

                let app = Router::new()
                    .route("/api/v1/rpc", post(handle_rpc_post))
                    .route("/api/v1/events", get(handle_events_get))
                    .route("/api/v1/check", get(handle_check_get))
                    .with_state(app_state);

                let listener = TcpListener::bind(&addr).await?;
                axum::serve(listener, app).await?;
            } else {
                eprintln!(
                    "warning: no daemon interface specified (--socket, --tcp, or --http); using stdin/stdout JSON-RPC"
                );
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
