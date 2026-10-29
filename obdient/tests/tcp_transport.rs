//! Integration tests for `TcpTransport` using a local `TcpListener` as a fake adapter.
//!
//! They check the `Transport` contract (timeouts, Ok(0) -> Disconnected, reconnect)
//! against the real OS socket behaviour, so they run in CI without the emulator.

use std::{
    io::{Read, Write},
    net::TcpListener,
    time::Duration,
};

use obdient::{
    error::TransportError,
    transport::{Transport, tcp::TcpTransport},
};

const SHORT_TIMEOUT: Duration = Duration::from_millis(100);
const READ_TIMEOUT: Duration = Duration::from_secs(1);

fn listener() -> (TcpListener, String) {
    let listener: TcpListener =
        TcpListener::bind("127.0.0.1:0").expect("failed to bind local listener");
    let address: String = listener
        .local_addr()
        .expect("failed to get listener address")
        .to_string();

    (listener, address)
}

fn io_kind<T>(result: Result<T, TransportError>) -> Option<std::io::ErrorKind> {
    match result {
        Err(TransportError::Io { kind, .. }) => Some(kind),
        _ => None,
    }
}

#[test]
fn given_server_sends_bytes_when_reading_then_returns_those_bytes() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, READ_TIMEOUT).expect("failed to connect");
    let (mut server_socket, _) = listener.accept().expect("failed to accept");
    server_socket
        .write_all(b"41 0C 14 5F \r\r>")
        .expect("failed to write from server");

    let mut buffer = [0u8; 64];
    let bytes_read: usize = transport.read(&mut buffer).expect("failed to read");

    let expected: &[u8] = b"41 0C 14 5F \r\r>";
    let actual: &[u8] = &buffer[..bytes_read];

    assert_eq!(actual, expected);
}

#[test]
fn given_transport_when_writing_then_server_receives_exact_bytes() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, READ_TIMEOUT).expect("failed to connect");
    let (mut server_socket, _) = listener.accept().expect("failed to accept");
    server_socket
        .set_read_timeout(Some(READ_TIMEOUT))
        .expect("failed to set server read timeout");

    transport.write_all(b"010C\r").expect("failed to write");
    let mut received = [0u8; 5];
    server_socket
        .read_exact(&mut received)
        .expect("server failed to read");

    let expected: &[u8] = b"010C\r";
    let actual: &[u8] = &received;

    assert_eq!(actual, expected);
}

#[test]
fn given_silent_server_when_reading_then_returns_timeout() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, SHORT_TIMEOUT).expect("failed to connect");
    let (_server_socket, _) = listener.accept().expect("failed to accept");

    let mut buffer = [0u8; 64];
    let expected: Result<usize, TransportError> = Err(TransportError::Timeout);
    let actual: Result<usize, TransportError> = transport.read(&mut buffer);

    assert_eq!(actual, expected);
}

#[test]
fn given_server_closed_connection_when_reading_then_returns_disconnected() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, READ_TIMEOUT).expect("failed to connect");
    let (server_socket, _) = listener.accept().expect("failed to accept");
    drop(server_socket);

    let mut buffer = [0u8; 64];
    let expected: Result<usize, TransportError> = Err(TransportError::Disconnected);
    let actual: Result<usize, TransportError> = transport.read(&mut buffer);

    assert_eq!(actual, expected);
}

#[test]
fn given_reconnected_transport_when_reading_then_receives_data_from_new_connection() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, READ_TIMEOUT).expect("failed to connect");
    let (_first_socket, _) = listener.accept().expect("failed to accept first");

    let reconnect_result: Result<(), TransportError> = transport.reconnect();
    let (mut second_socket, _) = listener.accept().expect("failed to accept second");
    second_socket
        .write_all(b"OK\r\r>")
        .expect("failed to write from server");

    let mut buffer = [0u8; 64];
    let bytes_read: usize = transport.read(&mut buffer).expect("failed to read");

    let expected_reconnect: Result<(), TransportError> = Ok(());
    let expected: &[u8] = b"OK\r\r>";
    let actual: &[u8] = &buffer[..bytes_read];

    assert_eq!(reconnect_result, expected_reconnect);
    assert_eq!(actual, expected);
}

#[test]
fn given_reconnected_transport_when_server_is_silent_then_read_times_out() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, SHORT_TIMEOUT).expect("failed to connect");
    let (_first_socket, _) = listener.accept().expect("failed to accept first");
    transport.reconnect().expect("failed to reconnect");
    let (_second_socket, _) = listener.accept().expect("failed to accept second");

    let mut buffer = [0u8; 64];
    let expected: Result<usize, TransportError> = Err(TransportError::Timeout);
    let actual: Result<usize, TransportError> = transport.read(&mut buffer);

    assert_eq!(actual, expected);
}

#[test]
fn given_no_listener_when_connecting_then_returns_connection_refused() {
    let (listener, address) = listener();
    drop(listener);

    let expected: Option<std::io::ErrorKind> = Some(std::io::ErrorKind::ConnectionRefused);
    let actual: Option<std::io::ErrorKind> = io_kind(TcpTransport::connect(&address, READ_TIMEOUT));

    assert_eq!(actual, expected);
}

#[test]
fn given_zero_read_timeout_when_connect_then_returns_invalid_input() {
    let (_listener, address) = listener();

    let expected: Option<std::io::ErrorKind> = Some(std::io::ErrorKind::InvalidInput);
    let actual: Option<std::io::ErrorKind> =
        io_kind(TcpTransport::connect(&address, Duration::ZERO));

    assert_eq!(actual, expected);
}

#[test]
fn given_listener_gone_when_reconnect_then_returns_connection_refused() {
    let (listener, address) = listener();
    let mut transport = TcpTransport::connect(&address, READ_TIMEOUT).expect("failed to connect");
    let (server_socket, _) = listener.accept().expect("failed to accept");
    drop(server_socket);
    drop(listener);

    let expected: Option<std::io::ErrorKind> = Some(std::io::ErrorKind::ConnectionRefused);
    let actual: Option<std::io::ErrorKind> = io_kind(transport.reconnect());

    assert_eq!(actual, expected);
}
