use crate::world::World;
// use crate::config::Settings;
use crate::hittable::{HitRecord, Hittable};
use crate::integrator::utils::{random_walk, veach_v, LightSourceType, Vertex, VertexType};
use crate::integrator::SamplerIntegrator;
use crate::material::Material;
use crate::materials::{MaterialEnum, MaterialId};
use crate::math::*;
use crate::spectral::BOUNDED_VISIBLE_RANGE as VISIBLE_RANGE;
use crate::TransportMode;
// use crate::world::EnvironmentMap;

use std::f32::INFINITY;
use std::sync::Arc;

pub struct PathTracingIntegrator {
    pub min_bounces: u16,
    pub max_bounces: u16,
    pub world: Arc<World>,
    pub russian_roulette: bool,
    pub light_samples: u16,
    pub only_direct: bool,
    pub wavelength_bounds: Bounds1D,
}

impl PathTracingIntegrator {
    fn estimate_direct_illumination(
        &self,
        hit: &HitRecord,
        frame: &TangentFrame,
        wi: Vec3,
        material: &MaterialEnum,
        throughput: SingleEnergy,
        light_pick_sample: Sample1D,
        additional_light_sample: Sample2D,
    ) -> SingleEnergy {
        if let Some((light, light_pick_pdf)) = self.world.pick_random_light(light_pick_sample) {
            // determine pick pdf
            // as of now the pick pdf is just num lights, however if it were to change this would be where it should change.
            // sample the primitive from hit_point
            // let (direction, light_pdf) = light.sample(sampler.draw_2d(), hit.point);
            let (point_on_light, normal, light_area_pdf) =
                light.sample_surface(additional_light_sample);
            debug_assert!(light_area_pdf.0.is_finite());
            if light_area_pdf.0 == 0.0 {
                return SingleEnergy::ZERO;
            }
            // direction is from shading point to light
            let direction = (point_on_light - hit.point).normalized();
            // direction is already in world space.
            // direction is also oriented away from the shading point already, so no need to negate directions until later.
            let local_light_direction = frame.to_local(&direction);
            let light_vertex_wi = TangentFrame::from_normal(normal).to_local(&(-direction));

            let dropoff = light_vertex_wi.z().abs();
            if dropoff == 0.0 {
                return SingleEnergy::ZERO;
            }
            // since direction is already in world space, no need to call frame.to_world(direction) in the above line
            let reflectance = material.f(&hit, wi, local_light_direction);
            // if reflectance.0 < 0.00001 {
            //     // if reflectance is 0 for all components, skip this light sample
            //     continue;
            // }

            let pdf = light.pdf(hit.normal, hit.point, point_on_light);
            let light_pdf = pdf * light_pick_pdf; // / light_vertex_wi.z().abs();
            if light_pdf.0 == 0.0 {
                // println!("light pdf was 0");
                // go to next pick
                return SingleEnergy::ZERO;
            }

            let light_material = self.world.get_material(light.get_material_id());
            let emission = light_material.emission(&hit, light_vertex_wi, None);
            // this should be the same as the other method, but maybe not.

            if veach_v(&self.world, point_on_light, hit.point) {
                let scatter_pdf_for_light_ray = material.value(&hit, wi, local_light_direction);
                let weight = power_heuristic(light_pdf.0, scatter_pdf_for_light_ray.0);

                debug_assert!(emission.0 >= 0.0);
                // successful_light_samples += 1;
                return reflectance * throughput * dropoff * emission * weight / light_pdf.0;
                // debug_assert!(
                //     !light_contribution.0.is_nan(),
                //     "l {:?} r {:?} b {:?} d {:?} s {:?} w {:?} p {:?} ",
                //     light_contribution,
                //     reflectance,
                //     beta,
                //     dropoff,
                //     emission,
                //     weight,
                //     light_pdf
                // );
            }
        }
        SingleEnergy::ZERO
    }

