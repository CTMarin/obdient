use std::time::{Duration, Instant};

use crate::{
    error::{ObdError, TransportError},
    pid::{Pid, Reading, decode, parse_response},
    transport::Transport,
};

const MAX_RESPONSE_LEN: usize = 256;
const READ_CHUNK_SIZE: usize = 64;
const INIT_SEQUENCE: [(&str, &str); 5] = [
    ("ATZ", "ELM327"),
    ("ATE0", "OK"),
    ("ATL0", "OK"),
    ("ATS0", "OK"),
    ("ATH0", "OK"),
];

pub struct Elm327<T: Transport> {
    transport: T,
    response_timeout: Duration,
    desynced: bool,
}

impl<T: Transport> Elm327<T> {
    pub fn new(transport: T, response_timeout: Duration) -> Self {
        Self {
            transport,
            response_timeout,
            desynced: false,
        }
    }

    pub fn initialize(&mut self) -> Result<(), ObdError> {
        for (command, expected_answer) in INIT_SEQUENCE {
            let response: String = self.send_command(command)?;
            if !response.contains(expected_answer) {
                return Err(ObdError::UnexpectedResponse {
                    command: command.to_owned(),
                    response,
                });
            }
        }

        Ok(())
    }

    pub fn read_pid(&mut self, pid: Pid) -> Result<Reading, ObdError> {
        let command: String = format!("01{:02X}", pid.code());
        let raw_response: String = self.send_command(&command)?;
        let reading_result: Result<Reading, ObdError> =
            parse_response(&raw_response, pid).and_then(|data| decode(pid, &data));

        if matches!(
            reading_result,
            Err(ObdError::Malformed(_) | ObdError::NotEnoughData { .. })
        ) {
            self.desynced = true;
        }

        reading_result
    }

    pub fn send_command(&mut self, command: &str) -> Result<String, ObdError> {
        if self.desynced {
            self.drain()?;
        }
        let response: Result<String, ObdError> = self.exchange(command);
        self.desynced = response.is_err();

        response
    }

    fn drain(&mut self) -> Result<(), ObdError> {
        let deadline: Instant = Instant::now() + self.response_timeout;
        loop {
            match self.transport.read(&mut [0u8; READ_CHUNK_SIZE]) {
                Ok(_) => {}
                Err(TransportError::Timeout) => return Ok(()),
                Err(error) => return Err(error.into()),
            }
            if Instant::now() >= deadline {
                return Err(ObdError::ResponseTimeout(String::new()));
            }
        }
    }

    fn as_text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    fn exchange(&mut self, command: &str) -> Result<String, ObdError> {
        let mut buf: [u8; READ_CHUNK_SIZE] = [0u8; READ_CHUNK_SIZE];

        self.transport
            .write_all(format!("{command}\r").as_bytes())?;
        let deadline: Instant = Instant::now() + self.response_timeout;
        let mut response_bytes: Vec<u8> = vec![];

        loop {
            match self.transport.read(&mut buf) {
                Ok(read_bytes_len) => response_bytes.extend_from_slice(&buf[..read_bytes_len]),
                Err(TransportError::Timeout) => {}
                Err(error) => return Err(error.into()),
            }
            if response_bytes.contains(&b'>') {
                return Ok(Self::as_text(&response_bytes));
            }
            if response_bytes.len() > MAX_RESPONSE_LEN {
                return Err(ObdError::Malformed(Self::as_text(&response_bytes)));
            }
            if Instant::now() >= deadline {
                return Err(ObdError::ResponseTimeout(Self::as_text(&response_bytes)));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn transport(&self) -> &T {
        &self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::MockTransport;

    fn elm(
        reads: Vec<Result<Vec<u8>, TransportError>>,
        response_timeout: Duration,
    ) -> Elm327<MockTransport> {
        Elm327::new(MockTransport::new(reads), response_timeout)
    }

    fn one_second() -> Duration {
        Duration::from_secs(1)
    }

    fn two_failures_then_nothing() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![
            Err(TransportError::Disconnected),
            Err(TransportError::Disconnected),
        ]
    }

    fn late_answer_script() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![
            Err(TransportError::Disconnected),
            Ok(b"41 0C 14 5F \r\r>".to_vec()),
            Err(TransportError::Timeout),
            Ok(b"41 0D 0A \r\r>".to_vec()),
        ]
    }

    #[test]
    fn given_complete_response_when_send_command_then_returns_response_text() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 5F \r\r>".to_vec())], one_second());

        let expected: Result<String, ObdError> = Ok("41 0C 14 5F \r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_complete_response_when_send_command_then_writes_command_with_carriage_return() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 5F \r\r>".to_vec())], one_second());
        let _ = elm.send_command("010C");

