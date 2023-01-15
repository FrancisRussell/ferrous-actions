use crate::system::rng;
use rustup_toolchain_manifest::HashValue;

pub fn build(num_bytes: usize) -> HashValue {
    let mut bytes = vec![0u8; num_bytes];
    let mut rng = rng::MathRandom::default();
    rng.fill_bytes(&mut bytes);
    HashValue::from_bytes(&bytes)
}
