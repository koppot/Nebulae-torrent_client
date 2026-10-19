use std::env;
use std::fs;
use minisign_verify::{PublicKey, Signature};

const TRUSTED_PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDQ1MkNBQThCQkJCQzZGNDEKUldSQmI3eTdpNm9zUmFSM2RQOVlHZllGeFM1dHlpNU5JRitYaytjK3BNMTFwMHYrZjBWcXllNVgK";

fn base64_to_string(base64_string: &str) -> Result<String, Box<dyn std::error::Error>> {
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        base64_string.trim(),
    )?;
    Ok(String::from_utf8(decoded)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: verify-payload-signature <payload_file> <sig_file>");
        std::process::exit(1);
    }
    let payload_path = &args[1];
    let sig_path = &args[2];

    let data = fs::read(payload_path)
        .map_err(|e| format!("Failed to read payload {payload_path}: {e}"))?;
    let sig_raw = fs::read_to_string(sig_path)
        .map_err(|e| format!("Failed to read signature {sig_path}: {e}"))?;

    println!("Verifying target payload: {} ({} bytes)", payload_path, data.len());

    let pub_key_decoded = base64_to_string(TRUSTED_PUBKEY_B64)?;
    let public_key = PublicKey::decode(&pub_key_decoded)
        .map_err(|e| format!("Invalid trusted public key: {e}"))?;

    let sig_text = if sig_raw.trim().starts_with("untrusted comment") {
        sig_raw.clone()
    } else {
        base64_to_string(&sig_raw)?
    };

    let signature = Signature::decode(&sig_text)
        .map_err(|e| format!("Failed to decode Minisign signature: {e}"))?;

    // Cryptographically verify payload data and global trusted comment
    public_key.verify(&data, &signature, true)
        .map_err(|e| format!("CRYPTOGRAPHIC VERIFICATION FAILED: {e}"))?;

    println!("============================================================");
    println!("CRYPTOGRAPHIC MINISIGN VERIFICATION: 100% SUCCESSFUL!");
    println!("- Payload: {}", payload_path);
    println!("- Ed25519 Payload Signature: VALID");
    println!("- Minisign Global Signature: VALID");
    println!("- Trusted Comment: {:?}", signature.trusted_comment());
    println!("============================================================");

    Ok(())
}
