/// 32-byte key type.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Key32(pub [u8; 32]);

/// Half-byte path representation (256 -> 16 possible values for trie sparsity)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NibblePath {
    pub nibbles: Vec<u8>,
}

impl From<Key32> for NibblePath {
    fn from(k: Key32) -> Self {
        let mut nibbles = Vec::with_capacity(64);
        for byte in k.0 {
            nibbles.push(byte >> 4); // extracts the higher 4 bits
            nibbles.push(byte & 0x0f); // extracts the lower 4 bits
        }
        NibblePath { nibbles }
    }
}

impl NibblePath {
    pub fn new(nibbles: Vec<u8>) -> Self {
        NibblePath { nibbles }
    }

    pub fn merge(&self, other: &NibblePath) -> NibblePath {
        let mut merged_nibbles = self.nibbles.clone();
        merged_nibbles.extend_from_slice(&other.nibbles);
        NibblePath {
            nibbles: merged_nibbles,
        }
    }

    pub fn lcp_len(&self, other: &[u8]) -> usize {
        let n = self.nibbles.len().min(other.len());
        for i in 0..n {
            if self.nibbles[i] != other[i] {
                return i;
            }
        }
        n
    }

    pub fn from_bytes(bytes: &[u8]) -> NibblePath {
        let mut nibbles = Vec::new();
        for byte in bytes {
            nibbles.push(byte >> 4);
            nibbles.push(byte & 0x0f);
        }

        NibblePath { nibbles }
    }
}

#[cfg(test)]

mod unit_tests {
    use super::*;
    use rand::random;

    #[test]
    fn key32_to_nibble_path_conversion() {
        let key = Key32(random::<[u8; 32]>());
        let path = NibblePath::from(key);

        assert_eq!(path.nibbles.len(), 64);

        // Verify conversion correctness
        for (i, byte) in key.0.iter().enumerate() {
            assert_eq!(path.nibbles[i * 2], byte >> 4);
            assert_eq!(path.nibbles[i * 2 + 1], byte & 0x0f);
        }
    }

    #[test]
    fn lcp_len_identical_slices() {
        let a = NibblePath::new(vec![1, 2, 3, 4]);
        let b = NibblePath::new(vec![1, 2, 3, 4]);
        assert_eq!(a.lcp_len(&b.nibbles), 4);
    }

    #[test]
    fn lcp_len_partial_match() {
        let a = NibblePath::new(vec![1, 2, 3, 4]);
        let b = NibblePath::new(vec![1, 2, 5, 6]);
        assert_eq!(a.lcp_len(&b.nibbles), 2);
    }

    #[test]
    fn lcp_len_no_match() {
        let a = NibblePath::new(vec![1, 2, 3, 4]);
        let b = NibblePath::new(vec![5, 6, 7, 8]);
        assert_eq!(a.lcp_len(&b.nibbles), 0);
    }

    #[test]
    fn lcp_len_different_lengths() {
        let a = NibblePath::new(vec![1, 2, 3, 4, 5]);
        let b = NibblePath::new(vec![1, 2, 3]);
        assert_eq!(a.lcp_len(&b.nibbles), 3);
    }

    #[test]
    fn bytes_to_nibbles_conversion() {
        let bytes = b"test";
        let nibbles = NibblePath::from_bytes(bytes);

        assert_eq!(nibbles.nibbles.len(), 8);
        assert_eq!(nibbles.nibbles[0], 0x7); // 't' = 0x74
        assert_eq!(nibbles.nibbles[1], 0x4);
        assert_eq!(nibbles.nibbles[2], 0x6); // 'e' = 0x65
        assert_eq!(nibbles.nibbles[3], 0x5);
    }
}
