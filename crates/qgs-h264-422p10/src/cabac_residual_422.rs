use crate::cabac::CabacDecoder;
use crate::cabac_nonzero::CabacNonZeroState422;
use crate::cabac_residual::{
    decode_residual_4x4_ac_category, decode_residual_4x4_category,
    decode_residual_chroma422_dc_category, i_slice_residual_context_bank,
    pb_slice_residual_context_bank, CabacResidualCategory, CabacResidualCategoryContexts,
    CabacResidualDecodeReport, CabacResidualError,
};
use crate::macroblock_type::{CodedBlockPatternChroma, MacroblockAddress, MacroblockGrid};
use crate::residual::ResidualBlock4x4;
use crate::transform::{inverse_chroma422_dc_transform, inverse_intra16x16_dc_transform};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Intra16x16LumaResidual422 {
    pub dc: ResidualBlock4x4,
    pub ac: [ResidualBlock4x4; 16],
    pub dc_report: CabacResidualDecodeReport,
    pub ac_reports: [CabacResidualDecodeReport; 16],
}

impl Intra16x16LumaResidual422 {
    pub fn into_reconstruction_blocks(self, qp_y: u8) -> [ResidualBlock4x4; 16] {
        let dc = inverse_intra16x16_dc_transform(self.dc.coeffs, qp_y);
        std::array::from_fn(|block_index| {
            let mut block = self.ac[block_index].clone();
            let block_x = (block_index & 1) + ((block_index >> 2) & 1) * 2;
            let block_y = ((block_index >> 1) & 1) + ((block_index >> 3) & 1) * 2;
            block.coeffs[0][0] = dc[block_y][block_x];
            block
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chroma422Residual {
    pub dc: [ResidualBlock4x4; 2],
    pub ac: [[ResidualBlock4x4; 8]; 2],
    pub dc_reports: [CabacResidualDecodeReport; 2],
    pub ac_reports: [[CabacResidualDecodeReport; 8]; 2],
}

impl Chroma422Residual {
    pub fn into_reconstruction_blocks(self, qp_cb: u8, qp_cr: u8) -> [[ResidualBlock4x4; 8]; 2] {
        let quantizers = [qp_cb, qp_cr];
        std::array::from_fn(|plane| {
            let dc_levels = chroma422_dc_levels(&self.dc[plane]);
            let dc = inverse_chroma422_dc_transform(dc_levels, quantizers[plane]);
            std::array::from_fn(|block_index| {
                let mut block = self.ac[plane][block_index].clone();
                block.coeffs[0][0] = dc[block_index / 2][block_index % 2];
                block
            })
        })
    }
}

#[derive(Clone, Debug)]
pub struct CabacResidualDecoder422 {
    nonzero: CabacNonZeroState422,
    luma16_dc: CabacResidualCategoryContexts,
    luma16_ac: CabacResidualCategoryContexts,
    luma4x4: CabacResidualCategoryContexts,
    chroma_dc: CabacResidualCategoryContexts,
    chroma_ac: CabacResidualCategoryContexts,
}

impl CabacResidualDecoder422 {
    pub fn new_i_slice(grid: MacroblockGrid, qp_y: u8) -> Self {
        let bank = i_slice_residual_context_bank(qp_y);
        Self::with_bank(grid, bank, true)
    }

    pub fn new_pb_slice(grid: MacroblockGrid, qp_y: u8, cabac_init_idc: u8) -> Self {
        let bank = pb_slice_residual_context_bank(qp_y, cabac_init_idc);
        Self::with_bank(grid, bank, false)
    }

    fn with_bank(
        grid: MacroblockGrid,
        bank: std::rc::Rc<std::cell::RefCell<Vec<crate::cabac::CabacContext>>>,
        unavailable_nonzero: bool,
    ) -> Self {
        let nonzero = if unavailable_nonzero {
            CabacNonZeroState422::new_i_slice(grid)
        } else {
            CabacNonZeroState422::new(grid)
        };
        Self {
            nonzero,
            luma16_dc: CabacResidualCategoryContexts::i_slice_with_bank(
                CabacResidualCategory::Luma16Dc,
                bank.clone(),
            ),
            luma16_ac: CabacResidualCategoryContexts::i_slice_with_bank(
                CabacResidualCategory::Luma16Ac,
                bank.clone(),
            ),
            luma4x4: CabacResidualCategoryContexts::i_slice_with_bank(
                CabacResidualCategory::Luma4x4,
                bank.clone(),
            ),
            chroma_dc: CabacResidualCategoryContexts::i_slice_with_bank(
                CabacResidualCategory::Chroma422Dc,
                bank.clone(),
            ),
            chroma_ac: CabacResidualCategoryContexts::i_slice_with_bank(
                CabacResidualCategory::Chroma422Ac,
                bank,
            ),
        }
    }

    pub fn nonzero(&self) -> &CabacNonZeroState422 {
        &self.nonzero
    }

    pub fn set_slice_first_mb(&mut self, slice_first_mb: u32) {
        self.nonzero.set_slice_first_mb(slice_first_mb);
    }

    pub fn set_current_macroblock_intra(&mut self, intra: bool) {
        self.nonzero.set_unavailable_nonzero(intra);
    }

    pub fn decode_intra16x16_luma(
        &mut self,
        cabac: &mut CabacDecoder<'_>,
        address: MacroblockAddress,
        coded_block_pattern_luma: u8,
    ) -> Result<Intra16x16LumaResidual422, CabacResidualError> {
        let dc_context = self.nonzero.luma16_dc_context(address);
        let dc = decode_residual_4x4_category(cabac, &mut self.luma16_dc, dc_context)?;
        self.record_luma16_dc(address, dc.report);

        let mut ac = zero_blocks_16();
        let mut ac_reports = zero_reports_16();
        for block_index in 0..16 {
            if coded_block_pattern_luma & (1 << (block_index / 4)) == 0 {
                self.nonzero.set_luma4x4_count(address, block_index, 0);
                continue;
            }
            let context = self.nonzero.luma4x4_context(address, block_index);
            let residual = decode_residual_4x4_ac_category(cabac, &mut self.luma16_ac, context)?;
            self.record_luma4x4(address, block_index, residual.report);
            ac[block_index] = residual.block;
            ac_reports[block_index] = residual.report;
        }

        Ok(Intra16x16LumaResidual422 {
            dc: dc.block,
            ac,
            dc_report: dc.report,
            ac_reports,
        })
    }

    pub fn decode_intra4x4_luma(
        &mut self,
        cabac: &mut CabacDecoder<'_>,
        address: MacroblockAddress,
        coded_block_pattern_luma: u8,
    ) -> Result<([ResidualBlock4x4; 16], [CabacResidualDecodeReport; 16]), CabacResidualError> {
        let mut blocks = zero_blocks_16();
        let mut reports = zero_reports_16();
        for block_index in 0..16 {
            if coded_block_pattern_luma & (1 << (block_index / 4)) == 0 {
                self.nonzero.set_luma4x4_count(address, block_index, 0);
                continue;
            }
            let context = self.nonzero.luma4x4_context(address, block_index);
            let residual = decode_residual_4x4_category(cabac, &mut self.luma4x4, context)?;
            self.record_luma4x4(address, block_index, residual.report);
            blocks[block_index] = residual.block;
            reports[block_index] = residual.report;
        }
        Ok((blocks, reports))
    }

    pub fn decode_inter4x4_luma(
        &mut self,
        cabac: &mut CabacDecoder<'_>,
        address: MacroblockAddress,
        coded_block_pattern_luma: u8,
    ) -> Result<([ResidualBlock4x4; 16], [CabacResidualDecodeReport; 16]), CabacResidualError> {
        self.decode_intra4x4_luma(cabac, address, coded_block_pattern_luma)
    }

    pub fn decode_chroma_422(
        &mut self,
        cabac: &mut CabacDecoder<'_>,
        address: MacroblockAddress,
        coded_block_pattern_chroma: CodedBlockPatternChroma,
    ) -> Result<Chroma422Residual, CabacResidualError> {
        let mut dc = [ResidualBlock4x4::zero(), ResidualBlock4x4::zero()];
        let mut ac = std::array::from_fn(|_| zero_blocks_8());
        let mut dc_reports = [zero_report(), zero_report()];
        let mut ac_reports = std::array::from_fn(|_| zero_reports_8());

        if coded_block_pattern_chroma != CodedBlockPatternChroma::Zero {
            for plane in 0..2 {
                let context = self.nonzero.chroma422_dc_context(address, plane);
                let residual =
                    decode_residual_chroma422_dc_category(cabac, &mut self.chroma_dc, context)?;
                self.record_chroma422_dc(address, plane, residual.report);
                dc[plane] = residual.block;
                dc_reports[plane] = residual.report;
            }
        } else {
            for plane in 0..2 {
                self.nonzero.set_chroma422_dc_count(address, plane, 0);
            }
        }

        if coded_block_pattern_chroma == CodedBlockPatternChroma::DcAndAc {
            for plane in 0..2 {
                for block_index in 0..8 {
                    let context = self
                        .nonzero
                        .chroma422_ac_context(address, plane, block_index);
                    let residual =
                        decode_residual_4x4_ac_category(cabac, &mut self.chroma_ac, context)?;
                    self.record_chroma422_ac(address, plane, block_index, residual.report);
                    ac[plane][block_index] = residual.block;
                    ac_reports[plane][block_index] = residual.report;
                }
            }
        } else {
            for plane in 0..2 {
                for block_index in 0..8 {
                    self.nonzero
                        .set_chroma422_ac_count(address, plane, block_index, 0);
                }
            }
        }

        Ok(Chroma422Residual {
            dc,
            ac,
            dc_reports,
            ac_reports,
        })
    }

    pub fn record_luma16_dc(
        &mut self,
        address: MacroblockAddress,
        report: CabacResidualDecodeReport,
    ) {
        self.nonzero
            .set_luma16_dc_count(address, report.non_zero_coefficients);
    }

    pub fn record_luma4x4(
        &mut self,
        address: MacroblockAddress,
        block_index: usize,
        report: CabacResidualDecodeReport,
    ) {
        self.nonzero
            .set_luma4x4_count(address, block_index, report.non_zero_coefficients);
    }

    pub fn record_luma8x8_count(
        &mut self,
        address: MacroblockAddress,
        block_index: usize,
        count: usize,
    ) {
        const LUMA8X8_BLOCKS: [[usize; 4]; 4] =
            [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]];
        if let Some(blocks) = LUMA8X8_BLOCKS.get(block_index) {
            for &block in blocks {
                self.nonzero.set_luma4x4_count(address, block, count);
            }
        }
    }

    pub fn record_inter_absent(&mut self, address: MacroblockAddress) {
        self.record_inter_luma_absent(address);
        self.record_inter_chroma_absent(address);
    }

    pub fn record_inter_luma_absent(&mut self, address: MacroblockAddress) {
        self.nonzero.clear_luma4x4(address);
    }

    pub fn record_inter_chroma_absent(&mut self, address: MacroblockAddress) {
        for plane in 0..2 {
            self.nonzero.set_chroma422_dc_count(address, plane, 0);
        }
        self.nonzero.clear_chroma422_ac(address);
    }

    pub fn record_chroma422_dc(
        &mut self,
        address: MacroblockAddress,
        plane: usize,
        report: CabacResidualDecodeReport,
    ) {
        self.nonzero
            .set_chroma422_dc_count(address, plane, report.non_zero_coefficients);
    }

    pub fn record_chroma422_ac(
        &mut self,
        address: MacroblockAddress,
        plane: usize,
        block_index: usize,
        report: CabacResidualDecodeReport,
    ) {
        self.nonzero.set_chroma422_ac_count(
            address,
            plane,
            block_index,
            report.non_zero_coefficients,
        );
    }

    pub fn record_pcm(&mut self, address: MacroblockAddress) {
        self.nonzero.mark_pcm(address);
    }
}

fn zero_blocks_16() -> [ResidualBlock4x4; 16] {
    std::array::from_fn(|_| ResidualBlock4x4::zero())
}

fn zero_blocks_8() -> [ResidualBlock4x4; 8] {
    std::array::from_fn(|_| ResidualBlock4x4::zero())
}

fn zero_report() -> CabacResidualDecodeReport {
    CabacResidualDecodeReport {
        coded: false,
        non_zero_coefficients: 0,
        last_scan_index: None,
    }
}

fn zero_reports_16() -> [CabacResidualDecodeReport; 16] {
    [zero_report(); 16]
}

fn zero_reports_8() -> [CabacResidualDecodeReport; 8] {
    [zero_report(); 8]
}

fn chroma422_dc_levels(block: &ResidualBlock4x4) -> [[i32; 2]; 4] {
    std::array::from_fn(|row| std::array::from_fn(|column| block.coeffs[row][column]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> MacroblockGrid {
        MacroblockGrid::new(32, 32).unwrap()
    }

    fn addr(address: u32) -> MacroblockAddress {
        grid().address(address).unwrap()
    }

    fn report(count: usize) -> CabacResidualDecodeReport {
        CabacResidualDecodeReport {
            coded: count > 0,
            non_zero_coefficients: count,
            last_scan_index: (count > 0).then_some(count - 1),
        }
    }

    #[test]
    fn records_intra16x16_luma_dc_for_neighbor_context() {
        let mut decoder = CabacResidualDecoder422::new_i_slice(grid(), 24);
        decoder.record_luma16_dc(addr(0), report(3));

        assert_eq!(decoder.nonzero().luma16_dc_context(addr(1)), 3);
        assert_eq!(decoder.nonzero().luma16_dc_context(addr(2)), 3);
        assert_eq!(decoder.nonzero().luma16_dc_context(addr(3)), 0);
    }

    #[test]
    fn records_luma4x4_counts_for_scan_neighbors() {
        let mut decoder = CabacResidualDecoder422::new_i_slice(grid(), 24);
        decoder.record_luma4x4(addr(3), 0, report(2));
        decoder.record_luma4x4(addr(3), 1, report(1));
        decoder.record_luma4x4(addr(3), 2, report(1));

        assert_eq!(decoder.nonzero().luma4x4_context(addr(3), 1), 1);
        assert_eq!(decoder.nonzero().luma4x4_context(addr(3), 2), 2);
        assert_eq!(decoder.nonzero().luma4x4_context(addr(3), 3), 3);
    }

    #[test]
    fn records_chroma422_dc_and_ac_counts_separately() {
        let mut decoder = CabacResidualDecoder422::new_i_slice(grid(), 24);
        decoder.record_chroma422_dc(addr(0), 0, report(4));
        decoder.record_chroma422_ac(addr(0), 1, 0, report(2));

        assert_eq!(decoder.nonzero().chroma422_dc_context(addr(1), 0), 3);
        assert_eq!(decoder.nonzero().chroma422_dc_context(addr(1), 1), 2);
        assert_eq!(decoder.nonzero().chroma422_ac_context(addr(0), 1, 1), 3);
    }

    #[test]
    fn pcm_marks_all_residual_categories_nonzero() {
        let mut decoder = CabacResidualDecoder422::new_i_slice(grid(), 24);
        decoder.record_pcm(addr(0));

        assert_eq!(decoder.nonzero().luma16_dc_context(addr(1)), 3);
        assert_eq!(decoder.nonzero().luma4x4_context(addr(0), 1), 3);
        assert_eq!(decoder.nonzero().chroma422_dc_context(addr(1), 0), 3);
        assert_eq!(decoder.nonzero().chroma422_ac_context(addr(0), 0, 1), 3);
    }

    #[test]
    fn intra16x16_residual_combines_dc_transform_with_ac_blocks() {
        let mut dc = ResidualBlock4x4::zero();
        dc.coeffs[0][0] = 1;
        let mut ac = zero_blocks_16();
        ac[5].coeffs[1][2] = -3;
        let residual = Intra16x16LumaResidual422 {
            dc,
            ac,
            dc_report: report(1),
            ac_reports: zero_reports_16(),
        };

        let blocks = residual.into_reconstruction_blocks(24);

        assert!(blocks.iter().all(|block| block.coeffs[0][0] > 0));
        assert_eq!(blocks[5].coeffs[1][2], -3);
    }

    #[test]
    fn chroma422_residual_combines_dc_transform_with_ac_blocks() {
        let mut cb_dc = ResidualBlock4x4::zero();
        cb_dc.coeffs[0][0] = 1;
        let cr_dc = ResidualBlock4x4::zero();
        let mut ac = std::array::from_fn(|_| zero_blocks_8());
        ac[0][3].coeffs[2][1] = 7;
        let residual = Chroma422Residual {
            dc: [cb_dc, cr_dc],
            ac,
            dc_reports: [report(1), zero_report()],
            ac_reports: std::array::from_fn(|_| zero_reports_8()),
        };

        let blocks = residual.into_reconstruction_blocks(24, 24);

        assert!(blocks[0].iter().all(|block| block.coeffs[0][0] > 0));
        assert!(blocks[1].iter().all(|block| block.coeffs[0][0] == 0));
        assert_eq!(blocks[0][3].coeffs[2][1], 7);
    }
}
