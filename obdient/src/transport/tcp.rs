use std::{
    io::{Error, ErrorKind, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use crate::{error::TransportError, transport::Transport};

#[derive(Debug)]
pub struct TcpTransport {
    tcp_stream: Option<TcpStream>,
    address: String,
    read_timeout: Duration,
    connect_timeout: Duration,
}

impl TcpTransport {
    pub fn connect(
        addr: &str,
        read_timeout: Duration,
        connect_timeout: Duration,
    ) -> Result<Self, TransportError> {
        let tcp_stream: TcpStream = create_tcp_stream(addr, read_timeout, connect_timeout)?;

        Ok(Self {
            tcp_stream: Some(tcp_stream),
            address: addr.to_string(),
            read_timeout,
            connect_timeout,
        })
    }
}

impl Transport for TcpTransport {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        match self
            .tcp_stream
            .as_mut()
            .ok_or(TransportError::Disconnected)?
            .read(buf)?
        {
            0 => Err(TransportError::Disconnected),
            n => Ok(n),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        Ok(self
            .tcp_stream
            .as_mut()
            .ok_or(TransportError::Disconnected)?
            .write_all(data)?)
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        self.tcp_stream = None;
        self.tcp_stream = Some(create_tcp_stream(
            &self.address,
            self.read_timeout,
            self.connect_timeout,
        )?);
        Ok(())
    }
}

fn create_tcp_stream(
    addr: &str,
    read_timeout: Duration,
    connect_timeout: Duration,
) -> Result<TcpStream, TransportError> {
    let mut last_error: Error = Error::new(
        ErrorKind::InvalidInput,
        "address resolved to no socket addresses",
    );
    for socket_addr in addr.to_socket_addrs().map_err(connect_error)? {
        match TcpStream::connect_timeout(&socket_addr, connect_timeout) {
            Ok(tcp_stream) => {
                tcp_stream.set_read_timeout(Some(read_timeout))?;
                return Ok(tcp_stream);
            }
            Err(error) => last_error = error,
        }
    }

    Err(connect_error(last_error))
}

fn connect_error(error: Error) -> TransportError {
    TransportError::Io {
        kind: error.kind(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_io_timed_out_when_connect_error_then_returns_io_timed_out() {
        let io_error: Error = Error::from(ErrorKind::TimedOut);
        let expected: TransportError = TransportError::Io {
            kind: ErrorKind::TimedOut,
            message: io_error.to_string(),
        };
        let actual: TransportError = connect_error(io_error);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_io_connection_reset_when_connect_error_then_returns_io_connection_reset() {
        let io_error: Error = Error::from(ErrorKind::ConnectionReset);
        let expected: TransportError = TransportError::Io {
            kind: ErrorKind::ConnectionReset,
            message: io_error.to_string(),
        };
        let actual: TransportError = connect_error(io_error);

        assert_eq!(actual, expected);
    }
}