    fn estimate_direct_illumination_from_world(
        &self,
        lambda: f32,
        hit: &HitRecord,
        frame: &TangentFrame,
        wi: Vec3,
        material: &MaterialEnum,
        throughput: SingleEnergy,
        sample: Sample2D,
    ) -> SingleEnergy {
        let (uv, light_pdf) = self
            .world
            .environment
            .sample_env_uv_given_wavelength(sample, lambda);
        let direction = uv_to_direction(uv);
        let local_light_direction = frame.to_local(&direction);
        if self
            .world
            .hit(Ray::new(hit.point, direction), 0.00001, INFINITY)
            .is_none()
        {
            // successfully hit nothing, which is to say, hit the world
            let emission = self.world.environment.emission(uv, lambda);
            let reflectance = material.f(&hit, wi, local_light_direction);

            let scatter_pdf_for_light_ray = material.value(&hit, wi, local_light_direction);
            let weight = power_heuristic(light_pdf.0, scatter_pdf_for_light_ray.0);
            reflectance * throughput * emission * weight / light_pdf.0
        } else {
            SingleEnergy::ZERO
        }
    }

    fn estimate_direct_illumination_with_loop(
        &self,
        lambda: f32,
        hit: &HitRecord,
        frame: &TangentFrame,
        wi: Vec3,
        material: &MaterialEnum,
        throughput: SingleEnergy,
        sampler: &mut Box<dyn Sampler>,
    ) -> SingleEnergy {
        let mut light_contribution = SingleEnergy::ZERO;
        let env_sampling_probability = self.world.get_env_sampling_probability();
        if self.world.lights.len() == 0 && env_sampling_probability == 0.0 {
            return SingleEnergy::ZERO;
        }
        for _i in 0..self.light_samples {
            if self.world.lights.len() > 0 {
                // decide whether to sample the lights or the world
                let (light_pick_sample, sample_world) =
                    sampler
                        .draw_1d()
                        .choose(env_sampling_probability, true, false);
                if sample_world {
                    // light_contribution += self.world.environment.sample
                    light_contribution += self.estimate_direct_illumination_from_world(
                        lambda,
                        hit,
                        frame,
                        wi,
                        material,
                        throughput,
                        sampler.draw_2d(),
                    );
                } else {
                    light_contribution += self.estimate_direct_illumination(
                        &hit,
                        &frame,
                        wi,
                        material,
                        throughput,
                        light_pick_sample,
                        sampler.draw_2d(),
                    );
                }
            } else {
                // do world sample, unless world sampling probability is 0
                if env_sampling_probability > 0.0 {
                    // do world sample
                    light_contribution += self.estimate_direct_illumination_from_world(
                        lambda,
                        hit,
                        frame,
                        wi,
                        material,
                        throughput,
                        sampler.draw_2d(),
                    );
                }
            }
        }
        light_contribution
    }
}

