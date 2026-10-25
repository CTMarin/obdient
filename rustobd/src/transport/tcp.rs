use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
};

use crate::{error::TransportError, transport::Transport};

#[derive(Debug)]
pub struct TcpTransport {
    tcp_stream: TcpStream,
    address: String,
    read_timeout: Duration,
}

impl TcpTransport {
    pub fn connect(addr: &str, read_timeout: Duration) -> Result<Self, TransportError> {
        let tcp_stream: TcpStream = create_tcp_stream(addr, read_timeout)?;

        Ok(Self {
            tcp_stream,
            address: addr.to_string(),
            read_timeout,
        })
    }
}

impl Transport for TcpTransport {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        match self.tcp_stream.read(buf)? {
            0 => Err(TransportError::Disconnected),
            n => Ok(n),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        Ok(self.tcp_stream.write_all(data)?)
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        self.tcp_stream = create_tcp_stream(&self.address, self.read_timeout)?;
        Ok(())
    }
}

fn create_tcp_stream(addr: &str, read_timeout: Duration) -> Result<TcpStream, TransportError> {
    let tcp_stream: TcpStream = TcpStream::connect(addr)?;
    tcp_stream.set_read_timeout(Some(read_timeout))?;

    Ok(tcp_stream)
}
