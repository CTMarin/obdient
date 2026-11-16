use crate::error::TransportError;

#[cfg(test)]
pub mod mock;
#[cfg(feature = "serial")]
pub mod serial;
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

/// Lets a caller lend a transport to a session and keep using it afterwards.
impl<T: Transport + ?Sized> Transport for &mut T {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        (**self).read(buf)
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        (**self).write_all(data)
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        (**self).reconnect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::mock::MockTransport;

    // `borrowed.read(..)` would resolve to `MockTransport::read` through auto-deref,
    // so the calls name the `&mut MockTransport` impl explicitly.

    #[test]
    fn given_borrowed_transport_when_read_then_reads_from_the_original() {
        let mut mock: MockTransport = MockTransport::new(vec![Ok(b"OK\r\r>".to_vec())], vec![]);
        let mut borrowed: &mut MockTransport = &mut mock;
        let mut buf: [u8; 8] = [0u8; 8];

        let expected: Result<usize, TransportError> = Ok(5);
        let actual: Result<usize, TransportError> =
            <&mut MockTransport as Transport>::read(&mut borrowed, &mut buf);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_borrowed_transport_when_write_all_then_writes_to_the_original() {
        let mut mock: MockTransport = MockTransport::new(vec![], vec![]);
        let mut borrowed: &mut MockTransport = &mut mock;
        let _ = <&mut MockTransport as Transport>::write_all(&mut borrowed, b"ATZ\r");

        let expected: &[u8] = b"ATZ\r";
        let actual: &[u8] = mock.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_borrowed_transport_when_reconnect_then_reconnects_the_original() {
        let mut mock: MockTransport = MockTransport::new(vec![], vec![]);
        let mut borrowed: &mut MockTransport = &mut mock;
        let _ = <&mut MockTransport as Transport>::reconnect(&mut borrowed);

        let expected: usize = 1;
        let actual: usize = mock.reconnects();

        assert_eq!(actual, expected);
    }
}
