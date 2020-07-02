use crate::hittable::HitRecord;
use crate::material::Material;
use crate::math::*;

#[derive(Clone, Debug)]
pub struct SharpLight {
    // pub color: Box<dyn SpectralPowerDistribution>,
    pub color: SPD,
    pub sharpness: f32,
    pub sidedness: Sidedness,
}

impl SharpLight {
    pub fn new(color: SPD, sharpness: f32, sidedness: Sidedness) -> SharpLight {
        SharpLight {
            color,
            sharpness: 1.0 + sharpness,
            sidedness,
        }
    }
}

fn evaluate(vec: Vec3, sharpness: f32) -> f32 {
    let cos_phi = vec.z();
    let cos_phi2 = cos_phi * cos_phi;
    let sin_phi2 = 1.0 - cos_phi2;
    // let sin_phi = sin_phi2.sqrt();
    let sharpness2 = sharpness * sharpness;

    let mut top_z = sharpness * cos_phi2;
    let mut bottom_z = top_z;
    let offset = cos_phi * (1.0 - sharpness2 * sin_phi2).sqrt();
    top_z += offset;
    bottom_z -= offset;
    let dist_top = ((1.0 - (top_z - sharpness).powi(2)) + top_z.powi(2)).sqrt();
    let dist_bottom = ((1.0 - (bottom_z - sharpness).powi(2)) + bottom_z.powi(2)).sqrt();
    (dist_top - dist_bottom) / (2.0 * PI)
}

impl Material for SharpLight {
    // don't implement the other functions, since the fallback default implementation does the exact same thing

    fn sample_emission(
        &self,
        point: Point3,
        normal: Vec3,
        wavelength_range: Bounds1D,
        mut scatter_sample: Sample2D,
        wavelength_sample: Sample1D,
    ) -> Option<(Ray, SingleWavelength, PDF)> {
        // wo localized to point and normal
        let mut swap = false;
        if self.sidedness == Sidedness::Reverse {
            swap = true;
        } else if self.sidedness == Sidedness::Dual {
            if scatter_sample.x < 0.5 {
                swap = true;
                scatter_sample.x *= 2.0;
            } else {
                scatter_sample.x = (1.0 - scatter_sample.x) * 2.0;
            }
        }

        // let mut local_wo =
        // (random_cosine_direction(scatter_sample) + self.sharpness * Vec3::Z).normalized();
        let mut non_normalized_local_wo = if self.sharpness == 1.0 {
            random_cosine_direction(scatter_sample)
        } else {
            random_on_unit_sphere(scatter_sample) + Vec3::Z * self.sharpness
        };
        // let mut local_wo = Vec3::Z;

        if swap {
            non_normalized_local_wo = -non_normalized_local_wo;
        }

        let fac = evaluate(non_normalized_local_wo.normalized(), self.sharpness);
        // needs to be converted to object space in a way that respects the surface normal
        let frame = TangentFrame::from_normal(normal);
        let object_wo = frame
            .to_world(&non_normalized_local_wo.normalized())
            .normalized();
        // let directional_pdf = local_wo.z().abs() / PI;
        // debug_assert!(directional_pdf > 0.0, "{:?} {:?}", local_wo, object_wo);
        let (sw, _pdf) = self
            .color
            .sample_power_and_pdf(wavelength_range, wavelength_sample);
        Some((
            Ray::new(point, object_wo),
            sw.with_energy(sw.energy),
            PDF::from(fac),
            // PDF::from(local_wo.z().abs() * pdf.0 / PI),
        ))
    }

    fn sample_emission_spectra(
        &self,
        _uv: (f32, f32),
        wavelength_range: Bounds1D,
        wavelength_sample: Sample1D,
    ) -> Option<(f32, PDF)> {
        let (sw, pdf) = self
            .color
            .sample_power_and_pdf(wavelength_range, wavelength_sample);
        Some((sw.lambda, pdf))
    }

    fn emission(&self, hit: &HitRecord, wi: Vec3, _wo: Option<Vec3>) -> SingleEnergy {
        // wi is in local space, and is normalized
        // lets check if it could have been constructed by sample_emission.

        let min_z = (1.0 - self.sharpness.powi(2).recip()).sqrt();
        if wi.z() > min_z {
            // could have been generated
            let fac = evaluate(wi, self.sharpness);
            SingleEnergy::new(fac * self.color.evaluate_power(hit.lambda))
        } else {
            SingleEnergy::ZERO
        }
    }
    // evaluate the directional pdf if the spectral power distribution
    fn emission_pdf(&self, _hit: &HitRecord, wo: Vec3) -> PDF {
        let min_z = (1.0 - self.sharpness.powi(2).recip()).sqrt();
        if wo.z() > min_z {
            let pdf = evaluate(wo, self.sharpness);
            pdf.into()
        } else {
            0.0.into()
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::curves;
//     #[test]
//     fn test_integral() {
//         let light = SharpLight::new(curves::void(), 4.0, Sidedness::Forward);
//         for _ in 0..10000 {
//             let generated = light.sample_emission(
//                 Point3::ORIGIN,
//                 Vec3::Z,
//                 curves::EXTENDED_VISIBLE_RANGE,
//                 Sample2D::new_random_sample(),
//                 Sample1D::new_random_sample(),
//             );
//         }
//     }
// }
