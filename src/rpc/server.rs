use crate::cli::Command;
use crate::commands::build_receive_envelope;
use crate::rpc::context::{DaemonContext, ManagerHandle};
use crate::rpc::handler;
use crate::rpc::models::*;
use axum::{
    Json, Router,
    extract::State,
    response::sse::{Event, Sse},
    routing::{get, post},
};
use futures::{StreamExt as _, pin_mut, stream::Stream};
use presage::model::messages::Received;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};

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
    // Serialization of our own response types should not fail; fall back to
    // null instead of panicking the axum task.
    let to_json = |v: serde_json::Value| v;
    if input.is_array() {
        let requests: Vec<JsonRpcRequest> = match serde_json::from_value(input) {
            Ok(reqs) => reqs,
            Err(_) => {
                return Json(to_json(
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
                    .unwrap_or(serde_json::Value::Null),
                ));
            }
        };

        let mut responses = Vec::new();
        for req in requests {
            let req_id = req.id.clone().unwrap_or(serde_json::Value::Null);
            let (tx, rx) = oneshot::channel();
            if state
                .rpc_tx
                .send(RpcRequestMessage::HandleRequest {
                    request: req,
                    response_tx: tx,
                })
                .await
                .is_err()
            {
                responses.push(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: "Internal error: actor unavailable".to_string(),
                        data: None,
                    }),
                    id: req_id,
                });
                continue;
            }
            match rx.await {
                Ok(res) => responses.push(res),
                Err(_) => responses.push(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: "Internal error: actor died".to_string(),
                        data: None,
                    }),
                    id: req_id,
                }),
            }
        }
        Json(serde_json::to_value(responses).unwrap_or(serde_json::Value::Null))
    } else {
        let request: JsonRpcRequest = match serde_json::from_value(input.clone()) {
            Ok(req) => req,
            Err(_) => {
                return Json(to_json(
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
                    .unwrap_or(serde_json::Value::Null),
                ));
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
        Json(serde_json::to_value(response).unwrap_or(serde_json::Value::Null))
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

fn spawn_receive_loop(
    ctx: Arc<DaemonContext>,
    account: String,
    notification_tx: broadcast::Sender<String>,
) {
    tokio::task::spawn_local(async move {
        let manager_handle: ManagerHandle = match ctx.manager_for(&account).await {
            Ok(h) => h,
            Err(e) => {
                warn!(%account, error = %e, "receive loop: failed to load account");
                return;
            }
        };
        // Clone under a short lock, then run the network stream on the clone
        // so RPC handlers are not blocked by `receive_messages().await`.
        let mut receive_manager = manager_handle.lock().await.clone();
        let messages = match receive_manager.receive_messages().await {
            Ok(s) => s,
            Err(e) => {
                warn!(%account, error = %e, "receive loop: failed to start message stream");
                return;
            }
        };
        pin_mut!(messages);
        while let Some(received_res) = messages.next().await {
            match received_res {
                Received::Content(content) => {
                    let envelope = build_receive_envelope(&receive_manager, &content).await;
                    let mut params = serde_json::Map::new();
                    params.insert(
                        "envelope".to_string(),
                        serde_json::to_value(envelope).unwrap_or(serde_json::Value::Null),
                    );
                    params.insert(
                        "account".to_string(),
                        serde_json::Value::String(account.clone()),
                    );
                    let notification = JsonRpcNotification {
                        jsonrpc: "2.0".to_string(),
                        method: "receive".to_string(),
                        params: Some(serde_json::Value::Object(params)),
                    };
                    if let Ok(json) = serde_json::to_string(&notification) {
                        let _ = notification_tx.send(json);
                    }
                }
                Received::QueueEmpty => {}
                _ => {}
            }
        }
        warn!(%account, "Message stream ended for account");
    });
}

pub async fn run_json_rpc(
    config_dir: PathBuf,
    active_account: Option<String>,
    command: Command,
    trust_mode: crate::cli::TrustMode,
) -> anyhow::Result<()> {
    let ctx = DaemonContext::new(config_dir, active_account, trust_mode);
    let (notification_tx, _) = broadcast::channel::<String>(100);
    let (rpc_tx, mut rpc_rx) = mpsc::channel::<RpcRequestMessage>(32);

    ctx.set_notifier(notification_tx.clone()).await;
    for account in ctx.accounts_for_receive()? {
        spawn_receive_loop(ctx.clone(), account, notification_tx.clone());
    }

    let ctx_rpc = ctx.clone();
    tokio::task::spawn_local(async move {
        while let Some(rpc_msg) = rpc_rx.recv().await {
            let RpcRequestMessage::HandleRequest {
                request,
                response_tx,
            } = rpc_msg;
            let response = handler::handle_daemon_request(request, ctx_rpc.clone()).await;
            let _ = response_tx.send(response);
        }
    });

    match command {
        Command::JsonRpc { .. } => {
            info!("Starting JSON-RPC on stdin/stdout");
            handle_client(
                ctx.clone(),
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
                let _ = tokio::fs::remove_file(&path).await;
                let listener = UnixListener::bind(&path)?;
                loop {
                    let (stream, _) = listener.accept().await?;
                    let (reader, writer) = stream.into_split();
                    let c = ctx.clone();
                    let n_rx = notification_tx.subscribe();
                    tokio::task::spawn_local(async move {
                        if let Err(e) = handle_client(c, n_rx, reader, writer).await {
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
                    let c = ctx.clone();
                    let n_rx = notification_tx.subscribe();
                    tokio::task::spawn_local(async move {
                        if let Err(e) = handle_client(c, n_rx, reader, writer).await {
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
                    ctx.clone(),
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
    ctx: Arc<DaemonContext>,
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

                let response = handler::handle_daemon_request(request, ctx.clone()).await;

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
