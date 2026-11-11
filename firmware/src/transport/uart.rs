use std::time::Duration;

use esp_idf_svc::{
    hal::{delay::TickType, uart::UartDriver},
    io::Write,
    sys::{ESP_ERR_TIMEOUT, EspError, TickType_t},
};
use obdient::{error::TransportError, transport::Transport};

pub struct UartTransport<'d> {
    uart: UartDriver<'d>,
    read_timeout: TickType_t,
}

impl<'d> UartTransport<'d> {
    pub fn new(uart: UartDriver<'d>, read_timeout: Duration) -> Self {
        Self {
            uart,
            read_timeout: TickType::from(read_timeout).ticks(),
        }
    }
}

impl Transport for UartTransport<'_> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        self.uart
            .read(buf, self.read_timeout)
            .map_err(transport_error)
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.uart
            .write_all(data)
            .map_err(|error| transport_error(error.0))
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        self.uart.clear_rx().map_err(transport_error)
    }
}

fn transport_error(error: EspError) -> TransportError {
    match error {
        error if error.code() == ESP_ERR_TIMEOUT => TransportError::Timeout,
        error => TransportError::Io {
            kind: std::io::ErrorKind::Other,
            message: error.to_string(),
        },
    }
}
