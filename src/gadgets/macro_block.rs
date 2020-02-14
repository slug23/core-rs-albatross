use algebra::fields::sw6::Fr as SW6Fr;
use r1cs_core::{ConstraintSystem, SynthesisError};
use r1cs_std::bits::boolean::Boolean;
use r1cs_std::groups::curves::short_weierstrass::bls12::bls12_377::{G1Gadget, G2Gadget};
use r1cs_std::prelude::{AllocGadget, CondSelectGadget, EqGadget, FieldGadget, GroupGadget};

use crate::gadgets::check_sig::CheckSigGadget;
use crate::gadgets::g2_to_blake2s::G2ToBlake2sGadget;
use crate::gadgets::reverse_inner_byte_order;
use crate::gadgets::smaller_than::SmallerThanGadget;
use crate::gadgets::xof_hash::XofHashGadget;
use crate::gadgets::xof_hash_to_g1::XofHashToG1Gadget;
use algebra::curves::bls12_377::{G1Projective, G2Projective};
use algebra::{One, Zero};
use crypto_primitives::prf::blake2s::constraints::Blake2sOutputGadget;
use nimiq_bls::PublicKey;
use nimiq_hash::{Blake2sHash, Blake2sHasher, Hasher};
use r1cs_std::fields::fp::FpGadget;
use r1cs_std::Assignment;
use std::borrow::{Borrow, Cow};

#[derive(Clone)]
pub struct MacroBlock {
    pub header_hash: [u8; 32],
    pub public_keys: Vec<G2Projective>,
    pub signature: Option<G1Projective>,
    pub signer_bitmap: Vec<bool>,
}

impl MacroBlock {
    pub const SLOTS: usize = 2; // TODO: Set correctly.

    pub fn hash(&self) -> Blake2sHash {
        let mut sum = G2Projective::zero();
        for key in self.public_keys.iter() {
            sum += key;
        }

        let sum_key = PublicKey { public_key: sum };

        let sum_hash = Blake2sHasher::new().chain(&sum_key).finish();

        let mut xor_bytes = [0u8; 32];
        for (i, (byte1, byte2)) in self
            .header_hash
            .iter()
            .zip(sum_hash.as_bytes().iter())
            .enumerate()
        {
            xor_bytes[i] = byte1 ^ byte2;
        }

        return Blake2sHash::from(xor_bytes);
    }
}

impl Default for MacroBlock {
    fn default() -> Self {
        MacroBlock {
            header_hash: [0; 32],
            public_keys: vec![G2Projective::zero(); MacroBlock::SLOTS],
            signature: None,
            signer_bitmap: vec![false; MacroBlock::SLOTS],
        }
    }
}

pub struct MacroBlockGadget {
    pub header_hash: Vec<Boolean>,
    pub public_keys: Vec<G2Gadget>,
    pub signature: G1Gadget,
    pub signer_bitmap: Vec<Boolean>,
}

impl MacroBlockGadget {
    pub const MAXIMUM_NON_SIGNERS: u64 = 170;

    pub fn public_keys(&self) -> &[G2Gadget] {
        &self.public_keys
    }

    pub fn verify<CS: ConstraintSystem<SW6Fr>>(
        &self,
        mut cs: CS,
        prev_public_keys: &[G2Gadget],
        max_non_signers: &FpGadget<SW6Fr>,
        generator: &G2Gadget,
    ) -> Result<(), SynthesisError> {
        let hash_point = self.to_g1(cs.ns(|| "create hash point"))?;

        let aggregate_public_key = Self::aggregate_public_key(
            cs.ns(|| "aggregate public keys"),
            prev_public_keys,
            &self.signer_bitmap,
            max_non_signers,
        )?;

        CheckSigGadget::check_signature(
            cs.ns(|| "check signature"),
            &aggregate_public_key,
            generator,
            &self.signature,
            &hash_point,
        )
    }

    pub fn to_g1<CS: ConstraintSystem<SW6Fr>>(
        &self,
        mut cs: CS,
    ) -> Result<G1Gadget, SynthesisError> {
        // Sum public keys and hash them.
        let mut sum = Cow::Borrowed(
            self.public_keys
                .get(0)
                .ok_or(SynthesisError::Unsatisfiable)?,
        );
        for (i, key) in self.public_keys.iter().skip(1).enumerate() {
            sum = Cow::Owned(sum.add(cs.ns(|| format!("add public key {}", i)), key)?);
        }

        let hash = G2ToBlake2sGadget::hash_from_g2(cs.ns(|| "public keys to hash"), &sum)?;

        if hash.len() != self.header_hash.len() {
            return Err(SynthesisError::Unsatisfiable);
        }

        // Xor hash with header hash.
        let mut bits = vec![];
        for (i, (h0, h1)) in hash.iter().zip(self.header_hash.iter()).enumerate() {
            let bit = Boolean::xor(cs.ns(|| format!("xor bit {}", i)), h0, h1)?;
            bits.push(bit);
        }

        // Feed normal blake2s hash into XOF.
        // Our gadget expects normal *Big-Endian* order.
        let xof_bits = XofHashGadget::xof_hash(cs.ns(|| "xof hash"), &bits)?;

        // Convert to G1 using try-and-increment method.
        let g1 = XofHashToG1Gadget::hash_to_g1(cs.ns(|| "xor hash to g1"), &xof_bits)?;

        Ok(g1)
    }

