//! Rust implementation of the Nigori protocol for Chrome Sync.
//!
//! Nigori securely stores secrets in the cloud using password-derived keys.
//! See: <https://www.cl.cam.ac.uk/~drt24/nigori/nigori-overview.pdf>

mod error;
mod keys;
mod stream;

pub use error::NigoriError;
pub use keys::{KEY_SIZE, KeyDerivationParams, Keys};

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use stream::NigoriStream;

const IV_SIZE: usize = 16;
const HASH_SIZE: usize = 32;
const NIGORI_KEY_NAME: &str = "nigori-key";

type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

/// A Nigori instance holds derived keys and provides encrypt/decrypt operations.
pub struct Nigori {
    keys: Keys,
}

impl Nigori {
    /// Create a Nigori by deriving keys from a password.
    pub fn create_by_derivation(
        params: &KeyDerivationParams,
        password: &str,
    ) -> Result<Self, NigoriError> {
        let keys = Keys::derive(params, password)?;
        Ok(Self { keys })
    }

    /// Create a Nigori by importing raw key material.
    pub fn create_by_import(
        user_key: &[u8],
        encryption_key: &[u8],
        mac_key: &[u8],
    ) -> Result<Self, NigoriError> {
        let keys = Keys::import(user_key, encryption_key, mac_key)?;
        Ok(Self { keys })
    }

    /// Derive a deterministic lookup name for this key.
    ///
    /// Computed as `Permute[Kenc, Kmac](Password || "nigori-key")`.
    /// Returns a Base64-encoded string.
    pub fn get_key_name(&self) -> String {
        let plaintext = NigoriStream::new()
            .push_type(1) // Nigori::Password
            .push_str(NIGORI_KEY_NAME)
            .into_bytes();

        let iv = [0u8; IV_SIZE];
        let ciphertext = aes_cbc_encrypt(&self.keys.encryption_key, &iv, &plaintext);
        let mac = hmac_sha256(&self.keys.mac_key, &ciphertext);

        let mut output = Vec::with_capacity(ciphertext.len() + HASH_SIZE);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&mac);
        BASE64.encode(output)
    }

    /// Encrypt a plaintext value. Returns a Base64-encoded string.
    ///
    /// Format: `Base64(IV || AES-CBC(plaintext) || HMAC-SHA256(ciphertext))`
    pub fn encrypt(&self, plaintext: &[u8]) -> String {
        let mut iv = [0u8; IV_SIZE];
        rand::rng().fill_bytes(&mut iv);

        let ciphertext = aes_cbc_encrypt(&self.keys.encryption_key, &iv, plaintext);
        let mac = hmac_sha256(&self.keys.mac_key, &ciphertext);

        let mut output = Vec::with_capacity(IV_SIZE + ciphertext.len() + HASH_SIZE);
        output.extend_from_slice(&iv);
        output.extend_from_slice(&ciphertext);
        output.extend_from_slice(&mac);
        BASE64.encode(output)
    }

    /// Decrypt a Base64-encoded ciphertext. Returns the plaintext bytes.
    pub fn decrypt(&self, encrypted: &str) -> Result<Vec<u8>, NigoriError> {
        let input = BASE64
            .decode(encrypted)
            .map_err(|_| NigoriError::Base64Decode)?;

        // Minimum: IV (16) + one AES block (16) + HMAC (32) = 64
        // But Chromium checks: kIvSize * 2 + kHashSize = 16 * 2 + 32 = 64
        if input.len() < IV_SIZE * 2 + HASH_SIZE {
            return Err(NigoriError::CiphertextTooShort);
        }

        let iv = &input[..IV_SIZE];
        let ciphertext = &input[IV_SIZE..input.len() - HASH_SIZE];
        let mac = &input[input.len() - HASH_SIZE..];

        // Verify HMAC before decryption
        let expected_mac = hmac_sha256(&self.keys.mac_key, ciphertext);
        if !constant_time_eq(mac, &expected_mac) {
            return Err(NigoriError::HmacVerification);
        }

        aes_cbc_decrypt(&self.keys.encryption_key, iv, ciphertext)
    }

    /// Export the raw key material.
    pub fn export_keys(&self) -> (&Option<[u8; KEY_SIZE]>, &[u8; KEY_SIZE], &[u8; KEY_SIZE]) {
        (
            &self.keys.user_key,
            &self.keys.encryption_key,
            &self.keys.mac_key,
        )
    }

    /// Generate a random 32-byte salt for Scrypt derivation.
    pub fn generate_scrypt_salt() -> Vec<u8> {
        let mut salt = vec![0u8; 32];
        rand::rng().fill_bytes(&mut salt);
        salt
    }
}

fn aes_cbc_encrypt(key: &[u8; KEY_SIZE], iv: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let enc = Aes128CbcEnc::new(key.into(), iv.into());
    enc.encrypt_padded_vec_mut::<Pkcs7>(plaintext)
}

fn aes_cbc_decrypt(
    key: &[u8; KEY_SIZE],
    iv: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, NigoriError> {
    let dec = Aes128CbcDec::new(key.into(), iv.into());
    dec.decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| NigoriError::Decryption)
}

