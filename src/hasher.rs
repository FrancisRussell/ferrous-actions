use rustup_toolchain_manifest::HashValue;

#[derive(Debug, Default)]
pub struct Blake3 {
    inner: blake3::Hasher,
}

impl std::hash::Hasher for Blake3 {
    fn finish(&self) -> u64 {
        let mut xof = self.inner.finalize_xof();
        let mut bytes = [0u8; 8];
        xof.fill(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn write(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }
}

impl Blake3 {
    pub fn inner(&self) -> &blake3::Hasher {
        &self.inner
    }

    pub fn hash_value(&self) -> HashValue {
        let hash = self.inner.finalize();
        HashValue::from_bytes(&hash.as_bytes()[..])
    }
}
