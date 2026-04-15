use hmac::Hmac;
use pbkdf2::pbkdf2;
use sha1::Sha1;

use crate::error::NigoriError;

pub const KEY_SIZE: usize = 16;

/// The three derived keys used by Nigori.
#[derive(Clone)]
pub struct Keys {
    /// Legacy user key (not used for encryption, kept for backward compatibility).
    pub user_key: Option<[u8; KEY_SIZE]>,
    /// AES-128-CBC encryption key.
    pub encryption_key: [u8; KEY_SIZE],
    /// HMAC-SHA256 MAC key.
    pub mac_key: [u8; KEY_SIZE],
}

/// Key derivation method selection.
pub enum KeyDerivationParams {
    Pbkdf2,
    Scrypt { salt: Vec<u8> },
}

impl KeyDerivationParams {
    pub fn pbkdf2() -> Self {
        Self::Pbkdf2
    }

    pub fn scrypt(salt: Vec<u8>) -> Self {
        Self::Scrypt { salt }
    }
}

/// Hardcoded PBKDF2 salt, derived historically from:
/// `PBKDF2_HMAC_SHA1(Ns("dummy") + Ns("localhost"), "saltsalt", 1001, 128)`
const PBKDF2_SALT: [u8; 16] = [
    0xc7, 0xca, 0xfb, 0x23, 0xec, 0x2a, 0x9d, 0x4c, 0x03, 0x5a, 0x90, 0xae, 0xed, 0x8b, 0xa4, 0x98,
];

impl Keys {
    /// Derive keys from a password using the specified method.
    pub fn derive(params: &KeyDerivationParams, password: &str) -> Result<Self, NigoriError> {
        match params {
            KeyDerivationParams::Pbkdf2 => Self::derive_pbkdf2(password),
            KeyDerivationParams::Scrypt { salt } => Self::derive_scrypt(password, salt),
        }
    }

    /// Import keys from raw bytes.
    pub fn import(
        user_key: &[u8],
        encryption_key: &[u8],
        mac_key: &[u8],
    ) -> Result<Self, NigoriError> {
        if encryption_key.len() != KEY_SIZE || mac_key.len() != KEY_SIZE {
            return Err(NigoriError::InvalidKeySize);
        }

        let user = if user_key.len() == KEY_SIZE {
            let mut buf = [0u8; KEY_SIZE];
            buf.copy_from_slice(user_key);
            Some(buf)
        } else {
            None
        };

        let mut enc = [0u8; KEY_SIZE];
        enc.copy_from_slice(encryption_key);
        let mut mac = [0u8; KEY_SIZE];
        mac.copy_from_slice(mac_key);

        Ok(Self {
            user_key: user,
            encryption_key: enc,
            mac_key: mac,
        })
    }

    /// PBKDF2-HMAC-SHA1 derivation with domain separation via iteration counts.
    fn derive_pbkdf2(password: &str) -> Result<Self, NigoriError> {
        let mut user_key = [0u8; KEY_SIZE];
        let mut encryption_key = [0u8; KEY_SIZE];
        let mut mac_key = [0u8; KEY_SIZE];

        pbkdf2::<Hmac<Sha1>>(password.as_bytes(), &PBKDF2_SALT, 1002, &mut user_key)
            .map_err(|_| NigoriError::KeyDerivation)?;
        pbkdf2::<Hmac<Sha1>>(password.as_bytes(), &PBKDF2_SALT, 1003, &mut encryption_key)
            .map_err(|_| NigoriError::KeyDerivation)?;
        pbkdf2::<Hmac<Sha1>>(password.as_bytes(), &PBKDF2_SALT, 1004, &mut mac_key)
            .map_err(|_| NigoriError::KeyDerivation)?;

        Ok(Self {
            user_key: Some(user_key),
            encryption_key,
            mac_key,
        })
    }

    /// Scrypt derivation: derives 32 bytes and splits into enc + mac keys.
    fn derive_scrypt(password: &str, salt: &[u8]) -> Result<Self, NigoriError> {
        let params =
            scrypt::Params::new(13, 8, 11, KEY_SIZE * 2).map_err(|_| NigoriError::KeyDerivation)?;

        let mut derived = [0u8; KEY_SIZE * 2];
        scrypt::scrypt(password.as_bytes(), salt, &params, &mut derived)
            .map_err(|_| NigoriError::KeyDerivation)?;

        let mut encryption_key = [0u8; KEY_SIZE];
        let mut mac_key = [0u8; KEY_SIZE];
        encryption_key.copy_from_slice(&derived[..KEY_SIZE]);
        mac_key.copy_from_slice(&derived[KEY_SIZE..]);

        // user_key is all zeros for backward compatibility with legacy clients.
        Ok(Self {
            user_key: Some([0u8; KEY_SIZE]),
            encryption_key,
            mac_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_known_vectors() {
        let keys = Keys::derive(&KeyDerivationParams::pbkdf2(), "hunter2").unwrap();
        let user = keys.user_key.unwrap();
        assert_eq!(hex::encode(user), "025599e143c4923d77f65b99d97019a3");
        assert_eq!(
            hex::encode(keys.encryption_key),
            "4596bf346572497d92b2a0e2146d93c1"
        );
        assert_eq!(
            hex::encode(keys.mac_key),
            "2292ad9db96fe590b22a58db50f6f545"
        );
    }

    #[test]
    fn scrypt_known_vectors() {
        let keys = Keys::derive(
            &KeyDerivationParams::scrypt(b"alpensalz".to_vec()),
            "hunter2",
        )
        .unwrap();
        let user = keys.user_key.unwrap();
        assert_eq!(hex::encode(user), "00000000000000000000000000000000");
        assert_eq!(
            hex::encode(keys.encryption_key),
            "8aa735e0091339a5e51da3b3dd1b328a"
        );
        assert_eq!(
            hex::encode(keys.mac_key),
            "a7e73611968dfd2bca5b3382aed451ba"
        );
    }

    #[test]
    fn import_valid_keys() {
        let enc = [1u8; KEY_SIZE];
        let mac = [2u8; KEY_SIZE];
        let user = [3u8; KEY_SIZE];
        let keys = Keys::import(&user, &enc, &mac).unwrap();
        assert_eq!(keys.encryption_key, enc);
        assert_eq!(keys.mac_key, mac);
        assert_eq!(keys.user_key.unwrap(), user);
    }

    #[test]
    fn import_empty_user_key() {
        let enc = [1u8; KEY_SIZE];
        let mac = [2u8; KEY_SIZE];
        let keys = Keys::import(&[], &enc, &mac).unwrap();
        assert!(keys.user_key.is_none());
    }

    #[test]
    fn import_invalid_enc_key_size() {
        let result = Keys::import(&[], &[0u8; 8], &[0u8; KEY_SIZE]);
        assert!(result.is_err());
    }

    #[test]
    fn import_all_empty_fails() {
        let result = Keys::import(&[], &[], &[]);
        assert!(result.is_err());
    }
}
