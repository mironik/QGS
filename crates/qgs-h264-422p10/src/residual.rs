use crate::transform::{inverse_transform_4x4, inverse_transform_8x8};

pub const ZIGZAG_4X4: [(usize, usize); 16] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (2, 0),
    (1, 1),
    (0, 2),
    (0, 3),
    (1, 2),
    (2, 1),
    (3, 0),
    (3, 1),
    (2, 2),
    (1, 3),
    (2, 3),
    (3, 2),
    (3, 3),
];

pub const ZIGZAG_8X8: [(usize, usize); 64] = [
    (0, 0),
    (0, 1),
    (1, 0),
    (2, 0),
    (1, 1),
    (0, 2),
    (0, 3),
    (1, 2),
    (2, 1),
    (3, 0),
    (4, 0),
    (3, 1),
    (2, 2),
    (1, 3),
    (0, 4),
    (0, 5),
    (1, 4),
    (2, 3),
    (3, 2),
    (4, 1),
    (5, 0),
    (6, 0),
    (5, 1),
    (4, 2),
    (3, 3),
    (2, 4),
    (1, 5),
    (0, 6),
    (0, 7),
    (1, 6),
    (2, 5),
    (3, 4),
    (4, 3),
    (5, 2),
    (6, 1),
    (7, 0),
    (7, 1),
    (6, 2),
    (5, 3),
    (4, 4),
    (3, 5),
    (2, 6),
    (1, 7),
    (2, 7),
    (3, 6),
    (4, 5),
    (5, 4),
    (6, 3),
    (7, 2),
    (7, 3),
    (6, 4),
    (5, 5),
    (4, 6),
    (3, 7),
    (4, 7),
    (5, 6),
    (6, 5),
    (7, 4),
    (7, 5),
    (6, 6),
    (5, 7),
    (6, 7),
    (7, 6),
    (7, 7),
];

const FLAT_4X4_SCALING: [[i32; 4]; 4] = [[16; 4]; 4];

const NORM_ADJUST_4X4: [[[i32; 4]; 4]; 6] = [
    [
        [10, 13, 10, 13],
        [13, 16, 13, 16],
        [10, 13, 10, 13],
        [13, 16, 13, 16],
    ],
    [
        [11, 14, 11, 14],
        [14, 18, 14, 18],
        [11, 14, 11, 14],
        [14, 18, 14, 18],
    ],
    [
        [13, 16, 13, 16],
        [16, 20, 16, 20],
        [13, 16, 13, 16],
        [16, 20, 16, 20],
    ],
    [
        [14, 18, 14, 18],
        [18, 23, 18, 23],
        [14, 18, 14, 18],
        [18, 23, 18, 23],
    ],
    [
        [16, 20, 16, 20],
        [20, 25, 20, 25],
        [16, 20, 16, 20],
        [20, 25, 20, 25],
    ],
    [
        [18, 23, 18, 23],
        [23, 29, 23, 29],
        [18, 23, 18, 23],
        [23, 29, 23, 29],
    ],
];

