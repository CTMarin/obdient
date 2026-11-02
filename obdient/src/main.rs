use std::{thread::sleep, time::Duration};

use clap::{ArgGroup, Parser};
use obdient::{
    elm327::Elm327,
    error::{ObdError, ObdErrorKind},
    pid::{Pid, SupportedPidsMask},
    poller::{CycleReport, Poller, policy::RetryPolicy, wait_for_supported_pids},
    transport::{Transport, serial::SerialTransport, tcp::TcpTransport},
};

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const VEHICLE_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const PIDS: [Pid; 5] = [
    Pid::EngineSpeed,
    Pid::VehicleSpeed,
    Pid::EngineCoolantTemperature,
    Pid::ThrottlePosition,
    Pid::FuelTankLevel,
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
    let mut elm327_driver: Elm327<T> = Elm327::new(transport, RESPONSE_TIMEOUT, SEARCH_TIMEOUT);

    elm327_driver.initialize()?;

    let supported_pids_mask: SupportedPidsMask =
        wait_for_supported_pids(&mut elm327_driver, VEHICLE_RETRY_INTERVAL)?;
    let (supported_pids, unsupported_pids): (Vec<Pid>, Vec<Pid>) = PIDS
        .into_iter()
        .partition(|pid| supported_pids_mask.contains(*pid));
    for pid in unsupported_pids {
        eprintln!("PID {pid:?}: not supported by vehicle")
    }

    let mut poller: Poller<T> = Poller::new(elm327_driver, supported_pids, RetryPolicy::default());

    while poller.has_active_pids() {
        match poller.poll_cycle() {
            Ok(report) => print_report(&report),
            Err(error) if error.kind() == ObdErrorKind::VehicleUnavailable => {
                eprintln!("{error}")
            }
            Err(error) => return Err(error),
        }
        sleep(Duration::from_millis(interval_ms));
    }

    eprintln!("no PIDs left to poll");
    Ok(())
}

fn print_report(report: &CycleReport) {
    for reading in &report.readings {
        println!("{reading}");
    }
    for (pid, error) in &report.failures {
        eprintln!("PID {pid:?}: {error}");
    }
    for pid in &report.newly_unavailable {
        eprintln!("PID {pid:?}: marked unavailable")
    }
    println!("----------------")
}
