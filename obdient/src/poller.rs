use std::{thread::sleep, time::Duration};

use crate::{
    elm327::Elm327,
    error::{ObdError, ObdErrorKind},
    pid::{Pid, Reading, SupportedPidsMask},
    poller::policy::RetryPolicy,
    transport::Transport,
};

pub mod policy;

#[derive(Debug, Default, PartialEq)]
pub struct CycleReport {
    pub readings: Vec<Reading>,
    pub failures: Vec<(Pid, ObdError)>,
    pub newly_unavailable: Vec<Pid>,
}

pub struct Poller<T: Transport> {
    elm327: Elm327<T>,
    active_pids: Vec<Pid>,
    read_policy: RetryPolicy,
    reconnect_policy: RetryPolicy,
}

impl<T: Transport> Poller<T> {
    pub fn new(
        elm327: Elm327<T>,
        pids: Vec<Pid>,
        read_policy: RetryPolicy,
        reconnect_policy: RetryPolicy,
    ) -> Self {
        Self {
            elm327,
            active_pids: pids,
            read_policy,
            reconnect_policy,
        }
    }

    pub fn has_active_pids(&self) -> bool {
        !self.active_pids.is_empty()
    }

    pub fn poll_cycle(&mut self) -> Result<CycleReport, ObdError> {
        let mut report: CycleReport = CycleReport::default();

        for pid in self.active_pids.clone() {
            match self.read_with_retry(pid) {
                Ok(reading) => report.readings.push(reading),
                Err(error) => match error.kind() {
                    ObdErrorKind::PidUnavailable if error == ObdError::UnknownCommand => {
                        report.newly_unavailable.push(pid);
                    }
                    ObdErrorKind::Transient | ObdErrorKind::PidUnavailable => {
                        report.failures.push((pid, error));
                    }
                    ObdErrorKind::VehicleUnavailable | ObdErrorKind::ConnectionLost => {
                        return Err(error);
                    }
                },
            }
        }

        let any_no_data: bool = report
            .failures
            .iter()
            .any(|(_, error)| *error == ObdError::NoData);
        if report.readings.is_empty() && any_no_data {
            return Err(ObdError::NoVehicleResponse);
        }

        self.active_pids
            .retain(|pid| !report.newly_unavailable.contains(pid));

        Ok(report)
    }

