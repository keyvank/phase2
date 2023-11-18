//! # zk-SNARK MPCs, made easy.
//!
//! ## Make your circuit
//!
//! Grab the [`bellman`](https://github.com/ebfull/bellman) and
//! [`pairing`](https://github.com/ebfull/pairing) crates. Bellman
//! provides a trait called `Circuit`, which you must implement
//! for your computation.
//!
//! Here's a silly example: proving you know the cube root of
//! a field element.
//!
//! ```rust
//! extern crate pairing;
//! extern crate bellman;
//!
//! use ff::PrimeField;
//! use bellman::{
//!     Circuit,
//!     ConstraintSystem,
//!     SynthesisError,
//! };
//!
//! struct CubeRoot<S: PrimeField> {
//!     cube_root: Option<S>
//! }
//!
//! impl<S: PrimeField> Circuit<S> for CubeRoot<S> {
//!     fn synthesize<CS: ConstraintSystem<S>>(
//!         self,
//!         cs: &mut CS
//!     ) -> Result<(), SynthesisError>
//!     {
//!         // Witness the cube root
//!         let root = cs.alloc(|| "root", || {
//!             self.cube_root.ok_or(SynthesisError::AssignmentMissing)
//!         })?;
//!
//!         // Witness the square of the cube root
//!         let square = cs.alloc(|| "square", || {
//!             self.cube_root
//!                 .ok_or(SynthesisError::AssignmentMissing)
//!                 .map(|mut root| { root.square() })
//!         })?;
//!
//!         // Enforce that `square` is root^2
//!         cs.enforce(
//!             || "squaring",
//!             |lc| lc + root,
//!             |lc| lc + root,
//!             |lc| lc + square
//!         );
//!
//!         // Witness the cube, as a public input
//!         let cube = cs.alloc_input(|| "cube", || {
//!             self.cube_root
//!                 .ok_or(SynthesisError::AssignmentMissing)
//!                 .map(|root| {
//!                     let mut tmp = root;
//!                     tmp = tmp.square();
//!                     tmp.mul_assign(&root);
//!                     tmp
//!                 })
//!         })?;
//!
//!         // Enforce that `cube` is root^3
//!         // i.e. that `cube` is `root` * `square`
//!         cs.enforce(
//!             || "cubing",
//!             |lc| lc + root,
//!             |lc| lc + square,
//!             |lc| lc + cube
//!         );
//!
//!         Ok(())
//!     }
//! }
//! ```
//!
//! ## Create some proofs
//!
//! Now that we have `CubeRoot<E>` implementing `Circuit`,
//! let's create some parameters and make some proofs.
//!
//! ```rust,ignore
//! use bls12_381::{Bls12, Scalar};
//! use ff::Field;
//! use std::ops::MulAssign;
//! use bellman::groth16::{
//!     generate_random_parameters,
//!     create_random_proof,
//!     prepare_verifying_key,
//!     verify_proof
//! };
//!
//! let rng = &mut rand::thread_rng();
//!
//! // Create public parameters for our circuit
//! let params = {
//!     let circuit = CubeRoot::<Scalar> {
//!         cube_root: None
//!     };
//!
//!     generate_random_parameters::<Bls12, _, _>(
//!         circuit,
//!         rng
//!     ).unwrap()
//! };
//!
//! // Prepare the verifying key for verification
//! let pvk = prepare_verifying_key(&params.vk);
//!
//! // Let's start making proofs!
//! for _ in 0..50 {
//!     // Verifier picks a cube in the field.
//!     // Let's just make a random one.
//!     let root = Scalar::random(rng);
//!     let mut cube = root;
//!     cube = cube.square();
//!     cube.mul_assign(&root);
//!
//!     // Prover gets the cube, figures out the cube
//!     // root, and makes the proof:
//!     let proof = create_random_proof(
//!         CubeRoot::<Scalar> {
//!             cube_root: Some(root)
//!         }, &params, rng
//!     ).unwrap();
//!
//!     // Verifier checks the proof against the cube
//!     assert!(verify_proof(&pvk, &proof, &[cube]).is_ok());
//! }
//! ```
//! ## Creating parameters
//!
//! Notice in the previous example that we created our zk-SNARK
//! parameters by calling `generate_random_parameters`. However,
//! if you wanted you could have called `generate_parameters`
//! with some secret numbers you chose, and kept them for
//! yourself. Given those numbers, you can create false proofs.
//!
//! In order to convince others you didn't, a multi-party
//! computation (MPC) can be used. The MPC has the property that
//! only one participant needs to be honest for the parameters to
//! be secure. This crate (`phase2`) is about creating parameters
//! securely using such an MPC.
//!
//! Let's start by using `phase2` to create some base parameters
//! for our circuit:
//!
//! ```rust,ignore
//! extern crate phase2;
//!
//! let mut params = phase2::MPCParameters::new(CubeRoot {
//!     cube_root: None
//! }).unwrap();
//! ```
//!
//! The first time you try this, it will try to read a file like
//! `phase1radix2m2` from the current directory. You need to grab
//! that from the [Powers of Tau](https://lists.z.cash.foundation/pipermail/zapps-wg/2018/000362.html).
//!
//! These parameters are not safe to use; false proofs can be
//! created for them. Let's contribute some randomness to these
//! parameters.
//!
//! ```rust,ignore
//! // Contribute randomness to the parameters. Remember this hash,
//! // it's how we know our contribution is in the parameters!
//! let hash = params.contribute(rng);
//! ```
//!
//! These parameters are now secure to use, so long as you weren't
//! malicious. That may not be convincing to others, so let them
//! contribute randomness too! `params` can be serialized and sent
//! elsewhere, where they can do the same thing and send new
//! parameters back to you. Only one person needs to be honest for
//! the final parameters to be secure.
//!
//! Once you're done setting up the parameters, you can verify the
//! parameters:
//!
//! ```rust,ignore
//! let contributions = params.verify(CubeRoot {
//!     cube_root: None
//! }).expect("parameters should be valid!");
//!
//! // We need to check the `contributions` to see if our `hash`
//! // is in it (see above, when we first contributed)
//! assert!(phase2::contains_contribution(&contributions, &hash));
//! ```
//!
//! Great, now if you're happy, grab the Groth16 `Parameters` with
//! `params.params()`, so that you can interact with the bellman APIs
//! just as before.

