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
}