    pub fn reconnect(
        &mut self,
        mut on_failed_attempt: impl FnMut(u32, u32, &ObdError),
    ) -> Result<(), ObdError> {
        let mut attempt: u32 = 1;
        let max_attempts: u32 = self.reconnect_policy.max_attempts;

        loop {
            match self.elm327.reconnect() {
                Err(error) if attempt < self.reconnect_policy.max_attempts => {
                    on_failed_attempt(attempt, max_attempts, &error);
                    sleep(self.reconnect_policy.backoff_for(attempt - 1));
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    fn read_with_retry(&mut self, pid: Pid) -> Result<Reading, ObdError> {
        let mut attempt: u32 = 1;

        loop {
            match self.elm327.read_pid(pid) {
                Err(error)
                    if error.kind() == ObdErrorKind::Transient
                        && attempt < self.read_policy.max_attempts =>
                {
                    sleep(self.read_policy.backoff_for(attempt - 1));
                    attempt += 1;
                }
                result => return result,
            }
        }
    }
}

pub fn wait_for_supported_pids<T: Transport>(
    elm327: &mut Elm327<T>,
    retry_interval: Duration,
    mut on_waiting: impl FnMut(&ObdError),
) -> Result<SupportedPidsMask, ObdError> {
    loop {
        match elm327.read_supported_pids() {
            Ok(supported) => return Ok(supported),
            Err(error) if should_wait_for_vehicle(&error) => {
                on_waiting(&error);
                sleep(retry_interval);
            }
            Err(error) => return Err(error),
        }
    }
}

fn should_wait_for_vehicle(error: &ObdError) -> bool {
    *error == ObdError::NoData
        || matches!(
            error.kind(),
            ObdErrorKind::Transient | ObdErrorKind::VehicleUnavailable
        )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{error::TransportError, transport::mock::MockTransport};

    use super::*;

    fn poller(
        reads: Vec<Result<Vec<u8>, TransportError>>,
        pids: Vec<Pid>,
    ) -> Poller<MockTransport> {
        poller_with_reconnects(reads, vec![], pids)
    }

    fn poller_with_reconnects(
        reads: Vec<Result<Vec<u8>, TransportError>>,
        reconnect_results: Vec<Result<(), TransportError>>,
        pids: Vec<Pid>,
    ) -> Poller<MockTransport> {
        let elm327: Elm327<MockTransport> = Elm327::new(
            MockTransport::new(reads, reconnect_results),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        Poller::new(elm327, pids, zero_backoff_policy(), zero_backoff_policy())
    }

    fn zero_backoff_policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        }
    }

    #[test]
    fn given_transient_error_then_valid_response_when_poll_cycle_then_returns_reading() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"41 0C 14 5F \r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            readings: vec![Reading::EngineSpeed(1303.75)],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_exhausted_retries_when_poll_cycle_then_records_failure_and_reads_next_pid() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"41 0D 0A \r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            readings: vec![Reading::VehicleSpeed(10)],
            failures: vec![(
                Pid::EngineSpeed,
                ObdError::BusError("CAN ERROR".to_string()),
            )],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_persistent_transient_error_when_poll_cycle_then_sends_command_max_attempts_times() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed],
        );
        let _ = poller.poll_cycle();

        let expected: &[u8] = b"010C\r010C\r010C\r";
        let actual: &[u8] = poller.elm327.transport().written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unable_to_connect_when_poll_cycle_then_aborts_without_reading_next_pid() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"UNABLE TO CONNECT\r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Err(ObdError::UnableToConnect);
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(poller.elm327.transport().written(), b"010C\r");
    }

