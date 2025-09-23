#[cfg(test)]

// =====================================================================
// API Tests - Test functionality through public interface
// =====================================================================
mod api_tests {

    use merkle_patricia_trie::trie::{Key32, Trie};
    use rand::random;

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
        assert_eq!(trie.get(key), Some(b"hello".to_vec()));
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

        assert_eq!(trie.get(key), Some(b"world".to_vec()));
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

        assert_eq!(trie.get(key1), Some(b"value1".to_vec()));
        assert_eq!(trie.get(key2), Some(b"value2".to_vec()));
    }

    #[test]
    fn multiple_keys_with_common_prefix() {
        let mut trie = Trie::new();

        let key1 = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
        let key2 = Key32(*b"123456zyxwvutsrqponmlkjihgfedcba");

        trie.set(key1, b"value1");
        trie.set(key2, b"value2");

        assert_eq!(trie.get(key1), Some(b"value1".to_vec()));
        assert_eq!(trie.get(key2), Some(b"value2".to_vec()));
    }

    #[test]
    fn delete_with_branch_collapse() {
        let mut trie = Trie::new();
        let key1 = Key32(*b"123456abcdefghijklmnopqrstuvwxyz");
        let key2 = Key32(*b"k23456zyxwvutsrqponmlkjihgfedcba");

        trie.set(key1, b"value1");
        trie.set(key2, b"value2");

        trie.print_tree();

        println!("\nDeleting key1\n");

        trie.delete(key1);

        trie.print_tree();
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
        trie_versions.push(trie.root().cloned()); // Empty trie

        // Insert all keys
        for (key, value) in keys.iter().zip(values.iter()) {
            trie.set(*key, value.to_vec());
            trie_versions.push(trie.root().cloned()); // Clone the Option<&Node> to Option<Node>
        }

        // Delete in reverse order and verify trie matches previous versions
        for i in (0..keys.len()).rev() {
            trie.delete(keys[i]);
            assert_eq!(
                trie.root(),
                trie_versions[i].as_ref(),
                "Failed after deleting key {}",
                i
            );
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
        assert_eq!(trie.get(keys[0]), Some(b"val0".to_vec()));
        assert_eq!(trie.get(keys[1]), None);
        assert_eq!(trie.get(keys[2]), Some(b"val2".to_vec()));
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

        trie.print_tree();

        // Delete key3 should cause branch to collapse and extensions to merge
        assert!(trie.delete(key3));

        trie.print_tree();

        assert_eq!(trie.get(key1), Some(b"val1".to_vec()));
        assert_eq!(trie.get(key2), Some(b"val2".to_vec()));
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
        assert_eq!(trie.get(key1), Some(b"value".to_vec()));
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

        assert_eq!(trie.get(key), Some(b"hello".to_vec()));
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
            assert_eq!(trie.get(*key), Some(value.to_vec()));
        }

        trie.print_tree();

        println!("root: {:?}", trie.root().unwrap());

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
        assert_eq!(trie.get(key1), Some(b"first".to_vec()));
        assert_eq!(trie.get(key2), Some(b"second".to_vec()));
        assert_eq!(trie.get(key3), Some(b"third".to_vec()));
    }

    #[test]
    fn complex_trie_operations_with_db() {
        let mut trie = Trie::with_db("db", "mpt");

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
            assert_eq!(trie.get(*key), Some(value.to_vec()));
        }

        trie.commit();

        let value = trie.get(keys[2]);
        println!(
            "value: {:?}",
            String::from_utf8(value.clone().unwrap()).unwrap()
        );
        assert_eq!(value, Some(b"val3".to_vec()));
    }

    #[test]
    fn commit_trie_with_db() {
        let mut trie = Trie::with_db("db", "mpt");
        let key = String::from("hello").into();
        println!("key: {:x?}", key);
        trie.set(key, b"world");

        trie.commit();

        println!("\n\n NOW WE GET THE VALUE FROM THE DB \n\n");

        //Now we get the value from the db
        let value = trie.get(key);
        println!("value: {:?}", value);
    }

    // #[test]
    // fn get_trie_with_db() {
    //     let trie = Trie::with_db("db", "mpt");
    //     let key = String::from("hello").into();
    //     let value = trie.get(key);
    //     assert_eq!(value, Some(b"world".to_vec()));
    // }
}
