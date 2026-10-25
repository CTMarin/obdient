//! Integration tests against ELM327-emulator over TCP.
//!
//! Start the emulator, then run the ignored tests:
//!     elm -n 35000
//!     cargo test --test emulator_tcp -- --ignored

use std::time::Duration;

use rustobd::{
    error::TransportError,
    transport::{Transport, tcp::TcpTransport},
};

const EMULATOR_ADDRESS: &str = "127.0.0.1:35000";
const READ_TIMEOUT: Duration = Duration::from_secs(2);

// Test-only framing helper; the real one will live in the Elm327 layer.
fn send_command(transport: &mut TcpTransport, command: &str) -> String {
    transport
        .write_all(format!("{command}\r").as_bytes())
        .expect("failed to write command to emulator");

    let mut response = String::new();
    let mut buffer = [0u8; 256];
    while !response.contains('>') {
        let bytes_read = transport
            .read(&mut buffer)
            .expect("failed to read response from emulator");
        response.push_str(&String::from_utf8_lossy(&buffer[..bytes_read]));
    }
    response
}

fn connect() -> TcpTransport {
    TcpTransport::connect(EMULATOR_ADDRESS, READ_TIMEOUT)
        .expect("failed to connect to emulator at 127.0.0.1:35000")
}

#[test]
#[ignore]
fn given_emulator_when_sending_atz_then_response_contains_elm327() {
    let mut transport = connect();

    let response = send_command(&mut transport, "ATZ");

    assert!(
        response.contains("ELM327"),
        "unexpected response: {response:?}"
    );
}

#[test]
#[ignore]
fn given_echo_off_when_requesting_engine_rpm_then_response_contains_41_0c() {
    let mut transport = connect();

    send_command(&mut transport, "ATE0");
    let response = send_command(&mut transport, "010C");

    assert!(
        response.contains("41 0C"),
        "unexpected response: {response:?}"
    );
}

#[test]
#[ignore]
fn given_reconnected_transport_when_sending_atz_then_response_contains_elm327() {
    let mut transport = connect();

    transport
        .reconnect()
        .expect("failed to reconnect to emulator");
    let response = send_command(&mut transport, "ATZ");

    assert!(
        response.contains("ELM327"),
        "unexpected response: {response:?}"
    );
}

#[test]
#[ignore]
fn given_no_pending_data_when_reading_then_returns_timeout() {
    let mut transport = connect();
    send_command(&mut transport, "ATZ");

    let mut buffer = [0u8; 256];
    let result = transport.read(&mut buffer);

    assert_eq!(result, Err(TransportError::Timeout));
}