use bellman::groth16::{Parameters, VerifyingKey};
use bellman::multicore::Worker;
use bellman::{Circuit, ConstraintSystem, Index, LinearCombination, SynthesisError, Variable};
use blake2_rfc::blake2b::Blake2b;
use bls12_381::Bls12;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use ff::{Field, PrimeField};
use group::{prime::PrimeCurveAffine, Wnaf, WnafGroup};
use pairing::group::{Curve, Group, UncompressedEncoding};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaChaRng;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Write};
use std::ops::{AddAssign, Mul};
use std::sync::Arc;

struct KeypairAssembly<Fr: PrimeField> {
    num_inputs: usize,
    num_aux: usize,
    num_constraints: usize,
    at_inputs: Vec<Vec<(Fr, usize)>>,
    bt_inputs: Vec<Vec<(Fr, usize)>>,
    ct_inputs: Vec<Vec<(Fr, usize)>>,
    at_aux: Vec<Vec<(Fr, usize)>>,
    bt_aux: Vec<Vec<(Fr, usize)>>,
    ct_aux: Vec<Vec<(Fr, usize)>>,
}

impl<Fr: PrimeField> ConstraintSystem<Fr> for KeypairAssembly<Fr> {
    type Root = Self;

    fn alloc<F, A, AR>(&mut self, _: A, _: F) -> Result<Variable, SynthesisError>
    where
        F: FnOnce() -> Result<Fr, SynthesisError>,
        A: FnOnce() -> AR,
        AR: Into<String>,
    {
        // There is no assignment, so we don't even invoke the
        // function for obtaining one.

        let index = self.num_aux;
        self.num_aux += 1;

        self.at_aux.push(vec![]);
        self.bt_aux.push(vec![]);
        self.ct_aux.push(vec![]);

        Ok(Variable::new_unchecked(Index::Aux(index)))
    }
    fn alloc_input<F, A, AR>(&mut self, _: A, _: F) -> Result<Variable, SynthesisError>
    where
        F: FnOnce() -> Result<Fr, SynthesisError>,
        A: FnOnce() -> AR,
        AR: Into<String>,
    {
        // There is no assignment, so we don't even invoke the
        // function for obtaining one.

        let index = self.num_inputs;
        self.num_inputs += 1;

        self.at_inputs.push(vec![]);
        self.bt_inputs.push(vec![]);
        self.ct_inputs.push(vec![]);

        Ok(Variable::new_unchecked(Index::Input(index)))
    }

    fn enforce<A, AR, LA, LB, LC>(&mut self, _: A, a: LA, b: LB, c: LC)
    where
        A: FnOnce() -> AR,
        AR: Into<String>,
        LA: FnOnce(LinearCombination<Fr>) -> LinearCombination<Fr>,
        LB: FnOnce(LinearCombination<Fr>) -> LinearCombination<Fr>,
        LC: FnOnce(LinearCombination<Fr>) -> LinearCombination<Fr>,
    {
        fn eval<Fr: PrimeField>(
            l: LinearCombination<Fr>,
            inputs: &mut [Vec<(Fr, usize)>],
            aux: &mut [Vec<(Fr, usize)>],
            this_constraint: usize,
        ) {
            for &(var, coeff) in l.as_ref() {
                match var.get_unchecked() {
                    Index::Input(id) => inputs[id].push((coeff, this_constraint)),
                    Index::Aux(id) => aux[id].push((coeff, this_constraint)),
                }
            }
        }

        eval(
            a(LinearCombination::zero()),
            &mut self.at_inputs,
            &mut self.at_aux,
            self.num_constraints,
        );
        eval(
            b(LinearCombination::zero()),
            &mut self.bt_inputs,
            &mut self.bt_aux,
            self.num_constraints,
        );
        eval(
            c(LinearCombination::zero()),
            &mut self.ct_inputs,
            &mut self.ct_aux,
            self.num_constraints,
        );

        self.num_constraints += 1;
    }

    fn push_namespace<NR, N>(&mut self, _: N)
    where
        NR: Into<String>,
        N: FnOnce() -> NR,
    {
        // Do nothing; we don't care about namespaces in this context.
    }

    fn pop_namespace(&mut self) {
        // Do nothing; we don't care about namespaces in this context.
    }

    fn get_root(&mut self) -> &mut Self::Root {
        self
    }
}

