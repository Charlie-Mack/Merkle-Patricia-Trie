use sled::{Db, Tree};

pub trait HashDB {
    type Error: std::fmt::Debug;
    fn get(&self, key: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error>;
    fn put(&self, key: [u8; 32], value: Vec<u8>) -> Result<(), Self::Error>;
    fn flush(&self) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub struct SledDB {
    tree: Tree,
}

impl SledDB {
    pub fn open(path: impl AsRef<std::path::Path>, tree_name: &str) -> Result<Self, sled::Error> {
        let db: Db = sled::open(path)?;
        let tree = db.open_tree(tree_name.as_bytes())?;
        Ok(Self { tree })
    }
}

impl HashDB for SledDB {
    type Error = sled::Error;

    fn get(&self, key: &[u8; 32]) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.tree.get(key)?.map(|ivec| ivec.to_vec()))
    }

    fn put(&self, key: [u8; 32], value: Vec<u8>) -> Result<(), Self::Error> {
        println!("putting key: {:x?}", key);
        println!("value: {:x?}", value);
        // idempotent: same key always same value
        self.tree.insert(key, value)?;

        Ok(())
    }

    fn flush(&self) -> Result<(), Self::Error> {
        self.tree.flush()?; // or flush_async().wait()
        Ok(())
    }
}
