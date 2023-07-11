// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::serde::{ByteRepr, BytesVisitor};
#[cfg(not(test))]
use crate::types::Vote;
use crate::types::{
    AuthorityIndex, BaseStatement, BlockReference, RoundNumber, StatementBlock, TimestampNs,
    Transaction,
};
#[cfg(not(test))]
use digest::Digest;
#[cfg(not(test))]
use ed25519_consensus::Signature;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
#[cfg(test)]
use std::hash::Hasher;
use zeroize::Zeroize;

pub const SIGNATURE_SIZE: usize = 64;
pub const TRANSACTION_DIGEST_SIZE: usize = 32;
pub const BLOCK_DIGEST_SIZE: usize = 32;

#[derive(Clone, Copy, Eq, Ord, PartialOrd, PartialEq, Default, Hash)]
pub struct TransactionDigest([u8; TRANSACTION_DIGEST_SIZE]);

#[derive(Clone, Copy, Eq, Ord, PartialOrd, PartialEq, Default, Hash)]
pub struct BlockDigest([u8; BLOCK_DIGEST_SIZE]);

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub struct PublicKey(ed25519_consensus::VerificationKey);

#[derive(Clone, Copy, Eq, Ord, PartialOrd, PartialEq, Hash)]
pub struct SignatureBytes([u8; SIGNATURE_SIZE]);

// Box ensures value is not copied in memory when Signer itself is moved around for better security
pub struct Signer(Box<ed25519_consensus::SigningKey>);

#[cfg(not(test))]
type TransactionHasher = blake2::Blake2b<digest::consts::U32>;
#[cfg(not(test))]
type BlockHasher = blake2::Blake2b<digest::consts::U32>;

impl TransactionDigest {
    pub fn new(transaction: &Transaction) -> Self {
        Self::from(transaction.data())
    }

    #[cfg(not(test))]
    pub fn from(data: &[u8]) -> Self {
        let mut hasher = TransactionHasher::default();
        hasher.update(data);
        Self(hasher.finalize().into())
    }

    #[cfg(test)]
    pub fn from(data: &[u8]) -> Self {
        let mut hasher = seahash::SeaHasher::default();
        hasher.write(data);
        let bytes = hasher.finish().to_le_bytes();
        let mut b = [0u8; TRANSACTION_DIGEST_SIZE];
        (&mut b[..8]).copy_from_slice(&bytes);
        Self(b)
    }
}

impl BlockDigest {
    #[cfg(not(test))]
    pub fn new(
        authority: AuthorityIndex,
        round: RoundNumber,
        includes: &[BlockReference],
        statements: &[BaseStatement],
        meta_creation_time_ns: TimestampNs,
        signature: &SignatureBytes,
    ) -> Self {
        let mut hasher = BlockHasher::default();
        Self::digest_without_signature(
            &mut hasher,
            authority,
            round,
            includes,
            statements,
            meta_creation_time_ns,
        );
        hasher.update(signature);
        Self(hasher.finalize().into())
    }

    #[cfg(test)]
    pub fn new(
        _authority: AuthorityIndex,
        _round: RoundNumber,
        _includes: &[BlockReference],
        _statements: &[BaseStatement],
        _meta_creation_time_ns: TimestampNs,
        _signature: &SignatureBytes,
    ) -> Self {
        Default::default()
    }

    /// There is a bit of a complexity around what is considered block digest and what is being signed
    ///
    /// * Block signature covers all the fields in the block, except for signature and reference.digest
    /// * Block digest(e.g. block.reference.digest) covers all the above **and** block signature
    ///
    /// This is not very beautiful, but it allows to optimize block synchronization,
    /// by skipping signature verification for all the descendants of the certified block.
    #[cfg(not(test))]
    fn digest_without_signature(
        hasher: &mut BlockHasher,
        authority: AuthorityIndex,
        round: RoundNumber,
        includes: &[BlockReference],
        statements: &[BaseStatement],
        meta_creation_time_ns: TimestampNs,
    ) {
        hasher.update(authority.to_le_bytes());
        hasher.update(round.to_le_bytes());
        // todo - we might want a generic interface to hash BlockReference/BaseStatement
        for include in includes {
            hasher.update(include.authority.to_le_bytes());
            hasher.update(include.round.to_le_bytes());
            hasher.update(include.digest);
        }
        for statement in statements {
            match statement {
                BaseStatement::Share(id, _) => {
                    hasher.update([0]);
                    hasher.update(id);
                }
                BaseStatement::Vote(id, Vote::Accept) => {
                    hasher.update([1]);
                    hasher.update(id);
                }
                BaseStatement::Vote(id, Vote::Reject(None)) => {
                    hasher.update([2]);
                    hasher.update(id);
                }
                BaseStatement::Vote(id, Vote::Reject(Some(other))) => {
                    hasher.update([3]);
                    hasher.update(id);
                    hasher.update(other);
                }
            }
        }
        hasher.update(meta_creation_time_ns.to_le_bytes())
    }
}