/// This allows others to verify that you contributed. The hash produced
/// by `MPCParameters::contribute` is just a BLAKE2b hash of this object.
#[derive(Clone)]
struct PublicKey {
    /// This is the delta (in G1) after the transformation, kept so that we
    /// can check correctness of the public keys without having the entire
    /// interstitial parameters for each contribution.
    delta_after: bls12_381::G1Affine,

    /// Random element chosen by the contributor.
    s: bls12_381::G1Affine,

    /// That element, taken to the contributor's secret delta.
    s_delta: bls12_381::G1Affine,

    /// r is H(last_pubkey | s | s_delta), r_delta proves knowledge of delta
    r_delta: bls12_381::G2Affine,

    /// Hash of the transcript (used for mapping to r)
    transcript: [u8; 64],
}

impl PartialEq for PublicKey {
    fn eq(&self, other: &PublicKey) -> bool {
        self.delta_after == other.delta_after
            && self.s == other.s
            && self.s_delta == other.s_delta
            && self.r_delta == other.r_delta
            && &self.transcript[..] == &other.transcript[..]
    }
}

#[derive(Clone)]
pub struct MPCParameters {
    params: Parameters<Bls12>,
    cs_hash: [u8; 64],
    contributions: Vec<PublicKey>,
}

impl PartialEq for MPCParameters {
    fn eq(&self, other: &MPCParameters) -> bool {
        self.params == other.params
            && &self.cs_hash[..] == &other.cs_hash[..]
            && self.contributions == other.contributions
    }
}

impl PublicKey {
    fn write<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writer.write_all(self.delta_after.to_uncompressed().as_ref())?;
        writer.write_all(self.s.to_uncompressed().as_ref())?;
        writer.write_all(self.s_delta.to_uncompressed().as_ref())?;
        writer.write_all(self.r_delta.to_uncompressed().as_ref())?;
        writer.write_all(&self.transcript)?;

        Ok(())
    }

    fn read<R: Read>(mut reader: R) -> io::Result<PublicKey> {
        let mut g1_repr = <bls12_381::G1Affine as UncompressedEncoding>::Uncompressed::default();
        let mut g2_repr = <bls12_381::G2Affine as UncompressedEncoding>::Uncompressed::default();

        reader.read_exact(g1_repr.as_mut())?;
        let delta_after: bls12_381::G1Affine = Option::from(
            <bls12_381::G1Affine as UncompressedEncoding>::from_uncompressed(&g1_repr),
        )
        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid Data!"))?;

        if delta_after.is_identity().into() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "point at infinity",
            ));
        }

        reader.read_exact(g1_repr.as_mut())?;
        let s: bls12_381::G1Affine = Option::from(
            <bls12_381::G1Affine as UncompressedEncoding>::from_uncompressed(&g1_repr),
        )
        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid Data!"))?;

        if s.is_identity().into() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "point at infinity",
            ));
        }

        reader.read_exact(g1_repr.as_mut())?;
        let s_delta: bls12_381::G1Affine = Option::from(
            <bls12_381::G1Affine as UncompressedEncoding>::from_uncompressed(&g1_repr),
        )
        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid Data!"))?;

        if s_delta.is_identity().into() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "point at infinity",
            ));
        }

        reader.read_exact(g2_repr.as_mut())?;
        let r_delta: bls12_381::G2Affine = Option::from(
            <bls12_381::G2Affine as UncompressedEncoding>::from_uncompressed(&g2_repr),
        )
        .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid Data!"))?;

        if r_delta.is_identity().into() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "point at infinity",
            ));
        }

        let mut transcript = [0u8; 64];
        reader.read_exact(&mut transcript)?;

        Ok(PublicKey {
            delta_after,
            s,
            s_delta,
            r_delta,
            transcript,
        })
    }
}

/// Abstraction over a writer which hashes the data being written.
struct HashWriter<W: Write> {
    writer: W,
    hasher: Blake2b,
}

impl Clone for HashWriter<io::Sink> {
    fn clone(&self) -> HashWriter<io::Sink> {
        HashWriter {
            writer: io::sink(),
            hasher: self.hasher.clone(),
        }
    }
}

impl<W: Write> HashWriter<W> {
    /// Construct a new `HashWriter` given an existing `writer` by value.
    pub fn new(writer: W) -> Self {
        HashWriter {
            writer: writer,
            hasher: Blake2b::new(64),
        }
    }

    /// Destroy this writer and return the hash of what was written.
    pub fn into_hash(self) -> [u8; 64] {
        let mut tmp = [0u8; 64];
        tmp.copy_from_slice(self.hasher.finalize().as_ref());
        tmp
    }
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let bytes = self.writer.write(buf)?;

        if bytes > 0 {
            self.hasher.update(&buf[0..bytes]);
        }

        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn hash_to_g2(digest: &[u8]) -> bls12_381::G2Projective {
    assert!(digest.len() >= 32);
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest[..32]);
    bls12_381::G2Projective::random(&mut ChaChaRng::from_seed(seed))
}

