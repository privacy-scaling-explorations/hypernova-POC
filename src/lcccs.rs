use ark_bls12_381::Fr;
use ark_std::One;
use ark_std::Zero;
use std::ops::Mul;

use ark_std::{rand::Rng, UniformRand};

use crate::ccs::{CCSError, CCS};

use crate::pedersen::{Commitment, Params as PedersenParams, Pedersen};
use crate::util::hypercube::BooleanHypercube;

/// Committed CCS instance
#[derive(Debug, Clone)]
pub struct CCCS {
    pub ccs: CCS,

    C: Commitment,
    pub x: Vec<Fr>,
}

/// Linearized Committed CCS instance
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LCCCS {
    pub ccs: CCS,

    pub C: Commitment, // Pedersen commitment of w
    pub u: Fr,
    pub x: Vec<Fr>,
    pub r_x: Vec<Fr>,
    pub v: Vec<Fr>,
}

/// Witness for the LCCCS & CCCS, containing the w vector, but also the r_w used as randomness in
/// the Pedersen commitment.
#[derive(Debug, Clone)]
pub struct Witness {
    pub w: Vec<Fr>,
    pub r_w: Fr, // randomness used in the Pedersen commitment of w
}

impl CCS {
    /// Compute v_j values of the linearized committed CCS form
    /// Given `r`, compute:  \sum_{y \in {0,1}^s'} M_j(r, y) * z(y)
    fn compute_v_j(&self, z: &Vec<Fr>, r: &[Fr]) -> Vec<Fr> {
        self.compute_all_sum_Mz_evals(z, r)
    }

    pub fn to_lcccs<R: Rng>(
        &self,
        rng: &mut R,
        pedersen_params: &PedersenParams,
        z: &[Fr],
    ) -> (LCCCS, Witness) {
        let w: Vec<Fr> = z[(1 + self.l)..].to_vec();
        let r_w = Fr::rand(rng);
        let C = Pedersen::commit(pedersen_params, &w, &r_w);

        let r_x: Vec<Fr> = (0..self.s).map(|_| Fr::rand(rng)).collect();
        let v = self.compute_v_j(&z.to_vec(), &r_x);

        (
            LCCCS {
                ccs: self.clone(),
                C,
                u: Fr::one(),
                x: z[1..(1 + self.l)].to_vec(),
                r_x: r_x,
                v: v,
            },
            Witness { w, r_w },
        )
    }

    pub fn to_cccs<R: Rng>(
        &self,
        rng: &mut R,
        pedersen_params: &PedersenParams,
        z: &[Fr],
    ) -> (CCCS, Witness) {
        let w: Vec<Fr> = z[(1 + self.l)..].to_vec();
        let r_w = Fr::rand(rng);
        let C = Pedersen::commit(pedersen_params, &w, &r_w);

        (
            CCCS {
                ccs: self.clone(),
                C,
                x: z[1..(1 + self.l)].to_vec(),
            },
            Witness { w, r_w },
        )
    }
}

impl CCCS {
    /// Perform the check of the CCCS instance described at section 4.1
    pub fn check_relation(
        &self,
        pedersen_params: &PedersenParams,
        w: &Witness,
    ) -> Result<(), CCSError> {
        // check that C is the commitment of w. Notice that this is not verifying a Pedersen
        // opening, but checking that the Commmitment comes from committing to the witness.
        assert_eq!(self.C.0, Pedersen::commit(pedersen_params, &w.w, &w.r_w).0);

        // check CCCS relation
        let z: Vec<Fr> = [vec![Fr::one()], self.x.clone(), w.w.to_vec()].concat();

        // A CCCS relation is satisfied if the q(x) multivariate polynomial evaluates to zero in the hypercube
        let q_x = self.ccs.compute_q(&z);
        for x in BooleanHypercube::new(self.ccs.s) {
            if !q_x.evaluate(&x).unwrap().is_zero() {
                return Err(CCSError::NotSatisfied);
            }
        }

        Ok(())
    }
}

impl LCCCS {
    /// Perform the check of the LCCCS instance described at section 4.2
    pub fn check_relation(
        &self,
        pedersen_params: &PedersenParams,
        w: &Witness,
    ) -> Result<(), CCSError> {
        // check that C is the commitment of w. Notice that this is not verifying a Pedersen
        // opening, but checking that the Commmitment comes from committing to the witness.
        assert_eq!(self.C.0, Pedersen::commit(pedersen_params, &w.w, &w.r_w).0);

        // check CCS relation
        let z: Vec<Fr> = [vec![self.u], self.x.clone(), w.w.to_vec()].concat();
        let computed_v = self.ccs.compute_all_sum_Mz_evals(&z, &self.r_x);
        assert_eq!(computed_v, self.v);
        Ok(())
    }