impl PublicKey {
    #[cfg(not(test))]
    pub fn verify_block(&self, block: &StatementBlock) -> Result<(), ed25519_consensus::Error> {
        let signature = Signature::from(block.signature().0);
        let mut hasher = BlockHasher::default();
        BlockDigest::digest_without_signature(
            &mut hasher,
            block.author(),
            block.round(),
            block.includes(),
            block.statements(),
            block.meta_creation_time_ns(),
        );
        let digest: [u8; BLOCK_DIGEST_SIZE] = hasher.finalize().into();
        self.0.verify(&signature, digest.as_ref())
    }

    #[cfg(test)]
    pub fn verify_block(&self, _block: &StatementBlock) -> Result<(), ed25519_consensus::Error> {
        Ok(())
    }
}

impl Signer {
    #[cfg(not(test))]
    pub fn sign_block(
        &self,
        authority: AuthorityIndex,
        round: RoundNumber,
        includes: &[BlockReference],
        statements: &[BaseStatement],
        meta_creation_time_ns: TimestampNs,
    ) -> SignatureBytes {
        let mut hasher = BlockHasher::default();
        BlockDigest::digest_without_signature(
            &mut hasher,
            authority,
            round,
            includes,
            statements,
            meta_creation_time_ns,
        );
        let digest: [u8; BLOCK_DIGEST_SIZE] = hasher.finalize().into();
        let signature = self.0.sign(digest.as_ref());
        SignatureBytes(signature.to_bytes())
    }

    #[cfg(test)]
    pub fn sign_block(
        &self,
        _authority: AuthorityIndex,
        _round: RoundNumber,
        _includes: &[BlockReference],
        _statements: &[BaseStatement],
        _meta_creation_time_ns: TimestampNs,
    ) -> SignatureBytes {
        Default::default()
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verification_key())
    }
}

impl AsRef<[u8]> for BlockDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for TransactionDigest {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for SignatureBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", hex::encode(self.0))
    }
}

impl fmt::Display for TransactionDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", hex::encode(&self.0[..4]))
    }
}

impl fmt::Debug for BlockDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", hex::encode(self.0))
    }
}

impl fmt::Display for BlockDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", hex::encode(&self.0[..4]))
    }
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer(public_key={:?})", self.public_key())
    }
}

impl fmt::Display for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer(public_key={:?})", self.public_key())
    }
}

impl Default for SignatureBytes {
    fn default() -> Self {
        Self([0u8; 64])
    }
}

impl ByteRepr for SignatureBytes {
    fn try_copy_from_slice<E: de::Error>(v: &[u8]) -> Result<Self, E> {
        if v.len() != SIGNATURE_SIZE {
            return Err(E::custom(format!("Invalid signature length: {}", v.len())));
        }
        let mut inner = [0u8; SIGNATURE_SIZE];
        inner.copy_from_slice(v);
        Ok(Self(inner))
    }
}

impl Serialize for SignatureBytes {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BytesVisitor::new())
    }
}

impl ByteRepr for TransactionDigest {
    fn try_copy_from_slice<E: de::Error>(v: &[u8]) -> Result<Self, E> {
        if v.len() != TRANSACTION_DIGEST_SIZE {
            return Err(E::custom(format!(
                "Invalid transaction digest length: {}",
                v.len()
            )));
        }
        let mut inner = [0u8; TRANSACTION_DIGEST_SIZE];
        inner.copy_from_slice(v);
        Ok(Self(inner))
    }
}

impl Serialize for TransactionDigest {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for TransactionDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BytesVisitor::new())
    }
}

impl ByteRepr for BlockDigest {
    fn try_copy_from_slice<E: de::Error>(v: &[u8]) -> Result<Self, E> {
        if v.len() != BLOCK_DIGEST_SIZE {
            return Err(E::custom(format!(
                "Invalid block digest length: {}",
                v.len()
            )));
        }
        let mut inner = [0u8; BLOCK_DIGEST_SIZE];
        inner.copy_from_slice(v);
        Ok(Self(inner))
    }
}

impl Serialize for BlockDigest {
    #[inline]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for BlockDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BytesVisitor::new())
    }
}

impl Drop for Signer {
    fn drop(&mut self) {
        self.0.zeroize()
    }
}

pub fn dummy_signer() -> Signer {
    Signer(Box::new(ed25519_consensus::SigningKey::from([0u8; 32])))
}

pub fn dummy_public_key() -> PublicKey {
    dummy_signer().public_key()
}