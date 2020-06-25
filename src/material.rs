use crate::hittable::HitRecord;
use crate::math::*;

use std::marker::{Send, Sync};

pub trait Material: Send + Sync {
    // provide default implementations
    // methods for sampling the bsdf
    fn value(&self, hit: &HitRecord, wi: Vec3, wo: Vec3) -> f32 {
        0.0
    }
    fn generate(&self, hit: &HitRecord, s: Sample2D, wi: Vec3) -> Option<Vec3> {
        None
    }
    // method to sample an emitted light ray with a wavelength and energy
    fn sample_emission(
        &self,
        point: Point3,
        normal: Vec3,
        wavelength_range: Bounds1D,
        scatter_sample: Sample2D,
        wavelength_sample: Sample1D,
    ) -> Option<(Ray, SingleWavelength)> {
        None
    }
    // evaluate bsdf
    fn f(&self, hit: &HitRecord, wi: Vec3, wo: Vec3) -> SingleEnergy {
        SingleEnergy::ZERO
    }
    // evaluate the spectral power distribution for the given light and angle
    fn emission(&self, hit: &HitRecord, wi: Vec3, wo: Option<Vec3>) -> SingleEnergy {
        SingleEnergy::ZERO
    }
}