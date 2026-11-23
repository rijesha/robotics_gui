//! Runs a [`Sim`] in real time behind a UDP socket, like a vehicle on a radio link.

use crate::Sim;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_BIND: &str = "127.0.0.1:14560";
pub const DEFAULT_GCS: &str = "127.0.0.1:14550";
const STEP_S: f64 = 0.02;

/// Step the simulator until `stop` is set, sending telemetry to `gcs`.
pub fn run(mut sim: Sim, bind: &str, gcs: &str, stop: Arc<AtomicBool>) -> std::io::Result<()> {
    let socket = UdpSocket::bind(bind)?;
    socket.set_nonblocking(true)?;
    let mut buf = vec![0u8; 65536];
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        while let Ok((n, _)) = socket.recv_from(&mut buf) {
            sim.receive(&buf[..n]);
        }
        sim.step(STEP_S);
        for frame in sim.drain() {
            // The ground station may not be up yet; telemetry is simply lost until it is.
            let _ = socket.send_to(&frame, gcs);
        }
        next += Duration::from_secs_f64(STEP_S);
        match next.checked_duration_since(Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            // Fell behind (machine asleep, debugger): resume from now instead of catching up.
            None => next = Instant::now(),
        }
    }
    Ok(())
}

pub fn spawn(sim: Sim, bind: String, gcs: String, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("rgf-sim".into())
        .spawn(move || {
            if let Err(err) = run(sim, &bind, &gcs, stop) {
                eprintln!("simulator stopped: {err}");
            }
        })
        .expect("spawning the simulator thread")
}
