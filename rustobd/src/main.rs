use std::{thread::sleep, time::Duration};

use clap::Parser;
use rustobd::{elm327::Elm327, error::ObdError, pid::Pid, transport::tcp::TcpTransport};

const PIDS: [Pid; 4] = [
    Pid::EngineSpeed,
    Pid::VehicleSpeed,
    Pid::EngineCoolantTemperature,
    Pid::ThrottlePosition,
];

#[derive(Parser)]
struct RustOBDArgs {
    #[arg(long)]
    tcp: String,
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
}

fn main() -> Result<(), ObdError> {
    let args: RustOBDArgs = RustOBDArgs::parse();
    let read_timeout: Duration = Duration::from_secs(2);

    let tcp_transport: TcpTransport = TcpTransport::connect(&args.tcp, read_timeout)?;
    let mut elm327_driver: Elm327<TcpTransport> = Elm327::new(tcp_transport, read_timeout);

    elm327_driver.initialize()?;

    loop {
        for pid in PIDS {
            match elm327_driver.read_pid(pid) {
                Ok(reading) => println!("{}", reading),
                Err(error) => eprintln!("PID {:?}: {}", pid, error),
            }
        }
        println!("----------------");
        sleep(Duration::from_millis(args.interval_ms));
    }
}
