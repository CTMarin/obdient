use std::fmt;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pid {
    EngineCoolantTemperature,
    ThrottlePosition,
    EngineSpeed,
    VehicleSpeed,
    FuelTankLevel,
}

impl Pid {
    pub fn code(self) -> u8 {
        match self {
            Pid::EngineCoolantTemperature => 0x05,
            Pid::ThrottlePosition => 0x11,
            Pid::EngineSpeed => 0x0C,
            Pid::VehicleSpeed => 0x0D,
            Pid::FuelTankLevel => 0x2F,
        }
    }

    pub fn data_len(self) -> usize {
        match self {
            Pid::EngineSpeed => 2,
            Pid::EngineCoolantTemperature
            | Pid::ThrottlePosition
            | Pid::VehicleSpeed
            | Pid::FuelTankLevel => 1,
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
    /// %
    FuelTankLevel(f32),
}

impl fmt::Display for Reading {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Reading::EngineCoolantTemperature(temperature) => write!(f, "{temperature} °C"),
            Reading::ThrottlePosition(throttle) => write!(f, "{throttle:.1} %"),
            Reading::EngineSpeed(rpm) => write!(f, "{rpm:.0} rpm"),
            Reading::VehicleSpeed(speed) => write!(f, "{speed} km/h"),
            Reading::FuelTankLevel(fuel) => write!(f, "{fuel:.1} %"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SupportedPidsMask([u32; 8]);

impl SupportedPidsMask {
    pub fn contains(self, pid: Pid) -> bool {
        let offset: usize = usize::from(pid.code() - 1);
        self.0[offset / 32] & (1u32 << (31 - offset % 32)) != 0
    }

    pub fn set_range(&mut self, base_code: u8, range_mask: u32) {
        self.0[usize::from(base_code) / 0x20] = range_mask;
    }
}

pub fn parse_response(raw: &str, pid: Pid) -> Result<Vec<u8>, ObdError> {
    raw.split('\r')
        .find_map(|line| parse_line(line, pid.code()))
        .unwrap_or_else(|| Err(ObdError::Malformed(raw.to_string())))
}

pub fn parse_supported_pids(raw: &str, base_code: u8) -> Result<u32, ObdError> {
    let mut result: Option<u32> = None;

    for line in raw.split('\r') {
        if let Some(line_result) = parse_line(line, base_code) {
            let line_mask: u32 = range_mask_from_bytes(&line_result?)?;
            result = Some(result.unwrap_or(0) | line_mask);
        }
    }

    result.ok_or_else(|| ObdError::Malformed(raw.to_string()))
}

fn range_mask_from_bytes(data: &[u8]) -> Result<u32, ObdError> {
    match data {
        [a, b, c, d, ..] => Ok(u32::from_be_bytes([*a, *b, *c, *d])),
        _ => Err(ObdError::NotEnoughData {
            expected: 4,
            got: data.len(),
        }),
    }
}

fn parse_line(line: &str, code: u8) -> Option<Result<Vec<u8>, ObdError>> {
    let clean_line: &str = line.trim().trim_end_matches('>');
    match clean_line {
        "NO DATA" => Some(Err(ObdError::NoData)),
        "?" => Some(Err(ObdError::UnknownCommand)),
        "UNABLE TO CONNECT" => Some(Err(ObdError::UnableToConnect)),
        "SEARCHING..." | "" => None,
        l if BUS_ERRORS.iter().any(|e| l.starts_with(e)) => {
            Some(Err(ObdError::BusError(l.to_string())))
        }
        data => parse_data_line(data, code).map(Ok),
    }
}

fn parse_data_line(line: &str, code: u8) -> Option<Vec<u8>> {
    let hex: String = Some(line.split_whitespace().collect::<String>())
        .filter(|h| h.is_ascii() && h.len().is_multiple_of(2))?;

    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect::<Result<Vec<u8>, _>>()
        .ok()?;

    match bytes.as_slice() {
        [0x41, pid_code, rest @ ..] if *pid_code == code => Some(rest.to_vec()),
        _ => None,
    }
}

pub fn decode(pid: Pid, data: &[u8]) -> Result<Reading, ObdError> {
    match (pid, data) {
        (Pid::ThrottlePosition, [a, ..]) => {
            Ok(Reading::ThrottlePosition(100.0 * f32::from(*a) / 255.0))
        }
        (Pid::FuelTankLevel, [a, ..]) => Ok(Reading::FuelTankLevel(100.0 * f32::from(*a) / 255.0)),
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
    fn given_half_throttle_position_when_decode_then_returns_exact_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::ThrottlePosition(50.196_08));
        let actual: Result<Reading, ObdError> = decode(Pid::ThrottlePosition, &[0x80]);

        assert_eq!(actual, expected);
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
    fn given_coolant_temperature_response_when_parse_response_then_returns_data_byte() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x7B]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 05 7B \r\r>", Pid::EngineCoolantTemperature);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_throttle_position_response_when_parse_response_then_returns_data_byte() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x33]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 11 33 \r\r>", Pid::ThrottlePosition);

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

    #[test]
    fn given_engine_speed_with_fraction_when_display_then_rounds_to_integer_rpm() {
        let expected: String = "1304 rpm".to_string();
        let actual: String = Reading::EngineSpeed(1303.75).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_tie_rounding_down_when_display_then_rounds_to_even() {
        // Rust rounds ties to even
        let expected: String = "620 rpm".to_string();
        let actual: String = Reading::EngineSpeed(620.5).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_engine_speed_tie_rounding_up_when_display_then_rounds_to_even() {
        let expected: String = "622 rpm".to_string();
        let actual: String = Reading::EngineSpeed(621.5).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_vehicle_speed_when_display_then_returns_km_per_hour() {
        let expected: String = "42 km/h".to_string();
        let actual: String = Reading::VehicleSpeed(42).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_negative_coolant_temperature_when_display_then_returns_celsius() {
        let expected: String = "-40 °C".to_string();
        let actual: String = Reading::EngineCoolantTemperature(-40).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_coolant_temperature_when_display_then_returns_celsius() {
        let expected: String = "215 °C".to_string();
        let actual: String = Reading::EngineCoolantTemperature(215).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_throttle_position_with_fraction_when_display_then_returns_one_decimal() {
        let expected: String = "50.2 %".to_string();
        let actual: String = Reading::ThrottlePosition(50.196_08).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_full_throttle_when_display_then_returns_one_decimal() {
        let expected: String = "100.0 %".to_string();
        let actual: String = Reading::ThrottlePosition(100.0).to_string();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_single_ecu_when_parse_supported_pids_then_returns_mask() {
        let expected: Result<u32, ObdError> = Ok(0x983A_8013);
        let actual: Result<u32, ObdError> = parse_supported_pids("41 00 98 3A 80 13\r\r>", 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_multiple_ecus_when_parse_supported_pids_then_merges_masks() {
        let raw: &str = "41 00 98 3A 80 13\r41 00 00 00 40 00\r\r>";
        let expected: Result<u32, ObdError> = Ok(0x983A_C013);
        let actual: Result<u32, ObdError> = parse_supported_pids(raw, 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_later_range_when_parse_supported_pids_then_returns_mask() {
        let expected: Result<u32, ObdError> = Ok(0x8000_0000);
        let actual: Result<u32, ObdError> = parse_supported_pids("41 20 80 00 00 00\r\r>", 0x20);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_other_range_when_parse_supported_pids_then_returns_malformed() {
        let raw: &str = "41 20 80 00 00 00\r\r>";
        let expected: Result<u32, ObdError> = Err(ObdError::Malformed(raw.to_string()));
        let actual: Result<u32, ObdError> = parse_supported_pids(raw, 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_parse_supported_pids_then_returns_no_data() {
        let expected: Result<u32, ObdError> = Err(ObdError::NoData);
        let actual: Result<u32, ObdError> = parse_supported_pids("NO DATA\r\r>", 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_error_after_data_when_parse_supported_pids_then_returns_error() {
        let expected: Result<u32, ObdError> = Err(ObdError::UnknownCommand);
        let actual: Result<u32, ObdError> = parse_supported_pids("41 00 98 3A 80 13\r?\r\r>", 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_short_data_when_parse_supported_pids_then_returns_not_enough_data() {
        let expected: Result<u32, ObdError> = Err(ObdError::NotEnoughData {
            expected: 4,
            got: 2,
        });
        let actual: Result<u32, ObdError> = parse_supported_pids("41 00 98 3A\r\r>", 0x00);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_mask_when_contains_then_checks_pid_bit() {
        let mut supported: SupportedPidsMask = SupportedPidsMask::default();
        supported.set_range(0x00, 0x983A_8013);

        assert!(supported.contains(Pid::EngineCoolantTemperature));
        assert!(supported.contains(Pid::ThrottlePosition));
        assert!(supported.contains(Pid::EngineSpeed));
        assert!(supported.contains(Pid::VehicleSpeed));
    }

    #[test]
    fn given_empty_mask_when_contains_then_returns_false() {
        let supported: SupportedPidsMask = SupportedPidsMask::default();

        assert!(!supported.contains(Pid::EngineSpeed));
    }

    #[test]
    fn given_later_range_when_contains_then_finds_pid() {
        let mut supported: SupportedPidsMask = SupportedPidsMask::default();
        supported.set_range(0x20, 0x0002_0000);

        assert!(supported.contains(Pid::FuelTankLevel));
        assert!(!supported.contains(Pid::EngineSpeed));
    }

    #[test]
    fn given_minimum_fuel_tank_level_when_decode_then_returns_zero() {
        let expected: Result<Reading, ObdError> = Ok(Reading::FuelTankLevel(0.0));
        let actual: Result<Reading, ObdError> = decode(Pid::FuelTankLevel, &[0x00]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_half_fuel_tank_level_when_decode_then_returns_exact_value() {
        let expected: Result<Reading, ObdError> = Ok(Reading::FuelTankLevel(50.196_08));
        let actual: Result<Reading, ObdError> = decode(Pid::FuelTankLevel, &[0x80]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_maximum_fuel_tank_level_when_decode_then_returns_maximum() {
        let expected: Result<Reading, ObdError> = Ok(Reading::FuelTankLevel(100.0));
        let actual: Result<Reading, ObdError> = decode(Pid::FuelTankLevel, &[0xFF]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_fuel_tank_level_response_when_parse_response_then_returns_data_byte() {
        let expected: Result<Vec<u8>, ObdError> = Ok(vec![0x80]);
        let actual: Result<Vec<u8>, ObdError> =
            parse_response("41 2F 80 \r\r>", Pid::FuelTankLevel);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_fuel_tank_level_with_fraction_when_display_then_returns_one_decimal() {
        let expected: String = "50.2 %".to_string();
        let actual: String = Reading::FuelTankLevel(50.196_08).to_string();

        assert_eq!(actual, expected);
    }
}
