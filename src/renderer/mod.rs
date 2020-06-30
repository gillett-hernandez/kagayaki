extern crate pbr;

mod film;

pub use film::Film;

use crate::camera::{Camera, CameraId};
use crate::config::Config;
use crate::config::RenderSettings;
use crate::integrator::*;
use crate::math::*;
use crate::spectral::BOUNDED_VISIBLE_RANGE as VISIBLE_RANGE;
use crate::tonemap::{sRGB, Tonemapper};
use crate::world::World;

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use pbr::ProgressBar;
use rayon::iter::ParallelIterator;
use rayon::prelude::*;

pub fn output_film(render_settings: &RenderSettings, film: &Film<XYZColor>) {
    let filename = render_settings.filename.as_ref();
    let filename_str = filename.cloned().unwrap_or(String::from("output"));
    let exr_filename = format!("output/{}.exr", filename_str);
    let png_filename = format!("output/{}.png", filename_str);

    let srgb_tonemapper = sRGB::new(film, render_settings.exposure.unwrap_or(1.0));
    srgb_tonemapper.write_to_files(film, &exr_filename, &png_filename);
}

pub fn parse_wavelength_bounds(config: &Vec<RenderSettings>, default: Bounds1D) -> Bounds1D {
    let mut wavelength_bounds: Option<Bounds1D> = None;
    for settings in config.iter() {
        if let Some((lower, upper)) = settings.wavelength_bounds {
            if wavelength_bounds.is_none() {
                wavelength_bounds = Some(Bounds1D::new(lower, upper));
            } else {
                let mut modified_bounds = wavelength_bounds.take().unwrap();
                modified_bounds.lower = modified_bounds.lower.min(lower);
                modified_bounds.upper = modified_bounds.upper.max(lower);
                wavelength_bounds = Some(modified_bounds);
            }
        }
    }
    let result = match wavelength_bounds {
        Some(bounds) => bounds,
        None => default,
    };
    println!("parsed wavelength bounds to be {:?}", result);
    result
}

pub struct NaiveRenderer {}

impl NaiveRenderer {
    pub fn new() -> NaiveRenderer {
        NaiveRenderer {}
    }