/// Verify a contribution, given the old parameters and
/// the new parameters. Returns the hash of the contribution.
pub fn verify_contribution(before: &MPCParameters, after: &MPCParameters) -> Result<[u8; 64], ()> {
    // Transformation involves a single new object
    if after.contributions.len() != (before.contributions.len() + 1) {
        return Err(());
    }

    // None of the previous transformations should change
    if &before.contributions[..] != &after.contributions[0..before.contributions.len()] {
        return Err(());
    }

    // H/L will change, but should have same length
    if before.params.h.len() != after.params.h.len() {
        return Err(());
    }
    if before.params.l.len() != after.params.l.len() {
        return Err(());
    }

    // A/B_G1/B_G2 doesn't change at all
    if before.params.a != after.params.a {
        return Err(());
    }
    if before.params.b_g1 != after.params.b_g1 {
        return Err(());
    }
    if before.params.b_g2 != after.params.b_g2 {
        return Err(());
    }

    // alpha/beta/gamma don't change
    if before.params.vk.alpha_g1 != after.params.vk.alpha_g1 {
        return Err(());
    }
    if before.params.vk.beta_g1 != after.params.vk.beta_g1 {
        return Err(());
    }
    if before.params.vk.beta_g2 != after.params.vk.beta_g2 {
        return Err(());
    }
    if before.params.vk.gamma_g2 != after.params.vk.gamma_g2 {
        return Err(());
    }

    // IC shouldn't change, as gamma doesn't change
    if before.params.vk.ic != after.params.vk.ic {
        return Err(());
    }

    // cs_hash should be the same
    if &before.cs_hash[..] != &after.cs_hash[..] {
        return Err(());
    }

    let sink = io::sink();
    let mut sink = HashWriter::new(sink);
    sink.write_all(&before.cs_hash[..]).unwrap();

    for pubkey in &before.contributions {
        pubkey.write(&mut sink).unwrap();
    }

    let pubkey = after.contributions.last().unwrap();
    sink.write_all(pubkey.s.to_uncompressed().as_ref()).unwrap();
    sink.write_all(pubkey.s_delta.to_uncompressed().as_ref())
        .unwrap();

    let h = sink.into_hash();

    // The transcript must be consistent
    if &pubkey.transcript[..] != h.as_ref() {
        return Err(());
    }

    let r = hash_to_g2(h.as_ref()).to_affine();

    // Check the signature of knowledge
    if !same_ratio((r, pubkey.r_delta), (pubkey.s, pubkey.s_delta)) {
        return Err(());
    }

    // Check the change from the old delta is consistent
    if !same_ratio(
        (before.params.vk.delta_g1, pubkey.delta_after),
        (r, pubkey.r_delta),
    ) {
        return Err(());
    }

    // Current parameters should have consistent delta in G1
    if pubkey.delta_after != after.params.vk.delta_g1 {
        return Err(());
    }

    // Current parameters should have consistent delta in G2
    if !same_ratio(
        (bls12_381::G1Affine::generator(), pubkey.delta_after),
        (bls12_381::G2Affine::generator(), after.params.vk.delta_g2),
    ) {
        return Err(());
    }

    // H and L queries should be updated with delta^-1
    if !same_ratio(
        merge_pairs(&before.params.h, &after.params.h),
        (after.params.vk.delta_g2, before.params.vk.delta_g2), // reversed for inverse
    ) {
        return Err(());
    }

    if !same_ratio(
        merge_pairs(&before.params.l, &after.params.l),
        (after.params.vk.delta_g2, before.params.vk.delta_g2), // reversed for inverse
    ) {
        return Err(());
    }

    let sink = io::sink();
    let mut sink = HashWriter::new(sink);
    pubkey.write(&mut sink).unwrap();
    let h = sink.into_hash();
    let mut response = [0u8; 64];
    response.copy_from_slice(h.as_ref());

    Ok(response)
}

fn same_ratio<G1: pairing::PairingCurveAffine>(g1: (G1, G1), g2: (G1::Pair, G1::Pair)) -> bool {
    g1.0.pairing_with(&g2.1) == g1.1.pairing_with(&g2.0)
}

fn merge_pairs<G: pairing::PairingCurveAffine>(v1: &[G], v2: &[G]) -> (G, G)
where
    G::Curve: WnafGroup,
{
    use rand::thread_rng;
    use std::sync::Mutex;

    assert_eq!(v1.len(), v2.len());

    let chunk = (v1.len() / num_cpus::get()) + 1;

    let s = Arc::new(Mutex::new(G::Curve::identity()));
    let sx = Arc::new(Mutex::new(G::Curve::identity()));

    crossbeam::scope(|scope| {
        for (v1, v2) in v1.chunks(chunk).zip(v2.chunks(chunk)) {
            let s = s.clone();
            let sx = sx.clone();

            scope.spawn(move || {
                // We do not need to be overly cautious of the RNG
                // used for this check.
                let rng = &mut thread_rng();

                let mut wnaf = Wnaf::new();
                let mut local_s = G::Curve::identity();
                let mut local_sx = G::Curve::identity();

                for (v1, v2) in v1.iter().zip(v2.iter()) {
                    let rho = G::Scalar::random(&mut *rng);
                    let mut wnaf = wnaf.scalar(&rho);
                    let v1 = wnaf.base(v1.to_curve());
                    let v2 = wnaf.base(v2.to_curve());

                    local_s.add_assign(&v1);
                    local_sx.add_assign(&v2);
                }

                s.lock().unwrap().add_assign(&local_s);
                sx.lock().unwrap().add_assign(&local_sx);
            });
        }
    });

    let s = s.lock().unwrap().to_affine();
    let sx = sx.lock().unwrap().to_affine();

    (s, sx)
}

/// This needs to be destroyed by at least one participant
/// for the final parameters to be secure.
struct PrivateKey {
    delta: bls12_381::Scalar,
}

