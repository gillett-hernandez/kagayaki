use crate::world::World;
// use crate::config::Settings;
use crate::aabb::HasBoundingBox;
use crate::hittable::{HitRecord, Hittable};
use crate::integrator::utils::*;
use crate::integrator::*;
use crate::material::Material;
use crate::materials::{MaterialEnum, MaterialId};
use crate::math::*;
use crate::spectral::BOUNDED_VISIBLE_RANGE as VISIBLE_RANGE;
use crate::TransportMode;
use crate::{INTERSECTION_TIME_OFFSET, NORMAL_OFFSET};

use std::f32::INFINITY;
use std::sync::Arc;

use rayon::iter::ParallelIterator;
use rayon::prelude::*;

pub struct PhotonMap {
    pub photons: Vec<Vertex>,
}

pub struct SPPMIntegrator {
    pub max_bounces: u16,
    pub world: Arc<World>,
    pub russian_roulette: bool,
    pub camera_samples: u16,
    pub wavelength_bounds: Bounds1D,
    pub photon_map: Option<Arc<PhotonMap>>,
}

impl GenericIntegrator for SPPMIntegrator {
    fn preprocess(&mut self, _sampler: &mut Box<dyn Sampler>, settings: &Vec<RenderSettings>) {
        let num_beams = 10000;
        // let num_photons = num_beams * self.max_bounces as usize;
        let mut beams: Vec<Vec<Vertex>> = Vec::with_capacity(num_beams);
        beams.par_iter_mut().for_each(|beam| {
            let mut sampler: Box<dyn Sampler> = Box::new(RandomSampler::new());
            let wavelength_sample = sampler.draw_1d();

            let env_sampling_probability = self.world.get_env_sampling_probability();

            let sampled;
            let start_light_vertex;
            loop {
                let mut light_pick_sample = sampler.draw_1d();
                if light_pick_sample.x >= env_sampling_probability {
                    light_pick_sample.x = ((light_pick_sample.x - env_sampling_probability)
                        / (1.0 - env_sampling_probability))
                        .clamp(0.0, 1.0);

                    if self.world.lights.len() == 0 {
                        continue;
                    }
                    let (light, light_pick_pdf) =
                        self.world.pick_random_light(light_pick_sample).unwrap();

                    // if we picked a light
                    let (light_surface_point, light_surface_normal, area_pdf) =
                        light.sample_surface(sampler.draw_2d());

                    let mat_id = light.get_material_id();
                    let material = self.world.get_material(mat_id);
                    // println!("sampled light emission in instance light branch");
                    let maybe_sampled = material.sample_emission(
                        light_surface_point,
                        light_surface_normal,
                        VISIBLE_RANGE,
                        sampler.draw_2d(),
                        wavelength_sample,
                    );
                    sampled = if let Some(data) = maybe_sampled {
                        data
                    } else {
                        println!(
                            "light instance is {:?}, material is {:?}",
                            light,
                            material.get_name()
                        );
                        panic!();
                    };

                    let directional_pdf = sampled.2;
                    // if delta light, the pdf_forward is only directional_pdf
                    let pdf_forward: PDF =
                        directional_pdf / (light_surface_normal * (&sampled.0).direction).abs();
                    let pdf_backward: PDF = light_pick_pdf * area_pdf;
                    debug_assert!(
                        pdf_forward.0.is_finite(),
                        "pdf_forward was not finite {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                        pdf_forward,  // NaN
                        pdf_backward, // 0.494
                        sampled.0,
                        material.get_name(),
                        directional_pdf, // NaN
                        light_surface_point,
                        light_surface_normal, // -Z
                        sampled.1.energy      // 9.88
                    );
                    debug_assert!(
                        pdf_backward.0.is_finite(),
                        "pdf_backward was not finite {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                        pdf_backward,
                        pdf_forward,
                        material.get_name(),
                        directional_pdf,
                        light_surface_point,
                        light_surface_normal,
                        sampled.1.energy
                    );

                    start_light_vertex = Vertex::new(
                        VertexType::LightSource(LightSourceType::Instance),
                        0.0,
                        sampled.1.lambda,
                        light_surface_point,
                        light_surface_normal,
                        (0.0, 0.0),
                        mat_id,
                        light.get_instance_id(),
                        sampled.1.energy,
                        pdf_forward.into(),
                        pdf_backward.into(),
                        1.0,
                    );
                    break;
                } else {
                    // world
                    light_pick_sample.x =
                        (light_pick_sample.x / env_sampling_probability).clamp(0.0, 1.0);
                    // sample world env
                    let world_radius = self.world.get_world_radius();
                    sampled = self.world.environment.sample_emission(
                        world_radius,
                        sampler.draw_2d(),
                        sampler.draw_2d(),
                        VISIBLE_RANGE,
                        wavelength_sample,
                    );
                    let light_g_term = 1.0;
                    let directional_pdf = sampled.2;
                    start_light_vertex = Vertex::new(
                        VertexType::LightSource(LightSourceType::Environment),
                        0.0,
                        sampled.1.lambda,
                        sampled.0.origin,
                        sampled.0.direction,
                        (0.0, 0.0),
                        MaterialId::Light(0),
                        0,
                        sampled.1.energy,
                        directional_pdf.0,
                        1.0,
                        light_g_term,
                    );
                    break;
                };
            }

            let light_ray = sampled.0;
            let lambda = sampled.1.lambda;
            assert!(
                (sampled.3).0 > 0.0,
                "{:?} {:?} {:?} {:?}",
                sampled.0,
                sampled.1,
                sampled.2,
                sampled.3
            );
            let radiance = sampled.1.energy;

            *beam = Vec::with_capacity(self.max_bounces as usize);
            beam.push(start_light_vertex);
            let _ = random_walk(
                light_ray,
                lambda,
                self.max_bounces,
                radiance,
                TransportMode::Radiance,
                &mut sampler,
                &self.world,
                beam,
                0,
            );
        });
        self.photon_map = Some(Arc::new(PhotonMap {
            photons: beams.into_iter().flatten().collect(),
        }));
    }
    fn color(
        &self,
        sampler: &mut Box<dyn Sampler>,
        _settings: &RenderSettings,
        camera_sample: ((f32, f32), CameraId),
        mut samples: &mut Vec<(Sample, CameraId)>,
    ) -> SingleWavelength {
        // naive implementation of SPPM
        // iterate through all deposited photons and add contributions based on if they are close to the eye vertex in question

        let camera_id = camera_sample.1;
        let camera = self.world.get_camera(camera_id as usize);
        let bounds = _settings.wavelength_bounds.unwrap();
        let sum = SingleWavelength::new_from_range(
            sampler.draw_1d().x,
            Bounds1D::new(bounds.0, bounds.1),
        );
        // let (direction, camera_pdf) = camera_surface.sample(camera_direction_sample, hit.point);
        // let direction = direction.normalized();
        let film_sample = Sample2D::new((camera_sample.0).0, (camera_sample.0).1);
        let aperture_sample = sampler.draw_2d(); // sometimes called aperture sample
        let (camera_ray, lens_normal, pdf) =
            camera.sample_we(film_sample, aperture_sample, sum.lambda);
        let camera_pdf = pdf;

        let mut path: Vec<Vertex> = vec![Vertex::new(
            VertexType::Camera,
            camera_ray.time,
            sum.lambda,
            camera_ray.origin,
            camera_ray.direction,
            (0.0, 0.0),
            MaterialId::Camera(0),
            0,
            SingleEnergy::ONE,
            0.0,
            0.0,
            1.0,
        )];

        let _ = random_walk(
            camera_ray,
            sum.lambda,
            1,
            SingleEnergy::ONE,
            TransportMode::Importance,
            sampler,
            &self.world,
            &mut path,
            0,
        );
        // camera random walk is now stored in path, with length limited to 1 (for now)
        SingleWavelength::BLACK
    }
}