const DEQUANT_8X8_FLAT: [[[i32; 8]; 8]; 6] = [
    [
        [20, 19, 25, 19, 20, 19, 25, 19],
        [19, 18, 24, 18, 19, 18, 24, 18],
        [25, 24, 32, 24, 25, 24, 32, 24],
        [19, 18, 24, 18, 19, 18, 24, 18],
        [20, 19, 25, 19, 20, 19, 25, 19],
        [19, 18, 24, 18, 19, 18, 24, 18],
        [25, 24, 32, 24, 25, 24, 32, 24],
        [19, 18, 24, 18, 19, 18, 24, 18],
    ],
    [
        [22, 21, 28, 21, 22, 21, 28, 21],
        [21, 19, 26, 19, 21, 19, 26, 19],
        [28, 26, 35, 26, 28, 26, 35, 26],
        [21, 19, 26, 19, 21, 19, 26, 19],
        [22, 21, 28, 21, 22, 21, 28, 21],
        [21, 19, 26, 19, 21, 19, 26, 19],
        [28, 26, 35, 26, 28, 26, 35, 26],
        [21, 19, 26, 19, 21, 19, 26, 19],
    ],
    [
        [26, 24, 33, 24, 26, 24, 33, 24],
        [24, 23, 31, 23, 24, 23, 31, 23],
        [33, 31, 42, 31, 33, 31, 42, 31],
        [24, 23, 31, 23, 24, 23, 31, 23],
        [26, 24, 33, 24, 26, 24, 33, 24],
        [24, 23, 31, 23, 24, 23, 31, 23],
        [33, 31, 42, 31, 33, 31, 42, 31],
        [24, 23, 31, 23, 24, 23, 31, 23],
    ],
    [
        [28, 26, 35, 26, 28, 26, 35, 26],
        [26, 25, 33, 25, 26, 25, 33, 25],
        [35, 33, 45, 33, 35, 33, 45, 33],
        [26, 25, 33, 25, 26, 25, 33, 25],
        [28, 26, 35, 26, 28, 26, 35, 26],
        [26, 25, 33, 25, 26, 25, 33, 25],
        [35, 33, 45, 33, 35, 33, 45, 33],
        [26, 25, 33, 25, 26, 25, 33, 25],
    ],
    [
        [32, 30, 40, 30, 32, 30, 40, 30],
        [30, 28, 38, 28, 30, 28, 38, 28],
        [40, 38, 51, 38, 40, 38, 51, 38],
        [30, 28, 38, 28, 30, 28, 38, 28],
        [32, 30, 40, 30, 32, 30, 40, 30],
        [30, 28, 38, 28, 30, 28, 38, 28],
        [40, 38, 51, 38, 40, 38, 51, 38],
        [30, 28, 38, 28, 30, 28, 38, 28],
    ],
    [
        [36, 34, 46, 34, 36, 34, 46, 34],
        [34, 32, 43, 32, 34, 32, 43, 32],
        [46, 43, 58, 43, 46, 43, 58, 43],
        [34, 32, 43, 32, 34, 32, 43, 32],
        [36, 34, 46, 34, 36, 34, 46, 34],
        [34, 32, 43, 32, 34, 32, 43, 32],
        [46, 43, 58, 43, 46, 43, 58, 43],
        [34, 32, 43, 32, 34, 32, 43, 32],
    ],
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidualBlock4x4 {
    pub coeffs: [[i32; 4]; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidualBlock8x8 {
    pub coeffs: [[i32; 8]; 8],
}

impl ResidualBlock4x4 {
    pub const fn zero() -> Self {
        Self {
            coeffs: [[0; 4]; 4],
        }
    }

    pub fn from_zigzag(levels: &[i32]) -> Self {
        let mut coeffs = [[0_i32; 4]; 4];
        for (index, value) in levels.iter().take(16).enumerate() {
            let (row, column) = ZIGZAG_4X4[index];
            coeffs[row][column] = *value;
        }
        Self { coeffs }
    }

    pub fn from_raster(levels: &[i32]) -> Self {
        let mut coeffs = [[0_i32; 4]; 4];
        for (index, value) in levels.iter().take(16).enumerate() {
            coeffs[index / 4][index % 4] = *value;
        }
        Self { coeffs }
    }

    pub fn inverse_quant_flat(&self, qp: u8) -> [[i32; 4]; 4] {
        inverse_quant_4x4(self.coeffs, qp, &FLAT_4X4_SCALING)
    }

    pub fn reconstruct_with_prediction(&self, qp: u8, prediction: [[u16; 4]; 4]) -> [[u16; 4]; 4] {
        inverse_transform_4x4(self.inverse_quant_flat(qp), prediction)
    }

    pub fn reconstruct_with_prescaled_dc(
        &self,
        qp: u8,
        prediction: [[u16; 4]; 4],
    ) -> [[u16; 4]; 4] {
        let mut levels = self.inverse_quant_flat(qp);
        levels[0][0] = self.coeffs[0][0];
        inverse_transform_4x4(levels, prediction)
    }
}

impl ResidualBlock8x8 {
    pub const fn zero() -> Self {
        Self {
            coeffs: [[0; 8]; 8],
        }
    }

    pub fn from_zigzag(levels: &[i32]) -> Self {
        let mut coeffs = [[0_i32; 8]; 8];
        for (index, value) in levels.iter().take(64).enumerate() {
            let (row, column) = ZIGZAG_8X8[index];
            coeffs[row][column] = *value;
        }
        Self { coeffs }
    }

    pub fn inverse_quant_flat(&self, qp: u8) -> [[i32; 8]; 8] {
        inverse_quant_8x8_flat(self.coeffs, qp)
    }

    pub fn reconstruct_with_prediction(&self, qp: u8, prediction: [[u16; 8]; 8]) -> [[u16; 8]; 8] {
        inverse_transform_8x8(self.inverse_quant_flat(qp), prediction)
    }
}

pub fn inverse_quant_4x4(
    coeffs: [[i32; 4]; 4],
    qp: u8,
    scaling_list: &[[i32; 4]; 4],
) -> [[i32; 4]; 4] {
    let qp = i32::from(qp).clamp(0, 63);
    let qbits = qp / 6;
    let norm = &NORM_ADJUST_4X4[(qp % 6) as usize];
    let mut out = [[0_i32; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            let level_scale = scaling_list[row][column] * norm[row][column];
            let value = i64::from(coeffs[row][column]) * i64::from(level_scale);
            let scaled = if qp >= 24 {
                value << (qbits - 4)
            } else {
                (value + (1_i64 << (3 - qbits))) >> (4 - qbits)
            };
            out[row][column] = clamp_i64_to_i32(scaled);
        }
    }
    out
}

pub fn inverse_quant_8x8_flat(coeffs: [[i32; 8]; 8], qp: u8) -> [[i32; 8]; 8] {
    let qp = i32::from(qp).clamp(0, 63);
    let qbits = qp / 6 - 6;
    let matrix = &DEQUANT_8X8_FLAT[(qp % 6) as usize];
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            // Flat LevelScale is normAdjust times the default weight 16.
            let value = i64::from(coeffs[row][column]) * i64::from(matrix[row][column]) * 16;
            let scaled = if qbits >= 0 {
                value << qbits
            } else {
                (value + (1_i64 << (-qbits - 1))) >> -qbits
            };
            clamp_i64_to_i32(scaled)
        })
    })
}

