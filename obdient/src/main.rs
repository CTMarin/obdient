use std::{thread::sleep, time::Duration};

use clap::{ArgGroup, Parser};
use obdient::{
    elm327::Elm327,
    error::ObdError,
    pid::Pid,
    transport::{Transport, serial::SerialTransport, tcp::TcpTransport},
};

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const PIDS: [Pid; 4] = [
    Pid::EngineSpeed,
    Pid::VehicleSpeed,
    Pid::EngineCoolantTemperature,
    Pid::ThrottlePosition,
];

#[derive(Parser)]
#[command(group(
    ArgGroup::new("transport")
        .required(true)
        .args(["tcp", "serial"])
))]
struct ObdientArgs {
    /// TCP adapter address, host:port
    #[arg(long)]
    tcp: Option<String>,
    /// Serial port path, /dev/ttyUSB0
    #[arg(long)]
    serial: Option<String>,
    /// Serial port baud rate
    #[arg(long, default_value_t = 38400, requires = "serial")]
    baud: u32,
    /// Pause between polling cycles (ms)
    #[arg(long, default_value_t = 1000)]
    interval_ms: u64,
}

fn main() -> Result<(), ObdError> {
    let args: ObdientArgs = ObdientArgs::parse();

    match (args.tcp, args.serial) {
        (Some(address), None) => run(
            TcpTransport::connect(&address, READ_TIMEOUT)?,
            args.interval_ms,
        ),
        (None, Some(path)) => run(
            SerialTransport::connect(path, args.baud, READ_TIMEOUT)?,
            args.interval_ms,
        ),
        _ => unreachable!("clap ArgGroup guarantees exactly one transport"),
    }
}

fn run<T: Transport>(transport: T, interval_ms: u64) -> Result<(), ObdError> {
    let mut elm327_driver: Elm327<T> = Elm327::new(transport, RESPONSE_TIMEOUT);

    elm327_driver.initialize()?;

    loop {
        for pid in PIDS {
            match elm327_driver.read_pid(pid) {
                Ok(reading) => println!("{}", reading),
                Err(error) => eprintln!("PID {:?}: {}", pid, error),
            }
        }
        println!("----------------");
        sleep(Duration::from_millis(interval_ms));
    }
}