    pub fn render_sampled<I: SamplerIntegrator>(
        integrator: I,
        settings: &RenderSettings,
        camera: &Camera,
    ) -> Film<XYZColor> {
        let (width, height) = (settings.resolution.width, settings.resolution.height);
        println!("starting render with film resolution {}x{}", width, height);
        let min_camera_rays = width * height * settings.min_samples as usize;
        println!("minimum total samples: {}", min_camera_rays);

        let now = Instant::now();

        // do stuff with film here

        let mut film: Film<XYZColor> = Film::new(width, height, XYZColor::BLACK);

        // for _ in 0..100 {
        //     print!("-");
        // }
        // println!("");
        // let output_divisor = (film.width * film.height / 100).max(1);
        let mut pb = ProgressBar::new((width * height) as u64);

        let total_pixels = width * height;

        let pixel_count = Arc::new(AtomicUsize::new(0));
        let clone1 = pixel_count.clone();
        let thread = thread::spawn(move || {
            let mut local_index = 0;
            while local_index < total_pixels {
                let pixels_to_increment = clone1.load(Ordering::Relaxed) - local_index;
                pb.add(pixels_to_increment as u64);
                local_index += pixels_to_increment;
                // pixels_to_increment = 0;
                // while pixels_to_increment > 0 {
                //     pb.inc();
                //     pixels_to_increment -= 1;
                //     local_index += 1;
                // }
                thread::sleep(Duration::from_millis(1));
            }
        });

        let clone2 = pixel_count.clone();
        film.buffer
            .par_iter_mut()
            .enumerate()
            .for_each(|(pixel_index, pixel_ref)| {
                // let clone = pixel_count.clone();
                let y: usize = pixel_index / width;
                let x: usize = pixel_index - width * y;
                // gen ray for pixel x, y
                // let r: Ray = Ray::new(Point3::ZERO, Vec3::X);
                // let mut temp_color = RGBColor::BLACK;
                let mut temp_color = XYZColor::BLACK;
                // let mut sampler: Box<dyn Sampler> = Box::new(StratifiedSampler::new(20, 20, 10));
                let mut sampler: Box<dyn Sampler> = Box::new(RandomSampler::new());
                // idea: use SPD::Tabulated to collect all the data for a single pixel as a SPD, then convert that whole thing to XYZ.
                for _s in 0..settings.min_samples {
                    let sample = sampler.draw_2d();
                    let r = camera.get_ray(
                        sampler.draw_2d(),
                        (x as f32 + sample.x) / (width as f32),
                        (y as f32 + sample.y) / (height as f32),
                    );
                    temp_color += XYZColor::from(integrator.color(&mut sampler, r));
                    // temp_color += RGBColor::from(integrator.color(&mut sampler, r));
                    debug_assert!(
                        temp_color.0.is_finite().all(),
                        "{:?} resulted in {:?}",
                        r,
                        temp_color
                    );
                }

                clone2.fetch_add(1, Ordering::Relaxed);
                // if pixel_index % output_divisor == 0 {
                //     let stdout = std::io::stdout();
                //     let mut handle = stdout.lock();
                //     handle.write_all(b".").unwrap();
                //     std::io::stdout().flush().expect("some error message")
                // }
                // pb.inc();
                // unsafe {
                *pixel_ref = temp_color / (settings.min_samples as f32);
                // }
            });

        if let Err(panic) = thread.join() {
            println!(
                "progress bar incrememnting thread threw an error {:?}",
                panic
            );
        }
        println!("");
        let elapsed = (now.elapsed().as_millis() as f32) / 1000.0;

        println!(
            "\ntook {}s at {} rays per second and {} rays per second per thread\n",
            elapsed,
            (min_camera_rays as f32) / elapsed,
            (min_camera_rays as f32) / elapsed / (settings.threads.unwrap() as f32)
        );
        film
    }
    pub fn render_splatted<I: GenericIntegrator>(
        integrator: I,
        renders: Vec<RenderSettings>,
        cameras: Vec<Camera>,
    ) -> Vec<(RenderSettings, Film<XYZColor>)> {
        // let (width, height) = (settings.resolution.width, settings.resolution.height);
        // let min_camera_rays = width * height * settings.min_samples as usize;

        let now = Instant::now();

        // let total_camera_rays =
        //     width * height * (settings.max_samples.unwrap_or(settings.min_samples) as usize);

        // do stuff with film here

        let mut total_camera_samples = 0;
        let mut total_pixels = 0;
        let mut films: Vec<(RenderSettings, Film<XYZColor>)> = Vec::new();
        let mut light_films: Vec<Film<XYZColor>> = Vec::new();
        for settings in renders.iter() {
            let (width, height) = (settings.resolution.width, settings.resolution.height);
            println!("starting render with film resolution {}x{}", width, height);
            let pixels = width * height;
            total_pixels += pixels;
            total_camera_samples += pixels * (settings.min_samples as usize);
            let image_film: Film<XYZColor> = Film::new(width, height, XYZColor::BLACK);
            let light_film: Film<XYZColor> = Film::new(width, height, XYZColor::BLACK);
            films.push((settings.clone(), image_film));
            light_films.push(light_film);
        }
        println!("total pixels: {}", total_pixels);
        println!("minimum total samples: {}", total_camera_samples);
        let maximum_threads = renders
            .iter()
            .max_by_key(|s| s.threads)
            .unwrap()
            .threads
            .unwrap();

        let mut pb = ProgressBar::new(total_pixels as u64);

        let pixel_count = Arc::new(AtomicUsize::new(0));
        let clone1 = pixel_count.clone();
        let thread = thread::spawn(move || {
            let mut local_index = 0;
            while local_index < total_pixels {
                let pixels_to_increment = clone1.load(Ordering::Relaxed) - local_index;
                pb.add(pixels_to_increment as u64);
                local_index += pixels_to_increment;
                // pixels_to_increment = 0;
                // while pixels_to_increment > 0 {
                //     pb.inc();
                //     pixels_to_increment -= 1;
                //     local_index += 1;
                // }
                thread::sleep(Duration::from_millis(1));
            }
        });

        let clone2 = pixel_count.clone();

        // let (tx, rx) = mpsc::channel();

        // let splatting_thread = thread::spawn(move || {

        // });

        // let tx1 = mpsc::Sender::clone(&tx);

        let mut additional_splats: Vec<(Sample, CameraId)> =
            films
                .par_iter_mut()
                .enumerate()
                .flat_map(
                    |(camera_id, (settings, film)): (
                        usize,
                        &mut (RenderSettings, Film<XYZColor>),
                    )|
                     -> Vec<(Sample, CameraId)> {
                        if let Some((s, t)) = settings.selected_pair {
                            println!("rendering specific pair {} {}", s, t);
                        }
                        let output_divisor = (film.width * film.height / 100).max(1);
                        let additional_samples = film
                            .buffer
                            .par_iter_mut()
                            // .iter_mut()
                            .enumerate()
                            .flat_map(|(pixel_index, pixel_ref)| -> Vec<(Sample, CameraId)> {
                                let y: usize = pixel_index / settings.resolution.width;
                                let x: usize = pixel_index - settings.resolution.width * y;
                                // gen ray for pixel x, y
                                // let r: Ray = Ray::new(Point3::ZERO, Vec3::X);
                                // let mut temp_color = RGBColor::BLACK;
                                let mut temp_color = XYZColor::BLACK;
                                // let mut sampler: Box<dyn Sampler> =
                                //     Box::new(StratifiedSampler::new(20, 20, 10));
                                let mut sampler: Box<dyn Sampler> = Box::new(RandomSampler::new());
                                let camera = cameras[camera_id];
                                // idea: use SPD::Tabulated to collect all the data for a single pixel as a SPD, then convert that whole thing to XYZ.
                                let mut local_additional_splats: Vec<(Sample, CameraId)> =
                                    Vec::new();
                                // use with capacity to preallocate
                                for _s in 0..settings.min_samples {
                                    let sample = sampler.draw_2d();
                                    let r = camera.get_ray(
                                        sampler.draw_2d(),
                                        (x as f32 + sample.x) / (settings.resolution.width as f32),
                                        (y as f32 + sample.y) / (settings.resolution.height as f32),
                                    );
                                    temp_color += XYZColor::from(integrator.color(
                                        &mut sampler,
                                        settings,
                                        (r, camera_id as u8),
                                        &mut local_additional_splats,
                                    ));
                                    // handle image samples that occur in local_additional_splats, as they correspond to the current pixel_index
                                    // if local_additional_splats.iter().filter()
                                    debug_assert!(
                                        temp_color.0.is_finite().all(),
                                        "integrator returned {:?}",
                                        temp_color
                                    );
                                }
                                if pixel_index % output_divisor == 0 {
                                    let stdout = std::io::stdout();
                                    let mut handle = stdout.lock();
                                    handle.write_all(b".").unwrap();
                                    std::io::stdout().flush().expect("some error message")
                                }
                                // unsafe {
                                *pixel_ref = temp_color / (settings.min_samples as f32);
                                clone2.fetch_add(1, Ordering::Relaxed);
                                local_additional_splats
                                // }
                            })
                            .collect();
                        additional_samples
                    },
                )
                .collect();

        let elapsed = (now.elapsed().as_millis() as f32) / 1000.0;

        if let Err(panic) = thread.join() {
            println!(
                "progress bar incrememnting thread threw an error {:?}",
                panic
            );
        }
        // println!("");

        println!(
            "\ntook {}s at {} rays per second and {} rays per second per thread\n",
            elapsed,
            (total_camera_samples as f32) / elapsed,
            (total_camera_samples as f32) / elapsed / (maximum_threads as f32)
        );

        let now = Instant::now();

        additional_splats.par_sort_unstable_by(|(_sample1, camera_id1), (_sample2, camera_id2)| {
            camera_id1.cmp(&camera_id2)
        });
        println!("found {} splats", additional_splats.len());
        for (sample, camera_id) in additional_splats {
            match sample {
                Sample::LightSample(radiance, (x, y)) => {
                    let light_film = &mut light_films[camera_id as usize];
                    // println!("splat index was {} x {}", x, y);
                    let (x, y) = (
                        (x * light_film.width as f32) as usize,
                        light_film.height - (y * light_film.height as f32) as usize - 1,
                    );
                    light_film.buffer[y * light_film.width + x] += XYZColor::from(radiance);
                }
                Sample::ImageSample(radiance, (x, y)) => {
                    let image_film = &mut films[camera_id as usize].1;
                    // println!("splat index was {} x {}", x, y);
                    let (x, y) = (
                        (x * image_film.width as f32) as usize,
                        image_film.height - (y * image_film.height as f32) as usize - 1,
                    );
                    image_film.buffer[y * image_film.width + x] += XYZColor::from(radiance);
                }
            }
        }

        // for i in 0..films.len() {
        //     let (settings, image_film) = &mut films[i];
        //     let light_film = &light_films[i];
        // films.push((*settings, light_film));
        // for (image_pixel, light_pixel) in
        //     image_film.buffer.iter_mut().zip(light_film.buffer.iter())
        // {
        //     // use veach section 10.3.4.3 here
        //     *image_pixel += *light_pixel / settings.min_samples.into();
        // }
        // }
        let mut i = 0;
        for light_film in light_films {
            let mut render_settings = films[i].0.clone();
            let new_filename = format!(
                "{}{}",
                render_settings
                    .filename
                    .expect("render didn't have filename, wtf"),
                "_lightfilm"
            );
            println!("new filename is {}", new_filename);
            render_settings.filename = Some(new_filename);
            films.push((render_settings, light_film));
            println!(
                "added light film to films vec, films vec length is now {}",
                films.len()
            );
            i += 1;
        }

        // let (_left, right): (Vec<RenderSettings>, Vec<Film<XYZColor>>) =
        //     films.iter().cloned().unzip();

        let elapsed = (now.elapsed().as_millis() as f32) / 1000.0;
        println!(
            "\ntook {}s to sort and deserialize the splats and save to film\n",
            elapsed,
        );
        films
    }
}

