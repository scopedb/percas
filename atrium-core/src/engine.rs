use foyer::Cache;

pub struct FoyerEngine {
    inner: Cache<Vec<u8>, Vec<u8>>,
}

impl FoyerEngine {
    pub fn new(cache: Cache<Vec<u8>, Vec<u8>>) -> Self {
        FoyerEngine { inner: cache }
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.inner.get(key).map(|v| v.value().clone())
    }

    pub fn put(&self, key: &[u8], value: &[u8]) {
        self.inner.insert(key.to_vec(), value.to_vec());
    }

    pub fn delete(&self, key: &[u8]) {
        self.inner.remove(key);
    }
}

#[cfg(test)]
mod tests {
    use foyer::CacheBuilder;

    use super::*;

    #[test]
    fn test_get() {
        let engine = FoyerEngine::new(CacheBuilder::new(1024).build());
        engine.put(b"foo", b"bar");
        assert_eq!(engine.get(b"foo"), Some(b"bar".to_vec()));
    }
}
