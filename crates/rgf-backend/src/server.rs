//! Network front for the [`Router`]: UDP towards the vehicle, WebSocket towards UIs.

use crate::Router;
use futures_util::{SinkExt, StreamExt};
use rgf_core::Decoder;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite::Message as WsMessage;

#[derive(Debug, Clone)]
pub struct Config {
    /// Where UIs connect.
    pub ws_bind: String,
    /// Where vehicle telemetry arrives.
    pub udp_bind: String,
    /// Where to send commands before the vehicle has been heard from.
    pub vehicle_addr: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ws_bind: "0.0.0.0:9002".into(),
            udp_bind: "0.0.0.0:14550".into(),
            vehicle_addr: Some("127.0.0.1:14560".into()),
        }
    }
}

pub async fn run(config: Config) -> std::io::Result<()> {
    let udp = UdpSocket::bind(&config.udp_bind).await?;
    let listener = TcpListener::bind(&config.ws_bind).await?;
    let mut vehicle_addr: Option<SocketAddr> = match &config.vehicle_addr {
        Some(addr) => tokio::net::lookup_host(addr).await?.next(),
        None => None,
    };

    // Telemetry fans out to every UI; a UI that falls behind skips ahead instead of stalling the rest.
    let (to_ui, _) = broadcast::channel::<Vec<u8>>(2048);
    let (from_ui_tx, mut from_ui) = mpsc::channel::<Vec<u8>>(1024);

    let mut router = Router::default();
    let mut vehicle_decoder = Decoder::new();
    let mut ui_decoder = Decoder::new();
    let start = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    let mut buf = vec![0u8; 65536];

    loop {
        tokio::select! {
            received = udp.recv_from(&mut buf) => {
                // A send to a vehicle that is not there yet surfaces here as an error; ignore it.
                if let Ok((n, addr)) = received {
                    vehicle_addr = Some(addr);
                    let now = start.elapsed().as_secs_f64();
                    for msg in vehicle_decoder.feed(&buf[..n]) {
                        router.from_vehicle(msg, now);
                    }
                }
            }
            Some(bytes) = from_ui.recv() => {
                let now = start.elapsed().as_secs_f64();
                for msg in ui_decoder.feed(&bytes) {
                    router.from_ui(msg, now);
                }
            }
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tokio::spawn(serve_ui(stream, to_ui.subscribe(), from_ui_tx.clone()));
                }
            }
            _ = tick.tick() => router.tick(start.elapsed().as_secs_f64()),
        }

        for frame in router.drain_to_vehicle() {
            if let Some(addr) = vehicle_addr {
                let _ = udp.send_to(&frame, addr).await;
            }
        }
        for frame in router.drain_to_ui() {
            // No UI connected is fine.
            let _ = to_ui.send(frame);
        }
    }
}

async fn serve_ui(stream: TcpStream, mut telemetry: broadcast::Receiver<Vec<u8>>, commands: mpsc::Sender<Vec<u8>>) {
    let _ = stream.set_nodelay(true);
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();
    loop {
        tokio::select! {
            frame = telemetry.recv() => match frame {
                Ok(frame) => {
                    if sink.send(WsMessage::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = source.next() => match incoming {
                Some(Ok(WsMessage::Binary(bytes))) => {
                    if commands.send(bytes.to_vec()).await.is_err() {
                        break;
                    }
                }
                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Run the backend on its own thread, for applications that embed it.
pub fn spawn(config: Config) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("rgf-backend".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("building the backend runtime");
            if let Err(err) = runtime.block_on(run(config)) {
                eprintln!("backend stopped: {err}");
            }
        })
        .expect("spawning the backend thread")
}