    #[test]
    fn given_disconnected_transport_when_poll_cycle_then_returns_transport_error() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Err(TransportError::Disconnected)],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<CycleReport, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_poll_cycle_then_removes_pid() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"?\r\r>".to_vec()), Ok(b"41 0D 0A \r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            readings: vec![Reading::VehicleSpeed(10)],
            newly_unavailable: vec![Pid::EngineSpeed],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(poller.active_pids, vec![Pid::VehicleSpeed]);
    }

    #[test]
    fn given_no_data_and_reading_when_poll_cycle_then_records_failure_and_keeps_pid() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"NO DATA\r\r>".to_vec()), Ok(b"41 0D 0A \r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            readings: vec![Reading::VehicleSpeed(10)],
            failures: vec![(Pid::EngineSpeed, ObdError::NoData)],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(
            poller.active_pids,
            vec![Pid::EngineSpeed, Pid::VehicleSpeed]
        );
    }

    #[test]
    fn given_all_no_data_when_poll_cycle_then_returns_no_vehicle_response_and_keeps_pids() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"NO DATA\r\r>".to_vec()), Ok(b"NO DATA\r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Err(ObdError::NoVehicleResponse);
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(
            poller.active_pids,
            vec![Pid::EngineSpeed, Pid::VehicleSpeed]
        );
    }

    #[test]
    fn given_no_data_and_exhausted_retries_when_poll_cycle_then_returns_no_vehicle_response() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"NO DATA\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Err(ObdError::NoVehicleResponse);
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(
            poller.active_pids,
            vec![Pid::EngineSpeed, Pid::VehicleSpeed]
        );
    }

    #[test]
    fn given_unknown_command_and_no_data_when_poll_cycle_then_discards_mark() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"?\r\r>".to_vec()), Ok(b"NO DATA\r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Err(ObdError::NoVehicleResponse);
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(
            poller.active_pids,
            vec![Pid::EngineSpeed, Pid::VehicleSpeed]
        );
    }

    #[test]
    fn given_all_unknown_commands_when_poll_cycle_then_removes_every_pid() {
        let mut poller: Poller<MockTransport> = poller(
            vec![Ok(b"?\r\r>".to_vec()), Ok(b"?\r\r>".to_vec())],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            newly_unavailable: vec![Pid::EngineSpeed, Pid::VehicleSpeed],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(poller.active_pids, vec![]);
    }

    #[test]
    fn given_all_retries_exhausted_when_poll_cycle_then_keeps_bus_errors_and_pids() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
                Ok(b"CAN ERROR\r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );

        let expected: Result<CycleReport, ObdError> = Ok(CycleReport {
            failures: vec![
                (
                    Pid::EngineSpeed,
                    ObdError::BusError("CAN ERROR".to_string()),
                ),
                (
                    Pid::VehicleSpeed,
                    ObdError::BusError("CAN ERROR".to_string()),
                ),
            ],
            ..CycleReport::default()
        });
        let actual: Result<CycleReport, ObdError> = poller.poll_cycle();

        assert_eq!(actual, expected);
        assert_eq!(
            poller.active_pids,
            vec![Pid::EngineSpeed, Pid::VehicleSpeed]
        );
    }

    #[test]
    fn given_removed_pid_when_next_poll_cycle_then_does_not_send_its_command() {
        let mut poller: Poller<MockTransport> = poller(
            vec![
                Ok(b"?\r\r>".to_vec()),
                Ok(b"41 0D 0A \r\r>".to_vec()),
                Ok(b"41 0D 0A \r\r>".to_vec()),
            ],
            vec![Pid::EngineSpeed, Pid::VehicleSpeed],
        );
        let _ = poller.poll_cycle();
        let _ = poller.poll_cycle();

        let expected: &[u8] = b"010C\r010D\r010D\r";
        let actual: &[u8] = poller.elm327.transport().written();

        assert_eq!(actual, expected);
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

    fn connection_refused() -> TransportError {
        TransportError::Io {
            kind: std::io::ErrorKind::ConnectionRefused,
            message: "connection refused".to_string(),
        }
    }

    fn two_failures_then_success() -> Vec<Result<(), TransportError>> {
        vec![
            Err(TransportError::Disconnected),
            Err(TransportError::Disconnected),
            Ok(()),
        ]
    }

    fn persistent_failures() -> Vec<Result<(), TransportError>> {
        vec![
            Err(TransportError::Disconnected),
            Err(TransportError::Disconnected),
            Err(connection_refused()),
        ]
    }

    fn unknown_atz_answer_then_init_answers() -> Vec<Result<Vec<u8>, TransportError>> {
        let mut reads: Vec<Result<Vec<u8>, TransportError>> = vec![Ok(b"?\r\r>".to_vec())];
        reads.extend(init_answers());
        reads
    }

    #[test]
    fn given_reconnect_succeeds_when_reconnect_then_returns_ok() {
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(init_answers(), vec![], vec![Pid::EngineSpeed]);

        let expected: Result<(), ObdError> = Ok(());
        let actual: Result<(), ObdError> = poller.reconnect(|_, _, _| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_failures_then_success_when_reconnect_then_returns_ok() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            init_answers(),
            two_failures_then_success(),
            vec![Pid::EngineSpeed],
        );

        let expected: Result<(), ObdError> = Ok(());
        let actual: Result<(), ObdError> = poller.reconnect(|_, _, _| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_failures_then_success_when_reconnect_then_attempts_three_times() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            init_answers(),
            two_failures_then_success(),
            vec![Pid::EngineSpeed],
        );
        let _ = poller.reconnect(|_, _, _| {});

        let expected: usize = 3;
        let actual: usize = poller.elm327.transport().reconnects();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_persistent_failure_when_reconnect_then_returns_last_error() {
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(vec![], persistent_failures(), vec![Pid::EngineSpeed]);

        let expected: Result<(), ObdError> = Err(ObdError::Transport(connection_refused()));
        let actual: Result<(), ObdError> = poller.reconnect(|_, _, _| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_persistent_failure_when_reconnect_then_attempts_max_attempts_times() {
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(vec![], persistent_failures(), vec![Pid::EngineSpeed]);
        let _ = poller.reconnect(|_, _, _| {});

        let expected: usize = 3;
        let actual: usize = poller.elm327.transport().reconnects();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transient_error_then_success_when_reconnect_then_returns_ok() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            init_answers(),
            vec![Err(TransportError::Timeout), Ok(())],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<(), ObdError> = Ok(());
        let actual: Result<(), ObdError> = poller.reconnect(|_, _, _| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_init_fails_then_succeeds_when_reconnect_then_returns_ok() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            unknown_atz_answer_then_init_answers(),
            vec![],
            vec![Pid::EngineSpeed],
        );

        let expected: Result<(), ObdError> = Ok(());
        let actual: Result<(), ObdError> = poller.reconnect(|_, _, _| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_init_fails_then_succeeds_when_reconnect_then_attempts_twice() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            unknown_atz_answer_then_init_answers(),
            vec![],
            vec![Pid::EngineSpeed],
        );
        let _ = poller.reconnect(|_, _, _| {});

        let expected: usize = 2;
        let actual: usize = poller.elm327.transport().reconnects();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_failures_then_success_when_reconnect_then_reports_each_failed_attempt() {
        let mut poller: Poller<MockTransport> = poller_with_reconnects(
            init_answers(),
            two_failures_then_success(),
            vec![Pid::EngineSpeed],
        );
        let mut failed_attempts: Vec<(u32, u32, ObdError)> = vec![];
        let _ = poller.reconnect(|attempt, max_attempts, error| {
            failed_attempts.push((attempt, max_attempts, error.clone()))
        });

        let expected: Vec<(u32, u32, ObdError)> = vec![
            (1, 3, ObdError::Transport(TransportError::Disconnected)),
            (2, 3, ObdError::Transport(TransportError::Disconnected)),
        ];
        let actual: Vec<(u32, u32, ObdError)> = failed_attempts;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_persistent_failure_when_reconnect_then_reports_all_but_the_returned_last_attempt() {
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(vec![], persistent_failures(), vec![Pid::EngineSpeed]);
        let mut failed_attempts: Vec<u32> = vec![];
        let _ = poller.reconnect(|attempt, _, _| failed_attempts.push(attempt));

        let expected: Vec<u32> = vec![1, 2];
        let actual: Vec<u32> = failed_attempts;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_reconnect_succeeds_first_time_when_reconnect_then_reports_no_failed_attempt() {
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(init_answers(), vec![], vec![Pid::EngineSpeed]);
        let mut failed_attempts: usize = 0;
        let _ = poller.reconnect(|_, _, _| failed_attempts += 1);

        let expected: usize = 0;
        let actual: usize = failed_attempts;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_reconnect_when_poll_cycle_then_polls_active_pids_without_reading_supported_pids() {
        let mut reads: Vec<Result<Vec<u8>, TransportError>> = init_answers();
        reads.push(Ok(b"41 0C 14 5F \r\r>".to_vec()));
        let mut poller: Poller<MockTransport> =
            poller_with_reconnects(reads, vec![], vec![Pid::EngineSpeed]);
        poller.reconnect(|_, _, _| {}).unwrap();
        let _ = poller.poll_cycle();

        let expected: &[u8] = b"ATZ\rATE0\rATL0\rATS0\rATH0\r010C\r";
        let actual: &[u8] = poller.elm327.transport().written();

        assert_eq!(actual, expected);
    }

    fn elm327(reads: Vec<Result<Vec<u8>, TransportError>>) -> Elm327<MockTransport> {
        Elm327::new(
            MockTransport::new(reads, vec![]),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
    }

    #[test]
    fn given_no_data_then_valid_response_when_wait_for_supported_pids_then_returns_mask() {
        let mut elm327: Elm327<MockTransport> = elm327(vec![
            Ok(b"NO DATA\r\r>".to_vec()),
            Ok(b"41 00 98 3A 80 12 \r\r>".to_vec()),
        ]);

        let supported: SupportedPidsMask =
            wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| {}).unwrap();

        assert!(supported.contains(Pid::EngineSpeed));
        assert_eq!(elm327.transport().written(), b"0100\r0100\r");
    }

    #[test]
    fn given_bus_error_then_valid_response_when_wait_for_supported_pids_then_returns_mask() {
        let mut elm327: Elm327<MockTransport> = elm327(vec![
            Ok(b"CAN ERROR\r\r>".to_vec()),
            Ok(b"41 00 98 3A 80 12 \r\r>".to_vec()),
        ]);

        let supported: SupportedPidsMask =
            wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| {}).unwrap();

        assert!(supported.contains(Pid::EngineSpeed));
        assert_eq!(elm327.transport().written(), b"0100\r0100\r");
    }

    #[test]
    fn given_unable_to_connect_then_valid_response_when_wait_for_supported_pids_then_returns_mask()
    {
        let mut elm327: Elm327<MockTransport> = elm327(vec![
            Ok(b"UNABLE TO CONNECT\r\r>".to_vec()),
            Ok(b"41 00 98 3A 80 12 \r\r>".to_vec()),
        ]);

        let supported: SupportedPidsMask =
            wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| {}).unwrap();

        assert!(supported.contains(Pid::EngineSpeed));
        assert_eq!(elm327.transport().written(), b"0100\r0100\r");
    }

    #[test]
    fn given_unknown_command_when_wait_for_supported_pids_then_returns_error_without_retry() {
        let mut elm327: Elm327<MockTransport> = elm327(vec![Ok(b"?\r\r>".to_vec())]);

        let expected: Result<SupportedPidsMask, ObdError> = Err(ObdError::UnknownCommand);
        let actual: Result<SupportedPidsMask, ObdError> =
            wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| {});

        assert_eq!(actual, expected);
        assert_eq!(elm327.transport().written(), b"0100\r");
    }

    #[test]
    fn given_disconnected_transport_when_wait_for_supported_pids_then_returns_transport_error() {
        let mut elm327: Elm327<MockTransport> = elm327(vec![Err(TransportError::Disconnected)]);

        let expected: Result<SupportedPidsMask, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<SupportedPidsMask, ObdError> =
            wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| {});

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_twice_then_valid_response_when_wait_for_supported_pids_then_reports_each_wait()
    {
        let mut elm327: Elm327<MockTransport> = elm327(vec![
            Ok(b"NO DATA\r\r>".to_vec()),
            Ok(b"CAN ERROR\r\r>".to_vec()),
            Ok(b"41 00 98 3A 80 12 \r\r>".to_vec()),
        ]);
        let mut waits: Vec<ObdError> = vec![];
        let _ = wait_for_supported_pids(&mut elm327, Duration::ZERO, |error| {
            waits.push(error.clone())
        });

        let expected: Vec<ObdError> = vec![
            ObdError::NoData,
            ObdError::BusError("CAN ERROR".to_string()),
        ];
        let actual: Vec<ObdError> = waits;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_valid_response_when_wait_for_supported_pids_then_reports_no_wait() {
        let mut elm327: Elm327<MockTransport> =
            elm327(vec![Ok(b"41 00 98 3A 80 12 \r\r>".to_vec())]);
        let mut waits: usize = 0;
        let _ = wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| waits += 1);

        let expected: usize = 0;
        let actual: usize = waits;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_wait_for_supported_pids_then_reports_no_wait() {
        let mut elm327: Elm327<MockTransport> = elm327(vec![Ok(b"?\r\r>".to_vec())]);
        let mut waits: usize = 0;
        let _ = wait_for_supported_pids(&mut elm327, Duration::ZERO, |_| waits += 1);

        let expected: usize = 0;
        let actual: usize = waits;

        assert_eq!(actual, expected);
    }
}
