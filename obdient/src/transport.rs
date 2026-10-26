use crate::error::TransportError;

#[cfg(test)]
pub mod mock;
pub mod tcp;

pub trait Transport {
    /// Reads the bytes that have arrived, at least 1.
    ///
    /// The data is a fragment of a stream: it may hold part of a response,
    /// or more than one. Never returns `Ok(0)`: a closed connection is
    /// `Err(Disconnected)`, and no data within the read timeout is `Err(Timeout)`.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;

    /// Sends all bytes of `data`, or fails. There are no partial writes.
    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError>;

    /// Closes the current connection and opens a new one with the same
    /// parameters used to create this transport.
    fn reconnect(&mut self) -> Result<(), TransportError>;
}
