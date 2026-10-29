//! Integration tests for `SerialTransport` using a pseudo-terminal pair as a fake adapter.
//!
//! They check the `Transport` contract (timeouts, Ok(0) -> Disconnected, reconnect)
//! against the real OS serial behaviour, so they run in CI without the emulator.
//! The pty-based tests only run on Linux: on macOS opening a pty fails with ENOTTY.

use std::time::Duration;

use obdient::{error::TransportError, transport::serial::SerialTransport};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use serialport::{SerialPort, TTYPort};

#[cfg(any(target_os = "linux", target_os = "macos"))]
const BAUD_RATE: u32 = 38400;
#[cfg(target_os = "linux")]
const SHORT_TIMEOUT: Duration = Duration::from_millis(100);
const READ_TIMEOUT: Duration = Duration::from_secs(1);

/// Returns the master end (the fake adapter) and the path of the slave end.
///
/// The slave is dropped before returning because serialport opens ports in
/// exclusive mode, so the transport must be the only one holding it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn pty() -> (TTYPort, String) {
    let (mut master, slave): (TTYPort, TTYPort) = TTYPort::pair().expect("failed to create pty");
    let slave_path: String = slave.name().expect("failed to get slave path");
    drop(slave);
    master
        .set_timeout(READ_TIMEOUT)
        .expect("failed to set master timeout");

    (master, slave_path)
}

#[cfg(target_os = "macos")]
fn io_kind<T>(result: Result<T, TransportError>) -> Option<std::io::ErrorKind> {
    match result {
        Err(TransportError::Io { kind, .. }) => Some(kind),
        _ => None,
    }
}

#[test]
fn given_nonexistent_path_when_connect_then_returns_disconnected() {
    let expected: Option<TransportError> = Some(TransportError::Disconnected);
    let actual: Option<TransportError> =
        SerialTransport::connect("/dev/obdient-missing".to_string(), 38400, READ_TIMEOUT).err();

    assert_eq!(actual, expected);
}

// Documents the ENOTTY limitation: opening a pty on macOS fails with a generic I/O error.
#[cfg(target_os = "macos")]
#[test]
fn given_macos_pty_when_connect_then_returns_io_other() {
    let (_master, slave_path) = pty();

    let expected: Option<std::io::ErrorKind> = Some(std::io::ErrorKind::Other);
    let actual: Option<std::io::ErrorKind> = io_kind(SerialTransport::connect(
        slave_path,
        BAUD_RATE,
        READ_TIMEOUT,
    ));

    assert_eq!(actual, expected);
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{Read, Write};

    use obdient::{error::TransportError, transport::Transport};

    use super::{BAUD_RATE, READ_TIMEOUT, SHORT_TIMEOUT, SerialTransport, pty};

    #[test]
    fn given_adapter_sends_bytes_when_read_then_returns_them() {
        let (mut master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, READ_TIMEOUT)
            .expect("failed to connect");
        master
            .write_all(b"41 0C 14 5F\r\r>")
            .expect("failed to write from adapter");

        let mut buffer = [0u8; 64];
        let bytes_read: usize = transport.read(&mut buffer).expect("failed to read");

        let expected: &[u8] = b"41 0C 14 5F\r\r>";
        let actual: &[u8] = &buffer[..bytes_read];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_transport_writes_when_adapter_reads_then_receives_same_bytes() {
        let (mut master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, READ_TIMEOUT)
            .expect("failed to connect");

        transport.write_all(b"010C\r").expect("failed to write");
        let mut received = [0u8; 5];
        master
            .read_exact(&mut received)
            .expect("adapter failed to read");

        let expected: &[u8] = b"010C\r";
        let actual: &[u8] = &received;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_no_data_when_read_then_returns_timeout() {
        let (_master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, SHORT_TIMEOUT)
            .expect("failed to connect");

        let mut buffer = [0u8; 64];
        let expected: Result<usize, TransportError> = Err(TransportError::Timeout);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_adapter_closed_when_read_then_returns_disconnected() {
        let (master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, READ_TIMEOUT)
            .expect("failed to connect");
        drop(master);

        let mut buffer = [0u8; 64];
        let expected: Result<usize, TransportError> = Err(TransportError::Disconnected);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_connected_when_reconnect_then_read_still_works() {
        let (mut master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, READ_TIMEOUT)
            .expect("failed to connect");
        transport.reconnect().expect("failed to reconnect");
        master
            .write_all(b"OK\r\r>")
            .expect("failed to write from adapter");

        let mut buffer = [0u8; 64];
        let bytes_read: usize = transport.read(&mut buffer).expect("failed to read");

        let expected: &[u8] = b"OK\r\r>";
        let actual: &[u8] = &buffer[..bytes_read];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_adapter_gone_when_reconnect_then_returns_disconnected() {
        let (master, slave_path) = pty();
        let mut transport = SerialTransport::connect(slave_path, BAUD_RATE, READ_TIMEOUT)
            .expect("failed to connect");
        drop(master);

        let expected: Result<(), TransportError> = Err(TransportError::Disconnected);
        let actual: Result<(), TransportError> = transport.reconnect();

        assert_eq!(actual, expected);
    }
}
