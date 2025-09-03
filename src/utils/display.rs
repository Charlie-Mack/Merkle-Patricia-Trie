use crate::trie::node::Node;
use std::fmt;

pub trait NodeDisplay {
    fn print_tree(&self);
    fn print_tree_recursive(&self, prefix: &str, is_last: bool);
    fn fmt_indent(&self, f: &mut fmt::Formatter, indent: usize) -> fmt::Result;
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_indent(f, 0)
    }
}

impl NodeDisplay for Node {
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

    fn print_tree(&self) {
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
                let active_children: Vec<_> = branch
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