pub trait Renderer {
    fn render(&self, world: World, cameras: Vec<Camera>, config: &Config);
}

impl Renderer for NaiveRenderer {
    fn render(&self, mut world: World, cameras: Vec<Camera>, config: &Config) {
        // bin the render settings into bins corresponding to what integrator they need.

        let mut bundled_cameras: Vec<Camera> = Vec::new();
        // let mut films: Vec<(RenderSettings, Film<XYZColor>)> = Vec::new();
        let mut sampled_renders: Vec<(IntegratorType, RenderSettings)> = Vec::new();
        let mut splatting_renders_and_cameras: HashMap<
            IntegratorType,
            Vec<(RenderSettings, Camera)>,
        > = HashMap::new();
        // splatting_renders_and_cameras.insert(IntegratorType::PathTracing, Vec::new());
        splatting_renders_and_cameras.insert(IntegratorType::BDPT, Vec::new());
        splatting_renders_and_cameras.insert(IntegratorType::LightTracing, Vec::new());

        // phase 1, gather and sort what renders need to be done
        for (_render_id, render_settings) in config.render_settings.iter().enumerate() {
            let camera_id = render_settings.camera_id.unwrap_or(0) as usize;

            let (width, height) = (
                render_settings.resolution.width,
                render_settings.resolution.height,
            );
            let aspect_ratio = width as f32 / height as f32;

            // copy camera and modify its aspect ratio (so that uv splatting works correctly)
            let copied_camera = cameras[camera_id].with_aspect_ratio(aspect_ratio);

            let integrator_type: IntegratorType = IntegratorType::from_string(
                &render_settings
                    .integrator
                    .as_ref()
                    .unwrap_or(&"PT".to_string()),
            );

            match integrator_type {
                IntegratorType::PathTracing => {
                    let mut updated_render_settings = render_settings.clone();
                    updated_render_settings.camera_id = Some(bundled_cameras.len() as u16);
                    bundled_cameras.push(copied_camera);
                    sampled_renders.push((IntegratorType::PathTracing, updated_render_settings));
                }
                t if splatting_renders_and_cameras.contains_key(&t) => {
                    // then determine new camera id
                    let list = splatting_renders_and_cameras.get_mut(&t).unwrap();
                    let mut updated_render_settings = render_settings.clone();
                    updated_render_settings.camera_id = Some(list.len() as u16);

                    list.push((updated_render_settings, copied_camera))
                }
                _ => {}
            }
        }
        // phase 2, for renders that don't require a splatted render, do them first, and output results as soon as they're finished

        for (integrator_type, render_settings) in sampled_renders.iter() {
            match integrator_type {
                IntegratorType::PathTracing => {
                    let arc_world = Arc::new(world.clone());
                    if let Some(Integrator::PathTracing(integrator)) =
                        Integrator::from_settings_and_world(
                            arc_world.clone(),
                            IntegratorType::PathTracing,
                            &bundled_cameras,
                            render_settings,
                        )
                    {
                        println!("rendering with path tracing integrator");
                        let (render_settings, film) = (
                            render_settings.clone(),
                            NaiveRenderer::render_sampled(
                                integrator,
                                render_settings,
                                &bundled_cameras[render_settings.camera_id.unwrap() as usize],
                            ),
                        );
                        output_film(&render_settings, &film);
                    }
                }
                _ => {}
            }
        }

        // phase 3, do renders where cameras can be combined, and output results as soon as they're finished

        for integrator_type in splatting_renders_and_cameras.keys() {
            if let Some(l) = splatting_renders_and_cameras.get(integrator_type) {
                if l.len() == 0 {
                    continue;
                }
            }
            match integrator_type {
                IntegratorType::BDPT => {
                    let (bundled_settings, bundled_cameras): (Vec<RenderSettings>, Vec<Camera>) =
                        splatting_renders_and_cameras
                            .get(integrator_type)
                            .unwrap()
                            .iter()
                            .cloned()
                            .unzip();
                    let mut max_bounces = 0;

                    for settings in bundled_settings.iter() {
                        max_bounces = max_bounces.max(settings.max_bounces.unwrap_or(2));
                    }
                    let wavelength_bounds =
                        parse_wavelength_bounds(&bundled_settings, VISIBLE_RANGE);
                    world.assign_cameras(bundled_cameras.clone(), true);
                    let arc_world = Arc::new(world.clone());
                    let integrator = BDPTIntegrator {
                        max_bounces,
                        world: arc_world.clone(),
                        wavelength_bounds,
                    };

                    println!("rendering with bidirectional path tracing integrator");
                    let render_splatted_result = NaiveRenderer::render_splatted(
                        integrator,
                        bundled_settings.clone(),
                        bundled_cameras.clone(),
                    );
                    assert!(render_splatted_result.len() > 0);
                    // films.extend(
                    //     (&bundled_settings)
                    //         .iter()
                    //         .cloned()
                    //         .zip(render_splatted_result),
                    // );
                    for (render_settings, film) in render_splatted_result {
                        output_film(&render_settings, &film);
                    }
                }
                IntegratorType::LightTracing => {
                    let (bundled_settings, bundled_cameras): (Vec<RenderSettings>, Vec<Camera>) =
                        splatting_renders_and_cameras
                            .get(integrator_type)
                            .unwrap()
                            .iter()
                            .cloned()
                            .unzip();
                    let mut max_bounces = 0;
                    for settings in bundled_settings.iter() {
                        max_bounces = max_bounces.max(settings.max_bounces.unwrap_or(2));
                    }
                    let wavelength_bounds =
                        parse_wavelength_bounds(&bundled_settings, VISIBLE_RANGE);
                    world.assign_cameras(bundled_cameras.clone(), true);
                    let arc_world = Arc::new(world.clone());
                    let integrator = LightTracingIntegrator {
                        max_bounces,
                        world: arc_world.clone(),
                        russian_roulette: true,
                        camera_samples: 4,
                        wavelength_bounds,
                    };

                    println!("rendering with light tracing integrator");
                    let render_splatted_result = NaiveRenderer::render_splatted(
                        integrator,
                        bundled_settings.clone(),
                        bundled_cameras.clone(),
                    );
                    assert!(render_splatted_result.len() > 0);
                    // films.extend(
                    //     (&bundled_settings)
                    //         .iter()
                    //         .cloned()
                    //         .zip(render_splatted_result),
                    // );
                    for (render_settings, film) in render_splatted_result {
                        output_film(&render_settings, &film);
                    }
                }
                _ => {}
            }
        }
    }
}
