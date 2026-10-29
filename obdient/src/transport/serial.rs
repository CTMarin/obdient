use std::time::Duration;

use serialport::SerialPort;

use crate::{error::TransportError, transport::Transport};

pub struct SerialTransport {
    port: Option<Box<dyn SerialPort>>,
    path: String,
    baud_rate: u32,
    read_timeout: Duration,
}

impl SerialTransport {
    pub fn connect(
        path: String,
        baud_rate: u32,
        read_timeout: Duration,
    ) -> Result<Self, TransportError> {
        let port: Box<dyn SerialPort> = create_serial_port(&path, baud_rate, read_timeout)?;

        Ok(Self {
            port: Some(port),
            path,
            baud_rate,
            read_timeout,
        })
    }
}

impl Transport for SerialTransport {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        match self
            .port
            .as_mut()
            .ok_or(TransportError::Disconnected)?
            .read(buf)?
        {
            0 => Err(TransportError::Disconnected),
            n => Ok(n),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.port
            .as_mut()
            .ok_or(TransportError::Disconnected)?
            .write_all(data)?;
        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        self.port = None;
        self.port = Some(create_serial_port(
            &self.path,
            self.baud_rate,
            self.read_timeout,
        )?);
        Ok(())
    }
}

fn create_serial_port(
    path: &str,
    baud_rate: u32,
    read_timeout: Duration,
) -> Result<Box<dyn SerialPort>, TransportError> {
    Ok(serialport::new(path, baud_rate)
        .timeout(read_timeout)
        .open()?)
}
