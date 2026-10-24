use std::convert::From;

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
}

#[derive(Debug, PartialEq)]
pub enum Reading {
    // Degrees Celsius
    EngineCoolantTemperature(i16),
    // %
    ThrottlePosition(f32),
    // RPM
    EngineSpeed(f32),
    // km/h
    VehicleSpeed(u8),
}

pub fn parse_response(raw: &str, pid: Pid) -> Option<Vec<u8>> {
    for line in raw.split('\r') {
        let clean_line: &str = line.trim().trim_end_matches('>');
        match clean_line {
            "NO DATA" | "?" | "UNABLE TO CONNECT" => return None,
            "SEARCHING..." | "" => continue,
            l if l.starts_with("BUS INIT") => return None,
            _ => {}
        }

        let hex: String = clean_line.split_whitespace().collect();
        if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
            continue;
        }

        let pairs = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect::<Result<Vec<u8>, _>>();
        let bytes: Vec<u8> = match pairs {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        match bytes.as_slice() {
            [0x41, pid_code, rest @ ..] if *pid_code == pid.code() => return Some(rest.to_vec()),
            _ => {}
        }
    }

    None
}

pub fn decode(pid: Pid, data: &[u8]) -> Option<Reading> {
    match (pid, data) {
        (Pid::ThrottlePosition, [a, ..]) => {
            Some(Reading::ThrottlePosition(100.0 * f32::from(*a) / 255.0))
        }
        (Pid::EngineCoolantTemperature, [a, ..]) => {
            Some(Reading::EngineCoolantTemperature(i16::from(*a) - 40))
        }
        (Pid::EngineSpeed, [a, b, ..]) => Some(Reading::EngineSpeed(
            (256.0 * f32::from(*a) + f32::from(*b)) / 4.0,
        )),
        (Pid::VehicleSpeed, [a, ..]) => Some(Reading::VehicleSpeed(*a)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_usual_engine_speed_when_decode_then_returns_value() {
        let expected: Option<Reading> = Some(Reading::EngineSpeed(1303.75));
        let actual: Option<Reading> = decode(Pid::EngineSpeed, &[0x14, 0x5F]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_from_log_when_decode_then_returns_value() {
        let expected: Option<Reading> = Some(Reading::EngineSpeed(740.0));
        let actual: Option<Reading> = decode(Pid::EngineSpeed, &[0x0B, 0x90]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_engine_speed_when_decode_then_returns_zero() {
        let expected: Option<Reading> = Some(Reading::EngineSpeed(0.0));
        let actual: Option<Reading> = decode(Pid::EngineSpeed, &[0x00, 0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_engine_speed_when_decode_then_returns_maximum() {
        let expected: Option<Reading> = Some(Reading::EngineSpeed(16383.75));
        let actual: Option<Reading> = decode(Pid::EngineSpeed, &[0xFF, 0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_with_one_byte_when_decode_then_returns_none() {
        let expected: Option<Reading> = None;
        let actual: Option<Reading> = decode(Pid::EngineSpeed, &[0x14]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_usual_vehicle_speed_when_decode_then_returns_value() {
        let expected: Option<Reading> = Some(Reading::VehicleSpeed(10));
        let actual: Option<Reading> = decode(Pid::VehicleSpeed, &[0x0A]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_vehicle_speed_when_decode_then_returns_maximum() {
        let expected: Option<Reading> = Some(Reading::VehicleSpeed(255));
        let actual: Option<Reading> = decode(Pid::VehicleSpeed, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_vehicle_speed_with_extra_bytes_when_decode_then_ignores_them() {
        let expected: Option<Reading> = Some(Reading::VehicleSpeed(10));
        let actual: Option<Reading> = decode(Pid::VehicleSpeed, &[0x0A, 0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_coolant_temperature_when_decode_then_returns_negative_value() {
        let expected: Option<Reading> = Some(Reading::EngineCoolantTemperature(-40));
        let actual: Option<Reading> = decode(Pid::EngineCoolantTemperature, &[0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_coolant_temperature_at_zero_celsius_when_decode_then_returns_zero() {
        let expected: Option<Reading> = Some(Reading::EngineCoolantTemperature(0));
        let actual: Option<Reading> = decode(Pid::EngineCoolantTemperature, &[0x28]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_coolant_temperature_when_decode_then_returns_maximum() {
        let expected: Option<Reading> = Some(Reading::EngineCoolantTemperature(215));
        let actual: Option<Reading> = decode(Pid::EngineCoolantTemperature, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_minimum_throttle_position_when_decode_then_returns_zero() {
        let expected: Option<Reading> = Some(Reading::ThrottlePosition(0.0));
        let actual: Option<Reading> = decode(Pid::ThrottlePosition, &[0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_half_throttle_position_when_decode_then_returns_approximate_value() {
        let expected: f32 = 50.196;
        let actual: Option<Reading> = decode(Pid::ThrottlePosition, &[0x80]);

        let Some(Reading::ThrottlePosition(value)) = actual else {
            panic!("expected ThrottlePosition, got {actual:?}");
        };
        assert!((value - expected).abs() < 0.01);
    }

    #[test]
    fn given_maximum_throttle_position_when_decode_then_returns_maximum() {
        let expected: Option<Reading> = Some(Reading::ThrottlePosition(100.0));
        let actual: Option<Reading> = decode(Pid::ThrottlePosition, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_empty_data_when_decode_then_returns_none_for_every_pid() {
        let pids = [
            Pid::EngineCoolantTemperature,
            Pid::ThrottlePosition,
            Pid::EngineSpeed,
            Pid::VehicleSpeed,
        ];

        for pid in pids {
            let expected: Option<Reading> = None;
            let actual: Option<Reading> = decode(pid, &[]);

            assert_eq!(actual, expected, "{pid:?}");
        }
    }

    #[test]
    fn given_usual_response_when_parse_response_then_returns_data_bytes() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> = parse_response("41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_without_spaces_when_parse_response_then_returns_data_bytes() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> = parse_response("410C145F\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_single_data_byte_response_when_parse_response_then_returns_data_byte() {
        let expected: Option<Vec<u8>> = Some(vec![0x0A]);
        let actual: Option<Vec<u8>> = parse_response("41 0D 0A \r\r>", Pid::VehicleSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_ecu_responses_when_parse_response_then_returns_first() {
        let expected: Option<Vec<u8>> = Some(vec![0x0B, 0x90]);
        let actual: Option<Vec<u8>> =
            parse_response("41 0C 0B 90 \r41 0C 0B 90 \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_searching_prefix_when_parse_response_then_skips_it() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> =
            parse_response("SEARCHING...\r41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_echoed_command_when_parse_response_then_skips_it() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> = parse_response("010C\r41 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_linefeeds_when_parse_response_then_returns_data_bytes() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> = parse_response("41 0C 14 5F \r\n\r\n>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_lowercase_hex_when_parse_response_then_returns_data_bytes() {
        let expected: Option<Vec<u8>> = Some(vec![0x14, 0x5F]);
        let actual: Option<Vec<u8>> = parse_response("41 0c 14 5f \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_header_without_data_when_parse_response_then_returns_empty_data() {
        let expected: Option<Vec<u8>> = Some(vec![]);
        let actual: Option<Vec<u8>> = parse_response("41 0C \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("NO DATA\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("?\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unable_to_connect_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("UNABLE TO CONNECT\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_searching_then_unable_to_connect_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> =
            parse_response("SEARCHING...\rUNABLE TO CONNECT\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_bus_init_error_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("BUS INIT: ...ERROR\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_response_for_other_pid_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("41 0D 0A \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_wrong_mode_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("42 0C 14 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_odd_length_hex_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("41 0C 14 5\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_invalid_hex_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("41 0C ZZ 5F \r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_non_ascii_bytes_when_parse_response_then_returns_none_without_panicking() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("41 0C 1ñ5\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_only_prompt_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("\r\r>", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_empty_input_when_parse_response_then_returns_none() {
        let expected: Option<Vec<u8>> = None;
        let actual: Option<Vec<u8>> = parse_response("", Pid::EngineSpeed);

        assert_eq!(actual, expected);
    }
}
