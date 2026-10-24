#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Transient,
    PidUnavailable,
    VehicleUnavailable,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ObdError {
    #[error("adapter could not connect to the vehicle")]
    UnableToConnect,
    #[error("vehicle returned no data for this PID")]
    NoData,
    #[error("adapter did not understand the command")]
    UnknownCommand,
    #[error("malformed response: {0:?}")]
    Malformed(String),
    #[error("bus error: {0}")]
    BusError(String),
    #[error("expected {expected} data bytes, got {got}")]
    NotEnoughData { expected: usize, got: usize },
}

impl ObdError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            ObdError::UnableToConnect => ErrorKind::VehicleUnavailable,
            ObdError::NoData | ObdError::UnknownCommand => ErrorKind::PidUnavailable,
            ObdError::Malformed(_) | ObdError::BusError(_) | ObdError::NotEnoughData { .. } => {
                ErrorKind::Transient
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_unable_to_connect_when_kind_then_returns_vehicle_unavailable() {
        let expected: ErrorKind = ErrorKind::VehicleUnavailable;
        let actual: ErrorKind = ObdError::UnableToConnect.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_kind_then_returns_pid_unavailable() {
        let expected: ErrorKind = ErrorKind::PidUnavailable;
        let actual: ErrorKind = ObdError::NoData.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_kind_then_returns_pid_unavailable() {
        let expected: ErrorKind = ErrorKind::PidUnavailable;
        let actual: ErrorKind = ObdError::UnknownCommand.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_malformed_when_kind_then_returns_transient() {
        let expected: ErrorKind = ErrorKind::Transient;
        let actual: ErrorKind = ObdError::Malformed(String::new()).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_bus_error_when_kind_then_returns_transient() {
        let expected: ErrorKind = ErrorKind::Transient;
        let actual: ErrorKind = ObdError::BusError(String::new()).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_not_enough_data_when_kind_then_returns_transient() {
        let expected: ErrorKind = ErrorKind::Transient;
        let actual: ErrorKind = ObdError::NotEnoughData {
            expected: 2,
            got: 1,
        }
        .kind();

        assert_eq!(actual, expected);
    }
}