fn hmac_sha256(key: &[u8; KEY_SIZE], data: &[u8]) -> [u8; HASH_SIZE] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key size is always valid");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // Chromium test vector: password="password", known key name
    #[test]
    fn get_key_name_chromium_vector() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let expected = "ibGL7ymU0Si+eYCXGS6SBHPFT+JCYiB6GDOYqj6vIwEi\
                        WJ7RENSHxmIQ8Q3rXd/UnZUmFHYB+jSIbthQADXvrQ==";
        assert_eq!(nigori.get_key_name(), expected);
    }

    #[test]
    fn get_key_name_is_deterministic() {
        let n1 = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let n2 = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        assert_eq!(n1.get_key_name(), n2.get_key_name());
    }

    // Chromium test vector: decrypt known ciphertext
    #[test]
    fn decrypt_chromium_vector() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let encrypted = "NNYlnzaaLPXWXyzz8J+u4OKgLiKRBPu2GJdjHWk0m3ADZrJhnmer30\
                         Zgiy4Ulxlfh6fmS71k8rop+UvSJdL1k/fcNLJ1C6sY5Z86ijyl1Jo=";
        let plaintext = nigori.decrypt(encrypted).unwrap();
        assert_eq!(
            std::str::from_utf8(&plaintext).unwrap(),
            "test, test, 1, 2, 3"
        );
    }

    #[test]
    fn encrypt_produces_different_iv_each_time() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let a = nigori.encrypt(b"value");
        let b = nigori.encrypt(b"value");
        assert_ne!(a, b);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let plaintext = b"value";
        let encrypted = nigori.encrypt(plaintext);
        let decrypted = nigori.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_empty_string() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let encrypted = nigori.encrypt(b"");
        let decrypted = nigori.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, b"");
    }

    #[test]
    fn corrupted_ciphertext_fails_hmac() {
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let mut encrypted = BASE64.decode(nigori.encrypt(b"test")).unwrap();

        // Corrupt a byte in the ciphertext region (after IV, before HMAC)
        encrypted[IV_SIZE + 2] ^= 0xff;

        let encoded = BASE64.encode(&encrypted);
        assert!(matches!(
            nigori.decrypt(&encoded),
            Err(NigoriError::HmacVerification)
        ));
    }

    #[test]
    fn export_import_roundtrip() {
        let n1 = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let (user_key, enc_key, mac_key) = n1.export_keys();
        let user_bytes = user_key.as_ref().map_or(&[] as &[u8], |k| k.as_slice());

        let n2 = Nigori::create_by_import(user_bytes, enc_key, mac_key).unwrap();

        // Cross-encrypt/decrypt
        let encrypted = n1.encrypt(b"test");
        assert_eq!(n2.decrypt(&encrypted).unwrap(), b"test");

        let encrypted = n2.encrypt(b"test");
        assert_eq!(n1.decrypt(&encrypted).unwrap(), b"test");

        // Same key name
        assert_eq!(n1.get_key_name(), n2.get_key_name());
    }

    #[test]
    fn import_empty_user_key_tolerated() {
        let n1 = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "password").unwrap();
        let (_, enc_key, mac_key) = n1.export_keys();

        let n2 = Nigori::create_by_import(&[], enc_key, mac_key).unwrap();
        let (user_key, _, _) = n2.export_keys();
        assert!(user_key.is_none());
    }

    #[test]
    fn import_empty_keys_fails() {
        assert!(Nigori::create_by_import(&[], &[], &[]).is_err());
    }

    #[test]
    fn import_invalid_size_fails() {
        assert!(Nigori::create_by_import(b"foo", b"bar", b"baz").is_err());
    }

    // Go-nigori test vector: permute
    #[test]
    fn go_nigori_permute_vector() {
        let password = "CAMSEM3y43hLmgd9Zr8e0U7YsioaIJTpcvWg+uX00KlEOAdJuLlKqGen1P0agzDUVV9fdlqK";
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), password).unwrap();
        let expected = "ptImbFuhf1RXqsQte0TrnmJZ1ij9azjQYIrXTheZlJY/\
                        xDg9e/QCNfpE5aMj7TagFPVNpy7PeG7jlW4xExVU0Q==";
        assert_eq!(nigori.get_key_name(), expected);
    }

    // Go-nigori test vector: decrypt known ciphertext
    #[test]
    fn go_nigori_decrypt_vector() {
        let nigori = Nigori::create_by_derivation(&KeyDerivationParams::pbkdf2(), "key").unwrap();
        let encrypted = "vMOfpOoalnFFTYKzkacJMMFG9F9F7f9d2mNPGL5JhgXKMOkdS9STLb9FY95/\
                         D7bZPk0vYkuyonIx68YszLBjCh2qnmjmnmQJF7qRTIeO9Ec=";
        let plaintext = nigori.decrypt(encrypted).unwrap();
        assert_eq!(
            std::str::from_utf8(&plaintext).unwrap(),
            "thisistuhotnuhnoehunteoh"
        );
    }

    #[test]
    fn scrypt_encrypt_decrypt_roundtrip() {
        let salt = Nigori::generate_scrypt_salt();
        let nigori =
            Nigori::create_by_derivation(&KeyDerivationParams::scrypt(salt), "password").unwrap();
        let plaintext = b"scrypt roundtrip test";
        let encrypted = nigori.encrypt(plaintext);
        let decrypted = nigori.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn generate_scrypt_salt_size() {
        assert_eq!(Nigori::generate_scrypt_salt().len(), 32);
    }

    #[test]
    fn generate_scrypt_salt_nontrivial() {
        let salt = Nigori::generate_scrypt_salt();
        // At least two different bytes
        let first = salt[0];
        assert!(salt.iter().any(|&b| b != first));
    }
}
