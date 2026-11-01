use std::thread::sleep;

use crate::{
    elm327::Elm327,
    error::{ObdError, ObdErrorKind},
    pid::{Pid, Reading},
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
    policy: RetryPolicy,
}

impl<T: Transport> Poller<T> {
    pub fn new(elm327: Elm327<T>, pids: Vec<Pid>, policy: RetryPolicy) -> Self {
        Self {
            elm327,
            active_pids: pids,
            policy,
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

    fn read_with_retry(&mut self, pid: Pid) -> Result<Reading, ObdError> {
        let mut retry: u32 = 0;

        loop {
            match self.elm327.read_pid(pid) {
                Err(error)
                    if error.kind() == ObdErrorKind::Transient
                        && retry + 1 < self.policy.max_attempts =>
                {
                    sleep(self.policy.backoff_for(retry));
                    retry += 1;
                }
                result => return result,
            }
        }
    }
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
        let elm327: Elm327<MockTransport> =
            Elm327::new(MockTransport::new(reads), Duration::from_secs(1));
        let policy: RetryPolicy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
        };

        Poller::new(elm327, pids, policy)
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
}
