//! Credentials
//!
//! A credential is (roughly) an El Gamal keypair used by a voter (section 4.7)
//! This keypair is generated by applying PBKDF2 to some "secret", represented as a Base58 value.

//! Belenios uses base58 in two places
//!   * defining UUIDs for each election, and
//!   * defining "credentials", which are later used to generate El Gamal keypairs.
//! This document defines both of these structs, and generally handles parsing base58.
use crate::datatypes::base58::{Base58, BASE58_STRLEN, INV_LOOKUPTABLE, LOOKUPTABLE};
use crate::datatypes::voter_ids::Voter_ID;
use crate::primitives::group::{Point, Scalar};
use ring::digest;
use ring::pbkdf2::{self, PBKDF2_HMAC_SHA256};
use ring::rand::SecureRandom;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};

/// A (public) Base58 string, which should uniquely identify the election that is occuring.
/// UUIDs need not have a valid checksum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UUID(Base58);

impl UUID {
    pub fn gen(rng: Arc<Mutex<dyn SecureRandom>>) -> Self {
        UUID(Base58::gen(rng))
    }
}

/// A (secret) Base58 string.
/// Note that Belenios uses 15-character strings, we use 22-character strings
/// to simplify the implementation.
///
/// For passwords to be valid, they must pass a certain checksum, described in section 4.7
/// of the specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Password(pub(crate) Base58);

impl Password {
    pub fn gen(rng: Arc<Mutex<dyn SecureRandom>>) -> Self {
        let mut pass = Password(Base58::gen(rng));
        pass.insert_checksum();
        pass
    }
    fn checksum(&self) -> u8 {
        let mut check: u128 = 0;
        for (idx, char_val) in self
            .0
             .0
            .as_bytes()
            .iter()
            .take(BASE58_STRLEN - 1)
            .rev()
            .enumerate()
        {
            // char_val is ASCII value corresponding to the character.
            // Convert to the index in ALPHABET_STR first
            // Note that ALPHABET.decode is normally a `pub(crate)`
            // field, so we forked `bs58` to remove the `(crate)`.
            check += (58_u128.pow(idx as u32) * (INV_LOOKUPTABLE[*char_val as usize] as u128))
                .rem_euclid(53_u128);
        }
        ((53_isize - (check as isize)).rem_euclid(53)) as u8
    }
    /// Inserts a checksum to the final index of a Base58,
    /// overwriting what is already there.
    fn insert_checksum(&mut self) {
        let check = self.checksum();
        self.0 .0.pop();
        self.0 .0.push(LOOKUPTABLE[check as usize] as char);
    }
    /// Validates that a `Base58` has a valid checksum.
    pub fn validate_checksum(&self) -> bool {
        self.0 .0.as_bytes()[BASE58_STRLEN - 1] == LOOKUPTABLE[self.checksum() as usize]
    }
}

#[derive(Debug, Clone)]
pub struct Credential {
    password: Password,
    uuid: UUID,
}

impl Credential {
    pub fn gen(rng: Arc<Mutex<dyn SecureRandom>>, uuid: &UUID) -> Self {
        let password = Password::gen(rng);
        Credential {
            password,
            uuid: uuid.clone(),
        }
    }
}

impl From<(Password, UUID)> for Credential {
    fn from((pass, uuid): (Password, UUID)) -> Self {
        Self {
            password: pass,
            uuid,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedCredential {
    pub(crate) password: Password,
    pub(crate) uuid: UUID,
    pub(crate) secret_key: Scalar,
    pub(crate) public_key: Point,
}

impl ExpandedCredential {
    pub fn gen(rng: Arc<Mutex<dyn SecureRandom>>, uuid: &UUID) -> Self {
        Credential::gen(rng, uuid).into()
    }
}

impl From<Credential> for ExpandedCredential {
    fn from(c: Credential) -> Self {
        // I do not believe the hash used in PBKDF2 needs to be domain-separated,
        // it seems like only really the hashes in the ZKPs need to be.
        let mut out = [0; digest::SHA256_OUTPUT_LEN];
        let algo = PBKDF2_HMAC_SHA256;
        let iter = NonZeroU32::new(1000).unwrap();
        let salt = (&c.uuid.0).into();
        let secret = (&c.password.0).into();
        pbkdf2::derive(algo, iter, salt, secret, &mut out);
        let secret_key = Scalar::from_bytes_mod_order(out);
        let public_key = Point::generator() * secret_key;

        ExpandedCredential {
            password: c.password,
            uuid: c.uuid,
            secret_key,
            public_key,
        }
    }
}

impl From<ExpandedCredential> for Credential {
    fn from(expanded: ExpandedCredential) -> Self {
        Credential {
            uuid: expanded.uuid,
            password: expanded.password,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_credential_generation() {
        //! Credentials should have valid checksums
        const RUNS: usize = 1000;
        let rng = Arc::new(Mutex::new(ring::rand::SystemRandom::new()));
        let uuid = UUID::gen(rng.clone());
        for _ in 0..RUNS {
            let cred = Credential::gen(rng.clone(), &uuid);
            // Ensures the generated password has a valid checksum
            assert!(cred.password.validate_checksum())
        }
    }
}
