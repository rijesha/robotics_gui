//! WebSocket connection to the backend. Works the same natively and in the browser.

use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Disconnected,
    Connecting,
    Connected,
}

pub struct Link {
    pub url: String,
    pub state: LinkState,
    pub last_error: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    sender: Option<WsSender>,
    receiver: Option<WsReceiver>,
    next_attempt: f64,
    /// Set when the connection has just opened; cleared by `take_opened`.
    opened: bool,
}

const RETRY_S: f64 = 2.0;

impl Link {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            state: LinkState::Disconnected,
            last_error: None,
            rx_bytes: 0,
            tx_bytes: 0,
            sender: None,
            receiver: None,
            next_attempt: 0.0,
            opened: false,
        }
    }

    /// Drop the connection; `poll` reconnects, to the current `url`.
    pub fn reconnect(&mut self) {
        self.sender = None;
        self.receiver = None;
        self.state = LinkState::Disconnected;
        self.next_attempt = 0.0;
    }

    pub fn take_opened(&mut self) -> bool {
        std::mem::take(&mut self.opened)
    }

    /// Keep the connection up and return whatever arrived.
    pub fn poll(&mut self, now: f64, ctx: &egui::Context) -> Vec<Vec<u8>> {
        if self.receiver.is_none() && now >= self.next_attempt {
            self.next_attempt = now + RETRY_S;
            let ctx = ctx.clone();
            match ewebsock::connect_with_wakeup(&self.url, ewebsock::Options::default(), move || ctx.request_repaint())
            {
                Ok((sender, receiver)) => {
                    self.sender = Some(sender);
                    self.receiver = Some(receiver);
                    self.state = LinkState::Connecting;
                }
                Err(err) => self.last_error = Some(err),
            }
        }

        let mut received = Vec::new();
        let mut closed = false;
        if let Some(receiver) = &self.receiver {
            while let Some(event) = receiver.try_recv() {
                match event {
                    WsEvent::Opened => {
                        self.state = LinkState::Connected;
                        self.last_error = None;
                        self.opened = true;
                    }
                    WsEvent::Message(WsMessage::Binary(bytes)) => {
                        self.rx_bytes += bytes.len() as u64;
                        received.push(bytes);
                    }
                    WsEvent::Message(_) => {}
                    WsEvent::Error(err) => {
                        self.last_error = Some(err);
                        closed = true;
                    }
                    WsEvent::Closed => closed = true,
                }
            }
        }
        if closed {
            self.sender = None;
            self.receiver = None;
            self.state = LinkState::Disconnected;
            self.next_attempt = now + RETRY_S;
        }
        received
    }

    /// Send a frame. Returns false if there is no connection to send it on.
    pub fn send(&mut self, frame: Vec<u8>) -> bool {
        match (&mut self.sender, self.state) {
            (Some(sender), LinkState::Connected) => {
                self.tx_bytes += frame.len() as u64;
                sender.send(WsMessage::Binary(frame));
                true
            }
            _ => false,
        }
    }
}
