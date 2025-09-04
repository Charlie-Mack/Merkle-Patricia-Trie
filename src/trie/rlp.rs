use std::fmt;

#[derive(Debug, PartialEq, Clone)]
pub enum RlpData {
    String(Vec<u8>),
    List(Vec<RlpData>),
}

/// Errors that can occur during RLP decoding
#[derive(Debug, PartialEq, Clone)]
pub enum RlpError {
    /// Input data is empty when it shouldn't be
    EmptyInput,
    /// Not enough bytes to decode the expected structure
    InsufficientData { expected: usize, actual: usize },
    /// Length encoding is invalid (e.g., leading zeros in multi-byte length)
    InvalidLengthEncoding,
    /// Length value exceeds maximum allowed (2^64)
    LengthTooLarge,
    /// Trailing bytes after decoding a complete RLP structure
    TrailingBytes { decoded: usize, total: usize },
}

impl fmt::Display for RlpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RlpError::EmptyInput => write!(f, "Empty input data"),
            RlpError::InsufficientData { expected, actual } => {
                write!(
                    f,
                    "Insufficient data: expected {} bytes, got {}",
                    expected, actual
                )
            }
            RlpError::InvalidLengthEncoding => write!(f, "Invalid length encoding"),
            RlpError::LengthTooLarge => write!(f, "Length exceeds maximum allowed value"),
            RlpError::TrailingBytes { decoded, total } => {
                write!(
                    f,
                    "Trailing bytes: decoded {} bytes, total {}",
                    decoded, total
                )
            }
        }
    }
}

impl std::error::Error for RlpError {}

pub type RlpResult<T> = Result<T, RlpError>;

/// Convert a length value to its minimal big-endian byte representation (no leading zeros)
fn length_to_minimal_bytes(len: usize) -> Vec<u8> {
    if len == 0 {
        return vec![0];
    }

    let bytes = len.to_be_bytes();
    // Find the first non-zero byte
    let first_non_zero = bytes
        .iter()
        .position(|&b| b != 0)
        .unwrap_or(bytes.len() - 1);
    bytes[first_non_zero..].to_vec()
}

fn bytes_to_integer(data: &[u8]) -> Result<usize, RlpError> {
    if data.len() == 0 {
        return Ok(0);
    }

    println!("data: {:?}", data);

    if data.len() >= 1 && data[0] == 0 {
        return Err(RlpError::InvalidLengthEncoding);
    }

    if data.len() > std::mem::size_of::<usize>() {
        return Err(RlpError::LengthTooLarge);
    }

    let mut result = 0usize;
    for &byte in data {
        result = result
            .checked_shl(8)
            .and_then(|r| r.checked_add(byte as usize))
            .ok_or(RlpError::LengthTooLarge)?;
    }
    Ok(result)
}

fn encode_length(len: usize, offset: u8) -> Vec<u8> {
    if len <= 55 {
        vec![len as u8 + offset]
    } else {
        let len_bytes = length_to_minimal_bytes(len);
        let len_of_len = len_bytes.len();

        let mut result = vec![len_of_len as u8 + offset + 55];
        result.extend_from_slice(&len_bytes);
        result
    }
}

fn decode_rlp_internal(data: &[u8]) -> RlpResult<(RlpData, usize)> {
    if data.is_empty() {
        return Err(RlpError::EmptyInput);
    }

    let prefix = data[0];

    match prefix {
        0x00..=0x7f => {
            //This is the case where we have a single byte
            Ok((RlpData::String(vec![prefix]), 1))
        }
        0x80..=0xb7 => {
            let payload_len = prefix - 0x80;

            if data.len() < (1 + payload_len) as usize {
                return Err(RlpError::InsufficientData {
                    expected: 1 + payload_len as usize,
                    actual: data.len(),
                });
            }

            Ok((
                RlpData::String(data[1..(payload_len + 1) as usize].to_vec()),
                (payload_len + 1) as usize,
            ))
        }
        // Long string (56+ bytes)
        0xb8..=0xbf => {
            let len_of_len = (prefix - 0xb7) as usize;

            if data.len() < 1 + len_of_len {
                return Err(RlpError::InsufficientData {
                    expected: 1 + len_of_len,
                    actual: data.len(),
                });
            }

            let payload_len = bytes_to_integer(&data[1..1 + len_of_len])?;
            let total_len = 1 + len_of_len + payload_len;

            if data.len() < total_len {
                return Err(RlpError::InsufficientData {
                    expected: total_len,
                    actual: data.len(),
                });
            }

            let start = 1 + len_of_len;
            Ok((
                RlpData::String(data[start..start + payload_len].to_vec()),
                total_len,
            ))
        }

        0xc0..=0xf7 => {
            let payload_len = (prefix - 0xc0) as usize;

            if data.len() < 1 + payload_len {
                return Err(RlpError::InsufficientData {
                    expected: 1 + payload_len,
                    actual: data.len(),
                });
            }

            let list_data = &data[1..1 + payload_len];
            let items = decode_list_items(list_data)?;

            Ok((RlpData::List(items), 1 + payload_len))
        }

        0xf8..=0xff => {
            let len_of_len = (prefix - 0xf7) as usize;

            if data.len() < 1 + len_of_len {
                return Err(RlpError::InsufficientData {
                    expected: 1 + len_of_len,
                    actual: data.len(),
                });
            }

            let payload_len = bytes_to_integer(&data[1..1 + len_of_len])?;
            let total_len = 1 + len_of_len + payload_len;

            if data.len() < total_len {
                return Err(RlpError::InsufficientData {
                    expected: total_len,
                    actual: data.len(),
                });
            }

            let start = 1 + len_of_len;
            let list_data = &data[start..start + payload_len];
            let items = decode_list_items(list_data)?;

            Ok((RlpData::List(items), total_len))
        }
    }
}

