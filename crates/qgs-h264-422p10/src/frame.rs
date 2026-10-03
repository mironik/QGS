#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plane422P10 {
    pub width: usize,
    pub height: usize,
    samples: Vec<u16>,
}

impl Plane422P10 {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            samples: vec![0; width.saturating_mul(height)],
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<u16> {
        (x < self.width && y < self.height).then(|| self.samples[y * self.width + x])
    }

    pub fn set(&mut self, x: usize, y: usize, value: u16) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.samples[y * self.width + x] = value.min(1023);
        true
    }

    pub fn write_block<const W: usize, const H: usize>(
        &mut self,
        x: usize,
        y: usize,
        block: &[[u16; W]; H],
    ) -> bool {
        if x + W > self.width || y + H > self.height {
            return false;
        }
        for (row, values) in block.iter().enumerate() {
            for (column, value) in values.iter().enumerate() {
                self.set(x + column, y + row, *value);
            }
        }
        true
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.samples.len() * 2);
        for sample in &self.samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedFrame422P10 {
    pub coded_width: usize,
    pub coded_height: usize,
    pub visible_width: usize,
    pub visible_height: usize,
    pub y: Plane422P10,
    pub cb: Plane422P10,
    pub cr: Plane422P10,
}

impl DecodedFrame422P10 {
    pub fn new(
        coded_width: usize,
        coded_height: usize,
        visible_width: usize,
        visible_height: usize,
    ) -> Self {
        Self {
            coded_width,
            coded_height,
            visible_width,
            visible_height,
            y: Plane422P10::new(coded_width, coded_height),
            cb: Plane422P10::new(coded_width / 2, coded_height),
            cr: Plane422P10::new(coded_width / 2, coded_height),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_preserves_422_chroma_geometry() {
        let frame = DecodedFrame422P10::new(1920, 1088, 1920, 1080);

        assert_eq!(frame.y.width, 1920);
        assert_eq!(frame.y.height, 1088);
        assert_eq!(frame.cb.width, 960);
        assert_eq!(frame.cb.height, 1088);
        assert_eq!(frame.cr.width, 960);
        assert_eq!(frame.cr.height, 1088);
    }

    #[test]
    fn plane_writes_and_exports_little_endian_10bit_samples() {
        let mut plane = Plane422P10::new(2, 1);
        assert!(plane.set(0, 0, 1023));
        assert!(plane.set(1, 0, 2048));

        assert_eq!(plane.get(0, 0), Some(1023));
        assert_eq!(plane.get(1, 0), Some(1023));
        assert_eq!(plane.to_le_bytes(), vec![0xff, 0x03, 0xff, 0x03]);
    }
}