/// Compute a keypair, given the current parameters. Keypairs
/// cannot be reused for multiple contributions or contributions
/// in different parameters.
fn keypair<R: Rng>(rng: &mut R, current: &MPCParameters) -> (PublicKey, PrivateKey) {
    // Sample random delta
    let delta: bls12_381::Scalar = bls12_381::Scalar::random(&mut *rng);

    // Compute delta s-pair in G1
    let s = bls12_381::G1Projective::random(rng).to_affine();
    let s_delta = s.mul(delta).to_affine();

    // H(cs_hash | <previous pubkeys> | s | s_delta)
    let h = {
        let sink = io::sink();
        let mut sink = HashWriter::new(sink);

        sink.write_all(&current.cs_hash[..]).unwrap();
        for pubkey in &current.contributions {
            pubkey.write(&mut sink).unwrap();
        }
        sink.write_all(s.to_uncompressed().as_ref()).unwrap();
        sink.write_all(s_delta.to_uncompressed().as_ref()).unwrap();

        sink.into_hash()
    };

    // This avoids making a weird assumption about the hash into the
    // group.
    let mut transcript = [0; 64];
    transcript.copy_from_slice(h.as_ref());

    // Compute delta s-pair in G2
    let r = hash_to_g2(h.as_ref()).to_affine();
    let r_delta = r.mul(delta).to_affine();

    (
        PublicKey {
            delta_after: current.params.vk.delta_g1.mul(delta).to_affine(),
            s: s,
            s_delta: s_delta,
            r_delta: r_delta,
            transcript: transcript,
        },
        PrivateKey { delta: delta },
    )
}

fn batch_normalization<C: group::Curve>(proj: &mut [C])
where
    C::AffineRepr: Clone + Into<C>,
{
    let mut affines = vec![C::identity().to_affine(); proj.len()];
    C::batch_normalize(&*proj, &mut affines);
    proj.iter_mut().zip(affines.iter()).for_each(|(a, b)| {
        *a = b.clone().into();
    });
}