fn clamp_i64_to_i32(value: i64) -> i32 {
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_places_coefficients_in_raster_positions() {
        let block = ResidualBlock4x4::from_zigzag(&(0..16).collect::<Vec<_>>());

        assert_eq!(block.coeffs[0][0], 0);
        assert_eq!(block.coeffs[0][1], 1);
        assert_eq!(block.coeffs[1][0], 2);
        assert_eq!(block.coeffs[3][3], 15);
    }

    #[test]
    fn raster_places_coefficients_without_scan_remapping() {
        let block = ResidualBlock4x4::from_raster(&(0..16).collect::<Vec<_>>());

        assert_eq!(block.coeffs[0], [0, 1, 2, 3]);
        assert_eq!(block.coeffs[1], [4, 5, 6, 7]);
    }

    #[test]
    fn zigzag_8x8_places_coefficients_in_raster_positions() {
        let block = ResidualBlock8x8::from_zigzag(&(0..64).collect::<Vec<_>>());

        assert_eq!(block.coeffs[0][0], 0);
        assert_eq!(block.coeffs[0][1], 1);
        assert_eq!(block.coeffs[1][0], 2);
        assert_eq!(block.coeffs[7][7], 63);
    }

    #[test]
    fn inverse_quant_flat_scales_dc_coefficient() {
        let mut block = ResidualBlock4x4::zero();
        block.coeffs[0][0] = 1;

        let quantized = block.inverse_quant_flat(24);

        assert_eq!(quantized[0][0], 160);
        assert_eq!(quantized[0][1], 0);
    }

    #[test]
    fn inverse_quant_8x8_flat_scales_dc_coefficient() {
        let mut block = ResidualBlock8x8::zero();
        block.coeffs[0][0] = 1;

        let quantized = block.inverse_quant_flat(36);

        assert_eq!(quantized[0][0], 320);
        assert_eq!(quantized[0][1], 0);
    }

    #[test]
    fn residual_block_reconstructs_predicted_samples() {
        let mut block = ResidualBlock4x4::zero();
        block.coeffs[0][0] = 1;
        let out = block.reconstruct_with_prediction(0, [[100_u16; 4]; 4]);

        assert!(out.iter().flatten().all(|sample| *sample >= 100));
    }
}
