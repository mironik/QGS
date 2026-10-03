use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Intra16x16PredictionMode {
    Vertical,
    Horizontal,
    Dc,
    Plane,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodedBlockPatternChroma {
    Zero,
    Dc,
    DcAndAc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ISliceMacroblockType {
    IntraNxN,
    Intra16x16 {
        prediction: Intra16x16PredictionMode,
        coded_block_pattern_chroma: CodedBlockPatternChroma,
        coded_block_pattern_luma: u8,
    },
    Pcm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PSliceMacroblockType {
    L0_16x16,
    L0L0_16x8,
    L0L0_8x16,
    P8x8,
    P8x8Ref0,
    Intra(ISliceMacroblockType),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BPredictionList {
    L0,
    L1,
    Bi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BSliceMacroblockType {
    Direct16x16,
    Pred16x16(BPredictionList),
    Pred16x8([BPredictionList; 2]),
    Pred8x16([BPredictionList; 2]),
    B8x8,
    Intra(ISliceMacroblockType),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BSubMacroblockType {
    Direct8x8,
    Pred8x8(BPredictionList),
    Pred8x4(BPredictionList),
    Pred4x8(BPredictionList),
    Pred4x4(BPredictionList),
}

pub fn b_sub_macroblock_type_from_code(
    code: u8,
) -> Result<BSubMacroblockType, MacroblockTypeError> {
    use BPredictionList::{Bi, L0, L1};
    match code {
        0 => Ok(BSubMacroblockType::Direct8x8),
        1 => Ok(BSubMacroblockType::Pred8x8(L0)),
        2 => Ok(BSubMacroblockType::Pred8x8(L1)),
        3 => Ok(BSubMacroblockType::Pred8x8(Bi)),
        4 => Ok(BSubMacroblockType::Pred8x4(L0)),
        5 => Ok(BSubMacroblockType::Pred4x8(L0)),
        6 => Ok(BSubMacroblockType::Pred8x4(L1)),
        7 => Ok(BSubMacroblockType::Pred4x8(L1)),
        8 => Ok(BSubMacroblockType::Pred8x4(Bi)),
        9 => Ok(BSubMacroblockType::Pred4x8(Bi)),
        10 => Ok(BSubMacroblockType::Pred4x4(L0)),
        11 => Ok(BSubMacroblockType::Pred4x4(L1)),
        12 => Ok(BSubMacroblockType::Pred4x4(Bi)),
        _ => Err(MacroblockTypeError::InvalidBSubMacroblockTypeCode(code)),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedIntraResidualPattern {
    pub coded_block_pattern_luma: u8,
    pub coded_block_pattern_chroma: CodedBlockPatternChroma,
}

impl ResolvedIntraResidualPattern {
    pub fn from_macroblock_type(
        mb_type: ISliceMacroblockType,
        signaled_luma: Option<u8>,
        signaled_chroma: Option<CodedBlockPatternChroma>,
    ) -> Result<Self, MacroblockTypeError> {
        match mb_type {
            ISliceMacroblockType::IntraNxN => Ok(Self {
                coded_block_pattern_luma: signaled_luma
                    .ok_or(MacroblockTypeError::MissingSignaledCodedBlockPattern)?,
                coded_block_pattern_chroma: signaled_chroma
                    .ok_or(MacroblockTypeError::MissingSignaledCodedBlockPattern)?,
            }),
            ISliceMacroblockType::Intra16x16 {
                coded_block_pattern_chroma,
                coded_block_pattern_luma,
                ..
            } => Ok(Self {
                coded_block_pattern_luma,
                coded_block_pattern_chroma,
            }),
            ISliceMacroblockType::Pcm => Ok(Self {
                coded_block_pattern_luma: 15,
                coded_block_pattern_chroma: CodedBlockPatternChroma::DcAndAc,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResidualPresence {
    Absent,
    Present,
    SignaledSeparately,
    PcmRawSamples,
}

impl ISliceMacroblockType {
    pub fn luma_residual_presence(self) -> ResidualPresence {
        match self {
            Self::IntraNxN => ResidualPresence::SignaledSeparately,
            Self::Intra16x16 {
                coded_block_pattern_luma,
                ..
            } => {
                if coded_block_pattern_luma == 0 {
                    ResidualPresence::Absent
                } else {
                    ResidualPresence::Present
                }
            }
            Self::Pcm => ResidualPresence::PcmRawSamples,
        }
    }

    pub fn chroma_dc_presence(self) -> ResidualPresence {
        match self {
            Self::IntraNxN => ResidualPresence::SignaledSeparately,
            Self::Intra16x16 {
                coded_block_pattern_chroma,
                ..
            } => {
                if coded_block_pattern_chroma == CodedBlockPatternChroma::Zero {
                    ResidualPresence::Absent
                } else {
                    ResidualPresence::Present
                }
            }
            Self::Pcm => ResidualPresence::PcmRawSamples,
        }
    }

    pub fn chroma_ac_presence(self) -> ResidualPresence {
        match self {
            Self::IntraNxN => ResidualPresence::SignaledSeparately,
            Self::Intra16x16 {
                coded_block_pattern_chroma,
                ..
            } => {
                if coded_block_pattern_chroma == CodedBlockPatternChroma::DcAndAc {
                    ResidualPresence::Present
                } else {
                    ResidualPresence::Absent
                }
            }
            Self::Pcm => ResidualPresence::PcmRawSamples,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacroblockAddress {
    pub address: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacroblockGrid {
    pub width_in_mbs: u32,
    pub height_in_mbs: u32,
}

impl MacroblockGrid {
    pub fn new(coded_width: u32, coded_height: u32) -> Result<Self, MacroblockTypeError> {
        if coded_width == 0 || coded_height == 0 {
            return Err(MacroblockTypeError::InvalidGrid);
        }
        if !coded_width.is_multiple_of(16) || !coded_height.is_multiple_of(16) {
            return Err(MacroblockTypeError::InvalidGrid);
        }
        Ok(Self {
            width_in_mbs: coded_width / 16,
            height_in_mbs: coded_height / 16,
        })
    }

    pub fn macroblock_count(self) -> u32 {
        self.width_in_mbs.saturating_mul(self.height_in_mbs)
    }

    pub fn address(self, address: u32) -> Option<MacroblockAddress> {
        (address < self.macroblock_count()).then_some(MacroblockAddress {
            address,
            x: address % self.width_in_mbs,
            y: address / self.width_in_mbs,
        })
    }
}

pub fn i_slice_macroblock_type_from_code(
    code: u8,
) -> Result<ISliceMacroblockType, MacroblockTypeError> {
    match code {
        0 => Ok(ISliceMacroblockType::IntraNxN),
        1..=24 => {
            let index = code - 1;
            Ok(ISliceMacroblockType::Intra16x16 {
                prediction: intra16x16_prediction_mode(
                    INTRA16X16_PREDICTION_MODE_ORDER[(index % 4) as usize],
                ),
                coded_block_pattern_chroma: coded_block_pattern_chroma((index / 4) % 3),
                coded_block_pattern_luma: if index >= 12 { 15 } else { 0 },
            })
        }
        25 => Ok(ISliceMacroblockType::Pcm),
        _ => Err(MacroblockTypeError::InvalidISliceMacroblockTypeCode(code)),
    }
}

const INTRA16X16_PREDICTION_MODE_ORDER: [u8; 4] = [0, 1, 2, 3];

pub fn p_slice_macroblock_type_from_code(
    code: u8,
) -> Result<PSliceMacroblockType, MacroblockTypeError> {
    match code {
        0 => Ok(PSliceMacroblockType::L0_16x16),
        1 => Ok(PSliceMacroblockType::L0L0_16x8),
        2 => Ok(PSliceMacroblockType::L0L0_8x16),
        3 => Ok(PSliceMacroblockType::P8x8),
        4 => Ok(PSliceMacroblockType::P8x8Ref0),
        5..=30 => Ok(PSliceMacroblockType::Intra(
            i_slice_macroblock_type_from_code(code - 5)?,
        )),
        _ => Err(MacroblockTypeError::InvalidPSliceMacroblockTypeCode(code)),
    }
}

pub fn b_slice_macroblock_type_from_code(
    code: u8,
) -> Result<BSliceMacroblockType, MacroblockTypeError> {
    use BPredictionList::{Bi, L0, L1};
    match code {
        0 => Ok(BSliceMacroblockType::Direct16x16),
        1 => Ok(BSliceMacroblockType::Pred16x16(L0)),
        2 => Ok(BSliceMacroblockType::Pred16x16(L1)),
        3 => Ok(BSliceMacroblockType::Pred16x16(Bi)),
        4 => Ok(BSliceMacroblockType::Pred16x8([L0, L0])),
        5 => Ok(BSliceMacroblockType::Pred8x16([L0, L0])),
        6 => Ok(BSliceMacroblockType::Pred16x8([L1, L1])),
        7 => Ok(BSliceMacroblockType::Pred8x16([L1, L1])),
        8 => Ok(BSliceMacroblockType::Pred16x8([L0, L1])),
        9 => Ok(BSliceMacroblockType::Pred8x16([L0, L1])),
        10 => Ok(BSliceMacroblockType::Pred16x8([L1, L0])),
        11 => Ok(BSliceMacroblockType::Pred8x16([L1, L0])),
        12 => Ok(BSliceMacroblockType::Pred16x8([L0, Bi])),
        13 => Ok(BSliceMacroblockType::Pred8x16([L0, Bi])),
        14 => Ok(BSliceMacroblockType::Pred16x8([L1, Bi])),
        15 => Ok(BSliceMacroblockType::Pred8x16([L1, Bi])),
        16 => Ok(BSliceMacroblockType::Pred16x8([Bi, L0])),
        17 => Ok(BSliceMacroblockType::Pred8x16([Bi, L0])),
        18 => Ok(BSliceMacroblockType::Pred16x8([Bi, L1])),
        19 => Ok(BSliceMacroblockType::Pred8x16([Bi, L1])),
        20 => Ok(BSliceMacroblockType::Pred16x8([Bi, Bi])),
        21 => Ok(BSliceMacroblockType::Pred8x16([Bi, Bi])),
        22 => Ok(BSliceMacroblockType::B8x8),
        23..=48 => Ok(BSliceMacroblockType::Intra(
            i_slice_macroblock_type_from_code(code - 23)?,
        )),
        _ => Err(MacroblockTypeError::InvalidBSliceMacroblockTypeCode(code)),
    }
}

fn intra16x16_prediction_mode(value: u8) -> Intra16x16PredictionMode {
    match value {
        0 => Intra16x16PredictionMode::Vertical,
        1 => Intra16x16PredictionMode::Horizontal,
        2 => Intra16x16PredictionMode::Dc,
        _ => Intra16x16PredictionMode::Plane,
    }
}

fn coded_block_pattern_chroma(value: u8) -> CodedBlockPatternChroma {
    match value {
        0 => CodedBlockPatternChroma::Zero,
        1 => CodedBlockPatternChroma::Dc,
        _ => CodedBlockPatternChroma::DcAndAc,
    }
}

#[derive(Debug)]
pub enum MacroblockTypeError {
    InvalidGrid,
    InvalidISliceMacroblockTypeCode(u8),
    InvalidPSliceMacroblockTypeCode(u8),
    InvalidBSliceMacroblockTypeCode(u8),
    InvalidBSubMacroblockTypeCode(u8),
    MissingSignaledCodedBlockPattern,
}

impl fmt::Display for MacroblockTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGrid => write!(f, "invalid H.264 macroblock grid"),
            Self::InvalidISliceMacroblockTypeCode(code) => {
                write!(f, "invalid I-slice macroblock type code {code}")
            }
            Self::InvalidPSliceMacroblockTypeCode(code) => {
                write!(f, "invalid P-slice macroblock type code {code}")
            }
            Self::InvalidBSliceMacroblockTypeCode(code) => {
                write!(f, "invalid B-slice macroblock type code {code}")
            }
            Self::InvalidBSubMacroblockTypeCode(code) => {
                write!(f, "invalid B-slice sub-macroblock type code {code}")
            }
            Self::MissingSignaledCodedBlockPattern => {
                write!(
                    f,
                    "macroblock type requires separately signaled coded-block pattern"
                )
            }
        }
    }
}

impl std::error::Error for MacroblockTypeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_slice_macroblock_type_maps_standard_table_entries() {
        assert_eq!(
            i_slice_macroblock_type_from_code(0).unwrap(),
            ISliceMacroblockType::IntraNxN
        );
        assert_eq!(
            i_slice_macroblock_type_from_code(1).unwrap(),
            ISliceMacroblockType::Intra16x16 {
                prediction: Intra16x16PredictionMode::Vertical,
                coded_block_pattern_chroma: CodedBlockPatternChroma::Zero,
                coded_block_pattern_luma: 0,
            }
        );
        assert_eq!(
            i_slice_macroblock_type_from_code(24).unwrap(),
            ISliceMacroblockType::Intra16x16 {
                prediction: Intra16x16PredictionMode::Plane,
                coded_block_pattern_chroma: CodedBlockPatternChroma::DcAndAc,
                coded_block_pattern_luma: 15,
            }
        );
        assert_eq!(
            i_slice_macroblock_type_from_code(25).unwrap(),
            ISliceMacroblockType::Pcm
        );
    }

    #[test]
    fn macroblock_grid_maps_raster_addresses() {
        let grid = MacroblockGrid::new(1920, 1088).unwrap();

        assert_eq!(grid.width_in_mbs, 120);
        assert_eq!(grid.height_in_mbs, 68);
        assert_eq!(
            grid.address(121),
            Some(MacroblockAddress {
                address: 121,
                x: 1,
                y: 1,
            })
        );
        assert_eq!(grid.address(grid.macroblock_count()), None);
    }

    #[test]
    fn p_slice_macroblock_type_maps_standard_table_entries() {
        assert_eq!(
            p_slice_macroblock_type_from_code(0).unwrap(),
            PSliceMacroblockType::L0_16x16
        );
        assert_eq!(
            p_slice_macroblock_type_from_code(4).unwrap(),
            PSliceMacroblockType::P8x8Ref0
        );
        assert_eq!(
            p_slice_macroblock_type_from_code(5).unwrap(),
            PSliceMacroblockType::Intra(ISliceMacroblockType::IntraNxN)
        );
        assert_eq!(
            p_slice_macroblock_type_from_code(30).unwrap(),
            PSliceMacroblockType::Intra(ISliceMacroblockType::Pcm)
        );
    }

    #[test]
    fn b_slice_macroblock_type_maps_standard_table_entries() {
        assert_eq!(
            b_slice_macroblock_type_from_code(0).unwrap(),
            BSliceMacroblockType::Direct16x16
        );
        assert_eq!(
            b_slice_macroblock_type_from_code(3).unwrap(),
            BSliceMacroblockType::Pred16x16(BPredictionList::Bi)
        );
        assert_eq!(
            b_slice_macroblock_type_from_code(8).unwrap(),
            BSliceMacroblockType::Pred16x8([BPredictionList::L0, BPredictionList::L1])
        );
        assert_eq!(
            b_slice_macroblock_type_from_code(22).unwrap(),
            BSliceMacroblockType::B8x8
        );
        assert_eq!(
            b_slice_macroblock_type_from_code(23).unwrap(),
            BSliceMacroblockType::Intra(ISliceMacroblockType::IntraNxN)
        );
        assert_eq!(
            b_slice_macroblock_type_from_code(48).unwrap(),
            BSliceMacroblockType::Intra(ISliceMacroblockType::Pcm)
        );
    }

    #[test]
    fn b_sub_macroblock_type_maps_standard_table_entries() {
        assert_eq!(
            b_sub_macroblock_type_from_code(0).unwrap(),
            BSubMacroblockType::Direct8x8
        );
        assert_eq!(
            b_sub_macroblock_type_from_code(3).unwrap(),
            BSubMacroblockType::Pred8x8(BPredictionList::Bi)
        );
        assert_eq!(
            b_sub_macroblock_type_from_code(12).unwrap(),
            BSubMacroblockType::Pred4x4(BPredictionList::Bi)
        );
    }

    #[test]
    fn macroblock_type_reports_residual_presence() {
        let intra16_no_residual = i_slice_macroblock_type_from_code(1).unwrap();
        let intra16_full_residual = i_slice_macroblock_type_from_code(24).unwrap();

        assert_eq!(
            intra16_no_residual.luma_residual_presence(),
            ResidualPresence::Absent
        );
        assert_eq!(
            intra16_no_residual.chroma_dc_presence(),
            ResidualPresence::Absent
        );
        assert_eq!(
            intra16_no_residual.chroma_ac_presence(),
            ResidualPresence::Absent
        );

        assert_eq!(
            intra16_full_residual.luma_residual_presence(),
            ResidualPresence::Present
        );
        assert_eq!(
            intra16_full_residual.chroma_dc_presence(),
            ResidualPresence::Present
        );
        assert_eq!(
            intra16_full_residual.chroma_ac_presence(),
            ResidualPresence::Present
        );
        assert_eq!(
            ISliceMacroblockType::IntraNxN.luma_residual_presence(),
            ResidualPresence::SignaledSeparately
        );
        assert_eq!(
            ISliceMacroblockType::Pcm.chroma_dc_presence(),
            ResidualPresence::PcmRawSamples
        );
    }

    #[test]
    fn resolved_residual_pattern_uses_embedded_or_signaled_coded_block_pattern() {
        let intra16 = i_slice_macroblock_type_from_code(24).unwrap();
        assert_eq!(
            ResolvedIntraResidualPattern::from_macroblock_type(intra16, None, None).unwrap(),
            ResolvedIntraResidualPattern {
                coded_block_pattern_luma: 15,
                coded_block_pattern_chroma: CodedBlockPatternChroma::DcAndAc,
            }
        );

        assert_eq!(
            ResolvedIntraResidualPattern::from_macroblock_type(
                ISliceMacroblockType::IntraNxN,
                Some(3),
                Some(CodedBlockPatternChroma::Dc)
            )
            .unwrap(),
            ResolvedIntraResidualPattern {
                coded_block_pattern_luma: 3,
                coded_block_pattern_chroma: CodedBlockPatternChroma::Dc,
            }
        );

        assert!(matches!(
            ResolvedIntraResidualPattern::from_macroblock_type(
                ISliceMacroblockType::IntraNxN,
                None,
                None
            ),
            Err(MacroblockTypeError::MissingSignaledCodedBlockPattern)
        ));
    }
}
