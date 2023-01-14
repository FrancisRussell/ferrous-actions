use base64::engine::general_purpose::GeneralPurpose;
use base64::Engine as _;

fn build_engine() -> GeneralPurpose {
    let config = base64::engine::general_purpose::NO_PAD;
    let alphabet = &base64::alphabet::URL_SAFE;
    GeneralPurpose::new(alphabet, config)
}

pub fn encode<I: AsRef<[u8]>>(input: I) -> String {
    let engine = build_engine();
    engine.encode(input.as_ref())
}

pub fn decode<I: AsRef<[u8]>>(input: I) -> Result<Vec<u8>, base64::DecodeError> {
    let engine = build_engine();
    let result = engine.decode(input.as_ref())?;
    Ok(result)
}
