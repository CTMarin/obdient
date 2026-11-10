use std::{thread::sleep, time::Duration};

use crate::{
    elm327::Elm327,
    error::{ObdError, ObdErrorKind},
    pid::{Pid, SupportedPidsMask},
    poller::{CycleReport, Poller, policy::RetryPolicy, wait_for_supported_pids},
    transport::Transport,
};

const DEFAULT_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFAULT_SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_CYCLE_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_VEHICLE_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const DEFAULT_RECONNECT_POLICY: RetryPolicy = RetryPolicy {
    max_attempts: 5,
    initial_backoff: Duration::from_secs(1),
    max_backoff: Duration::from_secs(8),
};
const DEFAULT_PIDS: [Pid; 5] = [
    Pid::EngineSpeed,
    Pid::VehicleSpeed,
    Pid::EngineCoolantTemperature,
    Pid::ThrottlePosition,
    Pid::FuelTankLevel,
];

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub pids: Vec<Pid>,
    pub response_timeout: Duration,
    pub search_timeout: Duration,
    pub vehicle_retry_interval: Duration,
    pub retry_policy: RetryPolicy,
    pub reconnect_policy: RetryPolicy,
    pub cycle_interval: Duration,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            pids: DEFAULT_PIDS.to_vec(),
            response_timeout: DEFAULT_RESPONSE_TIMEOUT,
            search_timeout: DEFAULT_SEARCH_TIMEOUT,
            vehicle_retry_interval: DEFAULT_VEHICLE_RETRY_INTERVAL,
            retry_policy: RetryPolicy::default(),
            reconnect_policy: DEFAULT_RECONNECT_POLICY,
            cycle_interval: DEFAULT_CYCLE_INTERVAL,
        }
    }
}

