// Custom codec for YouTube Lounge API protocol
// Handles the format: <text length>\n<message content>\n

use bytes::BytesMut;
use tokio_util::codec::Decoder;

pub struct LoungeCodec {
    // Current parsing state
    state: LoungeCodecState,
}

enum LoungeCodecState {
    // Waiting for a line containing the size
    ReadingSize,
    // Found size, now reading content
    ReadingContent { expected_size: usize },
}

impl Default for LoungeCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl LoungeCodec {
    pub fn new() -> Self {
        Self {
            state: LoungeCodecState::ReadingSize,
        }
    }
}

impl Decoder for LoungeCodec {
    type Item = String;
    type Error = std::io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match &mut self.state {
                LoungeCodecState::ReadingSize => {
                    // Look for a newline to delimit the size
                    if let Some(newline_pos) = buf.iter().position(|&b| b == b'\n') {
                        // Extract the size line (including the newline)
                        let line = buf.split_to(newline_pos + 1);

                        // Convert to UTF-8 and trim
                        let size_str =
                            std::str::from_utf8(&line[..line.len() - 1]).map_err(|_| {
                                std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Invalid UTF-8 in size header",
                                )
                            })?;
                        let size_str = size_str.trim();

                        // Ensure it’s numeric
                        if !size_str.chars().all(|c| c.is_ascii_digit()) {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Expected numeric size, got: {}", size_str),
                            ));
                        }

                        // Parse to usize
                        let expected_size = size_str.parse::<usize>().map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("Invalid size: {}", size_str),
                            )
                        })?;

                        // Move to next state
                        self.state = LoungeCodecState::ReadingContent { expected_size };

                        // Continue loop to handle content immediately
                        continue;
                    }

                    // Not enough data for a full size line
                    return Ok(None);
                }

                LoungeCodecState::ReadingContent { expected_size } => {
                    // Wait for enough data
                    if buf.len() >= *expected_size {
                        let content = buf.split_to(*expected_size);

                        let message = String::from_utf8(content.to_vec()).map_err(|_| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Invalid UTF-8 in message content",
                            )
                        })?;

                        // Reset state
                        self.state = LoungeCodecState::ReadingSize;

                        return Ok(Some(message));
                    }

                    // Wait for more data
                    return Ok(None);
                }
            }
        }
    }
}
