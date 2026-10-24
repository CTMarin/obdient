use crate::error::ObdError;

const BUS_ERRORS: [&str; 7] = [
    "BUS INIT",
    "BUS ERROR",
    "CAN ERROR",
    "BUFFER FULL",
    "DATA ERROR",
    "FB ERROR",
    "STOPPED",
];

#[derive(Debug, Clone, Copy)]
pub enum Pid {
    EngineCoolantTemperature,
    ThrottlePosition,
    EngineSpeed,
    VehicleSpeed,
}

impl Pid {
    pub fn code(self) -> u8 {
        match self {
            Pid::EngineCoolantTemperature => 0x05,
            Pid::ThrottlePosition => 0x11,
            Pid::EngineSpeed => 0x0C,
            Pid::VehicleSpeed => 0x0D,
        }
    }

    pub fn data_len(self) -> usize {
        match self {
            Pid::EngineSpeed => 2,
            Pid::EngineCoolantTemperature | Pid::ThrottlePosition | Pid::VehicleSpeed => 1,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Reading {
    /// Degrees Celsius
    EngineCoolantTemperature(i16),
    /// %
    ThrottlePosition(f32),
    /// RPM
    EngineSpeed(f32),
    /// km/h
    VehicleSpeed(u8),
}

pub fn parse_response(raw: &str, pid: Pid) -> Result<Vec<u8>, ObdError> {
    raw.split('\r')
        .find_map(|line| parse_line(line, pid))
        .unwrap_or_else(|| Err(ObdError::Malformed(raw.to_string())))
}

fn parse_line(line: &str, pid: Pid) -> Option<Result<Vec<u8>, ObdError>> {
    let clean_line: &str = line.trim().trim_end_matches('>');
    match clean_line {
        "NO DATA" => Some(Err(ObdError::NoData)),
        "?" => Some(Err(ObdError::UnknownCommand)),
        "UNABLE TO CONNECT" => Some(Err(ObdError::UnableToConnect)),
        "SEARCHING..." | "" => None,
        l if BUS_ERRORS.iter().any(|e| l.starts_with(e)) => {
            Some(Err(ObdError::BusError(l.to_string())))
        }
        data => parse_data_line(data, pid).map(Ok),
    }
}

fn parse_data_line(line: &str, pid: Pid) -> Option<Vec<u8>> {
    let hex: String = Some(line.split_whitespace().collect::<String>())
        .filter(|h| h.is_ascii() && h.len().is_multiple_of(2))?;

    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .ok()?;

    match bytes.as_slice() {
        [0x41, pid_code, rest @ ..] if *pid_code == pid.code() => Some(rest.to_vec()),
        _ => None,
    }
}

pub fn decode(pid: Pid, data: &[u8]) -> Result<Reading, ObdError> {
    match (pid, data) {
        (Pid::ThrottlePosition, [a, ..]) => {
            Ok(Reading::ThrottlePosition(100.0 * f32::from(*a) / 255.0))
        }
        (Pid::EngineCoolantTemperature, [a, ..]) => {
            Ok(Reading::EngineCoolantTemperature(i16::from(*a) - 40))
        }
        (Pid::EngineSpeed, [a, b, ..]) => Ok(Reading::EngineSpeed(
            (256.0 * f32::from(*a) + f32::from(*b)) / 4.0,
        )),
        (Pid::VehicleSpeed, [a, ..]) => Ok(Reading::VehicleSpeed(*a)),
        _ => Err(ObdError::NotEnoughData {
            expected: pid.data_len(),
            got: data.len(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_usual_engine_speed_when_decode_then_returns_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineSpeed(1303.75));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineSpeed, &[0x14, 0x5F]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_from_log_when_decode_then_returns_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineSpeed(740.0));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineSpeed, &[0x0B, 0x90]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_engine_speed_when_decode_then_returns_zero() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineSpeed(0.0));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineSpeed, &[0x00, 0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_engine_speed_when_decode_then_returns_maximum() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineSpeed(16383.75));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineSpeed, &[0xFF, 0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_with_one_byte_when_decode_then_returns_not_enough_data() {
        let expected: Result<Reading, ObdError> = Err(ObdError::NotEnoughData {
            expected: 2,
            got: 1,
        });
        let actual: Result<Reading, ObdError> = decode(Pid::EngineSpeed, &[0x14]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_usual_vehicle_speed_when_decode_then_returns_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::VehicleSpeed(10));
        let actual: Result<Reading, ObdError> = decode(Pid::VehicleSpeed, &[0x0A]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_vehicle_speed_when_decode_then_returns_maximum() {
        let expected: Result<Reading, ObdError> = Ok(Reading::VehicleSpeed(255));
        let actual: Result<Reading, ObdError> = decode(Pid::VehicleSpeed, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_vehicle_speed_with_extra_bytes_when_decode_then_ignores_them() {
        let expected: Result<Reading, ObdError> = Ok(Reading::VehicleSpeed(10));
        let actual: Result<Reading, ObdError> = decode(Pid::VehicleSpeed, &[0x0A, 0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_coolant_temperature_when_decode_then_returns_negative_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineCoolantTemperature(-40));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineCoolantTemperature, &[0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_coolant_temperature_at_zero_celsius_when_decode_then_returns_zero() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineCoolantTemperature(0));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineCoolantTemperature, &[0x28]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_coolant_temperature_when_decode_then_returns_maximum() {
        let expected: Result<Reading, ObdError> = Ok(Reading::EngineCoolantTemperature(215));
        let actual: Result<Reading, ObdError> = decode(Pid::EngineCoolantTemperature, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_throttle_position_when_decode_then_returns_zero() {
        let expected: Result<Reading, ObdError> = Ok(Reading::ThrottlePosition(0.0));
        let actual: Result<Reading, ObdError> = decode(Pid::ThrottlePosition, &[0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_half_throttle_position_when_decode_then_returns_approximate_value() {
        let expected: f32 = 50.196;
        let actual: Result<Reading, ObdError> = decode(Pid::ThrottlePosition, &[0x80]);

        let Ok(Reading::ThrottlePosition(value)) = actual else {
            panic!("expected ThrottlePosition, got {actual:?}");
        };
        assert!((value - expected).abs() < 0.01);
    }

    #[test]
    fn given_maximum_throttle_position_when_decode_then_returns_maximum() {
        let expected: Result<Reading, ObdError> = Ok(Reading::ThrottlePosition(100.0));
        let actual: Result<Reading, ObdError> = decode(Pid::ThrottlePosition, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_empty_data_when_decode_then_returns_not_enough_data_for_every_pid() {
        let pids = [
            Pid::EngineCoolantTemperature,
            Pid::ThrottlePosition,
            Pid::EngineSpeed,
            Pid::VehicleSpeed,
        ];

        for pid in pids {
            let expected: Result<Reading, ObdError> = Err(ObdError::NotEnoughData {
                expected: pid.data_len(),
                got: 0,
            });
            let actual: Result<Reading, ObdError> = decode(pid, &[]);

            assert_eq!(actual, expected, "{pid:?}");
        }
    }

    #[test]
    fn given_usual_response_when_parse_response_then_returns_data_bytes() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_without_spaces_when_parse_response_then_returns_data_bytes() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> = parse_response("410C145F\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_single_data_byte_response_when_parse_response_then_returns_data_byte() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x0A]);
        let actual: Result<Vec<u8>, ObdError> = parse_response("41 0D 0A \r\r>", Pid::VehicleSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_ecu_responses_when_parse_response_then_returns_first() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x0B, 0x90]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 0C 0B 90 \r41 0C 0B 90 \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_searching_prefix_when_parse_response_then_skips_it() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("SEARCHING...\r41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_echoed_command_when_parse_response_then_skips_it() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("010C\r41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_linefeeds_when_parse_response_then_returns_data_bytes() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 0C 14 5F \r\n\r\n>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_lowercase_hex_when_parse_response_then_returns_data_bytes() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x14, 0x5F]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 0c 14 5f \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_header_without_data_when_parse_response_then_returns_empty_data() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![]);
        let actual: Result<Vec<u8>, ObdError> = parse_response("41 0C \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_parse_response_then_returns_no_data() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::NoData);
        let actual: Result<Vec<u8>, ObdError> = parse_response("NO DATA\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_parse_response_then_returns_unknown_command() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::UnknownCommand);
        let actual: Result<Vec<u8>, ObdError> = parse_response("?\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unable_to_connect_when_parse_response_then_returns_unable_to_connect() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::UnableToConnect);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("UNABLE TO CONNECT\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_searching_then_unable_to_connect_when_parse_response_then_returns_unable_to_connect() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::UnableToConnect);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("SEARCHING...\rUNABLE TO CONNECT\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_bus_init_error_when_parse_response_then_returns_bus_error() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::BusError("BUS INIT: ...ERROR".to_string()));
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("BUS INIT: ...ERROR\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_for_other_pid_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::Malformed("41 0D 0A \r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("41 0D 0A \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_wrong_mode_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::Malformed("42 0C 14 5F \r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("42 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_odd_length_hex_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::Malformed("41 0C 14 5\r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("41 0C 14 5\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_invalid_hex_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::Malformed("41 0C ZZ 5F \r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 0C ZZ 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_non_ascii_bytes_when_parse_response_then_returns_malformed_without_panicking() {
        let expected: Result<Vec<u8>, ObdError> =
            Err(ObdError::Malformed("41 0C 1ñ5\r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("41 0C 1ñ5\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_only_prompt_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::Malformed("\r\r>".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_empty_input_when_parse_response_then_returns_malformed() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::Malformed("".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_stopped_when_parse_response_then_returns_bus_error() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::BusError("STOPPED".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("STOPPED\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_can_error_when_parse_response_then_returns_bus_error() {
        let expected: Result<Vec<u8>, ObdError> = Err(ObdError::BusError("CAN ERROR".to_string()));
        let actual: Result<Vec<u8>, ObdError> = parse_response("CAN ERROR\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }
}
