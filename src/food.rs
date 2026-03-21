use crate::simulation::hash_u32;

/// Generate a food density map with gaussian clumps.
///
/// Returns a `Vec<f32>` of length `width * height` (row-major), values in [0, 1].
/// Clump centers are placed pseudo-randomly based on `seed`.
#[allow(clippy::cast_precision_loss)]
pub fn generate_food_map(
    width: u32,
    height: u32,
    num_clumps: u32,
    clump_radius: f32,
    seed: u32,
) -> Vec<f32> {
    let n = (width * height) as usize;
    if num_clumps == 0 || clump_radius <= 0.0 {
        return vec![0.0; n];
    }

    // Generate clump centers from seed
    let centers: Vec<(f32, f32)> = (0..num_clumps)
        .map(|i| {
            let hx = hash_u32(seed.wrapping_add(i * 2));
            let hy = hash_u32(seed.wrapping_add(i * 2 + 1));
            let x = (hx as f32 / u32::MAX as f32) * width as f32;
            let y = (hy as f32 / u32::MAX as f32) * height as f32;
            (x, y)
        })
        .collect();

    let sigma = clump_radius / 2.0;
    let inv_2sigma2 = 1.0 / (2.0 * sigma * sigma);

    let mut pixels = vec![0.0f32; n];
    let mut max_val: f32 = 0.0;

    for y in 0..height {
        for x in 0..width {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut val = 0.0f32;
            for &(cx, cy) in &centers {
                let dx = px - cx;
                let dy = py - cy;
                val += (-((dx * dx + dy * dy) * inv_2sigma2)).exp();
            }
            let idx = (y * width + x) as usize;
            pixels[idx] = val;
            if val > max_val {
                max_val = val;
            }
        }
    }

    // Normalize to [0, 1]
    if max_val > 0.0 {
        let inv = 1.0 / max_val;
        for p in &mut pixels {
            *p *= inv;
        }
    }

    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn food_map_dimensions() {
        let map = generate_food_map(100, 50, 3, 20.0, 42);
        assert_eq!(map.len(), 5000);
    }

    #[test]
    fn food_map_values_normalized() {
        let map = generate_food_map(64, 64, 5, 15.0, 123);
        let max = map.iter().cloned().fold(0.0f32, f32::max);
        let min = map.iter().cloned().fold(f32::MAX, f32::min);
        assert!((max - 1.0).abs() < 1e-6, "max should be ~1.0, got {max}");
        assert!(min >= 0.0, "min should be >= 0.0, got {min}");
    }

    #[test]
    fn food_map_deterministic() {
        let a = generate_food_map(32, 32, 3, 10.0, 99);
        let b = generate_food_map(32, 32, 3, 10.0, 99);
        assert_eq!(a, b);
    }

    #[test]
    fn food_map_zero_clumps() {
        let map = generate_food_map(32, 32, 0, 10.0, 42);
        assert!(map.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn food_map_single_clump_peak_near_center() {
        let map = generate_food_map(64, 64, 1, 10.0, 42);
        // Find the peak pixel
        let (peak_idx, &peak_val) = map
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap();
        assert!((peak_val - 1.0).abs() < 1e-6, "peak should be 1.0");
        // Peak should be at the clump center (not at edges)
        let peak_x = peak_idx % 64;
        let peak_y = peak_idx / 64;
        assert!(peak_x > 0 && peak_x < 63, "peak not at x edge");
        assert!(peak_y > 0 && peak_y < 63, "peak not at y edge");
    }

    #[test]
    fn food_map_different_seeds_differ() {
        let a = generate_food_map(32, 32, 3, 10.0, 1);
        let b = generate_food_map(32, 32, 3, 10.0, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn food_map_large_radius_covers_map() {
        // A single clump with a very large radius should produce nonzero values
        // across most of the map
        let map = generate_food_map(32, 32, 1, 200.0, 42);
        let nonzero_count = map.iter().filter(|&&v| v > 0.01).count();
        let total = map.len();
        assert!(
            nonzero_count > total / 2,
            "large radius clump should cover most pixels, got {nonzero_count}/{total}"
        );
    }
}
