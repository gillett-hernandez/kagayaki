use std::vec::Vec;

use crate::config::RenderSettings;
use crate::integrator::Color;
use crate::world::World;
pub struct Film {
    pub buffer: Vec<u8>,
}

impl Film {
    pub fn new(width: usize, height: usize) -> Film {
        // allocate with
        let capacity: usize = (3 * width * height) as usize;
        let mut buffer: Vec<u8> = Vec::with_capacity(capacity as usize);
        for _ in 0..capacity {
            buffer.push(0);
        }
        Film { buffer: buffer }
    }
}

pub struct Renderer<I: Color> {
    pub world: World,
    integrator: I,
}

pub struct NaiveRenderer<I: Color> {}

trait Render {
    fn render(&self, film: &Film, config: &RenderSettings) {}
}
