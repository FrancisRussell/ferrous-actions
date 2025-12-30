use rustup_toolchain_manifest::HashValue;

pub fn build(num_bytes: usize) -> HashValue {
    let mut bytes = vec![0u8; num_bytes];
    getrandom::fill(&mut bytes).unwrap_or_else(|e| panic!("Unable to get random data: {}", e));
    HashValue::from_bytes(&bytes)
}
