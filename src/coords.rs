use serde::{Deserialize, Serialize};

use crate::cli::YAxis;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldMapping {
    pub origin: [f64; 2],
    pub units_per_pixel: f64,
    pub y_axis: YAxis,
}

impl WorldMapping {
    pub fn world_for_pixel(&self, pixel_x: u32, pixel_y: u32) -> [f64; 2] {
        let world_x = self.origin[0] + f64::from(pixel_x) * self.units_per_pixel;
        let delta_y = f64::from(pixel_y) * self.units_per_pixel;
        let world_y = match self.y_axis {
            YAxis::Down => self.origin[1] + delta_y,
            YAxis::Up => self.origin[1] - delta_y,
        };
        [world_x, world_y]
    }
}

#[cfg(test)]
mod tests {
    use super::WorldMapping;
    use crate::cli::YAxis;

    #[test]
    fn maps_world_coordinates_with_up_axis() {
        let world = WorldMapping {
            origin: [-4096.0, 4096.0],
            units_per_pixel: 0.5,
            y_axis: YAxis::Up,
        };

        assert_eq!(world.world_for_pixel(10, 20), [-4091.0, 4086.0]);
    }
}
