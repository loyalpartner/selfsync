#[derive(Debug, thiserror::Error)]
pub enum NigoriError {
    #[error("key derivation failed")]
    KeyDerivation,
    #[error("invalid key size")]
    InvalidKeySize,
    #[error("base64 decode failed")]
    Base64Decode,
    #[error("ciphertext too short")]
    CiphertextTooShort,
    #[error("HMAC verification failed")]
    HmacVerification,
    #[error("decryption failed")]
    Decryption,
}
