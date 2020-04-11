use algebra::curves::models::short_weierstrass_jacobian::{GroupAffine, GroupProjective};
use algebra::{Field, Fp3, Fp3Parameters, PrimeField, ProjectiveCurve, SWModelParameters};
use r1cs_core::{ConstraintSystem, SynthesisError};
use r1cs_std::bits::boolean::Boolean;
use r1cs_std::fields::{fp::FpGadget, fp3::Fp3Gadget};
use r1cs_std::groups::curves::short_weierstrass::AffineGadget;
use r1cs_std::prelude::FieldGadget;

/// This a gadget that is meant to allocate constant values in the circuit. Useful when you have a
/// variable with a predefined value and don't want to waste space having it as a public or
/// private input. It is implemented for several different types.
pub trait AllocConstantGadget<V, ConstraintF: Field>
where
    Self: Sized,
    V: ?Sized,
{
    fn alloc_const<CS: ConstraintSystem<ConstraintF>>(
        cs: CS,
        constant: &V,
    ) -> Result<Self, SynthesisError>;
}

impl<I, ConstraintF: Field, A: AllocConstantGadget<I, ConstraintF>>
    AllocConstantGadget<[I], ConstraintF> for Vec<A>
{
    fn alloc_const<CS: ConstraintSystem<ConstraintF>>(
        mut cs: CS,
        constant: &[I],
    ) -> Result<Self, SynthesisError> {
        let mut vec = Vec::new();
        for (i, value) in constant.iter().enumerate() {
            vec.push(A::alloc_const(cs.ns(|| format!("value_{}", i)), value)?);
        }
        Ok(vec)
    }
}

impl<F: PrimeField> AllocConstantGadget<F, F> for FpGadget<F> {
    fn alloc_const<CS: ConstraintSystem<F>>(
        mut cs: CS,
        constant: &F,
    ) -> Result<Self, SynthesisError> {
        let mut value = FpGadget::one(cs.ns(|| "alloc one"))?;
        value.mul_by_constant_in_place(cs.ns(|| "mul by const"), constant)?;
        Ok(value)
    }
}

impl<
        P: Fp3Parameters<Fp = ConstraintF>,
        ConstraintF: PrimeField + algebra_core::fields::SquareRootField,
    > AllocConstantGadget<Fp3<P>, ConstraintF> for Fp3Gadget<P, ConstraintF>
{
    fn alloc_const<CS: ConstraintSystem<ConstraintF>>(
        mut cs: CS,
        constant: &Fp3<P>,
    ) -> Result<Self, SynthesisError> {
        let c0 = AllocConstantGadget::alloc_const(cs.ns(|| "c0"), &constant.c0)?;
        let c1 = AllocConstantGadget::alloc_const(cs.ns(|| "c1"), &constant.c1)?;
        let c2 = AllocConstantGadget::alloc_const(cs.ns(|| "c2"), &constant.c2)?;
        let value = Fp3Gadget::new(c0, c1, c2);
        Ok(value)
    }
}

impl<P, ConstraintF, F> AllocConstantGadget<GroupProjective<P>, ConstraintF>
    for AffineGadget<P, ConstraintF, F>
where
    P: SWModelParameters,
    ConstraintF: PrimeField,
    F: FieldGadget<P::BaseField, ConstraintF> + AllocConstantGadget<P::BaseField, ConstraintF>,
{
    fn alloc_const<CS: ConstraintSystem<ConstraintF>>(
        cs: CS,
        constant: &GroupProjective<P>,
    ) -> Result<Self, SynthesisError> {
        let affine = constant.into_affine();
        AllocConstantGadget::alloc_const(cs, &affine)
    }
}

impl<P, ConstraintF, F> AllocConstantGadget<GroupAffine<P>, ConstraintF>
    for AffineGadget<P, ConstraintF, F>
where
    P: SWModelParameters,
    ConstraintF: PrimeField,
    F: FieldGadget<P::BaseField, ConstraintF> + AllocConstantGadget<P::BaseField, ConstraintF>,
{
    fn alloc_const<CS: ConstraintSystem<ConstraintF>>(
        mut cs: CS,
        constant: &GroupAffine<P>,
    ) -> Result<Self, SynthesisError> {
        let x = AllocConstantGadget::alloc_const(cs.ns(|| "x coordinate"), &constant.x)?;
        let y = AllocConstantGadget::alloc_const(cs.ns(|| "y coordinate"), &constant.y)?;
        let infinity = Boolean::constant(constant.infinity);
        Ok(AffineGadget::new(x, y, infinity))
    }
}