        let expected: &[u8] = b"010C\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_split_in_chunks_when_send_command_then_returns_joined_response() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![Ok(b"41 0C".to_vec()), Ok(b" 14 5F \r\r>".to_vec())],
            one_second(),
        );

        let expected: Result<String, ObdError> = Ok("41 0C 14 5F \r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_timeout_before_response_when_send_command_then_keeps_waiting() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![Err(TransportError::Timeout), Ok(b"OK\r\r>".to_vec())],
            one_second(),
        );

        let expected: Result<String, ObdError> = Ok("OK\r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("ATE0");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_disconnected_transport_when_send_command_then_returns_transport_error() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Err(TransportError::Disconnected)], one_second());

        let expected: Result<String, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_invalid_utf8_bytes_when_send_command_then_returns_replacement_character() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(vec![0xFF, b'>'])], one_second());

        let expected: Result<String, ObdError> = Ok("\u{FFFD}>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_bytes_after_prompt_in_same_chunk_when_send_command_then_keeps_them() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"OK\r\r>junk".to_vec())], one_second());

        let expected: Result<String, ObdError> = Ok("OK\r\r>junk".to_string());
        let actual: Result<String, ObdError> = elm.send_command("ATE0");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_zero_timeout_and_partial_response_when_send_command_then_returns_response_timeout_with_partial()
     {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"41 0C".to_vec())], Duration::ZERO);

        let expected: Result<String, ObdError> =
            Err(ObdError::ResponseTimeout("41 0C".to_string()));
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_zero_timeout_and_no_data_when_send_command_then_returns_response_timeout_with_empty_text()
     {
        let mut elm: Elm327<MockTransport> = elm(vec![], Duration::ZERO);

        let expected: Result<String, ObdError> = Err(ObdError::ResponseTimeout(String::new()));
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_zero_timeout_and_complete_response_when_send_command_then_prompt_wins_over_deadline() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"OK\r\r>".to_vec())], Duration::ZERO);

        let expected: Result<String, ObdError> = Ok("OK\r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("ATE0");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_over_limit_without_prompt_when_send_command_then_returns_malformed() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(vec![b'A'; 300])], one_second());

        let expected: Result<String, ObdError> = Err(ObdError::Malformed("A".repeat(300)));
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_exactly_at_limit_without_prompt_when_send_command_then_is_not_malformed() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![Ok(vec![b'A'; 256]), Err(TransportError::Disconnected)],
            one_second(),
        );

        let expected: Result<String, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<String, ObdError> = elm.send_command("010C");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_new_elm_when_created_then_is_not_desynced() {
        let elm: Elm327<MockTransport> = elm(vec![], one_second());

        let expected: bool = false;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_failed_command_when_send_command_then_is_desynced() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Err(TransportError::Disconnected)], one_second());
        let _ = elm.send_command("010C");

        let expected: bool = true;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_previous_success_when_send_command_again_then_does_not_drain() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![Ok(b"OK\r\r>".to_vec()), Ok(b"OK\r\r>".to_vec())],
            one_second(),
        );
        let _ = elm.send_command("ATE0");

        let expected: Result<String, ObdError> = Ok("OK\r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("ATE0");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_late_answer_after_failure_when_send_command_then_drains_it_and_returns_new_response() {
        let mut elm: Elm327<MockTransport> = elm(late_answer_script(), one_second());
        let _ = elm.send_command("010C");

        let expected: Result<String, ObdError> = Ok("41 0D 0A \r\r>".to_string());
        let actual: Result<String, ObdError> = elm.send_command("010D");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_late_answer_after_failure_when_send_command_then_is_resynced() {
        let mut elm: Elm327<MockTransport> = elm(late_answer_script(), one_second());
        let _ = elm.send_command("010C");
        let _ = elm.send_command("010D");

        let expected: bool = false;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_drain_fails_when_send_command_then_returns_drain_error() {
        let mut elm: Elm327<MockTransport> = elm(two_failures_then_nothing(), one_second());
        let _ = elm.send_command("010C");

        let expected: Result<String, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<String, ObdError> = elm.send_command("010D");

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_drain_fails_when_send_command_then_does_not_write_second_command() {
        let mut elm: Elm327<MockTransport> = elm(two_failures_then_nothing(), one_second());
        let _ = elm.send_command("010C");
        let _ = elm.send_command("010D");

        let expected: &[u8] = b"010C\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_drain_fails_when_send_command_then_stays_desynced() {
        let mut elm: Elm327<MockTransport> = elm(two_failures_then_nothing(), one_second());
        let _ = elm.send_command("010C");
        let _ = elm.send_command("010D");

        let expected: bool = true;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_zero_timeout_and_endless_garbage_when_draining_then_returns_response_timeout() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![Err(TransportError::Disconnected), Ok(b"garbage".to_vec())],
            Duration::ZERO,
        );
        let _ = elm.send_command("010C");

        let expected: Result<String, ObdError> = Err(ObdError::ResponseTimeout(String::new()));
        let actual: Result<String, ObdError> = elm.send_command("010D");

        assert_eq!(actual, expected);
    }

    fn atz_answer() -> Result<Vec<u8>, TransportError> {
        Ok(b"ATZ\r\r\rELM327 v1.5\r\r>".to_vec())
    }

    fn correct_init_answers() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![
            atz_answer(),
            Ok(b"ATE0\rOK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
            Ok(b"OK\r\r>".to_vec()),
        ]
    }

    fn atz_ok_then_unknown_command() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![atz_answer(), Ok(b"?\r\r>".to_vec())]
    }

    #[test]
    fn given_correct_answers_when_initialize_then_returns_ok() {
        let mut elm: Elm327<MockTransport> = elm(correct_init_answers(), one_second());

        let expected: Result<(), ObdError> = Ok(());
        let actual: Result<(), ObdError> = elm.initialize();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_correct_answers_when_initialize_then_writes_init_sequence() {
        let mut elm: Elm327<MockTransport> = elm(correct_init_answers(), one_second());
        let _ = elm.initialize();

        let expected: &[u8] = b"ATZ\rATE0\rATL0\rATS0\rATH0\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_answer_to_atz_when_initialize_then_returns_unexpected_response() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"?\r\r>".to_vec())], one_second());

        let expected: Result<(), ObdError> = Err(ObdError::UnexpectedResponse {
            command: "ATZ".to_string(),
            response: "?\r\r>".to_string(),
        });
        let actual: Result<(), ObdError> = elm.initialize();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_answer_to_ate0_when_initialize_then_returns_unexpected_response() {
        let mut elm: Elm327<MockTransport> = elm(atz_ok_then_unknown_command(), one_second());

        let expected: Result<(), ObdError> = Err(ObdError::UnexpectedResponse {
            command: "ATE0".to_string(),
            response: "?\r\r>".to_string(),
        });
        let actual: Result<(), ObdError> = elm.initialize();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_answer_to_ate0_when_initialize_then_stops_before_atl0() {
        let mut elm: Elm327<MockTransport> = elm(atz_ok_then_unknown_command(), one_second());
        let _ = elm.initialize();

        let expected: &[u8] = b"ATZ\rATE0\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_disconnect_after_atz_when_initialize_then_returns_transport_error() {
        let mut elm: Elm327<MockTransport> = elm(
            vec![atz_answer(), Err(TransportError::Disconnected)],
            one_second(),
        );

        let expected: Result<(), ObdError> = Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<(), ObdError> = elm.initialize();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unexpected_answer_when_initialize_then_is_not_desynced() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"?\r\r>".to_vec())], one_second());
        let _ = elm.initialize();

        let expected: bool = false;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_rpm_response_when_read_pid_engine_speed_then_returns_decoded_reading() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 5F \r\r>".to_vec())], one_second());

        let expected: Result<Reading, ObdError> = Ok(Reading::EngineSpeed(1303.75));
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_rpm_response_when_read_pid_engine_speed_then_writes_mode_01_command() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 5F \r\r>".to_vec())], one_second());
        let _ = elm.read_pid(Pid::EngineSpeed);

        let expected: &[u8] = b"010C\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_coolant_response_when_read_pid_coolant_temperature_then_writes_0105_command() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 05 7B \r\r>".to_vec())], one_second());
        let _ = elm.read_pid(Pid::EngineCoolantTemperature);

        let expected: &[u8] = b"0105\r";
        let actual: &[u8] = elm.transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_response_when_read_pid_then_returns_no_data() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"NO DATA\r\r>".to_vec())], one_second());

        let expected: Result<Reading, ObdError> = Err(ObdError::NoData);
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_response_when_read_pid_then_is_not_desynced() {
        let mut elm: Elm327<MockTransport> = elm(vec![Ok(b"NO DATA\r\r>".to_vec())], one_second());
        let _ = elm.read_pid(Pid::EngineSpeed);

        let expected: bool = false;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_one_data_byte_for_rpm_when_read_pid_then_returns_not_enough_data() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 \r\r>".to_vec())], one_second());

        let expected: Result<Reading, ObdError> = Err(ObdError::NotEnoughData {
            expected: 2,
            got: 1,
        });
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_for_other_pid_when_read_pid_then_returns_malformed() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0D 0A \r\r>".to_vec())], one_second());

        let expected: Result<Reading, ObdError> =
            Err(ObdError::Malformed("41 0D 0A \r\r>".to_string()));
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_disconnected_transport_when_read_pid_then_returns_transport_error() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Err(TransportError::Disconnected)], one_second());

        let expected: Result<Reading, ObdError> =
            Err(ObdError::Transport(TransportError::Disconnected));
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    // Noise produced a '>' in the middle of the answer, so the rest stays in the line.
    fn split_answer_reads() -> Vec<Result<Vec<u8>, TransportError>> {
        vec![
            Ok(b"41 0C>".to_vec()),
            Ok(b" 14 5F \r\r>".to_vec()),
            Err(TransportError::Timeout),
            Ok(b"41 0D 0A \r\r>".to_vec()),
        ]
    }

    #[test]
    fn given_early_prompt_in_answer_when_read_pid_then_returns_not_enough_data() {
        let mut elm: Elm327<MockTransport> = elm(split_answer_reads(), one_second());

        let expected: Result<Reading, ObdError> = Err(ObdError::NotEnoughData {
            expected: 2,
            got: 0,
        });
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_early_prompt_in_answer_when_next_read_pid_then_leftover_is_drained() {
        let mut elm: Elm327<MockTransport> = elm(split_answer_reads(), one_second());
        let _ = elm.read_pid(Pid::EngineSpeed);

        let expected: Result<Reading, ObdError> = Ok(Reading::VehicleSpeed(10));
        let actual: Result<Reading, ObdError> = elm.read_pid(Pid::VehicleSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_for_other_pid_when_read_pid_then_is_desynced() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0D 0A \r\r>".to_vec())], one_second());
        let _ = elm.read_pid(Pid::EngineSpeed);

        let expected: bool = true;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_one_data_byte_for_rpm_when_read_pid_then_is_desynced() {
        let mut elm: Elm327<MockTransport> =
            elm(vec![Ok(b"41 0C 14 \r\r>".to_vec())], one_second());
        let _ = elm.read_pid(Pid::EngineSpeed);

        let expected: bool = true;
        let actual: bool = elm.desynced;

        assert_eq!(actual, expected);
    }
}
