//! Port of `zxing/common/reedsolomon/*`: Reed-Solomon error correction decoding
//! over GF(2^8).
//!
//! The structure follows the original (polynomial division, the extended
//! Euclidean algorithm for sigma and omega, Chien search, and the Forney
//! formula). The only change is that `Ref<GenericGFPoly>` becomes the
//! value-semantics [`GfPoly`], with the field object passed by reference.

use crate::error::{illegal_argument, reed_solomon, ZXResult};

/// The field used by QR: x^8 + x^4 + x^3 + x^2 + 1, with generatorBase = 0.
pub const QR_CODE_FIELD_256: (i32, usize, i32) = (0x011D, 256, 0);

pub struct GenericGF {
    exp_table: Vec<i32>,
    log_table: Vec<i32>,
    size: usize,
    generator_base: i32,
}

impl GenericGF {
    pub fn new(primitive: i32, size: usize, b: i32) -> Self {
        let mut exp_table = vec![0i32; size];
        let mut log_table = vec![0i32; size];
        let mut x = 1i32;
        for i in 0..size {
            exp_table[i] = x;
            x <<= 1; // The generator is fixed at 2.
            if x >= size as i32 {
                x ^= primitive;
                x &= size as i32 - 1;
            }
        }
        for i in 0..size - 1 {
            log_table[exp_table[i] as usize] = i as i32;
        }
        Self {
            exp_table,
            log_table,
            size,
            generator_base: b,
        }
    }

    pub fn qr_code_field_256() -> Self {
        Self::new(
            QR_CODE_FIELD_256.0,
            QR_CODE_FIELD_256.1,
            QR_CODE_FIELD_256.2,
        )
    }

    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn generator_base(&self) -> i32 {
        self.generator_base
    }

    pub fn zero(&self) -> GfPoly {
        GfPoly {
            coefficients: vec![0],
        }
    }

    pub fn one(&self) -> GfPoly {
        GfPoly {
            coefficients: vec![1],
        }
    }

    pub fn build_monomial(&self, degree: i32, coefficient: i32) -> ZXResult<GfPoly> {
        if degree < 0 {
            return Err(illegal_argument("Degree must be non-negative"));
        }
        if coefficient == 0 {
            return Ok(self.zero());
        }
        let mut coefficients = vec![0i32; degree as usize + 1];
        coefficients[0] = coefficient;
        Ok(GfPoly::new(coefficients))
    }

    #[inline]
    pub fn add_or_subtract(a: i32, b: i32) -> i32 {
        a ^ b
    }

    #[inline]
    pub fn exp(&self, a: i32) -> i32 {
        self.exp_table[a as usize]
    }

    pub fn log(&self, a: i32) -> ZXResult<i32> {
        if a == 0 {
            return Err(illegal_argument("cannot give log(0)"));
        }
        Ok(self.log_table[a as usize])
    }

    pub fn inverse(&self, a: i32) -> ZXResult<i32> {
        if a == 0 {
            return Err(illegal_argument("Cannot calculate the inverse of 0"));
        }
        Ok(self.exp_table[self.size - self.log_table[a as usize] as usize - 1])
    }

    #[inline]
    pub fn multiply(&self, a: i32, b: i32) -> i32 {
        if a == 0 || b == 0 {
            return 0;
        }
        self.exp_table
            [((self.log_table[a as usize] + self.log_table[b as usize]) as usize) % (self.size - 1)]
    }
}

#[derive(Clone, Debug)]
pub struct GfPoly {
    coefficients: Vec<i32>,
}

impl GfPoly {
    /// Strips leading zeros on construction, matching the trimming done in the
    /// original constructor.
    pub fn new(coefficients: Vec<i32>) -> Self {
        debug_assert!(!coefficients.is_empty());
        if coefficients.len() > 1 && coefficients[0] == 0 {
            let first_non_zero = coefficients.iter().position(|&c| c != 0);
            match first_non_zero {
                None => Self {
                    coefficients: vec![0],
                },
                Some(i) => Self {
                    coefficients: coefficients[i..].to_vec(),
                },
            }
        } else {
            Self { coefficients }
        }
    }

    #[inline]
    pub fn coefficients(&self) -> &[i32] {
        &self.coefficients
    }

