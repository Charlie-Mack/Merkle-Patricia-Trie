use super::{DeleteResult, Key32, NibblePath, Node};
use crate::utils::display::NodeDisplay;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trie {
    root: Option<Node>, // None if empty, otherwise some node (Leaf/Ext/Branch)
}

impl Trie {
    pub fn new() -> Self {
        Trie { root: None }
    }

    pub fn print_tree(&self) {
        match &self.root {
            None => println!("Trie is empty"),
            Some(root) => root.print_tree(),
        }
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
    }

    pub fn get(&self, key: Key32) -> Option<&Vec<u8>> {
        match &self.root {
            None => None,
            Some(root) => root.get(key.into()),
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
}