fn decode_list_items(data: &[u8]) -> RlpResult<Vec<RlpData>> {
    let mut items = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        let (item, consumed) = decode_rlp_internal(&data[offset..])?;
        items.push(item);
        offset += consumed;
    }

    Ok(items)
}

pub fn encode_rlp(data: &RlpData) -> Vec<u8> {
    match data {
        RlpData::String(bytes) => {
            // Single byte less than 0x80 is encoded as itself
            if bytes.len() == 1 && bytes[0] < 0x80 {
                bytes.clone()
            } else {
                let mut encoded = encode_length(bytes.len(), 0x80);
                encoded.extend_from_slice(bytes);
                encoded
            }
        }
        RlpData::List(items) => {
            // First encode all items
            let mut payload = Vec::new();
            for item in items {
                payload.extend(encode_rlp(item));
            }

            // Then prepend the length
            let mut encoded = encode_length(payload.len(), 0xc0);
            encoded.extend(payload);
            encoded
        }
    }
}

pub fn decode_rlp(data: &[u8]) -> RlpResult<RlpData> {
    if data.is_empty() {
        // Empty input decodes to empty string (per RLP spec)
        return Ok(RlpData::String(vec![]));
    }

    let (decoded, consumed) = decode_rlp_internal(data)?;

    // Check for trailing bytes (strict decoding)
    if consumed != data.len() {
        return Err(RlpError::TrailingBytes {
            decoded: consumed,
            total: data.len(),
        });
    }

    Ok(decoded)
}

impl RlpData {
    /// Check if this is a string
    pub fn is_string(&self) -> bool {
        matches!(self, RlpData::String(_))
    }

    /// Check if this is a list
    pub fn is_list(&self) -> bool {
        matches!(self, RlpData::List(_))
    }

    /// Get as string bytes if this is a string
    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            RlpData::String(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Get as list if this is a list
    pub fn as_list(&self) -> Option<&[RlpData]> {
        match self {
            RlpData::List(items) => Some(items),
            _ => None,
        }
    }

    /// Consume and return as string bytes if this is a string
    pub fn into_string(self) -> Option<Vec<u8>> {
        match self {
            RlpData::String(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Consume and return as list if this is a list
    pub fn into_list(self) -> Option<Vec<RlpData>> {
        match self {
            RlpData::List(items) => Some(items),
            _ => None,
        }
    }
}

#[cfg(test)]

mod unit_tests {
    use super::*;

    #[test]
    fn error_handling() {
        // Test empty input for internal function
        assert_eq!(decode_rlp_internal(&[]), Err(RlpError::EmptyInput));

        // Test insufficient data
        assert_eq!(
            decode_rlp(&[0x83, 0x00]), // Claims 3 bytes but only has 1
            Err(RlpError::InsufficientData {
                expected: 4,
                actual: 2,
            })
        );

        // Test trailing bytes
        assert_eq!(
            decode_rlp(&[0x00, 0xff]), // Single byte 0x00 with trailing 0xff
            Err(RlpError::TrailingBytes {
                decoded: 1,
                total: 2,
            })
        );

        // Test invalid length encoding (leading zeros)
        let invalid = vec![0xb8, 0x00]; // Long string with length 0 (should use short form)
        assert!(matches!(
            decode_rlp(&invalid),
            Err(RlpError::InvalidLengthEncoding)
        ));
    }

    #[test]
    fn rlp_data_methods() {
        let string_data = RlpData::String(b"hello".to_vec());
        assert!(string_data.is_string());
        assert!(!string_data.is_list());
        assert_eq!(string_data.as_string(), Some(b"hello".as_ref()));
        assert_eq!(string_data.as_list(), None);

        let list_data = RlpData::List(vec![RlpData::String(b"item".to_vec())]);
        assert!(!list_data.is_string());
        assert!(list_data.is_list());
        assert_eq!(list_data.as_string(), None);
        assert!(list_data.as_list().is_some());
    }

    #[test]
    fn round_trip_encoding_and_decoding() {
        let test_cases = vec![
            RlpData::String(vec![]),
            RlpData::String(vec![0x00]),
            RlpData::String(vec![0x7f]),
            RlpData::String(vec![0x80]),
            RlpData::String(vec![0x01; 55]),
            RlpData::String(vec![0x01; 56]),
            RlpData::String(vec![0x01; 1024]),
            RlpData::List(vec![]),
            RlpData::List(vec![
                RlpData::String(b"cat".to_vec()),
                RlpData::String(b"dog".to_vec()),
            ]),
            RlpData::List(vec![
                RlpData::List(vec![RlpData::String(b"nested".to_vec())]),
                RlpData::String(b"data".to_vec()),
            ]),
        ];

        for original in test_cases {
            let encoded = encode_rlp(&original);
            let decoded = decode_rlp(&encoded).unwrap();
            assert_eq!(decoded, original, "Round trip failed for {:?}", original);
        }
    }
}