    #[inline]
    pub fn degree(&self) -> i32 {
        self.coefficients.len() as i32 - 1
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.coefficients[0] == 0
    }

    #[inline]
    pub fn coefficient(&self, degree: i32) -> i32 {
        self.coefficients[self.coefficients.len() - 1 - degree as usize]
    }

    pub fn evaluate_at(&self, field: &GenericGF, a: i32) -> i32 {
        if a == 0 {
            return self.coefficient(0);
        }
        if a == 1 {
            return self
                .coefficients
                .iter()
                .fold(0, |acc, &c| GenericGF::add_or_subtract(acc, c));
        }
        let mut result = self.coefficients[0];
        for &c in self.coefficients.iter().skip(1) {
            result = GenericGF::add_or_subtract(field.multiply(a, result), c);
        }
        result
    }

    pub fn add_or_subtract(&self, other: &GfPoly) -> GfPoly {
        if self.is_zero() {
            return other.clone();
        }
        if other.is_zero() {
            return self.clone();
        }
        let (smaller, larger) = if self.coefficients.len() > other.coefficients.len() {
            (&other.coefficients, &self.coefficients)
        } else {
            (&self.coefficients, &other.coefficients)
        };
        let length_diff = larger.len() - smaller.len();
        let mut sum_diff = larger.clone();
        for i in length_diff..larger.len() {
            sum_diff[i] = GenericGF::add_or_subtract(smaller[i - length_diff], larger[i]);
        }
        GfPoly::new(sum_diff)
    }

    pub fn multiply(&self, field: &GenericGF, other: &GfPoly) -> GfPoly {
        if self.is_zero() || other.is_zero() {
            return field.zero();
        }
        let a = &self.coefficients;
        let b = &other.coefficients;
        let mut product = vec![0i32; a.len() + b.len() - 1];
        for (i, &ac) in a.iter().enumerate() {
            for (j, &bc) in b.iter().enumerate() {
                product[i + j] = GenericGF::add_or_subtract(product[i + j], field.multiply(ac, bc));
            }
        }
        GfPoly::new(product)
    }

    pub fn multiply_scalar(&self, field: &GenericGF, scalar: i32) -> GfPoly {
        if scalar == 0 {
            return field.zero();
        }
        if scalar == 1 {
            return self.clone();
        }
        GfPoly::new(
            self.coefficients
                .iter()
                .map(|&c| field.multiply(c, scalar))
                .collect(),
        )
    }

    pub fn multiply_by_monomial(
        &self,
        field: &GenericGF,
        degree: i32,
        coefficient: i32,
    ) -> ZXResult<GfPoly> {
        if degree < 0 {
            return Err(illegal_argument("degree must not be less then 0"));
        }
        if coefficient == 0 {
            return Ok(field.zero());
        }
        let mut product = vec![0i32; self.coefficients.len() + degree as usize];
        for (i, &c) in self.coefficients.iter().enumerate() {
            product[i] = field.multiply(c, coefficient);
        }
        Ok(GfPoly::new(product))
    }
}

pub struct ReedSolomonDecoder {
    field: GenericGF,
}

impl ReedSolomonDecoder {
    pub fn new(field: GenericGF) -> Self {
        Self { field }
    }

    /// Corrects errors in place. `two_s` is the number of error correction
    /// codewords.
    pub fn decode(&self, received: &mut [i32], two_s: i32) -> ZXResult<()> {
        let field = &self.field;
        let poly = GfPoly::new(received.to_vec());

        let mut syndrome_coefficients = vec![0i32; two_s as usize];
        let mut no_error = true;
        for i in 0..two_s {
            let eval = poly.evaluate_at(field, field.exp(i + field.generator_base()));
            let n = syndrome_coefficients.len();
            syndrome_coefficients[n - 1 - i as usize] = eval;
            if eval != 0 {
                no_error = false;
            }
        }
        if no_error {
            return Ok(());
        }

        let syndrome = GfPoly::new(syndrome_coefficients);
        let monomial = field.build_monomial(two_s, 1)?;
        let (sigma, omega) = self.run_euclidean_algorithm(monomial, syndrome, two_s)?;
        let error_locations = self.find_error_locations(&sigma)?;
        let error_magnitudes = self.find_error_magnitudes(&omega, &error_locations)?;

        for i in 0..error_locations.len() {
            let position = received.len() as i32 - 1 - field.log(error_locations[i])?;
            if position < 0 {
                return Err(reed_solomon("Bad error location"));
            }
            received[position as usize] =
                GenericGF::add_or_subtract(received[position as usize], error_magnitudes[i]);
        }
        Ok(())
    }

