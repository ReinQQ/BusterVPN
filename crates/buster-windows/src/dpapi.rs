//! DPAPI (Data Protection API) wrapper for Windows.
//!
//! Provides encryption and decryption of secrets (server configs, credentials, subscriptions)
//! tied to the current Windows user profile, with app-specific entropy to prevent
//! cross-purpose decryption.

use crate::error::WindowsError;
use sha2::{Digest, Sha256};
use windows::core::PWSTR;
use windows::Win32::Foundation::LocalFree;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

fn derive_entropy(purpose: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    let label = format!("BusterVPN.{}.v1", purpose);
    hasher.update(label.as_bytes());
    hasher.finalize().to_vec()
}

/// Encrypts bytes using Windows DPAPI (CurrentUser) with entropy derived from `purpose`.
pub fn protect(plaintext: &[u8], purpose: &str) -> Result<Vec<u8>, WindowsError> {
    if plaintext.is_empty() {
        return Err(WindowsError::Win32(windows::core::Error::new(
            windows::core::HRESULT(-1),
            "DPAPI input is empty",
        )));
    }

    let entropy = derive_entropy(purpose);

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };

    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };

    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptProtectData(
            &in_blob,
            PWSTR::null(),
            Some(&entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )?;

        if out_blob.pbData.is_null() || out_blob.cbData == 0 {
            return Err(WindowsError::Win32(windows::core::Error::new(
                windows::core::HRESULT(-1),
                "DPAPI returned empty output",
            )));
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();

        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));

        Ok(result)
    }
}

/// Decrypts DPAPI-encrypted ciphertext with entropy derived from `purpose`.
pub fn unprotect(ciphertext: &[u8], purpose: &str) -> Result<Vec<u8>, WindowsError> {
    if ciphertext.is_empty() {
        return Err(WindowsError::Win32(windows::core::Error::new(
            windows::core::HRESULT(-1),
            "DPAPI ciphertext is empty",
        )));
    }

    let entropy = derive_entropy(purpose);

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: ciphertext.len() as u32,
        pbData: ciphertext.as_ptr() as *mut u8,
    };

    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };

    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    unsafe {
        CryptUnprotectData(
            &in_blob,
            None,
            Some(&entropy_blob),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )?;

        if out_blob.pbData.is_null() || out_blob.cbData == 0 {
            return Err(WindowsError::Win32(windows::core::Error::new(
                windows::core::HRESULT(-1),
                "DPAPI returned empty decrypted data",
            )));
        }

        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();

        let _ = LocalFree(windows::Win32::Foundation::HLOCAL(out_blob.pbData as _));

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpapi_roundtrip() {
        let secret = b"super_secret_vpn_private_key_or_password";
        let purpose = "ProfileStore";

        let encrypted = protect(secret, purpose).expect("encryption failed");
        assert_ne!(encrypted, secret);

        let decrypted = unprotect(&encrypted, purpose).expect("decryption failed");
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn test_dpapi_wrong_purpose_fails() {
        let secret = b"test_secret";
        let encrypted = protect(secret, "PurposeA").expect("encryption failed");

        // Decrypting with wrong purpose must fail
        let result = unprotect(&encrypted, "PurposeB");
        assert!(result.is_err());
    }
}
