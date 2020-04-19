use crate::math::*;
use packed_simd::f32x4;

pub trait HasBoundingBox {
    fn bounding_box(&self) -> AABB;
}
pub struct AABB {
    pub min: Point3,
    pub max: Point3,
}

impl AABB {
    pub fn new(min: Point3, max: Point3) -> AABB {
        AABB { min, max }
    }
    pub fn hit(&self, r: Ray, t0: f32, t1: f32) -> bool {
        // let tmin: f32x4 = f32x4::splat(t0);
        // let tmax: f32x4 = f32x4::splat(t1);
        // assert that the absolute value of all the components of direction are greater than 0
        assert!(
            r.direction.0.abs().gt(f32x4::splat(0.0)).all(),
            "{:?}",
            r.direction
        );
        let denom = r.direction.0;
        let min: f32x4 = ((self.min - r.origin).0 / denom);

        let max: f32x4 = ((self.max - r.origin).0 / denom);
        let tmin = min.min(max);
        let tmax = min.max(max);
        // if tmax.le(tmin).any() {
        //     return false
        // }
        // return whether all of tmax's elements were greater than tmins
        // this can be made safe by replacing NaNs with positive or negative 1 depending on the side
        tmax.gt(tmin).all()

        // t0 = ((self.min.x - r.origin.x()) / r.direction.x())
        //     .min((self.max.x() - r.origin.x()) / r.direction.x());
        // t1 = ((self.min.x() - r.origin.x()) / r.direction.x())
        //     .max((self.max.x() - r.origin.x()) / r.direction.x());
        // if tmax <= tmin {
        //     return false;
        // }
        // tmin = tmin.max(t0);
        // tmax = tmax.min(t1);

        // t0 = ((self.min.y() - r.origin.y()) / r.direction.y())
        //     .min((self.max.y() - r.origin.y()) / r.direction.y());
        // t1 = ((self.min.y() - r.origin.y()) / r.direction.y())
        //     .max((self.max.y() - r.origin.y()) / r.direction.y());
        // if tmax <= tmin {
        //     return false;
        // }
        // tmin = tmin.max(t0);
        // tmax = tmax.min(t1);

        // t0 = ((self.min.z() - r.origin.z()) / r.direction.z())
        //     .min((self.max.z() - r.origin.z()) / r.direction.z());
        // t1 = ((self.min.z() - r.origin.z()) / r.direction.z())
        //     .max((self.max.z() - r.origin.z()) / r.direction.z());
        // if tmax <= tmin {
        //     return false;
        // }
        // tmin = tmin.max(t0);
        // tmax = tmax.min(t1);

        // true
    }
}