use std::fmt;

use super::db::HashDB;
use super::encoder::{RlpData, decode_rlp, encode_rlp};
use crate::Key32;
use crate::trie::{BranchNode, ExtensionNode, LeafNode, NibblePath, Node};
use hex;
use sha3::{Digest, Keccak256};

#[derive(Debug)]
pub enum CompactEncodeError {
    InvalidNodeType { node: Node },
}

#[derive(Debug)]
pub enum CompactDecodeError {
    InvalidFlag { flag: u8 },
    EmptyPath,
}

#[derive(Debug)]
pub enum NodeRef {
    Hash([u8; 32]),
    Inline(Vec<u8>),
}

impl NodeRef {
    pub fn canonicalize_root(&self) -> [u8; 32] {
        match self {
            NodeRef::Hash(h) => *h,
            NodeRef::Inline(bytes) => Keccak256::digest(&bytes).into(),
        }
    }
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NodeRef::Hash(h) => write!(f, "0x{}", hex::encode(h)),
            NodeRef::Inline(bytes) => write!(f, "0x{}", hex::encode(bytes)),
        }
    }
}

impl fmt::Display for CompactDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompactDecodeError::InvalidFlag { flag } => {
                write!(f, "Invalid flag: {:?}", flag)
            }
            CompactDecodeError::EmptyPath => write!(f, "Empty path"),
        }
    }
}

impl fmt::Display for CompactEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CompactEncodeError::InvalidNodeType { node } => {
                write!(f, "Invalid node type: {:?}", node)
            }
        }
    }
}

impl std::error::Error for CompactEncodeError {}

pub fn commit_node(db: &mut impl HashDB, node: &Node) -> NodeRef {
    match node {
        Node::Leaf(leaf) => {
            let encoded_path = compact_encode(node).unwrap();

            println!("encoded_path: {:x?}", encoded_path);
            println!("length of encoded_path: {}", encoded_path.len());

            let rlp = RlpData::List(vec![
                RlpData::String(encoded_path),
                RlpData::String(leaf.value.clone()),
            ]);

            println!("rlp: {:x?}", rlp);

            inline_or_hash(db, rlp)
        }
        Node::Extension(extension) => {
            // commit the child first
            let child_stored = commit_node(db, &extension.child);
            let child_field = match child_stored {
                NodeRef::Inline(bytes) => RlpData::String(bytes), // inline
                NodeRef::Hash(h) => RlpData::String(h.to_vec()),  // 32-byte hash
            };
            let encoded_path = compact_encode(node).unwrap();
            let rlp = RlpData::List(vec![RlpData::String(encoded_path), child_field]);
            inline_or_hash(db, rlp)
        }
        Node::Branch(branch) => {
            let mut items: Vec<RlpData> = Vec::with_capacity(17);
            for i in 0..16 {
                if let Some(child) = &branch.children[i] {
                    let stored = commit_node(db, child);
                    items.push(match stored {
                        NodeRef::Inline(bytes) => RlpData::String(bytes),
                        NodeRef::Hash(h) => RlpData::String(h.to_vec()),
                    });
                } else {
                    items.push(RlpData::String(vec![])); // empty string for NULL
                }
            }
            items.push(match &branch.value {
                Some(v) => RlpData::String(v.clone()),
                None => RlpData::String(vec![]),
            });

            let rlp = RlpData::List(items);
            inline_or_hash(db, rlp)
        }
    }
}

fn inline_or_hash<N: HashDB>(db: &mut N, rlp: RlpData) -> NodeRef {
    let bytes = encode_rlp(&rlp);
    println!("Raw RLP encoded data for node: {:x?}", bytes);
    if bytes.len() < 32 {
        NodeRef::Inline(bytes)
    } else {
        let h: [u8; 32] = Keccak256::digest(&bytes).into();
        let _ = db.put(h, bytes);
        NodeRef::Hash(h)
    }
}

fn compact_decode(encoded: &[u8]) -> Result<NibblePath, CompactDecodeError> {
    let mut nibbles = NibblePath::from_bytes(encoded).nibbles;

    if nibbles.is_empty() {
        return Err(CompactDecodeError::EmptyPath);
    }

    let flag = nibbles[0];

    if flag > 0x03 {
        return Err(CompactDecodeError::InvalidFlag { flag: flag });
    }

    Ok(NibblePath::new(
        nibbles[(if flag % 2 == 0 { 2 } else { 1 })..].to_vec(),
    ))
}

fn compact_encode(node: &Node) -> Result<Vec<u8>, CompactEncodeError> {
    let mut path_nibbles = Vec::new();
    let mut encoded_path = Vec::new();

    match node {
        Node::Leaf(leaf) => {
            println!("Length of path: {}", leaf.path.nibbles.len());

            let odd_len = (&leaf.path.nibbles.len() % 2) as u8;
            path_nibbles.push(0x02 + odd_len);

            if odd_len == 0 {
                path_nibbles.push(0x00);
            }

            path_nibbles.extend_from_slice(&leaf.path.nibbles);
        }
        Node::Extension(extension) => {
            let odd_len = (&extension.path.nibbles.len() % 2) as u8;
            path_nibbles.push(0x00 + odd_len);

            if odd_len == 0 {
                path_nibbles.push(0x00);
            }

            path_nibbles.extend_from_slice(&extension.path.nibbles);
        }
        _ => {
            return Err(CompactEncodeError::InvalidNodeType { node: node.clone() });
        }
    }

    for i in (0..path_nibbles.len()).step_by(2) {
        encoded_path.push(path_nibbles[i] << 4 | path_nibbles[i + 1]);
    }

    Ok(encoded_path)
}

