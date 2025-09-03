use super::{Key32, NibblePath};
use std::{array, fmt};

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
pub enum DeleteResult {
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
        let k = self.path.lcp_len(b);

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
        let k = self.path.lcp_len(b);

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

impl Node {
    fn dummy() -> Self {
        Node::Branch(BranchNode::new())
    }

    pub fn new_leaf(path: NibblePath, value: Vec<u8>) -> Self {
        Node::Leaf(LeafNode::new(path, value))
    }

    pub fn delete(&mut self, path: NibblePath) -> DeleteResult {
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

    pub fn get(&self, path: NibblePath) -> Option<&Vec<u8>> {
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

    pub fn insert(&mut self, path: NibblePath, value: Vec<u8>) {
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

#[cfg(test)]
mod unit_tests {
    use super::*;

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
}
