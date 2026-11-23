use rgf_sim::{parse_vehicle, udp, Sim};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn main() -> std::io::Result<()> {
    let mut vehicle = "rover6".to_owned();
    let mut bind = udp::DEFAULT_BIND.to_owned();
    let mut gcs = udp::DEFAULT_GCS.to_owned();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--vehicle", Some(v)) => vehicle = v,
            ("--bind", Some(v)) => bind = v,
            ("--gcs", Some(v)) => gcs = v,
            _ => {
                eprintln!("usage: rgf-sim [--vehicle rover6|tractor|skid|boat] [--bind ADDR] [--gcs ADDR]");
                std::process::exit(2);
            }
        }
    }
    let Some(vehicle_type) = parse_vehicle(&vehicle) else {
        eprintln!("unknown vehicle '{vehicle}'; expected rover6, tractor, skid or boat");
        std::process::exit(2);
    };
    println!("simulating {vehicle} on {bind}, sending to {gcs}");
    udp::run(Sim::new(vehicle_type), &bind, &gcs, Arc::new(AtomicBool::new(false)))
}
