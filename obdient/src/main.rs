use std::time::Duration;

use clap::{ArgGroup, Parser};
use obdient::{
    error::ObdError,
    poller::CycleReport,
    session::{self, SessionConfig, SessionEvent},
    transport::{Transport, serial::SerialTransport, tcp::TcpTransport},
};

const READ_TIMEOUT: Duration = Duration::from_millis(100);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
            TcpTransport::connect(&address, READ_TIMEOUT, CONNECT_TIMEOUT)?,
            args.interval_ms,
        ),
        (None, Some(path)) => run(
            SerialTransport::connect(path, args.baud, READ_TIMEOUT)?,
            args.interval_ms,
        ),
        _ => unreachable!("clap ArgGroup guarantees exactly one transport"),
    }?;

    Ok(())
}

fn run<T: Transport>(transport: T, interval_ms: u64) -> Result<(), ObdError> {
    let config: SessionConfig = SessionConfig {
        cycle_interval: Duration::from_millis(interval_ms),
        ..SessionConfig::default()
    };

    session::run(transport, &config, |event| match event {
        SessionEvent::Unsupported(pid) => eprintln!("PID {pid:?}: not supported by vehicle"),
        SessionEvent::Report(report) => print_report(report),
        SessionEvent::VehicleUnavailable(error) => eprintln!("{error}"),
        SessionEvent::ConnectionLost(error) => eprintln!("connection lost: {error}, reconnecting"),
        SessionEvent::Reconnected => eprintln!("reconnected"),
        SessionEvent::WaitingForVehicle(error) => eprintln!("waiting for vehicle: {error}"),
        SessionEvent::ReconnectAttemptFailed {
            attempt,
            max_attempts,
            error,
        } => eprintln!("reconnect attempt {attempt}/{max_attempts} failed: {error}"),
    })?;

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