    fn run_euclidean_algorithm(
        &self,
        a: GfPoly,
        b: GfPoly,
        r_limit: i32,
    ) -> ZXResult<(GfPoly, GfPoly)> {
        let field = &self.field;
        let (a, b) = if a.degree() < b.degree() {
            (b, a)
        } else {
            (a, b)
        };

        let mut r_last = a;
        let mut r = b;
        let mut t_last = field.zero();
        let mut t = field.one();

        while r.degree() >= r_limit / 2 {
            let r_last_last = r_last.clone();
            let t_last_last = t_last.clone();
            r_last = r.clone();
            t_last = t.clone();

            if r_last.is_zero() {
                return Err(reed_solomon("r_{i-1} was zero"));
            }
            r = r_last_last;
            let mut q = field.zero();
            let denominator_leading_term = r_last.coefficient(r_last.degree());
            let dlt_inverse = field.inverse(denominator_leading_term)?;
            while r.degree() >= r_last.degree() && !r.is_zero() {
                let degree_diff = r.degree() - r_last.degree();
                let scale = field.multiply(r.coefficient(r.degree()), dlt_inverse);
                q = q.add_or_subtract(&field.build_monomial(degree_diff, scale)?);
                r = r.add_or_subtract(&r_last.multiply_by_monomial(field, degree_diff, scale)?);
            }

            t = q.multiply(field, &t_last).add_or_subtract(&t_last_last);

            if r.degree() >= r_last.degree() {
                return Err(reed_solomon(
                    "Division algorithm failed to reduce polynomial?",
                ));
            }
        }

        let sigma_tilde_at_zero = t.coefficient(0);
        if sigma_tilde_at_zero == 0 {
            return Err(reed_solomon("sigmaTilde(0) was zero"));
        }
        let inverse = field.inverse(sigma_tilde_at_zero)?;
        Ok((
            t.multiply_scalar(field, inverse),
            r.multiply_scalar(field, inverse),
        ))
    }

    /// Chien search: find the roots of the error locator polynomial.
    fn find_error_locations(&self, error_locator: &GfPoly) -> ZXResult<Vec<i32>> {
        let field = &self.field;
        let num_errors = error_locator.degree();
        if num_errors == 1 {
            return Ok(vec![error_locator.coefficient(1)]);
        }
        let mut result = vec![0i32; num_errors as usize];
        let mut e = 0usize;
        let mut i = 1i32;
        while i < field.size() as i32 && e < num_errors as usize {
            if error_locator.evaluate_at(field, i) == 0 {
                result[e] = field.inverse(i)?;
                e += 1;
            }
            i += 1;
        }
        if e != num_errors as usize {
            return Err(reed_solomon(
                "Error locator degree does not match number of root",
            ));
        }
        Ok(result)
    }

    /// Forney formula: compute the error value at each error location.
    fn find_error_magnitudes(
        &self,
        error_evaluator: &GfPoly,
        error_locations: &[i32],
    ) -> ZXResult<Vec<i32>> {
        let field = &self.field;
        let s = error_locations.len();
        let mut result = vec![0i32; s];
        for i in 0..s {
            let xi_inverse = field.inverse(error_locations[i])?;
            let mut denominator = 1;
            for j in 0..s {
                if i != j {
                    let term = field.multiply(error_locations[j], xi_inverse);
                    // The original implements "term + 1" with bit operations;
                    // adding 1 over GF(2) flips the lowest bit.
                    let term_plus1 = if (term & 0x1) == 0 {
                        term | 1
                    } else {
                        term & !1
                    };
                    denominator = field.multiply(denominator, term_plus1);
                }
            }
            result[i] = field.multiply(
                error_evaluator.evaluate_at(field, xi_inverse),
                field.inverse(denominator)?,
            );
            if field.generator_base() != 0 {
                result[i] = field.multiply(result[i], xi_inverse);
            }
        }
        Ok(result)
    }
}
