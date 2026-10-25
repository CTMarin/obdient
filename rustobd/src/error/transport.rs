use std::io::ErrorKind::{
    BrokenPipe, ConnectionAborted, ConnectionReset, NotConnected, TimedOut, UnexpectedEof,
    WouldBlock,
};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("timed out waiting for data")]
    Timeout,
    #[error("disconnected from device")]
    Disconnected,
    #[error("I/O error: {0}")]
    Io(std::io::ErrorKind),
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        match error.kind() {
            WouldBlock | TimedOut => TransportError::Timeout,
            UnexpectedEof | ConnectionReset | ConnectionAborted | BrokenPipe | NotConnected => {
                TransportError::Disconnected
            }
            other => TransportError::Io(other),
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
        let expected: TransportError = TransportError::Io(std::io::ErrorKind::ConnectionRefused);
        let actual: TransportError =
            TransportError::from(std::io::Error::from(std::io::ErrorKind::ConnectionRefused));

        assert_eq!(actual, expected);
    }
}
