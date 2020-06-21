use super::world::World;
use crate::config::Settings;
use crate::hittable::{HitRecord, Hittable};
use crate::material::Material;
use crate::math::*;
use std::f32::INFINITY;
use std::sync::Arc;

pub trait Integrator: Sync + Send {
    fn color(&self, sampler: &mut Box<dyn Sampler>, camera_ray: Ray) -> RGBColor;
}

pub struct PathTracingIntegrator {
    pub max_bounces: u16,
    pub world: Arc<World>,
    pub russian_roulette: bool,
    pub light_samples: u16,
    pub only_direct: bool,
}

impl Integrator for PathTracingIntegrator {
    fn color(&self, mut sampler: &mut Box<dyn Sampler>, camera_ray: Ray) -> RGBColor {
        let mut ray = camera_ray;
        // println!("{:?}", ray);
        let mut sum = SingleWavelength::new_from_range(sampler.draw_1d().x, 380.0, 780.0);
        let mut beta: SingleEnergy = SingleEnergy::ONE;
        let mut last_bsdf_pdf = 0.0;

        for current_bounce in 0..self.max_bounces {
            // println!("whatever0");
            match (self.world.hit(ray, 0.0, INFINITY)) {
                Some(mut hit) => {
                    // println!("whatever1");
                    hit.lambda = sum.lambda;
                    let id = match (hit.material) {
                        Some(id) => id as usize,
                        None => 0,
                    };
                    let frame = TangentFrame::from_normal(hit.normal);
                    let wi = frame.to_local(&-ray.direction).normalized();
                    // assert!(
                    //     wi.z() > 0.0,
                    //     "point: {:?}, normal {:?}, incoming: {:?}, in local space: {:?}",
                    //     hit.point,
                    //     hit.normal,
                    //     -ray.direction,
                    //     wi
                    // );

                    let material: &Box<dyn Material> = &self.world.materials[id as usize];

                    // wo is generated in tangent space.
                    let maybe_wo: Option<Vec3> = material.generate(&hit, &mut sampler, wi);
                    let emission = material.emission(&hit, wi, maybe_wo);

                    if emission.0 > 0.0 {
                        // check stuff here
                        if last_bsdf_pdf <= 0.0 || self.light_samples == 0 {
                            sum.energy += beta * emission
                        } else {
                            let hit_primitive = self.world.get_primitive(hit.instance_id);
                            // // println!("{:?}", hit);
                            let pdf = hit_primitive.pdf(hit.normal, ray.origin, hit.point);
                            let weight = power_heuristic(last_bsdf_pdf, pdf);
                            assert!(!pdf.is_nan() && !weight.is_nan(), "{}, {}", pdf, weight);
                            sum.energy += beta * emission * weight;
                        }
                    }
                    let mut light_contribution = SingleEnergy::ZERO;
                    let mut successful_light_samples = 0;
                    for i in 0..self.light_samples {
                        if let Some(light) = self.world.pick_random_light(&mut sampler) {
                            // determine pick pdf
                            // as of now the pick pdf is just num lights, however if it were to change this would be where it should change.
                            let pick_pdf = self.world.lights.len() as f32;
                            // sample the primitive from hit_point
                            let (direction, light_pdf) = light.sample(&mut sampler, hit.point);
                            // direction is already in world space.
                            // direction is also oriented away from the shading point already, so no need to negate directions until later.
                            let wo = frame.to_local(&direction);
                            let light_ray = Ray::new_with_time(
                                hit.point + hit.normal * 0.01,
                                direction,
                                ray.time,
                                // hit.time,
                            );
                            // note: time was changed to ray.time. change to hit.time?
                            // since direction is already in world space, no need to call frame.to_world(direction) in the above line
                            let reflectance = material.f(&hit, wi, wo);
                            // if reflectance.0.max_element() < 0.00000001 {
                            //     // if reflectance is 0 for all components, skip this light sample
                            //     continue;
                            // }
                            let dropoff = wo.z().max(0.0);
                            // let dropoff = wo.z().abs();
                            if let Some(mut light_hit) = self.world.hit(light_ray, 0.0, INFINITY) {
                                light_hit.lambda = sum.lambda;
                                // note: changed t0 to 0.0. change back to hit.time maybe?
                                //
                                // maybe if the instance that was hit was a light as well, redo the sampling calculations for that light instead?
                                // let light_pdf =
                                //     light.pdf(light_hit.normal, hit.point, light_hit.point);
                                let scatter_pdf_for_light_ray = material.value(&hit, wi, wo);
                                let weight = power_heuristic(light_pdf, scatter_pdf_for_light_ray);
                                if light_hit.instance_id == light.get_instance_id() {
                                    let emission_material =
                                        &self.world.materials[light_hit.material.unwrap() as usize];
                                    let light_wi = TangentFrame::from_normal(light_hit.normal)
                                        .to_local(&-direction);
                                    let sampled_light_emission =
                                        emission_material.emission(&light_hit, light_wi, None);
                                    assert!(sampled_light_emission.0 > 0.0);
                                    successful_light_samples += 1;
                                    light_contribution += reflectance
                                        * beta
                                        * dropoff
                                        * sampled_light_emission
                                        * weight
                                        / light_pdf
                                        / pick_pdf;
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    if self.light_samples > 0 {
                        // println!("light contribution: {:?}", light_contribution);
                        sum.energy += light_contribution / (self.light_samples as f32);
                    }
                    if self.only_direct {
                        break;
                    }
                    // println!("whatever!");
                    if let Some(wo) = maybe_wo {
                        let pdf = material.value(&hit, wi, wo);
                        assert!(pdf >= 0.0, "pdf was less than 0 {}", pdf);
                        if pdf < 0.0000001 {
                            break;
                        }
                        if self.russian_roulette {
                            // let attenuation = Vec3::from(beta).norm();
                            let attenuation = beta.0;
                            if attenuation < 1.0 && 0.001 < attenuation {
                                if sampler.draw_1d().x > attenuation {
                                    break;
                                }

                                beta = beta / attenuation;
                            }
                        }
                        let cos_i = wo.z();
                        beta *= material.f(&hit, wi, wo) * cos_i.abs() / pdf;
                        last_bsdf_pdf = pdf;
                        debug_assert!(wi.z() * wo.z() > 0.0, "{:?} {:?}", wi, wo);
                        // add normal to avoid self intersection
                        // also convert wo back to world space when spawning the new ray
                        // println!("whatever!!");
                        ray = Ray::new(
                            hit.point + hit.normal * 0.001,
                            frame.to_world(&wo).normalized(),
                        );
                    } else {
                        break;
                    }
                }
                None => {
                    /*vec3 unit_direction = unit_vector(r.direction());
                    // get phi and theta values for that direction, then convert to UV values for an environment map.
                    float u = (M_PI + atan2(unit_direction.y(), unit_direction.x())) / TAU;
                    float v = acos(unit_direction.z()) / M_PI;*/
                    let fake_hit_record: HitRecord =
                        HitRecord::new(0.0, Point3::ZERO, sum.lambda, Vec3::ZERO, None, 0);
                    let id = self.world.background;
                    let world_material: &Box<dyn Material> = &self.world.materials[id as usize];
                    let world_emission =
                        world_material.emission(&fake_hit_record, Vec3::ZERO, None);
                    // // println!("{:?}, {:?}", beta, world_emission);
                    sum.energy += beta * world_emission;
                    break;
                }
            }
        }
        sum.energy.0 * RGBColor::from(XYZColor::from(sum))
    }
}
