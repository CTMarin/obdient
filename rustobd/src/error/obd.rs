use crate::error::transport::TransportError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObdErrorKind {
    Transient,
    PidUnavailable,
    VehicleUnavailable,
    ConnectionLost,
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
    #[error("transport error: {0}")]
    Transport(#[from] TransportError),
}

impl ObdError {
    pub fn kind(&self) -> ObdErrorKind {
        match self {
            ObdError::UnableToConnect => ObdErrorKind::VehicleUnavailable,
            ObdError::NoData | ObdError::UnknownCommand => ObdErrorKind::PidUnavailable,
            ObdError::Malformed(_)
            | ObdError::BusError(_)
            | ObdError::NotEnoughData { .. }
            | ObdError::Transport(TransportError::Timeout) => ObdErrorKind::Transient,
            ObdError::Transport(TransportError::Disconnected)
            | ObdError::Transport(TransportError::Io(_)) => ObdErrorKind::ConnectionLost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_unable_to_connect_when_kind_then_returns_vehicle_unavailable() {
        let expected: ObdErrorKind = ObdErrorKind::VehicleUnavailable;
        let actual: ObdErrorKind = ObdError::UnableToConnect.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_kind_then_returns_pid_unavailable() {
        let expected: ObdErrorKind = ObdErrorKind::PidUnavailable;
        let actual: ObdErrorKind = ObdError::NoData.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_unknown_command_when_kind_then_returns_pid_unavailable() {
        let expected: ObdErrorKind = ObdErrorKind::PidUnavailable;
        let actual: ObdErrorKind = ObdError::UnknownCommand.kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_malformed_when_kind_then_returns_transient() {
        let expected: ObdErrorKind = ObdErrorKind::Transient;
        let actual: ObdErrorKind = ObdError::Malformed(String::new()).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_bus_error_when_kind_then_returns_transient() {
        let expected: ObdErrorKind = ObdErrorKind::Transient;
        let actual: ObdErrorKind = ObdError::BusError(String::new()).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_not_enough_data_when_kind_then_returns_transient() {
        let expected: ObdErrorKind = ObdErrorKind::Transient;
        let actual: ObdErrorKind = ObdError::NotEnoughData {
            expected: 2,
            got: 1,
        }
        .kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_timeout_when_kind_then_returns_transient() {
        let expected: ObdErrorKind = ObdErrorKind::Transient;
        let actual: ObdErrorKind = ObdError::Transport(TransportError::Timeout).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_disconnected_when_kind_then_returns_connection_lost() {
        let expected: ObdErrorKind = ObdErrorKind::ConnectionLost;
        let actual: ObdErrorKind = ObdError::Transport(TransportError::Disconnected).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_io_error_when_kind_then_returns_connection_lost() {
        let expected: ObdErrorKind = ObdErrorKind::ConnectionLost;
        let actual: ObdErrorKind =
            ObdError::Transport(TransportError::Io(std::io::ErrorKind::ConnectionRefused)).kind();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_error_when_converting_then_wraps_in_transport_variant() {
        let expected: ObdError = ObdError::Transport(TransportError::Timeout);
        let actual: ObdError = ObdError::from(TransportError::Timeout);

        assert_eq!(actual, expected);
    }
}