#[derive(Debug)]
pub enum SessionEvent<'a> {
    Unsupported(Pid),
    Report(&'a CycleReport),
    VehicleUnavailable(&'a ObdError),
    ConnectionLost(&'a ObdError),
    Reconnected,
    WaitingForVehicle(&'a ObdError),
    ReconnectAttemptFailed {
        attempt: u32,
        max_attempts: u32,
        error: &'a ObdError,
    },
}

pub fn run<T: Transport>(
    transport: T,
    config: &SessionConfig,
    mut on_event: impl FnMut(SessionEvent<'_>),
) -> Result<(), ObdError> {
    let mut elm327_driver: Elm327<T> =
        Elm327::new(transport, config.response_timeout, config.search_timeout);

    elm327_driver.initialize()?;

    let supported_pids_mask: SupportedPidsMask =
        wait_for_supported_pids(&mut elm327_driver, config.vehicle_retry_interval, |error| {
            on_event(SessionEvent::WaitingForVehicle(error))
        })?;
    let (supported_pids, unsupported_pids): (Vec<Pid>, Vec<Pid>) = config
        .pids
        .iter()
        .copied()
        .partition(|pid| supported_pids_mask.contains(*pid));
    for pid in unsupported_pids {
        on_event(SessionEvent::Unsupported(pid))
    }

    let mut poller: Poller<T> = Poller::new(
        elm327_driver,
        supported_pids,
        config.retry_policy,
        config.reconnect_policy,
    );

    while poller.has_active_pids() {
        match poller.poll_cycle() {
            Ok(report) => on_event(SessionEvent::Report(&report)),
            Err(error) if error.kind() == ObdErrorKind::VehicleUnavailable => {
                on_event(SessionEvent::VehicleUnavailable(&error))
            }
            Err(error) if error.kind() == ObdErrorKind::ConnectionLost => {
                on_event(SessionEvent::ConnectionLost(&error));
                poller.reconnect(|attempt, max_attempts, error| {
                    on_event(SessionEvent::ReconnectAttemptFailed {
                        attempt,
                        max_attempts,
                        error,
                    })
                })?;
                on_event(SessionEvent::Reconnected)
            }
            Err(error) => return Err(error),
        }
        sleep(config.cycle_interval);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{error::TransportError, transport::mock::MockTransport};

    use super::*;

    /// Owned copy of a `SessionEvent`, so a test can keep the events after `run` returns.
    #[derive(Debug, PartialEq)]
    enum Recorded {
        Unsupported(Pid),
        Report {
            readings: Vec<String>,
            failures: Vec<(Pid, ObdError)>,
            newly_unavailable: Vec<Pid>,
        },
        VehicleUnavailable(ObdError),
        ConnectionLost(ObdError),
        Reconnected,
        WaitingForVehicle(ObdError),
        ReconnectAttemptFailed {
            attempt: u32,
            max_attempts: u32,
            error: ObdError,
        },
    }

    impl From<SessionEvent<'_>> for Recorded {
        fn from(event: SessionEvent<'_>) -> Self {
            match event {
                SessionEvent::Unsupported(pid) => Recorded::Unsupported(pid),
                SessionEvent::Report(report) => Recorded::Report {
                    readings: report
                        .readings
                        .iter()
                        .map(|reading| format!("{reading:?}"))
                        .collect(),
                    failures: report.failures.clone(),
                    newly_unavailable: report.newly_unavailable.clone(),
                },
                SessionEvent::VehicleUnavailable(error) => {
                    Recorded::VehicleUnavailable(error.clone())
                }
                SessionEvent::ConnectionLost(error) => Recorded::ConnectionLost(error.clone()),
                SessionEvent::Reconnected => Recorded::Reconnected,
                SessionEvent::WaitingForVehicle(error) => {
                    Recorded::WaitingForVehicle(error.clone())
                }
                SessionEvent::ReconnectAttemptFailed {
                    attempt,
                    max_attempts,
                    error,
                } => Recorded::ReconnectAttemptFailed {
                    attempt,
                    max_attempts,
                    error: error.clone(),
                },
            }
        }
    }

    fn zero_backoff_policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }

    fn config(pids: Vec<Pid>) -> SessionConfig {
        SessionConfig {
            pids,
            response_timeout: Duration::from_secs(1),
            search_timeout: Duration::from_secs(1),
            vehicle_retry_interval: Duration::ZERO,
            retry_policy: zero_backoff_policy(),
            reconnect_policy: zero_backoff_policy(),
            cycle_interval: Duration::ZERO,
        }
    }

    /// Runs a session against a scripted adapter and records every event.
    /// Each script must end with `?` for every polled PID, so the session stops.
    fn run_recording(
        reads: Vec<Result<Vec<u8>, TransportError>>,
        reconnect_results: Vec<Result<(), TransportError>>,
        pids: Vec<Pid>,
    ) -> (Result<(), ObdError>, Vec<Recorded>) {
        let mut events: Vec<Recorded> = vec![];
        let result: Result<(), ObdError> = run(
            MockTransport::new(reads, reconnect_results),
            &config(pids),
            |event| events.push(Recorded::from(event)),
        );

        (result, events)
    }

    fn init_answers() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![
            Ok(b"ATZ\r\r\rELM327 v1.5\r\r>".to_vec()),
            Ok(b"ATE0\rOK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
        ]
    }

    /// `0100` answer: supports 0C (engine speed) and 0D (vehicle speed), not 2F (fuel level),
    /// and announces no later range.
    fn supported_pids_answer() -> Result<Vec<u8>, TransportError> {
        Ok(b"41 00 98 3A 80 12 \r\r>".to_vec())
    }

    fn unknown_command() -> Result<Vec<u8>, TransportError> {
        Ok(b"?\r\r>".to_vec())
    }

    fn reads(
        answers: Vec<Result<Vec<u8>, TransportError>>,
    ) -> Vec<Result<Vec<u8>, TransportError>> {
        let mut reads: Vec<Result<Vec<u8>, TransportError>> = init_answers();
        reads.push(supported_pids_answer());
        reads.extend(answers);
        reads
    }

    fn last_report_dropping(pid: Pid) -> Recorded {
        Recorded::Report {
            readings: vec![],
            failures: vec![],
            newly_unavailable: vec![pid],
        }
    }

    #[test]
    fn given_pid_answering_unknown_command_when_run_then_returns_ok() {
        let (actual, _) = run_recording(
            reads(vec![unknown_command()]),
            vec![],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<(), ObdError> = Ok(());

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unsupported_pid_in_config_when_run_then_reports_it_before_polling() {
        let (_, actual) = run_recording(
            reads(vec![Ok(b"41 0C 14 5F \r\r>".to_vec()), unknown_command()]),
            vec![],
            vec![Pid::EngineSpeed, Pid::FuelTankLevel],
        );

        let expected: Vec<Recorded> = vec![
            Recorded::Unsupported(Pid::FuelTankLevel),
            Recorded::Report {
                readings: vec!["EngineSpeed(1303.75)".to_string()],
                failures: vec![],
                newly_unavailable: vec![],
            },
            last_report_dropping(Pid::EngineSpeed),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_vehicle_not_answering_at_startup_when_run_then_reports_waiting_for_vehicle() {
        let mut reads: Vec<Result<Vec<u8>, TransportError>> = init_answers();
        reads.push(Ok(b"NO DATA\r\r>".to_vec()));
        reads.push(supported_pids_answer());
        reads.push(unknown_command());
        let (_, actual) = run_recording(reads, vec![], vec![Pid::EngineSpeed]);

        let expected: Vec<Recorded> = vec![
            Recorded::WaitingForVehicle(ObdError::NoData),
            last_report_dropping(Pid::EngineSpeed),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_in_a_cycle_when_run_then_reports_vehicle_unavailable_and_keeps_polling() {
        let (_, actual) = run_recording(
            reads(vec![Ok(b"NO DATA\r\r>".to_vec()), unknown_command()]),
            vec![],
            vec![Pid::EngineSpeed],
        );

        let expected: Vec<Recorded> = vec![
            Recorded::VehicleUnavailable(ObdError::NoVehicleResponse),
            last_report_dropping(Pid::EngineSpeed),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_connection_lost_in_a_cycle_when_run_then_reconnects_and_keeps_polling() {
        let mut answers: Vec<Result<Vec<u8>, TransportError>> =
            vec![Err(TransportError::Disconnected)];
        answers.extend(init_answers());
        answers.push(unknown_command());
        let (_, actual) = run_recording(reads(answers), vec![], vec![Pid::EngineSpeed]);

        let expected: Vec<Recorded> = vec![
            Recorded::ConnectionLost(ObdError::Transport(TransportError::Disconnected)),
            Recorded::Reconnected,
            last_report_dropping(Pid::EngineSpeed),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_failed_reconnect_attempt_when_run_then_reports_it_before_reconnected() {
        let mut answers: Vec<Result<Vec<u8>, TransportError>> =
            vec![Err(TransportError::Disconnected)];
        answers.extend(init_answers());
        answers.push(unknown_command());
        let (_, actual) = run_recording(
            reads(answers),
            vec![Err(TransportError::Disconnected), Ok(())],
            vec![Pid::EngineSpeed],
        );

        let expected: Vec<Recorded> = vec![
            Recorded::ConnectionLost(ObdError::Transport(TransportError::Disconnected)),
            Recorded::ReconnectAttemptFailed {
                attempt: 1,
                max_attempts: 3,
                error: ObdError::Transport(TransportError::Disconnected),
            },
            Recorded::Reconnected,
            last_report_dropping(Pid::EngineSpeed),
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_reconnect_attempts_exhausted_when_run_then_returns_last_error() {
        let (actual, _) = run_recording(
            reads(vec![Err(TransportError::Disconnected)]),
            vec![
                Err(TransportError::Disconnected),
                Err(TransportError::Disconnected),
                Err(TransportError::Timeout),
            ],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<(), ObdError> = Err(ObdError::Transport(TransportError::Timeout));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_reconnect_attempts_exhausted_when_run_then_does_not_report_reconnected() {
        let (_, actual) = run_recording(
            reads(vec![Err(TransportError::Disconnected)]),
            vec![
                Err(TransportError::Disconnected),
                Err(TransportError::Disconnected),
                Err(TransportError::Disconnected),
            ],
            vec![Pid::EngineSpeed],
        );

        let expected: Vec<Recorded> = vec![
            Recorded::ConnectionLost(ObdError::Transport(TransportError::Disconnected)),
            Recorded::ReconnectAttemptFailed {
                attempt: 1,
                max_attempts: 3,
                error: ObdError::Transport(TransportError::Disconnected),
            },
            Recorded::ReconnectAttemptFailed {
                attempt: 2,
                max_attempts: 3,
                error: ObdError::Transport(TransportError::Disconnected),
            },
        ];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_adapter_gone_at_startup_when_run_then_returns_error_without_events() {
        let (actual, events) = run_recording(
            vec![Err(TransportError::Disconnected)],
            vec![],
            vec![Pid::EngineSpeed],
        );

        let expected: (Result<(), ObdError>, Vec<Recorded>) = (
            Err(ObdError::Transport(TransportError::Disconnected)),
            vec![],
        );

        assert_eq!((actual, events), expected);
    }

    #[test]
    fn given_default_config_when_created_then_matches_cli_defaults() {
        let config: SessionConfig = SessionConfig::default();

        let expected = (
            vec![
                Pid::EngineSpeed,
                Pid::VehicleSpeed,
                Pid::EngineCoolantTemperature,
                Pid::ThrottlePosition,
                Pid::FuelTankLevel,
            ],
            Duration::from_secs(2),
            Duration::from_secs(15),
            Duration::from_secs(2),
            RetryPolicy::default(),
            RetryPolicy {
                max_attempts: 5,
                initial_backoff: Duration::from_secs(1),
                max_backoff: Duration::from_secs(8),
            },
            Duration::from_secs(1),
        );
        let actual = (
            config.pids,
            config.response_timeout,
            config.search_timeout,
            config.vehicle_retry_interval,
            config.retry_policy,
            config.reconnect_policy,
            config.cycle_interval,
        );

        assert_eq!(actual, expected);
    }
}