    pub fn fold(
        lcccs1: &Self,
        cccs2: &CCCS,
        sigmas: &[Fr],
        thetas: &[Fr],
        r_x_prime: Vec<Fr>,
        rho: Fr,
    ) -> Self {
        let C = Commitment(lcccs1.C.0 + cccs2.C.0.mul(rho));
        let u = lcccs1.u + rho;
        let x: Vec<Fr> = lcccs1
            .x
            .iter()
            .zip(cccs2.x.iter().map(|x_i| *x_i * rho).collect::<Vec<Fr>>())
            .map(|(a_i, b_i)| *a_i + b_i)
            .collect();
        let v: Vec<Fr> = sigmas
            .iter()
            .zip(thetas.iter().map(|x_i| *x_i * rho).collect::<Vec<Fr>>())
            .map(|(a_i, b_i)| *a_i + b_i)
            .collect();

        Self {
            C,
            ccs: lcccs1.ccs.clone(),
            u,
            x,
            r_x: r_x_prime,
            v,
        }
    }

    pub fn fold_witness(w1: Witness, w2: Witness, rho: Fr) -> Witness {
        let w: Vec<Fr> =
            w1.w.iter()
                .zip(w2.w.iter().map(|x_i| *x_i * rho).collect::<Vec<Fr>>())
                .map(|(a_i, b_i)| *a_i + b_i)
                .collect();
        let r_w = w1.r_w + rho * w2.r_w;
        Witness { w, r_w }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::ccs::{get_test_ccs, get_test_z};
    use ark_std::test_rng;
    use ark_std::UniformRand;

    #[test]
    /// Test linearized CCCS v_j against the L_j(x)
    fn test_lcccs_v_j() -> () {
        let mut rng = test_rng();

        let ccs = get_test_ccs();
        let z = get_test_z(3);
        ccs.check_relation(&z.clone()).unwrap();

        let pedersen_params = Pedersen::new_params(&mut rng, ccs.n - ccs.l - 1);
        let (running_instance, _) = ccs.to_lcccs(&mut rng, &pedersen_params, &z);

        // with our test vector comming from R1CS, v should have length 3
        assert_eq!(running_instance.v.len(), 3);

        let vec_L_j_x = ccs.compute_Ls(&z, &running_instance.r_x);
        assert_eq!(vec_L_j_x.len(), running_instance.v.len());

        for (v_i, L_j_x) in running_instance.v.into_iter().zip(vec_L_j_x) {
            let sum_L_j_x = BooleanHypercube::new(ccs.s)
                .into_iter()
                .map(|y| L_j_x.evaluate(&y).unwrap())
                .fold(Fr::zero(), |acc, result| acc + result);
            assert_eq!(v_i, sum_L_j_x);
        }
    }

    #[test]
    fn test_lcccs_fold() -> () {
        let ccs = get_test_ccs();
        let z1 = get_test_z(3);
        let z2 = get_test_z(4);
        ccs.check_relation(&z1).unwrap();
        ccs.check_relation(&z2).unwrap();

        let mut rng = test_rng();
        let r_x_prime: Vec<Fr> = (0..ccs.s).map(|_| Fr::rand(&mut rng)).collect();

        let (sigmas, thetas) = ccs.compute_sigmas_and_thetas(&z1, &z2, &r_x_prime);

        let pedersen_params = Pedersen::new_params(&mut rng, ccs.n - ccs.l - 1);

        let (lcccs, w1) = ccs.to_lcccs(&mut rng, &pedersen_params, &z1);
        let (cccs, w2) = ccs.to_cccs(&mut rng, &pedersen_params, &z2);

        lcccs.check_relation(&pedersen_params, &w1).unwrap();
        cccs.check_relation(&pedersen_params, &w2).unwrap();

        let mut rng = test_rng();
        let rho = Fr::rand(&mut rng);

        let folded = LCCCS::fold(&lcccs, &cccs, &sigmas, &thetas, r_x_prime, rho);

        let w_folded = LCCCS::fold_witness(w1, w2, rho);

        // check lcccs relation
        folded.check_relation(&pedersen_params, &w_folded).unwrap();
    }
}