impl MPCParameters {
    /// Create new Groth16 parameters (compatible with bellman) for a
    /// given circuit. The resulting parameters are unsafe to use
    /// until there are contributions (see `contribute()`).
    pub fn new<C>(circuit: C) -> Result<MPCParameters, SynthesisError>
    where
        C: Circuit<bls12_381::Scalar>,
    {
        let mut assembly = KeypairAssembly {
            num_inputs: 0,
            num_aux: 0,
            num_constraints: 0,
            at_inputs: vec![],
            bt_inputs: vec![],
            ct_inputs: vec![],
            at_aux: vec![],
            bt_aux: vec![],
            ct_aux: vec![],
        };

        // Allocate the "one" input variable
        assembly.alloc_input(|| "", || Ok(bls12_381::Scalar::ONE))?;

        // Synthesize the circuit.
        circuit.synthesize(&mut assembly)?;

        // Input constraints to ensure full density of IC query
        // x * 0 = 0
        for i in 0..assembly.num_inputs {
            assembly.enforce(
                || "",
                |lc| lc + Variable::new_unchecked(Index::Input(i)),
                |lc| lc,
                |lc| lc,
            );
        }

        // Compute the size of our evaluation domain
        let mut m = 1;
        let mut exp = 0;
        while m < assembly.num_constraints {
            m *= 2;
            exp += 1;

            // Powers of Tau ceremony can't support more than 2^21
            if exp > 21 {
                return Err(SynthesisError::PolynomialDegreeTooLarge);
            }
        }

        // Try to load "phase1radix2m{}"
        let f = match File::open(format!("phase1radix2m{}", exp)) {
            Ok(f) => f,
            Err(e) => {
                panic!("Couldn't load phase1radix2m{}: {:?}", exp, e);
            }
        };
        let f = &mut BufReader::with_capacity(1024 * 1024, f);

        let read_g1 = |reader: &mut BufReader<File>| -> io::Result<bls12_381::G1Affine> {
            let mut repr = <bls12_381::G1Affine as UncompressedEncoding>::Uncompressed::default();
            reader.read_exact(repr.as_mut())?;

            Option::from(
                <bls12_381::G1Affine as UncompressedEncoding>::from_uncompressed_unchecked(&repr),
            )
            .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid data"))
            .and_then(|e: bls12_381::G1Affine| {
                if e.is_identity().into() {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "point at infinity",
                    ))
                } else {
                    Ok(e)
                }
            })
        };

        let read_g2 = |reader: &mut BufReader<File>| -> io::Result<bls12_381::G2Affine> {
            let mut repr = <bls12_381::G2Affine as UncompressedEncoding>::Uncompressed::default();
            reader.read_exact(repr.as_mut())?;

            Option::from(
                <bls12_381::G2Affine as UncompressedEncoding>::from_uncompressed_unchecked(&repr),
            )
            .ok_or(io::Error::new(io::ErrorKind::InvalidData, "Invalid data"))
            .and_then(|e: bls12_381::G2Affine| {
                if e.is_identity().into() {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "point at infinity",
                    ))
                } else {
                    Ok(e)
                }
            })
        };

        let alpha = read_g1(f)?;
        let beta_g1 = read_g1(f)?;
        let beta_g2 = read_g2(f)?;

        let mut coeffs_g1 = Vec::with_capacity(m);
        for _ in 0..m {
            coeffs_g1.push(read_g1(f)?);
        }

        let mut coeffs_g2 = Vec::with_capacity(m);
        for _ in 0..m {
            coeffs_g2.push(read_g2(f)?);
        }

        let mut alpha_coeffs_g1 = Vec::with_capacity(m);
        for _ in 0..m {
            alpha_coeffs_g1.push(read_g1(f)?);
        }

        let mut beta_coeffs_g1 = Vec::with_capacity(m);
        for _ in 0..m {
            beta_coeffs_g1.push(read_g1(f)?);
        }

        // These are `Arc` so that later it'll be easier
        // to use multiexp during QAP evaluation (which
        // requires a futures-based API)
        let coeffs_g1 = Arc::new(coeffs_g1);
        let coeffs_g2 = Arc::new(coeffs_g2);
        let alpha_coeffs_g1 = Arc::new(alpha_coeffs_g1);
        let beta_coeffs_g1 = Arc::new(beta_coeffs_g1);

        let mut h = Vec::with_capacity(m - 1);
        for _ in 0..(m - 1) {
            h.push(read_g1(f)?);
        }

        let mut ic = vec![bls12_381::G1Projective::identity(); assembly.num_inputs];
        let mut l = vec![bls12_381::G1Projective::identity(); assembly.num_aux];
        let mut a_g1 =
            vec![bls12_381::G1Projective::identity(); assembly.num_inputs + assembly.num_aux];
        let mut b_g1 =
            vec![bls12_381::G1Projective::identity(); assembly.num_inputs + assembly.num_aux];
        let mut b_g2 =
            vec![bls12_381::G2Projective::identity(); assembly.num_inputs + assembly.num_aux];

        fn eval(
            // Lagrange coefficients for tau
            coeffs_g1: Arc<Vec<bls12_381::G1Affine>>,
            coeffs_g2: Arc<Vec<bls12_381::G2Affine>>,
            alpha_coeffs_g1: Arc<Vec<bls12_381::G1Affine>>,
            beta_coeffs_g1: Arc<Vec<bls12_381::G1Affine>>,

            // QAP polynomials
            at: &[Vec<(bls12_381::Scalar, usize)>],
            bt: &[Vec<(bls12_381::Scalar, usize)>],
            ct: &[Vec<(bls12_381::Scalar, usize)>],

            // Resulting evaluated QAP polynomials
            a_g1: &mut [bls12_381::G1Projective],
            b_g1: &mut [bls12_381::G1Projective],
            b_g2: &mut [bls12_381::G2Projective],
            ext: &mut [bls12_381::G1Projective],

            // Worker
            worker: &Worker,
        ) {
            // Sanity check
            assert_eq!(a_g1.len(), at.len());
            assert_eq!(a_g1.len(), bt.len());
            assert_eq!(a_g1.len(), ct.len());
            assert_eq!(a_g1.len(), b_g1.len());
            assert_eq!(a_g1.len(), b_g2.len());
            assert_eq!(a_g1.len(), ext.len());

            // Evaluate polynomials in multiple threads
            worker.scope(a_g1.len(), |scope, chunk| {
                for ((((((a_g1, b_g1), b_g2), ext), at), bt), ct) in a_g1
                    .chunks_mut(chunk)
                    .zip(b_g1.chunks_mut(chunk))
                    .zip(b_g2.chunks_mut(chunk))
                    .zip(ext.chunks_mut(chunk))
                    .zip(at.chunks(chunk))
                    .zip(bt.chunks(chunk))
                    .zip(ct.chunks(chunk))
                {
                    let coeffs_g1 = coeffs_g1.clone();
                    let coeffs_g2 = coeffs_g2.clone();
                    let alpha_coeffs_g1 = alpha_coeffs_g1.clone();
                    let beta_coeffs_g1 = beta_coeffs_g1.clone();

                    scope.spawn(move |_| {
                        for ((((((a_g1, b_g1), b_g2), ext), at), bt), ct) in a_g1
                            .iter_mut()
                            .zip(b_g1.iter_mut())
                            .zip(b_g2.iter_mut())
                            .zip(ext.iter_mut())
                            .zip(at.iter())
                            .zip(bt.iter())
                            .zip(ct.iter())
                        {
                            for &(coeff, lag) in at {
                                a_g1.add_assign(&coeffs_g1[lag].mul(coeff));
                                ext.add_assign(&beta_coeffs_g1[lag].mul(coeff));
                            }

                            for &(coeff, lag) in bt {
                                b_g1.add_assign(&coeffs_g1[lag].mul(coeff));
                                b_g2.add_assign(&coeffs_g2[lag].mul(coeff));
                                ext.add_assign(&alpha_coeffs_g1[lag].mul(coeff));
                            }

                            for &(coeff, lag) in ct {
                                ext.add_assign(&coeffs_g1[lag].mul(coeff));
                            }
                        }

                        // Batch normalize
                        batch_normalization(a_g1);
                        batch_normalization(b_g1);
                        batch_normalization(b_g2);
                        batch_normalization(ext);
                    });
                }
            });
        }

        let worker = Worker::new();

        // Evaluate for inputs.
        eval(
            coeffs_g1.clone(),
            coeffs_g2.clone(),
            alpha_coeffs_g1.clone(),
            beta_coeffs_g1.clone(),
            &assembly.at_inputs,
            &assembly.bt_inputs,
            &assembly.ct_inputs,
            &mut a_g1[0..assembly.num_inputs],
            &mut b_g1[0..assembly.num_inputs],
            &mut b_g2[0..assembly.num_inputs],
            &mut ic,
            &worker,
        );

        // Evaluate for auxillary variables.
        eval(
            coeffs_g1.clone(),
            coeffs_g2.clone(),
            alpha_coeffs_g1.clone(),
            beta_coeffs_g1.clone(),
            &assembly.at_aux,
            &assembly.bt_aux,
            &assembly.ct_aux,
            &mut a_g1[assembly.num_inputs..],
            &mut b_g1[assembly.num_inputs..],
            &mut b_g2[assembly.num_inputs..],
            &mut l,
            &worker,
        );

        // Don't allow any elements be unconstrained, so that
        // the L query is always fully dense.
        for e in l.iter() {
            if Into::<bool>::into(e.is_identity()) {
                return Err(SynthesisError::UnconstrainedVariable);
            }
        }

        let vk = VerifyingKey {
            alpha_g1: alpha,
            beta_g1: beta_g1,
            beta_g2: beta_g2,
            gamma_g2: bls12_381::G2Affine::generator(),
            delta_g1: bls12_381::G1Affine::generator(),
            delta_g2: bls12_381::G2Affine::generator(),
            ic: ic.into_iter().map(|e| e.to_affine()).collect(),
        };

        let params = Parameters {
            vk: vk,
            h: Arc::new(h),
            l: Arc::new(l.into_iter().map(|e| e.to_affine()).collect()),

            // Filter points at infinity away from A/B queries
            a: Arc::new(
                a_g1.into_iter()
                    .filter(|e| !Into::<bool>::into(e.is_identity()))
                    .map(|e| e.to_affine())
                    .collect(),
            ),
            b_g1: Arc::new(
                b_g1.into_iter()
                    .filter(|e| !Into::<bool>::into(e.is_identity()))
                    .map(|e| e.to_affine())
                    .collect(),
            ),
            b_g2: Arc::new(
                b_g2.into_iter()
                    .filter(|e| !Into::<bool>::into(e.is_identity()))
                    .map(|e| e.to_affine())
                    .collect(),
            ),
        };

        let h = {
            let sink = io::sink();
            let mut sink = HashWriter::new(sink);

            params.write(&mut sink).unwrap();

            sink.into_hash()
        };

        let mut cs_hash = [0; 64];
        cs_hash.copy_from_slice(h.as_ref());

        Ok(MPCParameters {
            params: params,
            cs_hash: cs_hash,
            contributions: vec![],
        })
    }

    /// Get the underlying Groth16 `Parameters`
    pub fn get_params(&self) -> &Parameters<Bls12> {
        &self.params
    }

    /// Contributes some randomness to the parameters. Only one
    /// contributor needs to be honest for the parameters to be
    /// secure.
    ///
    /// This function returns a "hash" that is bound to the
    /// contribution. Contributors can use this hash to make
    /// sure their contribution is in the final parameters, by
    /// checking to see if it appears in the output of
    /// `MPCParameters::verify`.
    pub fn contribute<R: Rng>(&mut self, rng: &mut R) -> [u8; 64] {
        // Generate a keypair
        let (pubkey, privkey) = keypair(rng, self);

        fn batch_exp(bases: &mut [bls12_381::G1Affine], coeff: bls12_381::Scalar) {
            let mut projective = vec![bls12_381::G1Projective::identity(); bases.len()];
            let cpus = num_cpus::get();
            let chunk_size = if bases.len() < cpus {
                1
            } else {
                bases.len() / cpus
            };

            // Perform wNAF over multiple cores, placing results into `projective`.
            crossbeam::scope(|scope| {
                for (bases, projective) in bases
                    .chunks_mut(chunk_size)
                    .zip(projective.chunks_mut(chunk_size))
                {
                    scope.spawn(move || {
                        let mut wnaf = Wnaf::new();

                        for (base, projective) in bases.iter_mut().zip(projective.iter_mut()) {
                            *projective = wnaf.base(base.to_curve(), 1).scalar(&coeff);
                        }
                    });
                }
            });

            // Perform batch normalization
            crossbeam::scope(|scope| {
                for projective in projective.chunks_mut(chunk_size) {
                    scope.spawn(move || {
                        batch_normalization(projective);
                    });
                }
            });

            // Turn it all back into affine points
            for (projective, affine) in projective.iter().zip(bases.iter_mut()) {
                *affine = projective.to_affine();
            }
        }

        let delta_inv = privkey.delta.invert().expect("nonzero");
        let mut l = (&self.params.l[..]).to_vec();
        let mut h = (&self.params.h[..]).to_vec();
        batch_exp(&mut l, delta_inv);
        batch_exp(&mut h, delta_inv);
        self.params.l = Arc::new(l);
        self.params.h = Arc::new(h);

        self.params.vk.delta_g1 = self.params.vk.delta_g1.mul(privkey.delta).to_affine();
        self.params.vk.delta_g2 = self.params.vk.delta_g2.mul(privkey.delta).to_affine();

        self.contributions.push(pubkey.clone());

        // Calculate the hash of the public key and return it
        {
            let sink = io::sink();
            let mut sink = HashWriter::new(sink);
            pubkey.write(&mut sink).unwrap();
            let h = sink.into_hash();
            let mut response = [0u8; 64];
            response.copy_from_slice(h.as_ref());
            response
        }
    }

    /// Verify the correctness of the parameters, given a circuit
    /// instance. This will return all of the hashes that
    /// contributors obtained when they ran
    /// `MPCParameters::contribute`, for ensuring that contributions
    /// exist in the final parameters.
    pub fn verify<C: Circuit<bls12_381::Scalar>>(&self, circuit: C) -> Result<Vec<[u8; 64]>, ()> {
        let initial_params = MPCParameters::new(circuit).map_err(|_| ())?;

        // H/L will change, but should have same length
        if initial_params.params.h.len() != self.params.h.len() {
            return Err(());
        }
        if initial_params.params.l.len() != self.params.l.len() {
            return Err(());
        }

        // A/B_G1/B_G2 doesn't change at all
        if initial_params.params.a != self.params.a {
            return Err(());
        }
        if initial_params.params.b_g1 != self.params.b_g1 {
            return Err(());
        }
        if initial_params.params.b_g2 != self.params.b_g2 {
            return Err(());
        }

        // alpha/beta/gamma don't change
        if initial_params.params.vk.alpha_g1 != self.params.vk.alpha_g1 {
            return Err(());
        }
        if initial_params.params.vk.beta_g1 != self.params.vk.beta_g1 {
            return Err(());
        }
        if initial_params.params.vk.beta_g2 != self.params.vk.beta_g2 {
            return Err(());
        }
        if initial_params.params.vk.gamma_g2 != self.params.vk.gamma_g2 {
            return Err(());
        }

        // IC shouldn't change, as gamma doesn't change
        if initial_params.params.vk.ic != self.params.vk.ic {
            return Err(());
        }

        // cs_hash should be the same
        if &initial_params.cs_hash[..] != &self.cs_hash[..] {
            return Err(());
        }

        let sink = io::sink();
        let mut sink = HashWriter::new(sink);
        sink.write_all(&initial_params.cs_hash[..]).unwrap();

        let mut current_delta = bls12_381::G1Affine::generator();
        let mut result = vec![];

        for pubkey in &self.contributions {
            let mut our_sink = sink.clone();
            our_sink
                .write_all(pubkey.s.to_uncompressed().as_ref())
                .unwrap();
            our_sink
                .write_all(pubkey.s_delta.to_uncompressed().as_ref())
                .unwrap();

            pubkey.write(&mut sink).unwrap();

            let h = our_sink.into_hash();

            // The transcript must be consistent
            if &pubkey.transcript[..] != h.as_ref() {
                return Err(());
            }

            let r = hash_to_g2(h.as_ref()).to_affine();

            // Check the signature of knowledge
            if !same_ratio((r, pubkey.r_delta), (pubkey.s, pubkey.s_delta)) {
                return Err(());
            }

            // Check the change from the old delta is consistent
            if !same_ratio((current_delta, pubkey.delta_after), (r, pubkey.r_delta)) {
                return Err(());
            }

            current_delta = pubkey.delta_after;

            {
                let sink = io::sink();
                let mut sink = HashWriter::new(sink);
                pubkey.write(&mut sink).unwrap();
                let h = sink.into_hash();
                let mut response = [0u8; 64];
                response.copy_from_slice(h.as_ref());
                result.push(response);
            }
        }

        // Current parameters should have consistent delta in G1
        if current_delta != self.params.vk.delta_g1 {
            return Err(());
        }

        // Current parameters should have consistent delta in G2
        if !same_ratio(
            (bls12_381::G1Affine::generator(), current_delta),
            (bls12_381::G2Affine::generator(), self.params.vk.delta_g2),
        ) {
            return Err(());
        }

        // H and L queries should be updated with delta^-1
        if !same_ratio(
            merge_pairs(&initial_params.params.h, &self.params.h),
            (self.params.vk.delta_g2, bls12_381::G2Affine::generator()), // reversed for inverse
        ) {
            return Err(());
        }

        if !same_ratio(
            merge_pairs(&initial_params.params.l, &self.params.l),
            (self.params.vk.delta_g2, bls12_381::G2Affine::generator()), // reversed for inverse
        ) {
            return Err(());
        }

        Ok(result)
    }

    /// Serialize these parameters. The serialized parameters
    /// can be read by bellman as Groth16 `Parameters`.
    pub fn write<W: Write>(&self, mut writer: W) -> io::Result<()> {
        self.params.write(&mut writer)?;
        writer.write_all(&self.cs_hash)?;

        writer.write_u32::<BigEndian>(self.contributions.len() as u32)?;
        for pubkey in &self.contributions {
            pubkey.write(&mut writer)?;
        }

        Ok(())
    }

    /// Deserialize these parameters. If `checked` is false,
    /// we won't perform curve validity and group order
    /// checks.
    pub fn read<R: Read>(mut reader: R, checked: bool) -> io::Result<MPCParameters> {
        let params = Parameters::read(&mut reader, checked)?;

        let mut cs_hash = [0u8; 64];
        reader.read_exact(&mut cs_hash)?;

        let contributions_len = reader.read_u32::<BigEndian>()? as usize;

        let mut contributions = vec![];
        for _ in 0..contributions_len {
            contributions.push(PublicKey::read(&mut reader)?);
        }

        Ok(MPCParameters {
            params,
            cs_hash,
            contributions,
        })
    }
}

pub fn contains_contribution(contributions: &[[u8; 64]], my_contribution: &[u8; 64]) -> bool {
    for contrib in contributions {
        if &contrib[..] == &my_contribution[..] {
            return true;
        }
    }

    return false;
}
