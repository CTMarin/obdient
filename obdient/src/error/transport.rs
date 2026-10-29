use std::io::ErrorKind::{
    BrokenPipe, ConnectionAborted, ConnectionReset, NotConnected, TimedOut, UnexpectedEof,
    WouldBlock,
};

use serialport::ErrorKind::{InvalidInput, Io, NoDevice, Unknown};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("timed out waiting for data")]
    Timeout,
    #[error("disconnected from device")]
    Disconnected,
    #[error("I/O error ({kind:?}): {message}")]
    Io {
        kind: std::io::ErrorKind,
        message: String,
    },
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            WouldBlock | TimedOut => TransportError::Timeout,
            UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe | NotConnected => {
                TransportError::Disconnected
            }
            other => TransportError::Io {
                kind: other,
                message: error.to_string(),
            },
        }
    }
}

impl From<serialport::Error> for TransportError {
    fn from(error: serialport::Error) -> Self {
        match error.kind() {
            NoDevice | Io(std::io::ErrorKind::NotFound) => TransportError::Disconnected,
            InvalidInput => TransportError::Io {
                kind: std::io::ErrorKind::InvalidInput,
                message: error.to_string(),
            },
            Unknown => TransportError::Io {
                kind: std::io::ErrorKind::Other,
                message: error.to_string(),
            },
            Io(kind) => TransportError::from(std::io::Error::from(kind)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_io_error_would_block_when_converting_then_returns_timeout() {
        let expected: TransportError = TransportError::Timeout;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::WouldBlock));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_timed_out_when_converting_then_returns_timeout() {
        let expected: TransportError = TransportError::Timeout;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::TimedOut));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_unexpected_eof_when_converting_then_returns_disconnected() {
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_connection_reset_when_converting_then_returns_disconnected() {
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::ConnectionReset));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_connection_aborted_when_converting_then_returns_disconnected() {
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::ConnectionAborted));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_broken_pipe_when_converting_then_returns_disconnected() {
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::BrokenPipe));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_not_connected_when_converting_then_returns_disconnected() {
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::NotConnected));

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_connection_refused_when_converting_then_returns_io() {
        let io_error: std::io::Error = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        let expected: TransportError = TransportError::Io {
            kind: std::io::ErrorKind::ConnectionRefused,
            message: io_error.to_string(),
        };
        let actual: TransportError = TransportError::from(io_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_error_with_message_when_converting_then_keeps_message() {
        let io_error: std::io::Error = std::io::Error::other("Not a typewriter");
        let expected: TransportError = TransportError::Io {
            kind: std::io::ErrorKind::Other,
            message: "Not a typewriter".to_string(),
        };
        let actual: TransportError = TransportError::from(io_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_serialport_no_device_when_converting_then_returns_disconnected() {
        let serial_error: serialport::Error =
            serialport::Error::new(serialport::ErrorKind::NoDevice, "no such device");
        let expected: TransportError = TransportError::Disconnected;
        let actual: TransportError = TransportError::from(serial_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_serialport_invalid_input_when_converting_then_returns_io_invalid_input() {
        let serial_error: serialport::Error =
            serialport::Error::new(serialport::ErrorKind::InvalidInput, "invalid baud rate");
        let expected: TransportError = TransportError::Io {
            kind: std::io::ErrorKind::InvalidInput,
            message: "invalid baud rate".to_string(),
        };
        let actual: TransportError = TransportError::from(serial_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_serialport_unknown_error_when_converting_then_returns_io_other_with_description() {
        let serial_error: serialport::Error =
            serialport::Error::new(serialport::ErrorKind::Unknown, "Not a typewriter");
        let expected: TransportError = TransportError::Io {
            kind: std::io::ErrorKind::Other,
            message: "Not a typewriter".to_string(),
        };
        let actual: TransportError = TransportError::from(serial_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_serialport_io_timed_out_when_converting_then_returns_timeout() {
        let serial_error: serialport::Error = serialport::Error::new(
            serialport::ErrorKind::Io(std::io::ErrorKind::TimedOut),
            "timed out",
        );
        let expected: TransportError = TransportError::Timeout;
        let actual: TransportError = TransportError::from(serial_error);

        assert_eq!(actual, expected);
    }
}
