use std::{thread::sleep, time::Duration};

use clap::Parser;
use rustobd::{elm327::Elm327, error::ObdError, pid::Pid, transport::tcp::TcpTransport};

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const PIDS: [Pid; 4] = [
    Pid::EngineSpeed,
    Pid::VehicleSpeed,
    Pid::EngineCoolantTemperature,
    Pid::ThrottlePosition,
];

#[derive(Parser)]
struct RustOBDArgs {
    /// Adapter address, host:port
    #[arg(long)]
    tcp: String,
    /// Pause between polling cycles (ms)
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
}

fn main() -> Result<(), ObdError> {
    let args: RustOBDArgs = RustOBDArgs::parse();

    let tcp_transport: TcpTransport = TcpTransport::connect(&args.tcp, READ_TIMEOUT)?;
    let mut elm327_driver: Elm327<TcpTransport> = Elm327::new(tcp_transport, RESPONSE_TIMEOUT);

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
