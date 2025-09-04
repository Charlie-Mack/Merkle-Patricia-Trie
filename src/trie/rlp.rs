#[derive(Debug, PartialEq, Clone)]
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

fn to_integer(data: &[u8]) -> usize {
    let mut result = 0;
    for &byte in data {
        result = (result << 8) | byte as usize;
    }
    result
}

fn encode_length(l: usize, offset: u8) -> Vec<u8> {
    match l {
        0..=55 => {
            vec![(l as u8) + offset]
        }
        56..=usize::MAX => {
            let len_bytes = length_to_minimal_bytes(l);
            let len_of_len = len_bytes.len();

            let mut result = vec![(len_of_len as u8) + offset + 55];
            result.extend_from_slice(&len_bytes);
            result
        }
        _ => {
            panic!("Length of the string is too large");
        }
    }
}

fn decode_length(data: &[u8]) -> Result<(RlpData, usize), String> {
    let full_len = data.len();

    if full_len == 0 {
        panic!("Length of the data is 0");
    }

    let prefix = data[0];

    match prefix {
        0..=0x7f => {
            //This is the case where we have a single byte
            Ok((RlpData::String(vec![prefix]), 1))
        }
        0x80..=0xb7 => {
            let payload_len = prefix - 0x80;

            if full_len < (payload_len + 1) as usize {
                return Err("Insufficient data for string".to_string());
            }

            Ok((
                RlpData::String(data[1..(payload_len + 1) as usize].to_vec()),
                (payload_len + 1) as usize,
            ))
        }
        0xb8..=0xbf => {
            // Long string (56+ bytes)
            let len_of_len = (prefix - 0xb7) as usize;
            if full_len < (len_of_len + 1) as usize {
                return Err("Insufficient data for length prefix".to_string());
            }
            let payload_len = to_integer(&data[1..(len_of_len + 1) as usize]);
            if full_len < (len_of_len + 1 + payload_len) as usize {
                return Err("Insufficient data for long string".to_string());
            }
            let start = len_of_len + 1;
            let end = start + payload_len;
            Ok((RlpData::String(data[start..end].to_vec()), end))
        }

        0xc0..=0xf7 => {
            let list_len = (prefix - 0xc0) as usize;
            if full_len < (list_len + 1) as usize {
                return Err("Insufficient data for list".to_string());
            }

            let list_data = &data[1..(list_len + 1) as usize];
            let mut items = Vec::new();
            let mut offset = 0;
            while offset < list_data.len() {
                let (item, new_offset) = decode_length(&list_data[offset..])?;
                items.push(item);
                offset += new_offset;
            }

            Ok((RlpData::List(items), 1 + list_len as usize))
        }

        0xf8..=0xff => {
            let len_of_len = (prefix - 0xf7) as usize;
            if full_len < (len_of_len + 1) as usize {
                return Err("Insufficient data for length prefix".to_string());
            }
            let payload_len = to_integer(&data[1..(len_of_len + 1) as usize]);
            if full_len < (len_of_len + 1 + payload_len) as usize {
                return Err("Insufficient data for long list".to_string());
            }
            let start = len_of_len + 1;
            let end = start + payload_len;
            let list_data = &data[start..end];
            let mut items = Vec::new();
            let mut offset = 0;
            while offset < list_data.len() {
                let (item, new_offset) = decode_length(&list_data[offset..])?;
                items.push(item);
                offset += new_offset;
            }
            Ok((RlpData::List(items), end))
        }
    }
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

pub fn decode_rlp(data: &[u8]) -> RlpData {
    if data.len() == 0 {
        return RlpData::String(vec![]);
    }

    let (data, _) = decode_length(data).unwrap();
    data
}

#[cfg(test)]

mod unit_tests {
    use super::*;
    use rand::random;

    #[test]
    fn encode_empty_string() {
        let data = vec![];
        assert_eq!(encode_rlp(RlpData::String(data)), vec![0x80]);
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

    #[test]
    fn encode_short_list() {
        let data = vec![b"cat", b"dog"];
        println!("{:?}", data);
        let mut expected = vec![0xc8]; // 0xc0 + 8 (length of list)
        expected.extend_from_slice(&[0x83, 0x63, 0x61, 0x74, 0x83, 0x64, 0x6f, 0x67]);
        assert_eq!(
            encode_rlp(RlpData::List(
                data.into_iter()
                    .map(|s| RlpData::String(s.to_vec()))
                    .collect()
            )),
            expected
        );
    }

    #[test]
    fn encode_long_list() {
        let data = vec![
            random::<[u8; 60]>(),
            random::<[u8; 60]>(),
            random::<[u8; 60]>(),
        ];
        println!("{:?}", data);
        //We expect the length of each item in the list to be 60 + 2 = 62 bytes
        //Total length of the list is 62 * 3 = 186 bytes

        let encoded_data = encode_rlp(RlpData::List(
            data.into_iter()
                .map(|s| RlpData::String(s.to_vec()))
                .collect(),
        ));

        println!("{:x?}", encoded_data);

        //Assert the first 2 bytes are 0xf8 and 186
        assert_eq!(encoded_data[0], 0xf8);
        assert_eq!(encoded_data[1], 186);

        //Assert the total length to be 186 + 2 = 188 bytes
        assert_eq!(encoded_data.len(), 188);
    }

    #[test]
    fn encode_empty_list() {
        let data = vec![];
        assert_eq!(encode_rlp(RlpData::List(data)), vec![0xc0]);
    }

    #[test]
    fn encode_list_of_lists() {
        let data = vec![
            RlpData::List(vec![
                RlpData::String(b"dog".to_vec()),
                RlpData::String(b"puppy".to_vec()),
            ]),
            RlpData::String(b"cat".to_vec()),
        ];
        let encoded_data = encode_rlp(RlpData::List(data));
        println!("{:x?}", encoded_data);
        assert_eq!(encoded_data[0], 0xcf);
        assert_eq!(encoded_data[1], 0xca);

        // Length of the list is 16 bytes:
        // dog: 3 + 1 = 4
        // puppy: 5 + 1 = 6
        // sublist: 1 byte
        // cat: 3 + 1 = 4
        // list: 1 byte
        // Total: 4 + 6 + 4 + 1 + 1 = 16

        assert_eq!(encoded_data.len(), 16);
    }

    #[test]
    fn encode_long_list_of_lists() {
        // Create a list with multiple sublists, each containing multiple strings
        let data = vec![
            RlpData::List(vec![
                RlpData::String(b"item1".to_vec()),
                RlpData::String(b"item2".to_vec()),
                RlpData::String(b"item3".to_vec()),
            ]),
            RlpData::List(vec![
                RlpData::String(b"subitem1".to_vec()),
                RlpData::String(b"subitem2".to_vec()),
            ]),
            RlpData::String(b"standalone".to_vec()),
            RlpData::List(vec![
                RlpData::String(b"another1".to_vec()),
                RlpData::String(b"another2".to_vec()),
                RlpData::String(b"another3".to_vec()),
                RlpData::String(b"another4".to_vec()),
            ]),
        ];

        let encoded_data = encode_rlp(RlpData::List(data));
        println!("Encoded long list of lists: {:x?}", encoded_data);
        println!("Total length: {} bytes", encoded_data.len());

        // Verify it starts with the correct list prefix
        // The total payload should be quite large, so it should use the long list encoding
        assert!(encoded_data[0] >= 0xf8); // Long list encoding starts at 0xf8

        // Verify the structure by checking for list markers (0xc0 + length)
        // Look for any list encoding pattern
        let has_list_markers = encoded_data.iter().any(|&b| b >= 0xc0 && b <= 0xf7);
        assert!(has_list_markers, "Should contain list encoding markers");

        // Should contain the expected strings
        let encoded_str = String::from_utf8_lossy(&encoded_data);
        assert!(encoded_str.contains("item1"));
        assert!(encoded_str.contains("subitem1"));
        assert!(encoded_str.contains("standalone"));
    }

    #[test]
    fn slice_to_integer() {
        // Test single byte
        assert_eq!(to_integer(&[0x00]), 0);
        assert_eq!(to_integer(&[0x01]), 1);
        assert_eq!(to_integer(&[0xff]), 255);

        // Test two bytes
        assert_eq!(to_integer(&[0x01, 0x00]), 256);
        assert_eq!(to_integer(&[0xff, 0xff]), 65535);

        // Test four bytes
        assert_eq!(to_integer(&[0x01, 0x00, 0x00, 0x00]), 16777216);
        assert_eq!(to_integer(&[0xff, 0xff, 0xff, 0xff]), 4294967295);

        // Test eight bytes
        assert_eq!(
            to_integer(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            72057594037927936
        );

        // Edge cases
        assert_eq!(to_integer(&[]), 0);
        assert_eq!(
            to_integer(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            0
        );
    }

    #[test]
    fn decode_rlp_single_byte() {
        let data = vec![0x7f];
        assert_eq!(decode_rlp(&data), RlpData::String(vec![0x7f]));
        let data = vec![0x00];
        assert_eq!(decode_rlp(&data), RlpData::String(vec![0x00]));
        let data = vec![0x01];
        assert_eq!(decode_rlp(&data), RlpData::String(vec![0x01]));
    }

    #[test]
    fn decode_rlp_short_string() {
        let data = vec![0x83, 0x63, 0x61, 0x74];
        assert_eq!(decode_rlp(&data), RlpData::String(vec![0x63, 0x61, 0x74]));
        let data = vec![0x83, 0x64, 0x6f, 0x67];
        assert_eq!(decode_rlp(&data), RlpData::String(vec![0x64, 0x6f, 0x67]));
    }

    #[test]
    fn decode_rlp_long_string() {
        //56 byte string
        let data = vec![0x01; 56];
        let encoded_data = encode_rlp(RlpData::String(data.clone()));
        assert_eq!(decode_rlp(&encoded_data), RlpData::String(data));
    }

    #[test]
    fn decode_short_list() {
        // Test empty list
        let data = vec![0xc0];
        let (decoded, consumed) = decode_length(&data).unwrap();
        assert_eq!(decoded, RlpData::List(vec![]));
        assert_eq!(consumed, 1);

        // Test list with two strings: ["cat", "dog"]
        let data = vec![0xc8, 0x83, 0x63, 0x61, 0x74, 0x83, 0x64, 0x6f, 0x67];
        let (decoded, consumed) = decode_length(&data).unwrap();
        assert_eq!(consumed, 9);
        if let RlpData::List(items) = decoded {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0], RlpData::String(b"cat".to_vec()));
            assert_eq!(items[1], RlpData::String(b"dog".to_vec()));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn decode_long_list() {
        // Create a list with 3 items of 60 bytes each (total 186 bytes)
        let item1 = vec![0x01; 60];
        let item2 = vec![0x02; 60];
        let item3 = vec![0x03; 60];

        // Encode the list
        let encoded = encode_rlp(RlpData::List(vec![
            RlpData::String(item1.clone()),
            RlpData::String(item2.clone()),
            RlpData::String(item3.clone()),
        ]));

        // Decode it back
        let (decoded, consumed) = decode_length(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());

        if let RlpData::List(items) = decoded {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], RlpData::String(item1));
            assert_eq!(items[1], RlpData::String(item2));
            assert_eq!(items[2], RlpData::String(item3));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn decode_list_of_lists() {
        // Test: [["dog", "puppy"], "cat"]
        let data = vec![
            0xcf, 0xca, 0x83, 0x64, 0x6f, 0x67, 0x85, 0x70, 0x75, 0x70, 0x70, 0x79, 0x83, 0x63,
            0x61, 0x74,
        ];

        let (decoded, consumed) = decode_length(&data).unwrap();
        assert_eq!(consumed, data.len());

        if let RlpData::List(items) = decoded {
            assert_eq!(items.len(), 2);

            // First item should be a list
            if let RlpData::List(sub_items) = &items[0] {
                assert_eq!(sub_items.len(), 2);
                assert_eq!(sub_items[0], RlpData::String(b"dog".to_vec()));
                assert_eq!(sub_items[1], RlpData::String(b"puppy".to_vec()));
            } else {
                panic!("Expected first item to be a list");
            }

            // Second item should be a string
            assert_eq!(items[1], RlpData::String(b"cat".to_vec()));
        } else {
            panic!("Expected list");
        }
    }

    #[test]
    fn decode_long_list_of_lists() {
        // Create a complex nested structure
        let data = RlpData::List(vec![
            RlpData::List(vec![
                RlpData::String(b"item1".to_vec()),
                RlpData::String(b"item2".to_vec()),
                RlpData::String(b"item3".to_vec()),
            ]),
            RlpData::List(vec![
                RlpData::String(b"subitem1".to_vec()),
                RlpData::String(b"subitem2".to_vec()),
            ]),
            RlpData::String(b"standalone".to_vec()),
            RlpData::List(vec![
                RlpData::String(b"another1".to_vec()),
                RlpData::String(b"another2".to_vec()),
                RlpData::String(b"another3".to_vec()),
                RlpData::String(b"another4".to_vec()),
            ]),
        ]);

        // Encode it
        let encoded = encode_rlp(data.clone());
        println!("Encoded: {:x?}", encoded);

        // Decode it back
        let (decoded, consumed) = decode_length(&encoded).unwrap();
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_roundtrip_test() {
        // Test that encode -> decode -> encode produces the same result
        let original = RlpData::List(vec![
            RlpData::String(b"hello".to_vec()),
            RlpData::List(vec![
                RlpData::String(b"nested".to_vec()),
                RlpData::String(b"list".to_vec()),
            ]),
            RlpData::String(b"world".to_vec()),
        ]);

        let encoded = encode_rlp(original.clone());
        let (decoded, _) = decode_length(&encoded).unwrap();

        assert_eq!(decoded, original);
    }
}
