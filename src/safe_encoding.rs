use base64::engine::fast_portable::FastPortable;

fn build_engine() -> FastPortable {
    let config = base64::engine::fast_portable::NO_PAD;
    let alphabet = &base64::alphabet::URL_SAFE;
    FastPortable::from(&alphabet, config)
}

pub fn encode<I: AsRef<[u8]>>(input: I) -> String {
    let engine = build_engine();
    base64::encode_engine(input.as_ref(), &engine)
}

pub fn decode<I: AsRef<[u8]>>(input: I) -> Result<Vec<u8>, base64::DecodeError> {
    let engine = build_engine();
    let result = base64::decode_engine(input.as_ref(), &engine)?;
    Ok(result)
}
