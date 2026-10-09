use crate::macroblock_type::{MacroblockAddress, MacroblockGrid};

pub const SCAN8: [usize; 16 * 3 + 3] = [
    4 + 8,
    5 + 8,
    4 + 2 * 8,
    5 + 2 * 8,
    6 + 8,
    7 + 8,
    6 + 2 * 8,
    7 + 2 * 8,
    4 + 3 * 8,
    5 + 3 * 8,
    4 + 4 * 8,
    5 + 4 * 8,
    6 + 3 * 8,
    7 + 3 * 8,
    6 + 4 * 8,
    7 + 4 * 8,
    4 + 6 * 8,
    5 + 6 * 8,
    4 + 7 * 8,
    5 + 7 * 8,
    6 + 6 * 8,
    7 + 6 * 8,
    6 + 7 * 8,
    7 + 7 * 8,
    4 + 8 * 8,
    5 + 8 * 8,
    4 + 9 * 8,
    5 + 9 * 8,
    6 + 8 * 8,
    7 + 8 * 8,
    6 + 9 * 8,
    7 + 9 * 8,
    4 + 11 * 8,
    5 + 11 * 8,
    4 + 12 * 8,
    5 + 12 * 8,
    6 + 11 * 8,
    7 + 11 * 8,
    6 + 12 * 8,
    7 + 12 * 8,
    4 + 13 * 8,
    5 + 13 * 8,
    4 + 14 * 8,
    5 + 14 * 8,
    6 + 13 * 8,
    7 + 13 * 8,
    6 + 14 * 8,
    7 + 14 * 8,
    0,
    5 * 8,
    10 * 8,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacNonZeroState422 {
    width_in_mbs: u32,
    slice_first_mb: u32,
    unavailable_nonzero: bool,
    luma16_dc: Vec<u8>,
    luma4x4: Vec<[u8; 16]>,
    chroma_dc: Vec<[u8; 2]>,
    chroma_ac: Vec<[[u8; 8]; 2]>,
}

impl CabacNonZeroState422 {
    pub fn new(grid: MacroblockGrid) -> Self {
        let count = grid.macroblock_count() as usize;
        Self {
            width_in_mbs: grid.width_in_mbs,
            slice_first_mb: 0,
            unavailable_nonzero: false,
            luma16_dc: vec![0; count],
            luma4x4: vec![[0; 16]; count],
            chroma_dc: vec![[0; 2]; count],
            chroma_ac: vec![[[0; 8]; 2]; count],
        }
    }

    pub fn set_slice_first_mb(&mut self, slice_first_mb: u32) {
        self.slice_first_mb = slice_first_mb;
    }

    pub fn set_unavailable_nonzero(&mut self, unavailable_nonzero: bool) {
        self.unavailable_nonzero = unavailable_nonzero;
    }

    pub fn new_i_slice(grid: MacroblockGrid) -> Self {
        Self {
            unavailable_nonzero: true,
            ..Self::new(grid)
        }
    }

    pub fn luma16_dc_context(&self, address: MacroblockAddress) -> usize {
        self.mb_context(address, |state, index| state.luma16_dc[index])
    }

    pub fn set_luma16_dc_count(&mut self, address: MacroblockAddress, count: usize) {
        if let Some(slot) = self.luma16_dc.get_mut(address.address as usize) {
            *slot = count.min(u8::MAX as usize) as u8;
        }
    }

    pub fn luma4x4_context(&self, address: MacroblockAddress, block_index: usize) -> usize {
        let left = self.luma4x4_left_count(address, block_index);
        let top = self.luma4x4_top_count(address, block_index);
        usize::from(left > 0) + 2 * usize::from(top > 0)
    }

    pub fn luma4x4_counts(&self, address: MacroblockAddress) -> [u8; 16] {
        self.luma4x4
            .get(address.address as usize)
            .copied()
            .unwrap_or([0; 16])
    }

    pub fn set_luma4x4_count(
        &mut self,
        address: MacroblockAddress,
        block_index: usize,
        count: usize,
    ) {
        if let Some(blocks) = self.luma4x4.get_mut(address.address as usize) {
            if let Some(slot) = blocks.get_mut(block_index) {
                *slot = count.min(u8::MAX as usize) as u8;
            }
        }
    }

    pub fn clear_luma4x4(&mut self, address: MacroblockAddress) {
        if let Some(blocks) = self.luma4x4.get_mut(address.address as usize) {
            *blocks = [0; 16];
        }
    }

    pub fn chroma422_dc_context(&self, address: MacroblockAddress, plane: usize) -> usize {
        self.mb_context(address, |state, index| state.chroma_dc[index][plane])
    }

    pub fn set_chroma422_dc_count(
        &mut self,
        address: MacroblockAddress,
        plane: usize,
        count: usize,
    ) {
        if let Some(planes) = self.chroma_dc.get_mut(address.address as usize) {
            if let Some(slot) = planes.get_mut(plane) {
                *slot = count.min(u8::MAX as usize) as u8;
            }
        }
    }

    pub fn chroma422_ac_context(
        &self,
        address: MacroblockAddress,
        plane: usize,
        block_index: usize,
    ) -> usize {
        let left = self.chroma422_ac_left_count(address, plane, block_index);
        let top = self.chroma422_ac_top_count(address, plane, block_index);
        usize::from(left > 0) + 2 * usize::from(top > 0)
    }

    pub fn set_chroma422_ac_count(
        &mut self,
        address: MacroblockAddress,
        plane: usize,
        block_index: usize,
        count: usize,
    ) {
        if let Some(planes) = self.chroma_ac.get_mut(address.address as usize) {
            if let Some(blocks) = planes.get_mut(plane) {
                if let Some(slot) = blocks.get_mut(block_index) {
                    *slot = count.min(u8::MAX as usize) as u8;
                }
            }
        }
    }

    pub fn clear_chroma422_ac(&mut self, address: MacroblockAddress) {
        if let Some(planes) = self.chroma_ac.get_mut(address.address as usize) {
            *planes = [[0; 8]; 2];
        }
    }

    pub fn mark_pcm(&mut self, address: MacroblockAddress) {
        self.set_luma16_dc_count(address, 16);
        if let Some(blocks) = self.luma4x4.get_mut(address.address as usize) {
            *blocks = [16; 16];
        }
        if let Some(planes) = self.chroma_dc.get_mut(address.address as usize) {
            *planes = [8; 2];
        }
        if let Some(planes) = self.chroma_ac.get_mut(address.address as usize) {
            *planes = [[15; 8]; 2];
        }
    }

    fn luma4x4_left_count(&self, address: MacroblockAddress, block_index: usize) -> u8 {
        let (x, y) = luma4x4_position(block_index);
        if x > 0 {
            self.luma4x4
                .get(address.address as usize)
                .map(|blocks| blocks[luma4x4_index(x - 1, y)])
                .unwrap_or(0)
        } else {
            self.left_mb_index(address)
                .and_then(|index| {
                    self.luma4x4
                        .get(index)
                        .map(|blocks| blocks[luma4x4_index(3, y)])
                })
                .unwrap_or_else(|| self.unavailable_count())
        }
    }

    fn luma4x4_top_count(&self, address: MacroblockAddress, block_index: usize) -> u8 {
        let (x, y) = luma4x4_position(block_index);
        if y > 0 {
            self.luma4x4
                .get(address.address as usize)
                .map(|blocks| blocks[luma4x4_index(x, y - 1)])
                .unwrap_or(0)
        } else {
            self.top_mb_index(address)
                .and_then(|index| {
                    self.luma4x4
                        .get(index)
                        .map(|blocks| blocks[luma4x4_index(x, 3)])
                })
                .unwrap_or_else(|| self.unavailable_count())
        }
    }

    fn chroma422_ac_left_count(
        &self,
        address: MacroblockAddress,
        plane: usize,
        block_index: usize,
    ) -> u8 {
        let x = block_index % 2;
        let y = block_index / 2;
        if x > 0 {
            self.chroma_ac
                .get(address.address as usize)
                .map(|planes| planes[plane][block_index - 1])
                .unwrap_or(0)
        } else {
            self.left_mb_index(address)
                .and_then(|index| {
                    self.chroma_ac
                        .get(index)
                        .map(|planes| planes[plane][y * 2 + 1])
                })
                .unwrap_or_else(|| self.unavailable_count())
        }
    }

    fn chroma422_ac_top_count(
        &self,
        address: MacroblockAddress,
        plane: usize,
        block_index: usize,
    ) -> u8 {
        let x = block_index % 2;
        let y = block_index / 2;
        if y > 0 {
            self.chroma_ac
                .get(address.address as usize)
                .map(|planes| planes[plane][block_index - 2])
                .unwrap_or(0)
        } else {
            self.top_mb_index(address)
                .and_then(|index| self.chroma_ac.get(index).map(|planes| planes[plane][6 + x]))
                .unwrap_or_else(|| self.unavailable_count())
        }
    }

    fn mb_context(&self, address: MacroblockAddress, read: impl Fn(&Self, usize) -> u8) -> usize {
        let left = self
            .left_mb_index(address)
            .map(|index| read(self, index))
            .unwrap_or_else(|| self.unavailable_count());
        let top = self
            .top_mb_index(address)
            .map(|index| read(self, index))
            .unwrap_or_else(|| self.unavailable_count());
        usize::from(left > 0) + 2 * usize::from(top > 0)
    }

    fn unavailable_count(&self) -> u8 {
        u8::from(self.unavailable_nonzero)
    }

    fn left_mb_index(&self, address: MacroblockAddress) -> Option<usize> {
        let left = address.address.saturating_sub(1);
        (address.x > 0 && left >= self.slice_first_mb).then_some(left as usize)
    }

    fn top_mb_index(&self, address: MacroblockAddress) -> Option<usize> {
        let top = address.address.saturating_sub(self.width_in_mbs);
        (address.y > 0 && top >= self.slice_first_mb).then_some(top as usize)
    }
}

fn luma4x4_position(block_index: usize) -> (usize, usize) {
    let x = (block_index & 1) + ((block_index >> 2) & 1) * 2;
    let y = ((block_index >> 1) & 1) + ((block_index >> 3) & 1) * 2;
    (x, y)
}

fn luma4x4_index(x: usize, y: usize) -> usize {
    (x & 1) + (y & 1) * 2 + (x >> 1) * 4 + (y >> 1) * 8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> MacroblockGrid {
        MacroblockGrid::new(32, 32).unwrap()
    }

    #[test]
    fn scan8_matches_progressive_luma_block_layout() {
        assert_eq!(SCAN8[0], 12);
        assert_eq!(SCAN8[1], 13);
        assert_eq!(SCAN8[2], 20);
        assert_eq!(SCAN8[15], 39);
        assert_eq!(SCAN8[16], 52);
        assert_eq!(SCAN8[31], 79);
        assert_eq!(SCAN8[48], 0);
        assert_eq!(SCAN8[49], 40);
        assert_eq!(SCAN8[50], 80);
    }

    #[test]
    fn luma4x4_context_uses_left_and_top_blocks_inside_macroblock() {
        let mut state = CabacNonZeroState422::new(grid());
        let mb = grid().address(0).unwrap();
        state.set_luma4x4_count(mb, 0, 1);
        state.set_luma4x4_count(mb, 1, 3);
        state.set_luma4x4_count(mb, 2, 2);

        assert_eq!(state.luma4x4_context(mb, 1), 1);
        assert_eq!(state.luma4x4_context(mb, 2), 2);
        assert_eq!(state.luma4x4_context(mb, 3), 3);
    }

    #[test]
    fn luma4x4_context_crosses_macroblock_edges() {
        let mut state = CabacNonZeroState422::new(grid());
        let left = grid().address(0).unwrap();
        let right = grid().address(1).unwrap();
        let bottom = grid().address(2).unwrap();
        state.set_luma4x4_count(left, 5, 4);
        state.set_luma4x4_count(left, 10, 0);
        state.set_luma4x4_count(left, 11, 5);
        state.set_luma4x4_count(left, 14, 0);
        state.set_luma4x4_count(left, 15, 6);

        assert_eq!(state.luma4x4_context(right, 0), 1);
        assert_eq!(state.luma4x4_context(bottom, 0), 0);
        assert_eq!(state.luma4x4_context(bottom, 1), 2);
        assert_eq!(state.luma4x4_context(bottom, 5), 2);
    }

    #[test]
    fn chroma422_ac_context_uses_two_by_four_block_geometry() {
        let mut state = CabacNonZeroState422::new(grid());
        let mb = grid().address(0).unwrap();
        state.set_chroma422_ac_count(mb, 0, 0, 1);
        state.set_chroma422_ac_count(mb, 0, 1, 1);
        state.set_chroma422_ac_count(mb, 0, 2, 2);

        assert_eq!(state.chroma422_ac_context(mb, 0, 1), 1);
        assert_eq!(state.chroma422_ac_context(mb, 0, 2), 2);
        assert_eq!(state.chroma422_ac_context(mb, 0, 3), 3);
    }

    #[test]
    fn mb_level_context_uses_left_and_top_macroblocks() {
        let mut state = CabacNonZeroState422::new(grid());
        let top_left = grid().address(0).unwrap();
        let top_right = grid().address(1).unwrap();
        let bottom_left = grid().address(2).unwrap();
        let bottom_right = grid().address(3).unwrap();
        state.set_luma16_dc_count(top_right, 1);
        state.set_luma16_dc_count(bottom_left, 1);
        state.set_chroma422_dc_count(top_right, 1, 1);
        state.set_chroma422_dc_count(bottom_left, 1, 1);

        assert_eq!(state.luma16_dc_context(top_left), 0);
        assert_eq!(state.luma16_dc_context(bottom_right), 3);
        assert_eq!(state.chroma422_dc_context(bottom_right, 1), 3);
    }

    #[test]
    fn pcm_marks_all_residual_neighbors_present() {
        let mut state = CabacNonZeroState422::new(grid());
        let mb = grid().address(0).unwrap();
        state.mark_pcm(mb);

        assert_eq!(state.luma16_dc_context(grid().address(1).unwrap()), 1);
        assert_eq!(state.luma4x4_context(mb, 1), 1);
        assert_eq!(state.chroma422_ac_context(mb, 0, 1), 1);
    }
}