/// Read the hex-prefix (HP) flag nibble from the compact-encoded path bytes
fn hp_flag(encoded_path: &[u8]) -> Option<u8> {
    let nibbles = NibblePath::from_bytes(encoded_path).nibbles;
    nibbles.first().copied()
}

/// Distinguish inline bytes vs 32-byte hash, and load the child node accordingly.
fn load_child(db: &impl HashDB, field: &RlpData) -> Option<Node> {
    let bytes = match field {
        RlpData::String(b) => b,
        _ => return None,
    };

    if bytes.is_empty() {
        // For children, empty means no child (this shouldn't occur for ext child).
        return None;
    }

    if bytes.len() < 32 {
        // Inline child: `bytes` are the child's **RLP**. Decode them and recurse.
        let child_rlp = decode_rlp(bytes).ok()?;
        parse_node(db, &child_rlp)
    } else if bytes.len() == 32 {
        // Hashed child: `bytes` are the 32-byte keccak of the child's RLP. Fetch from DB.
        let h: [u8; 32] = bytes.as_slice().try_into().ok()?;
        load_node(db, &h)
    } else {
        // Shouldn't happen in Ethereum MPT encoding
        None
    }
}

fn parse_node(db: &impl HashDB, rlp: &RlpData) -> Option<Node> {
    let list = match rlp {
        RlpData::List(items) => items,
        _ => return None, // top-level node must be a list
    };

    match list.len() {
        2 => {
            // Leaf or Extension
            let path_bytes = match &list[0] {
                RlpData::String(b) => b.as_slice(),
                _ => return None,
            };
            println!("path_bytes: {:x?}", path_bytes);
            let flag = hp_flag(path_bytes)?;
            let path = compact_decode(path_bytes).ok()?; // your existing helper

            println!("path: {:x?}", path);

            if flag <= 0x01 {
                // Extension: [encoded_path, child_ref]
                let child = load_child(db, &list[1])?;
                Some(Node::Extension(ExtensionNode {
                    path,
                    child: Box::new(child),
                }))
            } else {
                // Leaf: [encoded_path, value]
                let value = match &list[1] {
                    RlpData::String(v) => v.clone(),
                    _ => return None,
                };
                println!("value: {:x?}", value);
                Some(Node::Leaf(LeafNode { path, value }))
            }
        }
        17 => {
            // Branch: 16 children + value
            let mut branch = BranchNode::new();

            for i in 0..16 {
                match &list[i] {
                    RlpData::String(b) if b.is_empty() => { /* no child */ }
                    RlpData::String(_) => {
                        if let Some(child) = load_child(db, &list[i]) {
                            branch.children[i] = Some(Box::new(child));
                        } else {
                            return None;
                        }
                    }
                    _ => return None,
                }
            }

            // Value is the 17th item
            branch.value = match &list[16] {
                RlpData::String(v) if v.is_empty() => None,
                RlpData::String(v) => Some(v.clone()),
                _ => return None,
            };

            Some(Node::Branch(branch))
        }
        _ => None,
    }
}

fn load_node(db: &impl HashDB, key: &[u8; 32]) -> Option<Node> {
    let encoded = db.get(key).ok()??; // Result<Option<Vec<u8>>> → Option<Vec<u8>>
    let rlp = decode_rlp(&encoded).ok()?; // RlpData
    println!("rlp: {:x?}", rlp);
    parse_node(db, &rlp) // Node
}

pub fn get_value(db: &impl HashDB, key: &[u8; 32], root_hash: &[u8; 32]) -> Option<Vec<u8>> {
    let root = load_node(db, root_hash)?;
    println!("root: {:x?}", root);
    let path = NibblePath::from(Key32(*key));
    println!("path to find: {:x?}", path);
    root.get(path).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trie::{ExtensionNode, LeafNode, NibblePath, Node};

    #[test]
    fn compact_encode_leaf() {
        let test_cases: [(Node, Vec<u8>); 4] = [
            (
                Node::Extension(ExtensionNode::new(
                    NibblePath::new(vec![0x1, 0x2, 0x3, 0x4, 0x5]),
                    Box::new(Node::dummy()),
                )),
                vec![0x11, 0x23, 0x45],
            ),
            (
                Node::Extension(ExtensionNode::new(
                    NibblePath::new(vec![0x0, 0x1, 0x2, 0x3, 0x4, 0x5]),
                    Box::new(Node::dummy()),
                )),
                vec![0x00, 0x01, 0x23, 0x45],
            ),
            (
                Node::Leaf(LeafNode::new(
                    NibblePath::new(vec![0x0, 0xf, 0x1, 0xc, 0xb, 0x8]),
                    b"world".to_vec(),
                )),
                vec![0x20, 0x0f, 0x1c, 0xb8],
            ),
            (
                Node::Leaf(LeafNode::new(
                    NibblePath::new(vec![0xf, 0x1, 0xc, 0xb, 0x8]),
                    b"world".to_vec(),
                )),
                vec![0x3f, 0x1c, 0xb8],
            ),
        ];

        for test in test_cases {
            let original_path = test.0.path().unwrap();
            let expected_encoding = &test.1;
            let encoded = compact_encode(&test.0).unwrap();
            let decoded = compact_decode(&encoded).unwrap();

            assert_eq!(encoded, *expected_encoding);
            assert_eq!(decoded, *original_path);
        }
    }
}
