use obdient::{poller::CycleReport, session::SessionEvent};

pub mod transport;

pub fn log_event(event: SessionEvent) {
    match event {
        SessionEvent::Unsupported(pid) => log::warn!("PID {pid:?}: not supported by vehicle"),
        SessionEvent::Report(report) => print_report(report),
        SessionEvent::VehicleUnavailable(error) => log::warn!("{error}"),
        SessionEvent::ConnectionLost(error) => {
            log::warn!("connection lost: {error}, reconnecting")
        }
        SessionEvent::Reconnected => log::info!("reconnected"),
        SessionEvent::WaitingForVehicle(error) => log::warn!("waiting for vehicle: {error}"),
        SessionEvent::ReconnectAttemptFailed {
            attempt,
            max_attempts,
            error,
        } => log::warn!("reconnect attempt {attempt}/{max_attempts} failed: {error}"),
    }
}

fn print_report(report: &CycleReport) {
    for reading in &report.readings {
        log::info!("{reading}");
    }
    for (pid, error) in &report.failures {
        log::warn!("PID {pid:?}: {error}");
    }
    for pid in &report.newly_unavailable {
        log::warn!("PID {pid:?}: marked unavailable")
    }
    log::info!("----------------")
}