    /// Public keys must be in an order such that the `key_bitmap` is true for the first public key.
    pub fn aggregate_public_key<CS: r1cs_core::ConstraintSystem<SW6Fr>>(
        mut cs: CS,
        public_keys: &[G2Gadget],
        key_bitmap: &[Boolean],
        max_non_signers: &FpGadget<SW6Fr>,
    ) -> Result<G2Gadget, SynthesisError> {
        // Sum public keys.
        // Since the neutral element is not allowed, we enforce the first public key to be always
        // included in the `key_bitmap`.
        key_bitmap[0]
            .enforce_equal(cs.ns(|| "enforce first key true"), &Boolean::constant(true))?;
        let mut num_non_signers = FpGadget::zero(cs.ns(|| "num used public keys"))?;

        let mut sum = Cow::Borrowed(public_keys.get(0).ok_or(SynthesisError::Unsatisfiable)?);

        // Conditionally add all other public keys.
        for (i, (key, included)) in public_keys
            .iter()
            .zip(key_bitmap.iter())
            .skip(1)
            .enumerate()
        {
            let new_sum = sum.add(cs.ns(|| format!("add public key {}", i)), key)?;
            let cond_sum = CondSelectGadget::conditionally_select(
                cs.ns(|| format!("conditionally add public key {}", i)),
                included,
                &new_sum,
                sum.as_ref(),
            )?;
            num_non_signers = num_non_signers.conditionally_add_constant(
                cs.ns(|| format!("public key count {}", i)),
                &included.not(),
                SW6Fr::one(),
            )?;
            sum = Cow::Owned(cond_sum);
        }

        // Enforce enough signers.
        SmallerThanGadget::enforce_smaller_than(
            cs.ns(|| "non signers < 171"),
            &num_non_signers,
            max_non_signers,
        )?;

        Ok(sum.into_owned())
    }
}

impl AllocGadget<MacroBlock, SW6Fr> for MacroBlockGadget {
    fn alloc<F, T, CS: ConstraintSystem<SW6Fr>>(
        mut cs: CS,
        value_gen: F,
    ) -> Result<Self, SynthesisError>
    where
        F: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<MacroBlock>,
    {
        let empty_block = MacroBlock::default();
        let value = match value_gen() {
            Ok(val) => val.borrow().clone(),
            Err(_) => empty_block,
        };

        assert_eq!(value.public_keys.len(), MacroBlock::SLOTS);

        let header_hash =
            Blake2sOutputGadget::alloc(cs.ns(|| "header hash"), || Ok(&value.header_hash))?;
        // While the bytes of the Blake2sOutputGadget start with the most significant first,
        // the bits internally start with the least significant.
        // Thus, we need to reverse the bit order there.
        let header_hash = header_hash
            .0
            .into_iter()
            .flat_map(|n| reverse_inner_byte_order(&n.into_bits_le()))
            .collect::<Vec<Boolean>>();
        let public_keys =
            Vec::<G2Gadget>::alloc(cs.ns(|| "public keys"), || Ok(&value.public_keys[..]))?;
        let signer_bitmap =
            Vec::<Boolean>::alloc(cs.ns(|| "signer bitmap"), || Ok(&value.signer_bitmap[..]))?;
        let signature = G1Gadget::alloc(cs.ns(|| "signature"), || value.signature.get())?;

        Ok(MacroBlockGadget {
            header_hash,
            public_keys,
            signature,
            signer_bitmap,
        })
    }

    fn alloc_input<F, T, CS: ConstraintSystem<SW6Fr>>(
        mut cs: CS,
        value_gen: F,
    ) -> Result<Self, SynthesisError>
    where
        F: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<MacroBlock>,
    {
        let empty_block = MacroBlock::default();
        let value = match value_gen() {
            Ok(val) => val.borrow().clone(),
            Err(_) => empty_block,
        };

        assert_eq!(value.public_keys.len(), MacroBlock::SLOTS);

        let header_hash =
            Blake2sOutputGadget::alloc_input(cs.ns(|| "header hash"), || Ok(&value.header_hash))?;
        // While the bytes of the Blake2sOutputGadget start with the most significant first,
        // the bits internally start with the least significant.
        // Thus, we need to reverse the bit order there.
        let header_hash = header_hash
            .0
            .into_iter()
            .flat_map(|n| reverse_inner_byte_order(&n.into_bits_le()))
            .collect::<Vec<Boolean>>();
        let public_keys =
            Vec::<G2Gadget>::alloc_input(cs.ns(|| "public keys"), || Ok(&value.public_keys[..]))?;
        let signer_bitmap = Vec::<Boolean>::alloc_input(cs.ns(|| "signer bitmap"), || {
            Ok(&value.signer_bitmap[..])
        })?;
        let signature = G1Gadget::alloc_input(cs.ns(|| "signature"), || value.signature.get())?;

        Ok(MacroBlockGadget {
            header_hash,
            public_keys,
            signature,
            signer_bitmap,
        })
    }
}
