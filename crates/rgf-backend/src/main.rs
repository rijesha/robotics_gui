use rgf_backend::server::{self, Config};

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut config = Config::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--ws", Some(v)) => config.ws_bind = v,
            ("--udp", Some(v)) => config.udp_bind = v,
            ("--vehicle", Some(v)) => config.vehicle_addr = Some(v),
            _ => {
                eprintln!("usage: rgf-backend [--ws ADDR] [--udp ADDR] [--vehicle ADDR]");
                std::process::exit(2);
            }
        }
    }
    println!("UIs on ws://{}, vehicle link on udp://{}", config.ws_bind, config.udp_bind);
    server::run(config).await
}