impl SamplerIntegrator for PathTracingIntegrator {
    fn color(&self, sampler: &mut Box<dyn Sampler>, camera_ray: Ray) -> SingleWavelength {
        // println!("{:?}", ray);
        let mut sum = SingleWavelength::new_from_range(sampler.draw_1d().x, VISIBLE_RANGE);
        let lambda = sum.lambda;

        let mut path: Vec<Vertex> = Vec::with_capacity(1 + self.max_bounces as usize);

        path.push(Vertex::new(
            VertexType::Camera,
            camera_ray.time,
            lambda,
            camera_ray.origin,
            camera_ray.direction,
            (0.0, 0.0),
            MaterialId::Camera(0),
            0,
            SingleEnergy::ONE,
            0.0,
            0.0,
            1.0,
        ));
        let _ = random_walk(
            camera_ray,
            lambda,
            self.max_bounces,
            SingleEnergy::ONE,
            TransportMode::Importance,
            sampler,
            &self.world,
            &mut path,
            self.min_bounces,
        );

        for (index, vertex) in path.iter().enumerate() {
            if index == 0 {
                continue;
            }
            let prev_vertex = path[index - 1];
            // for every vertex past the 1st one (which is on the camera), evaluate the direct illumination at that vertex, and if it hits a light evaluate the added energy
            if let VertexType::LightSource(light_source) = vertex.vertex_type {
                if light_source == LightSourceType::Environment {
                    let wo = -vertex.normal;
                    let uv = direction_to_uv(wo);
                    let emission = self.world.environment.emission(uv, lambda);
                    sum.energy += emission * vertex.throughput;
                } else {
                    let hit = HitRecord::from(*vertex);
                    let frame = TangentFrame::from_normal(hit.normal);
                    let dir_to_prev = (prev_vertex.point - vertex.point).normalized();
                    let maybe_dir_to_next = path
                        .get(index + 1)
                        .map(|v| (v.point - vertex.point).normalized());
                    let wi = frame.to_local(&dir_to_prev);
                    let wo = maybe_dir_to_next.map(|dir| frame.to_local(&dir));
                    let material = self.world.get_material(vertex.material_id);

                    let emission = material.emission(&hit, wi, wo);

                    if emission.0 > 0.0 {
                        // this will likely never get triggered, since hitting a light source is handled in the above branch
                        if prev_vertex.pdf_forward <= 0.0 || self.light_samples == 0 {
                            sum.energy += vertex.throughput * emission;
                            debug_assert!(!sum.energy.is_nan());
                        } else {
                            let hit_primitive = self.world.get_primitive(hit.instance_id);
                            // // println!("{:?}", hit);
                            let pdf =
                                hit_primitive.pdf(prev_vertex.normal, prev_vertex.point, hit.point);
                            let weight = power_heuristic(prev_vertex.pdf_forward, pdf.0);
                            debug_assert!(
                                !pdf.is_nan() && !weight.is_nan(),
                                "{:?}, {}",
                                pdf,
                                weight
                            );
                            sum.energy += vertex.throughput * emission * weight;
                            debug_assert!(!sum.energy.is_nan());
                        }
                    }
                }
            } else {
                let hit = HitRecord::from(*vertex);
                let frame = TangentFrame::from_normal(hit.normal);
                let dir_to_prev = (prev_vertex.point - vertex.point).normalized();
                let maybe_dir_to_next = path
                    .get(index + 1)
                    .map(|v| (v.point - vertex.point).normalized());
                let wi = frame.to_local(&dir_to_prev);
                let wo = maybe_dir_to_next.map(|dir| frame.to_local(&dir));
                let material = self.world.get_material(vertex.material_id);

                let emission = material.emission(&hit, wi, wo);

                if emission.0 > 0.0 {
                    // this will likely never get triggered, since hitting a light source is handled in the above branch
                    if prev_vertex.pdf_forward <= 0.0 || self.light_samples == 0 {
                        sum.energy += vertex.throughput * emission;
                        debug_assert!(!sum.energy.is_nan());
                    } else {
                        let hit_primitive = self.world.get_primitive(hit.instance_id);
                        // // println!("{:?}", hit);
                        let pdf =
                            hit_primitive.pdf(prev_vertex.normal, prev_vertex.point, hit.point);
                        let weight = power_heuristic(prev_vertex.pdf_forward, pdf.0);
                        debug_assert!(!pdf.is_nan() && !weight.is_nan(), "{:?}, {}", pdf, weight);
                        sum.energy += vertex.throughput * emission * weight;
                        debug_assert!(!sum.energy.is_nan());
                    }
                }

                if self.light_samples > 0 {
                    let light_contribution = self.estimate_direct_illumination_with_loop(
                        sum.lambda,
                        &hit,
                        &frame,
                        wi,
                        material,
                        vertex.throughput,
                        sampler,
                    );
                    // println!("light contribution: {:?}", light_contribution);
                    sum.energy += light_contribution / (self.light_samples as f32);
                    debug_assert!(
                        !sum.energy.is_nan(),
                        "{:?} {:?}",
                        light_contribution,
                        self.light_samples
                    );
                }
            }
            if self.only_direct {
                break;
            }
        }

        sum
    }
}
