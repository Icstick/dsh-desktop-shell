//! Length-prefixed framing codec.
//!
//! Wire format: a `u32` little-endian payload length followed by the raw
//! payload. The codec works over any `std::io::Read` stream, which is the
//! extension point for future native carriers (Named Pipe / UDS).

use std::io::{self, Read};

/// Default maximum payload size in bytes (64 KiB, AC-IPC-002).
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Size of the length prefix in bytes.
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// Framing violations detected by the codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Declared length exceeds the configured maximum.
    Oversized { declared: u64, max: usize },
    /// EOF in the middle of a frame.
    Truncated { expected: u64, got: usize },
}

/// Result of a frame read: either a framing violation or an I/O error.
#[derive(Debug)]
pub enum FrameReadError {
    Frame(FrameError),
    Io(io::Error),
}

impl From<io::Error> for FrameReadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<FrameError> for FrameReadError {
    fn from(e: FrameError) -> Self {
        Self::Frame(e)
    }
}

/// Prefix a payload with its length (`u32` LE).
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Like [`encode_frame`], but rejects payloads above `max` (AC-IPC-002).
pub fn encode_frame_checked(payload: &[u8], max: usize) -> Result<Vec<u8>, FrameError> {
    if payload.len() > max {
        return Err(FrameError::Oversized {
            declared: payload.len() as u64,
            max,
        });
    }
    Ok(encode_frame(payload))
}

/// Read exactly one frame from `reader`.
///
/// - `Ok(None)`: clean EOF at a frame boundary (peer closed).
/// - `Err(FrameReadError::Frame(FrameError::Oversized { .. }))`: the length
///   prefix alone exceeds `max`; the caller must close the connection
///   without reading the payload.
/// - `Err(FrameReadError::Frame(FrameError::Truncated { .. }))`: EOF in the
///   middle of a frame.
pub fn read_frame(reader: &mut impl Read, max: usize) -> Result<Option<Vec<u8>>, FrameReadError> {
    let mut prefix = [0u8; LENGTH_PREFIX_BYTES];
    let got = read_some(reader, &mut prefix)?;
    if got == 0 {
        return Ok(None);
    }
    if got < LENGTH_PREFIX_BYTES {
        return Err(FrameError::Truncated {
            expected: LENGTH_PREFIX_BYTES as u64,
            got,
        }
        .into());
    }
    let declared = u32::from_le_bytes(prefix) as u64;
    if declared > max as u64 {
        return Err(FrameError::Oversized { declared, max }.into());
    }
    let mut payload = vec![0u8; declared as usize];
    let got = read_some(reader, &mut payload)?;
    if got < payload.len() {
        return Err(FrameError::Truncated {
            expected: declared,
            got,
        }
        .into());
    }
    Ok(Some(payload))
}

/// Fill `buf` as much as possible; returns the number of bytes read and
/// stops early only on EOF. I/O errors (including deadline timeouts) are
/// propagated to the caller.
fn read_some(reader: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn roundtrip_various_sizes() {
        for size in [
            0usize,
            1,
            4,
            64,
            1024,
            4096,
            MAX_FRAME_BYTES - 1,
            MAX_FRAME_BYTES,
        ] {
            let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let frame = encode_frame(&payload);
            assert_eq!(frame.len(), LENGTH_PREFIX_BYTES + size);
            let mut cursor = Cursor::new(frame);
            let decoded = read_frame(&mut cursor, MAX_FRAME_BYTES)
                .expect("read ok")
                .expect("not eof");
            assert_eq!(decoded, payload);
            assert_eq!(
                read_frame(&mut cursor, MAX_FRAME_BYTES).expect("read ok"),
                None
            );
        }
    }

    #[test]
    fn clean_eof_returns_none() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        assert_eq!(
            read_frame(&mut cursor, MAX_FRAME_BYTES).expect("read ok"),
            None
        );
    }

    #[test]
    fn partial_prefix_is_truncated() {
        let mut cursor = Cursor::new(vec![0x10u8, 0x00]);
        let err = read_frame(&mut cursor, MAX_FRAME_BYTES).expect_err("must fail");
        assert!(matches!(
            err,
            FrameReadError::Frame(FrameError::Truncated {
                expected: 4,
                got: 2
            })
        ));
    }

    #[test]
    fn partial_payload_is_truncated() {
        let payload = vec![0u8; 100];
        let mut frame = encode_frame(&payload);
        frame.truncate(LENGTH_PREFIX_BYTES + 10);
        let err = read_frame(&mut Cursor::new(frame), MAX_FRAME_BYTES).expect_err("must fail");
        assert!(matches!(
            err,
            FrameReadError::Frame(FrameError::Truncated {
                expected: 100,
                got: 10
            })
        ));
    }

    #[test]
    fn oversized_prefix_rejected_without_payload() {
        let frame = (MAX_FRAME_BYTES as u64 + 1).to_le_bytes().to_vec();
        let err = read_frame(&mut Cursor::new(frame), MAX_FRAME_BYTES).expect_err("must fail");
        assert!(matches!(
            err,
            FrameReadError::Frame(FrameError::Oversized { declared, max })
                if declared == MAX_FRAME_BYTES as u64 + 1 && max == MAX_FRAME_BYTES
        ));
    }

    #[test]
    fn encode_checked_rejects_over_max() {
        let payload = vec![0u8; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            encode_frame_checked(&payload, MAX_FRAME_BYTES),
            Err(FrameError::Oversized { .. })
        ));
        let ok =
            encode_frame_checked(&payload[..MAX_FRAME_BYTES], MAX_FRAME_BYTES).expect("at max ok");
        assert_eq!(ok.len(), LENGTH_PREFIX_BYTES + MAX_FRAME_BYTES);
    }

    #[test]
    fn length_prefix_is_little_endian() {
        let payload = vec![7u8; 0x0102_0304];
        let frame = encode_frame(&payload);
        assert_eq!(&frame[..4], &[0x04, 0x03, 0x02, 0x01]);
    }
}
