use std::collections::VecDeque;

use crate::{error::TransportError, transport::Transport};

pub struct MockTransport {
    reads: VecDeque<Result<Vec<u8>, TransportError>>,
    written: Vec<u8>,
    reconnects: usize,
}

impl MockTransport {
    pub fn new(reads: Vec<Result<Vec<u8>, TransportError>>) -> Self {
        Self {
            reads: VecDeque::from(reads),
            written: vec![],
            reconnects: 0,
        }
    }

    pub fn written(&self) -> &[u8] {
        &self.written
    }

    pub fn reconnects(&self) -> usize {
        self.reconnects
    }
}

impl Transport for MockTransport {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        match self.reads.pop_front() {
            Some(Ok(mut chunk)) => {
                assert!(
                    !chunk.is_empty(),
                    "MockTransport: empty chunk would break the Transport contract"
                );

                let copy_len: usize = chunk.len().min(buf.len());
                let rest: Vec<u8> = chunk.split_off(copy_len);
                buf[..copy_len].copy_from_slice(&chunk);
                if !rest.is_empty() {
                    self.reads.push_front(Ok(rest));
                }

                Ok(copy_len)
            }
            Some(Err(error)) => Err(error),
            None => Err(TransportError::Timeout),
        }
    }

    fn write_all(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.written.extend_from_slice(data);
        Ok(())
    }

    fn reconnect(&mut self) -> Result<(), TransportError> {
        self.reconnects += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_chunk_smaller_than_buffer_when_read_then_returns_chunk_length() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"41 0C".to_vec())]);
        let mut buffer: [u8; 16] = [0u8; 16];

        let expected: Result<usize, TransportError> = Ok(5);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_chunk_smaller_than_buffer_when_read_then_fills_buffer_with_chunk() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"41 0C".to_vec())]);
        let mut buffer: [u8; 16] = [0u8; 16];
        transport.read(&mut buffer).unwrap();

        let expected: &[u8] = b"41 0C";
        let actual: &[u8] = &buffer[..5];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_chunk_larger_than_buffer_when_read_then_returns_buffer_length() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"ABCDEF".to_vec())]);
        let mut buffer: [u8; 4] = [0u8; 4];

        let expected: Result<usize, TransportError> = Ok(4);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_chunk_larger_than_buffer_when_read_then_fills_buffer_with_prefix() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"ABCDEF".to_vec())]);
        let mut buffer: [u8; 4] = [0u8; 4];
        transport.read(&mut buffer).unwrap();

        let expected: &[u8] = b"ABCD";
        let actual: &[u8] = &buffer;

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_chunk_larger_than_buffer_when_read_twice_then_second_read_returns_remainder_length() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"ABCDEF".to_vec())]);
        let mut buffer: [u8; 4] = [0u8; 4];
        transport.read(&mut buffer).unwrap();

        let expected: Result<usize, TransportError> = Ok(2);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_chunk_larger_than_buffer_when_read_twice_then_second_read_fills_remainder() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(b"ABCDEF".to_vec())]);
        let mut buffer: [u8; 4] = [0u8; 4];
        transport.read(&mut buffer).unwrap();
        transport.read(&mut buffer).unwrap();

        let expected: &[u8] = b"EF";
        let actual: &[u8] = &buffer[..2];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_scripted_error_when_read_then_returns_error_as_is() {
        let mut transport: MockTransport =
            MockTransport::new(vec![Err(TransportError::Disconnected)]);
        let mut buffer: [u8; 16] = [0u8; 16];

        let expected: Result<usize, TransportError> = Err(TransportError::Disconnected);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_empty_queue_when_read_then_returns_timeout() {
        let mut transport: MockTransport = MockTransport::new(vec![]);
        let mut buffer: [u8; 16] = [0u8; 16];

        let expected: Result<usize, TransportError> = Err(TransportError::Timeout);
        let actual: Result<usize, TransportError> = transport.read(&mut buffer);

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_chunks_when_read_then_first_read_returns_first_chunk() {
        let mut transport: MockTransport =
            MockTransport::new(vec![Ok(b"A".to_vec()), Ok(b"B".to_vec())]);
        let mut buffer: [u8; 16] = [0u8; 16];
        transport.read(&mut buffer).unwrap();

        let expected: &[u8] = b"A";
        let actual: &[u8] = &buffer[..1];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_chunks_when_read_twice_then_second_read_returns_second_chunk() {
        let mut transport: MockTransport =
            MockTransport::new(vec![Ok(b"A".to_vec()), Ok(b"B".to_vec())]);
        let mut buffer: [u8; 16] = [0u8; 16];
        transport.read(&mut buffer).unwrap();
        transport.read(&mut buffer).unwrap();

        let expected: &[u8] = b"B";
        let actual: &[u8] = &buffer[..1];

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_writes_when_written_then_returns_accumulated_bytes() {
        let mut transport: MockTransport = MockTransport::new(vec![]);
        transport.write_all(b"ATZ\r").unwrap();
        transport.write_all(b"ATE0\r").unwrap();

        let expected: &[u8] = b"ATZ\rATE0\r";
        let actual: &[u8] = transport.written();

        assert_eq!(actual, expected);
    }

    #[test]
    fn given_two_reconnects_when_reconnects_then_returns_two() {
        let mut transport: MockTransport = MockTransport::new(vec![]);
        transport.reconnect().unwrap();
        transport.reconnect().unwrap();

        let expected: usize = 2;
        let actual: usize = transport.reconnects();

        assert_eq!(actual, expected);
    }

    #[test]
    #[should_panic(expected = "empty chunk")]
    fn given_empty_scripted_chunk_when_read_then_panics() {
        let mut transport: MockTransport = MockTransport::new(vec![Ok(vec![])]);
        let mut buffer: [u8; 16] = [0u8; 16];

        let _ = transport.read(&mut buffer);
    }
}
