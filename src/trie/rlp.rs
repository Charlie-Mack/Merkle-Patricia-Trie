pub enum RlpData {
    String(Vec<u8>),
    List(Vec<RlpData>),
}

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

fn encode_length(l: usize, offset: u8) -> Vec<u8> {
    let mut encoded_len = Vec::new();

    if l <= 55 {
        encoded_len.push((l as u8) + offset);
        return encoded_len;
    }

    if l > 55 {
        let len_bytes = length_to_minimal_bytes(l as usize);
        let len_of_len = len_bytes.len();

        if len_of_len > 8 {
            panic!("Length of the string is too large")
        }

        encoded_len.push((len_of_len as u8) + offset + 55);
        encoded_len.extend_from_slice(&len_bytes);

        return encoded_len;
    }

    unreachable!("All cases should be covered")
}

pub fn encode_rlp(data: RlpData) -> Vec<u8> {
    //how do we know if its a string or a list?

    match data {
        RlpData::String(bytes) => {
            if bytes.len() == 1 && bytes[0] < 0x80 {
                return bytes;
            }
            let mut encoded_data = encode_length(bytes.len(), 0x80);
            encoded_data.extend_from_slice(&bytes);
            encoded_data
        }
        RlpData::List(items) => {
            let mut combined_data = Vec::new();
            for item in items {
                combined_data.extend_from_slice(&encode_rlp(item));
            }
            let mut encoded_data = encode_length(combined_data.len(), 0xc0);
            encoded_data.extend_from_slice(&combined_data);
            encoded_data
        }
    }
}

// pub fn decode_rlp(data: &[u8]) -> Vec<u8> {}

#[cfg(test)]

mod unit_tests {
    use super::*;
    use rand::random;

    #[test]
    fn rlp() {
        let byte_str = random::<[u8; 1024]>();
        let l_of_l = length_to_minimal_bytes(byte_str.len());

        println!("{:?}", l_of_l);
        println!("{:#x?}", l_of_l);

        println!("{:?}", 0x4 << 8)
    }

    #[test]
    fn encode_single_byte() {
        assert_eq!(encode_rlp(RlpData::String(vec![0x7f])), vec![0x7f]);
        assert_eq!(encode_rlp(RlpData::String(vec![0x00])), vec![0x00]);
        assert_eq!(encode_rlp(RlpData::String(vec![0x01])), vec![0x01]);
    }

    #[test]
    fn encode_short_string() {
        // Empty string
        assert_eq!(encode_rlp(RlpData::String(vec![])), vec![0x80]);

        // Single byte >= 0x80
        assert_eq!(encode_rlp(RlpData::String(vec![0x80])), vec![0x81, 0x80]);

        // Multiple bytes (up to 55)
        let data = vec![0x01, 0x02, 0x03];
        assert_eq!(
            encode_rlp(RlpData::String(data)),
            vec![0x83, 0x01, 0x02, 0x03]
        );

        // 55 bytes exactly
        let data = vec![0x01; 55];
        let mut expected = vec![0xb7]; // 0x80 + 55
        expected.extend_from_slice(&data);
        assert_eq!(encode_rlp(RlpData::String(data)), expected);
    }

    #[test]
    fn encode_long_string() {
        // 56 bytes (just over the threshold)
        let data = vec![0x01; 56];
        let mut expected = vec![0xb8]; // 0xb7 + 1 (length of length)
        expected.push(56); // The length itself
        expected.extend_from_slice(&data);
        assert_eq!(encode_rlp(RlpData::String(data)), expected);

        // 1024 bytes
        let data = vec![0x01; 1024];
        let mut expected = vec![0xb9]; // 0xb7 + 2 (length of length is 2 bytes)
        expected.extend_from_slice(&[0x04, 0x00]); // 1024 in big-endian
        expected.extend_from_slice(&data);
        assert_eq!(encode_rlp(RlpData::String(data)), expected);
    }
}
