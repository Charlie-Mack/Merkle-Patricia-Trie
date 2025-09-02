use rand::random;
use std::array;
use std::fmt;

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
    pub fn merge(&self, other: &NibblePath) -> NibblePath {
        let mut merged_nibbles = self.nibbles.clone();
        merged_nibbles.extend_from_slice(&other.nibbles);
        NibblePath {
            nibbles: merged_nibbles,
        }
    }
}

//--- Node Kinds ---
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchNode {
    pub children: [Option<Box<Node>>; 16], // 0-15 nibbles
    pub value: Option<Vec<u8>>,            // vt
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionNode {
    pub path: NibblePath,
    pub child: Box<Node>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafNode {
    pub path: NibblePath,
    pub value: Vec<u8>,
}

//--- Merkle Patricia Node ---
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Branch(BranchNode),
    Extension(ExtensionNode),
    Leaf(LeafNode),
}

#[derive(Debug)]
enum DeleteResult {
    NotFound,                // Key wasn't found
    Deleted,                 // Key was deleted
    DeletedAndReplace(Node), // Key was deleted, replace this node with the returned node
}

impl BranchNode {
    pub fn new() -> Self {
        Self {
            children: array::from_fn(|_| None),
            value: None,
        }
    }

    pub fn add_leaf(&mut self, slot: usize, path: NibblePath, value: Vec<u8>) {
        let child_leaf = Node::Leaf(LeafNode::new(path, value));
        self.children[slot] = Some(Box::new(child_leaf));
    }

    pub fn add_child(&mut self, slot: usize, child: Box<Node>) {
        self.children[slot] = Some(child);
    }
}

impl ExtensionNode {
    pub fn new(path: NibblePath, child: Box<Node>) -> Self {
        Self { path, child }
    }
    pub fn merge_with(mut self, path: NibblePath, value: Vec<u8>) -> Node {
        let a = &self.path.nibbles;
        let b = &path.nibbles[..a.len()];
        let k = lcp_len(a, b);

        if k == a.len() {
            //with an identical extension we will need to insert the rest of the path to the extensions child
            let rem_path = &path.nibbles[k..];

            self.child.insert(
                NibblePath {
                    nibbles: rem_path.to_vec(),
                },
                value.clone(),
            );

            return Node::Extension(self);
        } else {
            let mut branch = BranchNode::new();

            // Here we need to create a new extension with the remaining path if the path is not empty.

            let rem_ext_path = self.path.nibbles[k..].to_vec();

            if !rem_ext_path.is_empty() {
                branch.add_child(
                    rem_ext_path[0] as usize,
                    Box::new(Node::Extension(ExtensionNode::new(
                        NibblePath {
                            nibbles: rem_ext_path[1..].to_vec(),
                        },
                        std::mem::replace(&mut self.child, Box::new(Node::dummy())),
                    ))),
                );
            } else {
                branch.add_child(
                    a[k] as usize,
                    std::mem::replace(&mut self.child, Box::new(Node::dummy())),
                );
            }

            let new_rem = &path.nibbles[k..];

            if new_rem.is_empty() {
                branch.value = Some(value);
            } else {
                branch.add_leaf(
                    new_rem[0] as usize,
                    NibblePath {
                        nibbles: new_rem[1..].to_vec(),
                    },
                    value,
                );
            }

            if k > 0 {
                let new_ext = ExtensionNode::new(
                    NibblePath {
                        nibbles: a[..k].to_vec(),
                    },
                    Box::new(Node::Branch(branch)),
                );

                Node::Extension(new_ext)
            } else {
                Node::Branch(branch)
            }
        }
    }
}

impl LeafNode {
    pub fn new(path: NibblePath, value: Vec<u8>) -> Self {
        Self { path, value }
    }

    pub fn diverge_with(&self, path: NibblePath, value: Vec<u8>) -> Node {
        let a = &self.path.nibbles;
        let b = &path.nibbles;
        let k = lcp_len(a, b);

        if k == a.len() && k == b.len() {
            //identical keys, we should override the value
            return Node::Leaf(LeafNode::new(path, value));
        }

        //Build a branch at the divergent point
        let mut branch = BranchNode::new();

        //old side (existing leaf)
        let old_rem = &a[k..];

        branch.add_leaf(
            old_rem[0] as usize,
            NibblePath {
                nibbles: old_rem[1..].to_vec(),
            },
            self.value.clone(),
        );

        let new_rem = &b[k..];

        branch.add_leaf(
            new_rem[0] as usize,
            NibblePath {
                nibbles: new_rem[1..].to_vec(),
            },
            value,
        );

        if k > 0 {
            let ext = ExtensionNode::new(
                NibblePath {
                    nibbles: a[..k].to_vec(),
                },
                Box::new(Node::Branch(branch)),
            );

            return Node::Extension(ext);
        }

        return Node::Branch(branch);
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_indent(f, 0)
    }
}

impl Node {
    fn dummy() -> Self {
        Node::Branch(BranchNode::new())
    }

    fn delete(&mut self, path: NibblePath) -> DeleteResult {
        match self {
            Node::Leaf(_) => {
                panic!("Delete should not be called directly on a leaf node");
            }

            Node::Branch(branch) => {
                let child_opt = &mut branch.children[path.nibbles[0] as usize];

                if let Some(child) = child_opt {
                    let rem_path = NibblePath {
                        nibbles: path.nibbles[1..].to_vec(),
                    };

                    if let Node::Leaf(leaf) = &**child {
                        if leaf.path == rem_path {
                            *child_opt = None;
                            // Check if branch needs collapsing
                            return try_collapse_branch(branch);
                        } else {
                            return DeleteResult::NotFound;
                        }
                    }

                    let delete_result = child.delete(rem_path);

                    match delete_result {
                        DeleteResult::NotFound => DeleteResult::NotFound,
                        DeleteResult::Deleted => {
                            // Child was successfully deleted, check if branch needs collapsing
                            try_collapse_branch(branch)
                        }
                        DeleteResult::DeletedAndReplace(new_child) => {
                            // Replace the child with the new node
                            *child_opt = Some(Box::new(new_child));
                            // Check if branch still needs collapsing
                            try_collapse_branch(branch)
                        }
                    }
                } else {
                    DeleteResult::NotFound
                }
            }
            Node::Extension(ext) => {
                //we have to match the path to the extension path, it must match the whole path otherwise ther path doesnt exist
                let a = &ext.path.nibbles;
                let b = &path.nibbles;

                if b.len() >= a.len() && &b[..a.len()] == a {
                    let rem_path = NibblePath {
                        nibbles: b[a.len()..].to_vec(),
                    };
                    match ext.child.delete(rem_path) {
                        DeleteResult::NotFound => DeleteResult::NotFound,
                        DeleteResult::Deleted => DeleteResult::Deleted,
                        DeleteResult::DeletedAndReplace(new_child) => {
                            // Check if we can merge with the new child
                            match new_child {
                                Node::Leaf(leaf) => {
                                    // Extension → Leaf: merge paths
                                    let merged_path = ext.path.merge(&leaf.path);
                                    DeleteResult::DeletedAndReplace(Node::Leaf(LeafNode::new(
                                        merged_path,
                                        leaf.value,
                                    )))
                                }
                                Node::Extension(child_ext) => {
                                    // Extension → Extension: merge paths
                                    let merged_path = ext.path.merge(&child_ext.path);
                                    DeleteResult::DeletedAndReplace(Node::Extension(
                                        ExtensionNode::new(merged_path, child_ext.child),
                                    ))
                                }
                                Node::Branch(_) => {
                                    // Extension → Branch: keep as is
                                    ext.child = Box::new(new_child);
                                    DeleteResult::Deleted
                                }
                            }
                        }
                    }
                } else {
                    DeleteResult::NotFound
                }
            }
        }
    }

    fn get(&self, path: NibblePath) -> Option<&Vec<u8>> {
        //If self is a leaf node we just need to get the value if the key matches
        //If self is an extension we need to pattern match the path to the extension then follow it to the next node
        //If self is a branch we need to get the child at the first nibble of the path and then recursively call get on that child

        match self {
            Node::Leaf(leaf) => {
                if leaf.path == path {
                    Some(&leaf.value)
                } else {
                    None
                }
            }
            Node::Extension(ext) => {
                //If we are extension lets match the path to the extension path
                if ext.path.nibbles == path.nibbles[..ext.path.nibbles.len()] {
                    //We have a match, so we need to follow the extension to the next node

                    let rem_path = NibblePath {
                        nibbles: path.nibbles[ext.path.nibbles.len()..].to_vec(),
                    };

                    ext.child.get(rem_path)
                } else {
                    None
                }
            }
            Node::Branch(branch) => {
                let child_opt = &branch.children[path.nibbles[0] as usize];

                if let Some(child) = child_opt {
                    let rem_path = NibblePath {
                        nibbles: path.nibbles[1..].to_vec(),
                    };

                    child.get(rem_path)
                } else {
                    None
                }
            }
        }
    }

    fn insert(&mut self, path: NibblePath, value: Vec<u8>) {
        match self {
            Node::Branch(branch) => {
                // If we are inserting into a branch node we are going to do one of the following:
                // 1. We are going to see whether we already have a child node for the first nibble of the path
                //    a) if we do already have a child for the nibble that child is either going to be a leaf or an extension node
                //    b) if it is an extension node we check to see whether the first nibble of the extension matches the second nibble of the path
                //       i) if it does then we work through the nibbles of the extension until we find a diverging path at which point we create a new branch node
                //       ii) branch -> extension -> branch -> leaves >> branch -> extension -> branch -> extension -> branch -> leaves
                //    c) if it is a leaf node then we check to see whether we need to create a branch or an extension from that by comparing the leaf node path to our new path
                // 2. If we dont have a child for the first nibble of the path we create a new leaf node and add it to the branch node

                let child_opt = &mut branch.children[path.nibbles[0] as usize];

                if let Some(child) = child_opt {
                    // we have a child lets match the child to a node type
                    match &mut **child {
                        Node::Leaf(child_leaf) => {
                            // we have a leaf node so we need to compare the leaf node path to our new path
                            **child = child_leaf.diverge_with(
                                NibblePath {
                                    nibbles: path.nibbles[1..].to_vec(),
                                },
                                value,
                            );
                        }
                        Node::Extension(_) => {
                            // we have an extension node so we need to compare the extension node path to our new path
                            // The extension struct includes a path and a child, so we need to compare the extension node path to path.nibbles[1..]
                            // at the point of divergence we create a new branch node and then sandwich that new branch by the two extensions inbetween (if needed)

                            let old = std::mem::replace(&mut **child, Node::dummy());

                            if let Node::Extension(child_extension) = old {
                                **child = child_extension.merge_with(
                                    NibblePath {
                                        nibbles: path.nibbles[1..].to_vec(),
                                    },
                                    value,
                                );
                            }
                        }
                        Node::Branch(_) => {
                            let rem_path = NibblePath {
                                nibbles: path.nibbles[1..].to_vec(),
                            };

                            (**child).insert(rem_path, value.clone());
                        }
                    }
                } else {
                    // we dont have a child at the nibble so we can create a leaf node and add it to the branch
                    branch.add_leaf(
                        path.nibbles[0] as usize,
                        NibblePath {
                            nibbles: path.nibbles[1..].to_vec(),
                        },
                        value,
                    );
                    return;
                }
            }
            Node::Extension(_) => {
                let old = std::mem::replace(self, Node::dummy());

                if let Node::Extension(extension) = old {
                    *self = extension.merge_with(path, value);
                }
            }
            Node::Leaf(leaf) => {
                // if its a leaf node we are inserting into, it generally means that we are inserting into a new trie, when we have two leaves, we will have a branch and two leaves.
                // So here we need to create a new branch node that has two child leaf nodes
                *self = leaf.diverge_with(path, value);
            }
        }
    }
    fn fmt_indent(&self, f: &mut fmt::Formatter, indent: usize) -> fmt::Result {
        let prefix = "  ".repeat(indent);
        match self {
            Node::Leaf(leaf) => {
                writeln!(
                    f,
                    "{}Leaf: {:?} -> {:?}",
                    prefix, leaf.path.nibbles, leaf.value
                )
            }
            Node::Extension(ext) => {
                writeln!(f, "{}Extension: {:?}", prefix, ext.path.nibbles)?;
                ext.child.fmt_indent(f, indent + 1)
            }
            Node::Branch(branch) => {
                writeln!(f, "{}Branch:", prefix)?;
                for (i, child) in branch.children.iter().enumerate() {
                    if let Some(c) = child {
                        writeln!(f, "{}  [{}]:", prefix, i)?;
                        c.fmt_indent(f, indent + 2)?;
                    }
                }
                if let Some(v) = &branch.value {
                    writeln!(f, "{}  Value: {:?}", prefix, v)?;
                }
                Ok(())
            }
        }
    }

    pub fn print_tree(&self) {
        self.print_tree_recursive("", true);
    }

    fn print_tree_recursive(&self, prefix: &str, is_last: bool) {
        let connector = if is_last { "└── " } else { "├── " };
        match self {
            Node::Leaf(leaf) => {
                println!(
                    "{}{}Leaf({} nibbles)",
                    prefix,
                    connector,
                    leaf.path.nibbles.len()
                );
            }
            Node::Extension(ext) => {
                println!(
                    "{}{}Ext({} nibbles)",
                    prefix,
                    connector,
                    ext.path.nibbles.len()
                );
                let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                ext.child.print_tree_recursive(&new_prefix, true);
            }
            Node::Branch(branch) => {
                println!("{}{}Branch", prefix, connector);
                let new_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                let mut active_children: Vec<_> = branch
                    .children
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| c.as_ref().map(|child| (i, child)))
                    .collect();

                for (idx, (nibble, child)) in active_children.iter().enumerate() {
                    let is_last_child = idx == active_children.len() - 1;
                    println!(
                        "{}{}[{:x}]",
                        new_prefix,
                        if is_last_child {
                            "└── "
                        } else {
                            "├── "
                        },
                        nibble
                    );
                    let child_prefix = format!(
                        "{}{}",
                        new_prefix,
                        if is_last_child { "    " } else { "│   " }
                    );
                    child.print_tree_recursive(&child_prefix, true);
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trie {
    root: Option<Node>, // None if empty, otherwise some node (Leaf/Ext/Branch)
}

impl Trie {
    pub fn new() -> Self {
        Trie { root: None }
    }

    pub fn set(&mut self, key: Key32, value: impl AsRef<[u8]>) {
        let v = value.as_ref().to_vec();
        match &mut self.root {
            None => {
                let path = NibblePath::from(key);
                //Trie is empty, so create a new leaf node
                self.root = Some(Node::Leaf(LeafNode { path, value: v }));
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

// =====================================================================
// Helpers
// =====================================================================

//Longest Common Prefix Length
fn lcp_len(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return i;
        }
    }
    n
}

fn bytes_to_nibbles(bytes: &[u8]) -> NibblePath {
    let mut nibbles = Vec::new();
    for byte in bytes {
        nibbles.push(byte >> 4);
        nibbles.push(byte & 0x0f);
    }

    NibblePath { nibbles }
}

fn try_collapse_branch(branch: &BranchNode) -> DeleteResult {
    let active_children: Vec<_> = branch
        .children
        .iter()
        .enumerate()
        .filter_map(|(i, child)| child.as_ref().map(|c| (i, c)))
        .collect();

    if active_children.len() == 1 && branch.value.is_none() {
        // Branch with single child and no value should collapse
        let (nibble, child) = active_children[0];

        // Create appropriate collapsed node based on child type
        return match &**child {
            Node::Leaf(leaf) => {
                // Branch → Leaf: create leaf with extended path
                let mut new_path = vec![nibble as u8];
                println!("new path: {:?}", new_path);
                new_path.extend_from_slice(&leaf.path.nibbles);
                DeleteResult::DeletedAndReplace(Node::Leaf(LeafNode::new(
                    NibblePath { nibbles: new_path },
                    leaf.value.clone(),
                )))
            }
            Node::Extension(ext) => {
                // Branch → Extension: create extension with extended path
                let mut new_path = vec![nibble as u8];
                new_path.extend_from_slice(&ext.path.nibbles);
                DeleteResult::DeletedAndReplace(Node::Extension(ExtensionNode::new(
                    NibblePath { nibbles: new_path },
                    ext.child.clone(),
                )))
            }
            Node::Branch(_) => {
                // Branch → Branch: create extension with single nibble
                DeleteResult::DeletedAndReplace(Node::Extension(ExtensionNode::new(
                    NibblePath {
                        nibbles: vec![nibble as u8],
                    },
                    child.clone(),
                )))
            }
        };
    } else {
        // Branch has multiple children or has a value with children
        // No collapse needed
        DeleteResult::Deleted
    }
}

fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // API Tests - Test functionality through public interface
    // =====================================================================
    mod api_tests {
        use super::*;

        #[test]
        fn empty_trie_returns_none() {
            let trie = Trie::new();
            let key = Key32(random::<[u8; 32]>());
            assert_eq!(trie.get(key), None);
        }

        #[test]
        fn single_key_insert_and_get() {
            let mut trie = Trie::new();
            let key = Key32(random::<[u8; 32]>());

            trie.set(key, b"hello");
            assert_eq!(trie.get(key), Some(&b"hello".to_vec()));
        }

        #[test]
        fn single_key_insert_and_delete() {
            let mut trie = Trie::new();
            let key = Key32(random::<[u8; 32]>());

            trie.set(key, b"hello");
            assert_eq!(trie.delete(key), true);
            assert_eq!(trie.get(key), None);
        }

        #[test]
        fn overwrite_existing_key() {
            let mut trie = Trie::new();
            let key = Key32(random::<[u8; 32]>());

            trie.set(key, b"hello");
            trie.set(key, b"world");

            assert_eq!(trie.get(key), Some(&b"world".to_vec()));
        }

        #[test]
        fn multiple_keys_no_common_prefix() {
            let mut trie = Trie::new();

            let key1 = Key32(random::<[u8; 32]>());
            let mut key2 = Key32(random::<[u8; 32]>());

            // Ensure keys have different first nibbles
            while key1.0[0] >> 4 == key2.0[0] >> 4 {
                key2 = Key32(random::<[u8; 32]>());
            }

            trie.set(key1, b"value1");
            trie.set(key2, b"value2");

            assert_eq!(trie.get(key1), Some(&b"value1".to_vec()));
            assert_eq!(trie.get(key2), Some(&b"value2".to_vec()));
        }

        #[test]
        fn multiple_keys_with_common_prefix() {
            let mut trie = Trie::new();

            let key1 = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
            let key2 = Key32(*b"123456zyxwvutsrqponmlkjihgfedcba");

            trie.set(key1, b"value1");
            trie.set(key2, b"value2");

            assert_eq!(trie.get(key1), Some(&b"value1".to_vec()));
            assert_eq!(trie.get(key2), Some(&b"value2".to_vec()));
        }

        #[test]
        fn delete_with_branch_collapse() {
            let mut trie = Trie::new();
            let key1 = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
            let key2 = Key32(*b"k23456zyxwvutsrqponmlkjihgfedcba");

            trie.set(key1, b"value1");
            trie.set(key2, b"value2");

            trie.root.as_ref().unwrap().print_tree();

            println!("\nDeleting key1\n");

            trie.delete(key1);

            trie.root.as_ref().unwrap().print_tree();
        }

        #[test]
        fn delete_with_complex_restructuring() {
            let mut trie = Trie::new();

            let keys = [
                Key32(*b"j23456abcdefghijklmnopqrstuvwxyz"),
                Key32(*b"523456abcdefghijklmnopqrstuvwxyz"),
                Key32(*b"523456zyxwvutsrqponmlkjihgfedcba"),
                Key32(*b"523abcdefghijklmnopqrstuvwxyz123"),
                Key32(*b"523456q1111111111111111111111111"),
            ];

            let values = [b"val1", b"val2", b"val3", b"val4", b"val5"];

            // Store trie states after each insertion
            let mut trie_versions = vec![];
            trie_versions.push(trie.clone()); // Empty trie

            // Insert all keys
            for (key, value) in keys.iter().zip(values.iter()) {
                trie.set(*key, value.to_vec());
                trie_versions.push(trie.clone());
            }

            // Delete in reverse order and verify trie matches previous versions
            for i in (0..keys.len()).rev() {
                trie.delete(keys[i]);
                assert_eq!(trie, trie_versions[i], "Failed after deleting key {}", i);
            }
        }

        #[test]
        fn delete_middle_key_branch_scenarios() {
            let mut trie = Trie::new();

            // Test deleting middle key from a sequence
            let keys = [
                Key32(*b"abc11111111111111111111111111111"),
                Key32(*b"abc22222222222222222222222222222"),
                Key32(*b"abc33333333333333333333333333333"),
            ];

            for (i, key) in keys.iter().enumerate() {
                trie.set(*key, format!("val{}", i).as_bytes().to_vec());
            }

            // Delete middle key - should keep extension but modify branch
            assert!(trie.delete(keys[1]));
            assert_eq!(trie.get(keys[0]), Some(&b"val0".to_vec()));
            assert_eq!(trie.get(keys[1]), None);
            assert_eq!(trie.get(keys[2]), Some(&b"val2".to_vec()));
        }

        #[test]
        fn delete_causes_extension_merge() {
            let mut trie = Trie::new();

            // Create: Extension -> Branch -> Extension -> Leaf structure
            let key1 = Key32(*b"common11111111111111111111111111");
            let key2 = Key32(*b"common22222222222222222222222222");
            let key3 = Key32(*b"common23333333333333333333333333");

            trie.set(key1, b"val1");
            trie.set(key2, b"val2");
            trie.set(key3, b"val3");

            trie.root.as_ref().unwrap().print_tree();

            // Delete key3 should cause branch to collapse and extensions to merge
            assert!(trie.delete(key3));

            trie.root.as_ref().unwrap().print_tree();

            assert_eq!(trie.get(key1), Some(&b"val1".to_vec()));
            assert_eq!(trie.get(key2), Some(&b"val2".to_vec()));
            assert_eq!(trie.get(key3), None);
        }

        #[test]
        fn delete_nonexistent_key() {
            let mut trie = Trie::new();

            let key1 = Key32(*b"exists11111111111111111111111111");
            let key2 = Key32(*b"nothere1111111111111111111111111");

            trie.set(key1, b"value");

            // Should return false for non-existent key
            assert!(!trie.delete(key2));
            assert_eq!(trie.get(key1), Some(&b"value".to_vec()));
        }

        #[test]
        fn delete_from_empty_trie() {
            let mut trie = Trie::new();
            let key = Key32(*b"anykey11111111111111111111111111");

            assert!(!trie.delete(key));
        }

        #[test]
        fn nonexistent_key_returns_none() {
            let mut trie = Trie::new();

            let key = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
            let bad_key = Key32(*b"zyxwvutsrqponmlkjihgfedcba123456");

            trie.set(key, b"hello");

            assert_eq!(trie.get(key), Some(&b"hello".to_vec()));
            assert_eq!(trie.get(bad_key), None);
        }

        #[test]
        fn complex_trie_operations() {
            let mut trie = Trie::new();

            // This test builds a complex trie structure with branches and extensions
            let keys = [
                Key32(*b"j23456abcdefghijklmnopqrstuvwxyz"),
                Key32(*b"523456abcdefghijklmnopqrstuvwxyz"),
                Key32(*b"523456zyxwvutsrqponmlkjihgfedcba"),
                Key32(*b"523abcdefghijklmnopqrstuvwxyz123"),
                Key32(*b"523456q1111111111111111111111111"),
            ];

            let values = [b"val1", b"val2", b"val3", b"val4", b"val5"];

            // Insert all keys
            for (key, value) in keys.iter().zip(values.iter()) {
                trie.set(*key, value.to_vec());
            }

            // Verify all keys can be retrieved
            for (key, value) in keys.iter().zip(values.iter()) {
                assert_eq!(trie.get(*key), Some(&value.to_vec()));
            }

            // Verify non-existent key returns None
            let bad_key = Key32(*b"999999abcdefghijklmnopqrstuvwxyz");
            assert_eq!(trie.get(bad_key), None);
        }

        #[test]
        fn extension_splitting_scenario() {
            let mut trie = Trie::new();

            // First two keys create an extension
            let key1 = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
            let key2 = Key32(*b"123456abcdefghijklmnopqrstuvwxya");

            trie.set(key1, b"first");
            trie.set(key2, b"second");

            // This key should split the extension
            let key3 = Key32(*b"123456abcdefghijblmnopqrstuvwxyz");
            trie.set(key3, b"third");

            // All keys should be retrievable
            assert_eq!(trie.get(key1), Some(&b"first".to_vec()));
            assert_eq!(trie.get(key2), Some(&b"second".to_vec()));
            assert_eq!(trie.get(key3), Some(&b"third".to_vec()));
        }
    }

    // =====================================================================
    // Structure Tests - Verify internal trie structure for correctness
    // =====================================================================
    mod structure_tests {
        use super::*;

        #[test]
        fn empty_trie_has_none_root() {
            let trie = Trie::new();
            assert_eq!(trie.root, None);
        }

        #[test]
        fn single_insert_creates_leaf_root() {
            let mut trie = Trie::new();
            let key = Key32(random::<[u8; 32]>());

            trie.set(key, b"hello");

            assert!(matches!(trie.root, Some(Node::Leaf(_))));

            if let Some(Node::Leaf(leaf)) = &trie.root {
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
            assert!(matches!(trie.root, Some(Node::Branch(_))));
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
            assert!(matches!(trie.root, Some(Node::Extension(_))));

            if let Some(Node::Extension(ext)) = &trie.root {
                // Extension path should be the common prefix
                assert_eq!(ext.path, bytes_to_nibbles(&key1[..6]));
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
            assert!(matches!(trie.root, Some(Node::Branch(_))));

            if let Some(Node::Branch(root_branch)) = &trie.root {
                // Check j branch has a leaf
                let j_child = &root_branch.children[0x6a >> 4]; // 'j' nibble
                assert!(matches!(j_child, Some(b) if matches!(**b, Node::Leaf(_))));

                // Check 5 branch has an extension (common "23456")
                let five_child = &root_branch.children[0x35 >> 4]; // '5' nibble
                assert!(matches!(five_child, Some(b) if matches!(**b, Node::Extension(_))));
            }
        }
    }

    // =====================================================================
    // Unit Tests - Test individual methods and components
    // =====================================================================
    mod unit_tests {
        use super::*;

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
        fn leaf_diverge_with_identical_paths() {
            let path = NibblePath {
                nibbles: vec![1, 2, 3, 4],
            };
            let leaf = LeafNode::new(path.clone(), b"old".to_vec());

            let result = leaf.diverge_with(path.clone(), b"new".to_vec());

            // Should return a leaf with new value
            assert!(matches!(result, Node::Leaf(_)));
            if let Node::Leaf(new_leaf) = result {
                assert_eq!(new_leaf.path, path);
                assert_eq!(new_leaf.value, b"new".to_vec());
            }
        }

        #[test]
        fn leaf_diverge_with_different_paths() {
            let path1 = NibblePath {
                nibbles: vec![1, 2, 3, 4],
            };
            let path2 = NibblePath {
                nibbles: vec![1, 2, 5, 6],
            };
            let leaf = LeafNode::new(path1, b"val1".to_vec());

            let result = leaf.diverge_with(path2, b"val2".to_vec());

            // Should create an extension with branch
            assert!(matches!(result, Node::Extension(_)));
        }

        #[test]
        fn lcp_len_identical_slices() {
            let a = vec![1, 2, 3, 4];
            let b = vec![1, 2, 3, 4];
            assert_eq!(lcp_len(&a, &b), 4);
        }

        #[test]
        fn lcp_len_partial_match() {
            let a = vec![1, 2, 3, 4];
            let b = vec![1, 2, 5, 6];
            assert_eq!(lcp_len(&a, &b), 2);
        }

        #[test]
        fn lcp_len_no_match() {
            let a = vec![1, 2, 3, 4];
            let b = vec![5, 6, 7, 8];
            assert_eq!(lcp_len(&a, &b), 0);
        }

        #[test]
        fn lcp_len_different_lengths() {
            let a = vec![1, 2, 3, 4, 5];
            let b = vec![1, 2, 3];
            assert_eq!(lcp_len(&a, &b), 3);
        }

        #[test]
        fn bytes_to_nibbles_conversion() {
            let bytes = b"test";
            let nibbles = bytes_to_nibbles(bytes);

            assert_eq!(nibbles.nibbles.len(), 8);
            assert_eq!(nibbles.nibbles[0], 0x7); // 't' = 0x74
            assert_eq!(nibbles.nibbles[1], 0x4);
            assert_eq!(nibbles.nibbles[2], 0x6); // 'e' = 0x65
            assert_eq!(nibbles.nibbles[3], 0x5);
        }
    }
}
