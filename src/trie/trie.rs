use super::{DeleteResult, Key32, NibblePath, Node};
use crate::kv::db::{HashDB, SledDB};
use crate::kv::storage::{NodeRef, commit_node, get_value};
use crate::utils::display::NodeDisplay;
use sha3::{Digest, Keccak256};

pub struct Trie {
    root: Option<Node>, // None if empty, otherwise some node (Leaf/Ext/Branch)
    db: Option<SledDB>,
}

impl Trie {
    pub fn new() -> Self {
        Trie {
            root: None,
            db: None,
        }
    }

    pub fn with_db(path: impl AsRef<std::path::Path>, tree: &str) -> Self {
        let db = SledDB::open(path, tree).expect("open sled");
        Self {
            root: None,
            db: Some(db),
        }
    }

    pub fn print_tree(&self) {
        match &self.root {
            None => println!("Trie is empty"),
            Some(root) => root.print_tree(),
        }
    }

    pub fn commit(&mut self) -> NodeRef {
        let db = self.db.as_mut().unwrap();

        let root = match &self.root {
            None => NodeRef::Inline(vec![]),
            Some(n) => commit_node(db, n),
        };

        let root_key = Keccak256::digest(b"__ROOT__").into();

        println!("Root Key: {:x?}", root_key);

        let _ = db.put(root_key, root.canonicalize_root().to_vec());

        root
    }

    pub fn root(&self) -> Option<&Node> {
        self.root.as_ref()
    }

    pub fn set(&mut self, key: Key32, value: impl AsRef<[u8]>) {
        let v = value.as_ref().to_vec();
        match &mut self.root {
            None => {
                let path = NibblePath::from(key);
                //Trie is empty, so create a new leaf node
                self.root = Some(Node::new_leaf(path, v));
            }
            Some(root) => {
                root.insert(key.into(), v);
            }
        }

        println!("Node in memory: {:x?}", self.root);
    }

    pub fn get(&self, key: Key32) -> Option<Vec<u8>> {
        //Do we have a db?
        if let Some(db) = &self.db {
            let root_key: [u8; 32] = Keccak256::digest(b"__ROOT__").into();
            let mut root_hash = db.get(&root_key);

            //here i need to say that the root hash is a [u8; 32]

            if let Ok(Some(root_hash)) = root_hash {
                let root_hash: [u8; 32] = root_hash
                    .try_into()
                    .expect("Root hash should always be 32 bytes");
                get_value(db, &key.0, &root_hash);
            }
        }

        //If we don't have a db, we just get the root from the trie
        match &self.root {
            None => None,
            Some(root) => root.get(key.into()).cloned(),
        }
    }

    pub fn delete(&mut self, key: Key32) -> bool {
        match &mut self.root {
            None => false, // Key doesn't exist in empty trie
            Some(root) => {
                //if the root is a leaf node and the key matches the path, set the root to None
                if let Node::Leaf(leaf) = root {
                    if leaf.path == key.into() {
                        self.root = None;
                        true
                    } else {
                        false
                    }
                } else {
                    match root.delete(key.into()) {
                        DeleteResult::Deleted => true,
                        DeleteResult::NotFound => false,
                        DeleteResult::DeletedAndReplace(new_root) => {
                            *root = new_root;
                            true
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use rand::random;

    #[test]
    fn empty_trie_has_none_root() {
        let trie = Trie::new();
        assert_eq!(trie.root(), None);
    }

    #[test]
    fn single_insert_creates_leaf_root() {
        let mut trie = Trie::new();
        let key = Key32(random::<[u8; 32]>());

        trie.set(key, b"hello");

        assert!(matches!(trie.root(), Some(Node::Leaf(_))));

        if let Some(Node::Leaf(leaf)) = &trie.root() {
            assert_eq!(leaf.path, NibblePath::from(key));
            assert_eq!(leaf.value, b"hello".to_vec());
        }
    }

    #[test]
    fn divergent_keys_create_branch_root() {
        let mut trie = Trie::new();

        let key1 = Key32(random::<[u8; 32]>());
        let mut key2 = Key32(random::<[u8; 32]>());

        // Ensure different first nibbles
        while key1.0[0] >> 4 == key2.0[0] >> 4 {
            key2 = Key32(random::<[u8; 32]>());
        }

        trie.set(key1, b"hello");
        trie.set(key2, b"world");

        // Root should be a branch
        assert!(matches!(trie.root(), Some(Node::Branch(_))));
    }

    #[test]
    fn common_prefix_creates_extension() {
        let mut trie = Trie::new();

        let mut key1 = random::<[u8; 32]>();
        let mut key2 = random::<[u8; 32]>();

        // Create common prefix
        key1[..6].copy_from_slice(b"common");
        key2[..6].copy_from_slice(b"common");

        // Make rest different
        key1[6..].copy_from_slice(b"abcdefghijklmnopqrstuvwxyz");
        key2[6..].copy_from_slice(b"zyxwvutsrqponmlkjihgfedcba");

        trie.set(Key32(key1), b"hello");
        trie.set(Key32(key2), b"world");

        // Root should be an extension
        assert!(matches!(trie.root(), Some(Node::Extension(_))));

        if let Some(Node::Extension(ext)) = &trie.root() {
            // Extension path should be the common prefix
            assert_eq!(ext.path, NibblePath::from_bytes(&key1[..6]));
            // Child should be a branch
            assert!(matches!(*ext.child, Node::Branch(_)));
        }
    }

    #[test]
    fn verify_complex_structure() {
        let mut trie = Trie::new();

        // Build specific structure: Branch -> Extension -> Branch -> Leaves
        let key1 = Key32(*b"j23456abcdefghijklmnopqrstuvwxyz");
        let key2 = Key32(*b"523456abcdefghijklmnopqrstuvwxyz");
        let key3 = Key32(*b"523456zyxwvutsrqponmlkjihgfedcba");

        trie.set(key1, b"val1");
        trie.set(key2, b"val2");
        trie.set(key3, b"val3");

        // Verify root is a branch (j vs 5)
        assert!(matches!(trie.root(), Some(Node::Branch(_))));

        if let Some(Node::Branch(root_branch)) = &trie.root() {
            // Check j branch has a leaf
            let j_child = &root_branch.children[0x6a >> 4]; // 'j' nibble
            assert!(matches!(j_child, Some(b) if matches!(**b, Node::Leaf(_))));

            // Check 5 branch has an extension (common "23456")
            let five_child = &root_branch.children[0x35 >> 4]; // '5' nibble
            assert!(matches!(five_child, Some(b) if matches!(**b, Node::Extension(_))));
        }
    }

    #[test]
    fn commit_trie_with_db() {
        let mut trie = Trie::with_db("db", "mpt");
        let key = String::from("hello").into();
        trie.set(key, b"world");
        let root_hash = trie.commit();
        println!("root_hash: {}", root_hash);
    }

    // #[test]
    // fn commit_trie_with_db_and_complex_structure() {
    //     let mut trie = Trie::with_db("db", "mpt");

    //     let keys = [
    //         Key32(*b"j23456abcdefghijklmnopqrstuvwxyz"),
    //         Key32(*b"523456abcdefghijklmnopqrstuvwxyz"),
    //         Key32(*b"523456zyxwvutsrqponmlkjihgfedcba"),
    //         Key32(*b"523abcdefghijklmnopqrstuvwxyz123"),
    //         Key32(*b"523456q1111111111111111111111111"),
    //     ];

    //     for key in keys {
    //         trie.set(key, b"hello");
    //     }

    //     let root_hash = trie.commit();

    //     println!("root_hash: {}", root_hash);
    // }
}
