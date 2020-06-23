use crate::math::*;
use rand::seq::SliceRandom;
use rand::{thread_rng, RngCore};

#[derive(Debug)]
pub struct Sample1D {
    pub x: f32,
}

impl Sample1D {
    pub fn new_random_sample() -> Self {
        Sample1D { x: random() }
    }
}

#[derive(Debug)]
pub struct Sample2D {
    pub x: f32,
    pub y: f32,
}

impl Sample2D {
    pub fn new_random_sample() -> Self {
        Sample2D {
            x: random(),
            y: random(),
        }
    }
}
#[derive(Debug)]
pub struct Sample3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Sample3D {
    pub fn new_random_sample() -> Self {
        Sample3D {
            x: random(),
            y: random(),
            z: random(),
        }
    }
}

pub trait Sampler {
    fn draw_1d(&mut self) -> Sample1D;
    fn draw_2d(&mut self) -> Sample2D;
    fn draw_3d(&mut self) -> Sample3D;
}

pub struct RandomSampler {}

impl RandomSampler {
    pub const fn new() -> RandomSampler {
        RandomSampler {}
    }
}

impl Sampler for RandomSampler {
    fn draw_1d(&mut self) -> Sample1D {
        Sample1D::new_random_sample()
    }
    fn draw_2d(&mut self) -> Sample2D {
        Sample2D::new_random_sample()
    }
    fn draw_3d(&mut self) -> Sample3D {
        Sample3D::new_random_sample()
    }
}
pub struct StratifiedSampler {
    pub dims: [usize; 3],
    pub indices: [usize; 3],
    pub first: Vec<usize>,
    pub second: Vec<usize>,
    pub third: Vec<usize>,
    rng: Box<dyn RngCore>,
}

impl StratifiedSampler {
    pub fn new(xdim: usize, ydim: usize, zdim: usize) -> Self {
        StratifiedSampler {
            dims: [xdim, ydim, zdim],
            indices: [0, 0, 0],
            first: (0..xdim).into_iter().collect(),
            second: (0..(xdim * ydim)).into_iter().collect(),
            third: (0..(xdim * ydim * zdim)).into_iter().collect(),
            rng: Box::new(thread_rng()),
        }
    }
}

impl Sampler for StratifiedSampler {
    fn draw_1d(&mut self) -> Sample1D {
        if self.indices[0] == 0 {
            // shuffle, then draw.
            self.first.shuffle(&mut self.rng);
            // println!("shuffled strata order for draw_1d");
            // print!(".");
        }
        let idx = self.first[self.indices[0]];
        let (width, _depth, _height) = (self.dims[0], self.dims[1], self.dims[2]);
        self.indices[0] += 1;
        if self.indices[0] >= width {
            self.indices[0] = 0;
        }
        // convert idx to the "pixel" based on dims
        let mut sample = Sample1D::new_random_sample();
        let x = idx;
        sample.x = sample.x / (width as f32) + (x as f32) / (width as f32);
        sample
    }
    fn draw_2d(&mut self) -> Sample2D {
        if self.indices[1] == 0 {
            // shuffle, then draw.
            self.second.shuffle(&mut self.rng);
            // println!("shuffled strata order for draw_2d");
            // print!("*");
        }
        let idx = self.second[self.indices[1]];
        let (width, depth, _height) = (self.dims[0], self.dims[1], self.dims[2]);
        self.indices[1] += 1;
        if self.indices[1] >= width * depth {
            self.indices[1] = 0;
        }
        // convert idx to the "pixel" based on dims
        let (x, y) = (idx % width, idx / width);
        let mut sample = Sample2D::new_random_sample();
        sample.x = sample.x / (width as f32) + (x as f32) / (width as f32);
        sample.y = sample.y / (depth as f32) + (y as f32) / (depth as f32);
        sample
    }
    fn draw_3d(&mut self) -> Sample3D {
        if self.indices[2] == 0 {
            // shuffle, then draw.
            self.third.shuffle(&mut self.rng);
            println!("shuffled strata order for draw_3d");
        }
        let (width, depth, height) = (self.dims[0], self.dims[1], self.dims[2]);
        let idx = self.third[self.indices[2]];
        self.indices[2] += 1;
        if self.indices[2] >= width * depth * height {
            self.indices[2] = 0;
        }
        // idx = x + width * y + width * depth * z
        // convert idx to the "pixel" based on dims
        let z = idx / (depth * width);
        // z coordinate is how many slices high the sample is
        let y = (idx / width) % depth;
        // y coordinate is how far into a slice a given "pixel" is
        let x = idx % width;
        // x coordinate is how far along width a given pixel is
        let mut sample = Sample3D::new_random_sample();
        sample.x = sample.x / (width as f32) + (x as f32) / (width as f32);
        sample.y = sample.y / (depth as f32) + (y as f32) / (depth as f32);
        sample.z = sample.z / (height as f32) + (z as f32) / (height as f32);
        sample
    }
}

#[cfg(test)]
mod test {
    use super::*;
    fn function(x: f32) -> f32 {
        x * x - x + 1.0
    }
    #[test]
    fn test_random_sampler_1d() {
        let mut sampler = Box::new(RandomSampler::new());
        let mut s = 0.0;
        for _i in 0..1000000 {
            let sample = sampler.draw_1d();
            assert!(0.0 <= sample.x && sample.x < 1.0, "{}", sample.x);
            s += function(sample.x);
        }
        println!("{}", s / 1000000.0);
    }
    #[test]
    fn test_stratified_sampler_1d() {
        let mut sampler = Box::new(StratifiedSampler::new(10, 10, 10));
        let mut s = 0.0;
        for _i in 0..1000000 {
            let sample = sampler.draw_1d();
            assert!(0.0 <= sample.x && sample.x < 1.0, "{}", sample.x);
            s += function(sample.x);
        }
        println!("{}", s / 1000000.0);
    }
    #[test]
    fn test_stratified_sampler_2d() {
        let mut sampler = Box::new(StratifiedSampler::new(10, 10, 10));

        for i in 0..1000000 {
            let sample = sampler.draw_2d();
            assert!(0.0 <= sample.x && sample.x <= 1.0, "{}", sample.x);
            assert!(0.0 <= sample.y && sample.y <= 1.0, "{}", sample.y);
        }
    }
    #[test]
    fn test_stratified_sampler_3d() {
        let mut sampler = Box::new(StratifiedSampler::new(10, 10, 10));

        for _i in 0..1000000 {
            let sample = sampler.draw_3d();
            assert!(0.0 <= sample.x && sample.x <= 1.0, "{}", sample.x);
            assert!(0.0 <= sample.y && sample.y <= 1.0, "{}", sample.y);
            assert!(0.0 <= sample.z && sample.z <= 1.0, "{}", sample.z);
        }
    }
}
