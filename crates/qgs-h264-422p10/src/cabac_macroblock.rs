use std::fmt;

use crate::cabac::{CabacContext, CabacDecoder, CabacError};
use crate::macroblock_type::{
    b_slice_macroblock_type_from_code, b_sub_macroblock_type_from_code,
    i_slice_macroblock_type_from_code, p_slice_macroblock_type_from_code, BSliceMacroblockType,
    BSubMacroblockType, CodedBlockPatternChroma, ISliceMacroblockType, MacroblockTypeError,
    PSliceMacroblockType, PSubMacroblockType,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacISliceMbTypeContexts {
    pub branch: [CabacContext; 3],
    pub suffix: [CabacContext; 5],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacPSliceMbTypeContexts {
    pub prefix: [CabacContext; 5],
    pub intra: CabacISliceMbTypeContexts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacBSliceMbTypeContexts {
    pub prefix: [CabacContext; 6],
    pub intra: CabacISliceMbTypeContexts,
}

impl CabacBSliceMbTypeContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self {
            prefix: [context; 6],
            intra: CabacISliceMbTypeContexts::flat(context),
        }
    }
}

impl CabacPSliceMbTypeContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self {
            prefix: [context; 5],
            intra: CabacISliceMbTypeContexts::flat(context),
        }
    }
}

impl CabacISliceMbTypeContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self {
            branch: [context; 3],
            suffix: [context; 5],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacCodedBlockPatternContexts {
    pub luma: [CabacContext; 4],
    pub chroma_dc: [CabacContext; 4],
    pub chroma_ac: [CabacContext; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacTransformSize8x8Contexts {
    pub bins: [CabacContext; 3],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacBSubMbTypeContexts {
    pub bins: [CabacContext; 4],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacPSubMbTypeContexts {
    pub bins: [CabacContext; 3],
}

impl CabacPSubMbTypeContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self { bins: [context; 3] }
    }
}

impl CabacBSubMbTypeContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self { bins: [context; 4] }
    }
}

impl CabacCodedBlockPatternContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self {
            luma: [context; 4],
            chroma_dc: [context; 4],
            chroma_ac: [context; 4],
        }
    }
}

impl CabacTransformSize8x8Contexts {
    pub fn flat(context: CabacContext) -> Self {
        Self { bins: [context; 3] }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodedBlockPattern {
    pub luma: u8,
    pub chroma: CodedBlockPatternChroma,
}

impl CodedBlockPattern {
    pub fn luma_block_present(self, index: usize) -> bool {
        index < 4 && ((self.luma >> index) & 1) != 0
    }

    pub fn chroma_dc_present(self) -> bool {
        self.chroma != CodedBlockPatternChroma::Zero
    }

    pub fn chroma_ac_present(self) -> bool {
        self.chroma == CodedBlockPatternChroma::DcAndAc
    }
}

pub fn decode_coded_block_pattern(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacCodedBlockPatternContexts,
) -> Result<CodedBlockPattern, CabacMacroblockError> {
    let mut luma = 0_u8;
    for index in 0..4 {
        if decoder.decode_decision(&mut contexts.luma[index])? {
            luma |= 1 << index;
        }
    }

    let chroma = if !decoder.decode_decision(&mut contexts.chroma_dc[0])? {
        CodedBlockPatternChroma::Zero
    } else if !decoder.decode_decision(&mut contexts.chroma_ac[0])? {
        CodedBlockPatternChroma::Dc
    } else {
        CodedBlockPatternChroma::DcAndAc
    };

    Ok(CodedBlockPattern { luma, chroma })
}

pub fn decode_coded_block_pattern_with_neighbors(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacCodedBlockPatternContexts,
    left_cbp: Option<u8>,
    top_cbp: Option<u8>,
) -> Result<CodedBlockPattern, CabacMacroblockError> {
    let mut luma = 0_u8;
    let left_zero = left_cbp.map(|cbp| (cbp & 0x02) == 0).unwrap_or(false);
    let top_zero = top_cbp.map(|cbp| (cbp & 0x04) == 0).unwrap_or(false);
    let context = usize::from(left_zero) + 2 * usize::from(top_zero);
    if decoder.decode_decision(&mut contexts.luma[context])? {
        luma |= 1;
    }
    let top_zero = top_cbp.map(|cbp| (cbp & 0x08) == 0).unwrap_or(false);
    let context = usize::from(luma & 0x01 == 0) + 2 * usize::from(top_zero);
    if decoder.decode_decision(&mut contexts.luma[context])? {
        luma |= 1 << 1;
    }
    let left_zero = left_cbp.map(|cbp| (cbp & 0x08) == 0).unwrap_or(false);
    let context = usize::from(left_zero) + 2 * usize::from(luma & 0x01 == 0);
    if decoder.decode_decision(&mut contexts.luma[context])? {
        luma |= 1 << 2;
    }
    let context = usize::from(luma & 0x04 == 0) + 2 * usize::from(luma & 0x02 == 0);
    if decoder.decode_decision(&mut contexts.luma[context])? {
        luma |= 1 << 3;
    }

    let left_chroma = left_cbp.map(|cbp| (cbp >> 4) & 0x03);
    let top_chroma = top_cbp.map(|cbp| (cbp >> 4) & 0x03);
    let chroma_dc_context = usize::from(left_chroma.map(|chroma| chroma > 0).unwrap_or(false))
        + 2 * usize::from(top_chroma.map(|chroma| chroma > 0).unwrap_or(false));
    let chroma = if !decoder.decode_decision(&mut contexts.chroma_dc[chroma_dc_context])? {
        CodedBlockPatternChroma::Zero
    } else {
        let chroma_ac_context = usize::from(left_chroma.map(|chroma| chroma == 2).unwrap_or(false))
            + 2 * usize::from(top_chroma.map(|chroma| chroma == 2).unwrap_or(false));
        if decoder.decode_decision(&mut contexts.chroma_ac[chroma_ac_context])? {
            CodedBlockPatternChroma::DcAndAc
        } else {
            CodedBlockPatternChroma::Dc
        }
    };

    Ok(CodedBlockPattern { luma, chroma })
}

pub fn decode_transform_size_8x8_flag(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacTransformSize8x8Contexts,
    left_transform_8x8: bool,
    top_transform_8x8: bool,
) -> Result<bool, CabacMacroblockError> {
    let context_index = usize::from(left_transform_8x8) + usize::from(top_transform_8x8);
    Ok(decoder.decode_decision(&mut contexts.bins[context_index])?)
}

pub fn decode_i_slice_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacISliceMbTypeContexts,
) -> Result<ISliceMacroblockType, CabacMacroblockError> {
    decode_i_slice_macroblock_type_with_context(decoder, contexts, 0)
}

pub fn decode_i_slice_macroblock_type_with_context(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacISliceMbTypeContexts,
    branch_context: usize,
) -> Result<ISliceMacroblockType, CabacMacroblockError> {
    decode_intra_macroblock_type(decoder, contexts, true, branch_context)
}

pub fn decode_inter_slice_intra_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacISliceMbTypeContexts,
) -> Result<ISliceMacroblockType, CabacMacroblockError> {
    decode_intra_macroblock_type(decoder, contexts, false, 0)
}

fn decode_intra_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacISliceMbTypeContexts,
    intra_slice: bool,
    branch_context: usize,
) -> Result<ISliceMacroblockType, CabacMacroblockError> {
    let branch_context = branch_context.min(contexts.branch.len().saturating_sub(1));
    if !decoder.decode_decision(&mut contexts.branch[branch_context])? {
        return Ok(ISliceMacroblockType::IntraNxN);
    }
    if decoder.decode_terminate()? {
        return Ok(ISliceMacroblockType::Pcm);
    }
    let mut code = 1_u8;
    if intra_slice {
        if decoder.decode_decision(&mut contexts.suffix[0])? {
            code = code.saturating_add(12);
        }
        if decoder.decode_decision(&mut contexts.suffix[1])? {
            code = code.saturating_add(4);
            if decoder.decode_decision(&mut contexts.suffix[2])? {
                code = code.saturating_add(4);
            }
        }
        if decoder.decode_decision(&mut contexts.suffix[3])? {
            code = code.saturating_add(2);
        }
        if decoder.decode_decision(&mut contexts.suffix[4])? {
            code = code.saturating_add(1);
        }
        return Ok(i_slice_macroblock_type_from_code(code)?);
    }

    if decoder.decode_decision(&mut contexts.suffix[0])? {
        code = code.saturating_add(12);
    }
    if decoder.decode_decision(&mut contexts.suffix[1])? {
        code = code.saturating_add(4);
        if decoder.decode_decision(&mut contexts.suffix[1])? {
            code = code.saturating_add(4);
        }
    }
    if decoder.decode_decision(&mut contexts.suffix[2])? {
        code = code.saturating_add(2);
    }
    if decoder.decode_decision(&mut contexts.suffix[2])? {
        code = code.saturating_add(1);
    }
    Ok(i_slice_macroblock_type_from_code(code)?)
}

pub fn decode_p_slice_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacPSliceMbTypeContexts,
) -> Result<PSliceMacroblockType, CabacMacroblockError> {
    if !decoder.decode_decision(&mut contexts.prefix[0])? {
        if !decoder.decode_decision(&mut contexts.prefix[1])? {
            let code = 3 * u8::from(decoder.decode_decision(&mut contexts.prefix[2])?);
            return Ok(p_slice_macroblock_type_from_code(code)?);
        }
        let code = 2 - u8::from(decoder.decode_decision(&mut contexts.prefix[3])?);
        return Ok(p_slice_macroblock_type_from_code(code)?);
    }

    // ctxIdx 17 is shared by the P inter branch and the first bin of the
    // embedded I macroblock type.
    contexts.intra.branch[0] = contexts.prefix[3];
    let intra_result = decode_inter_slice_intra_macroblock_type(decoder, &mut contexts.intra);
    contexts.prefix[3] = contexts.intra.branch[0];
    let intra = intra_result?;
    Ok(PSliceMacroblockType::Intra(intra))
}

pub fn decode_b_slice_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacBSliceMbTypeContexts,
) -> Result<BSliceMacroblockType, CabacMacroblockError> {
    decode_b_slice_macroblock_type_with_context(decoder, contexts, 0)
}

pub fn decode_b_slice_macroblock_type_with_context(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacBSliceMbTypeContexts,
    neighbor_context: usize,
) -> Result<BSliceMacroblockType, CabacMacroblockError> {
    if !decode_b_mb_type_bin(decoder, contexts, neighbor_context.min(2))? {
        return Ok(BSliceMacroblockType::Direct16x16);
    }

    if !decode_b_mb_type_bin(decoder, contexts, 3)? {
        let code = 1 + u8::from(decode_b_mb_type_bin(decoder, contexts, 5)?);
        return Ok(b_slice_macroblock_type_from_code(code)?);
    }

    let mut bits = u8::from(decode_b_mb_type_bin(decoder, contexts, 4)?) << 3;
    bits |= u8::from(decode_b_mb_type_bin(decoder, contexts, 5)?) << 2;
    bits |= u8::from(decode_b_mb_type_bin(decoder, contexts, 5)?) << 1;
    bits |= u8::from(decode_b_mb_type_bin(decoder, contexts, 5)?);

    let code = if bits < 8 {
        bits + 3
    } else {
        match bits {
            13 => {
                // ctxIdx 32 is shared by the outer B mb_type prefix and the
                // first bin of the embedded I mb_type. Keep the single
                // normative context state across that syntax boundary.
                contexts.intra.branch[0] = contexts.prefix[5];
                let intra_result =
                    decode_inter_slice_intra_macroblock_type(decoder, &mut contexts.intra);
                contexts.prefix[5] = contexts.intra.branch[0];
                let intra = intra_result?;
                return Ok(BSliceMacroblockType::Intra(intra));
            }
            14 => 11,
            15 => 22,
            _ => {
                bits = (bits << 1) | u8::from(decode_b_mb_type_bin(decoder, contexts, 5)?);
                bits.checked_sub(4)
                    .ok_or(CabacMacroblockError::SymbolOverflow)?
            }
        }
    };
    Ok(b_slice_macroblock_type_from_code(code)?)
}

fn decode_b_mb_type_bin(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacBSliceMbTypeContexts,
    bin_index: usize,
) -> Result<bool, CabacError> {
    let context_index = bin_index.min(contexts.prefix.len().saturating_sub(1));
    decoder.decode_decision(&mut contexts.prefix[context_index])
}

pub fn decode_b_sub_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacBSubMbTypeContexts,
) -> Result<BSubMacroblockType, CabacMacroblockError> {
    if !decoder.decode_decision(&mut contexts.bins[0])? {
        return Ok(BSubMacroblockType::Direct8x8);
    }
    if !decoder.decode_decision(&mut contexts.bins[1])? {
        let code = 1 + u8::from(decoder.decode_decision(&mut contexts.bins[3])?);
        return Ok(b_sub_macroblock_type_from_code(code)?);
    }

    let mut code = 3_u8;
    if decoder.decode_decision(&mut contexts.bins[2])? {
        if decoder.decode_decision(&mut contexts.bins[3])? {
            let code = 11 + u8::from(decoder.decode_decision(&mut contexts.bins[3])?);
            return Ok(b_sub_macroblock_type_from_code(code)?);
        }
        code = code.saturating_add(4);
    }
    code = code.saturating_add(2 * u8::from(decoder.decode_decision(&mut contexts.bins[3])?));
    code = code.saturating_add(u8::from(decoder.decode_decision(&mut contexts.bins[3])?));
    Ok(b_sub_macroblock_type_from_code(code)?)
}

pub fn decode_p_sub_macroblock_type(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacPSubMbTypeContexts,
) -> Result<PSubMacroblockType, CabacMacroblockError> {
    if decoder.decode_decision(&mut contexts.bins[0])? {
        return Ok(PSubMacroblockType::L0_8x8);
    }
    if !decoder.decode_decision(&mut contexts.bins[1])? {
        return Ok(PSubMacroblockType::L0_8x4);
    }
    if decoder.decode_decision(&mut contexts.bins[2])? {
        return Ok(PSubMacroblockType::L0_4x8);
    }
    Ok(PSubMacroblockType::L0_4x4)
}

#[derive(Debug)]
pub enum CabacMacroblockError {
    Cabac(CabacError),
    MacroblockType(MacroblockTypeError),
    SymbolOverflow,
}

impl fmt::Display for CabacMacroblockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cabac(error) => write!(f, "{error}"),
            Self::MacroblockType(error) => write!(f, "{error}"),
            Self::SymbolOverflow => write!(f, "CABAC macroblock symbol overflowed"),
        }
    }
}

impl std::error::Error for CabacMacroblockError {}

impl From<CabacError> for CabacMacroblockError {
    fn from(value: CabacError) -> Self {
        Self::Cabac(value)
    }
}

impl From<MacroblockTypeError> for CabacMacroblockError {
    fn from(value: MacroblockTypeError) -> Self {
        Self::MacroblockType(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coded_block_pattern_can_decode_all_zero_intra_pattern() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacCodedBlockPatternContexts::flat(CabacContext::new(0, false));

        let pattern = decode_coded_block_pattern(&mut decoder, &mut contexts).unwrap();

        assert_eq!(
            pattern,
            CodedBlockPattern {
                luma: 0,
                chroma: CodedBlockPatternChroma::Zero,
            }
        );
        assert!(!pattern.luma_block_present(0));
        assert!(!pattern.chroma_dc_present());
    }

    #[test]
    fn i_slice_macroblock_type_can_decode_intra_nxn() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacISliceMbTypeContexts::flat(CabacContext::new(0, false));

        let mb_type = decode_i_slice_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(mb_type, ISliceMacroblockType::IntraNxN);
    }

    #[test]
    fn i_slice_macroblock_type_can_decode_full_intra16x16_code() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0, 0, 0]).unwrap();
        let mut contexts = CabacISliceMbTypeContexts::flat(CabacContext::new(0, true));

        let mb_type = decode_i_slice_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(
            mb_type,
            ISliceMacroblockType::Intra16x16 {
                prediction: crate::macroblock_type::Intra16x16PredictionMode::Plane,
                coded_block_pattern_chroma: CodedBlockPatternChroma::DcAndAc,
                coded_block_pattern_luma: 15,
            }
        );
    }

    #[test]
    fn p_slice_macroblock_type_can_decode_l0_16x16() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacPSliceMbTypeContexts::flat(CabacContext::new(0, false));

        let mb_type = decode_p_slice_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(mb_type, PSliceMacroblockType::L0_16x16);
    }

    #[test]
    fn p_slice_macroblock_type_can_decode_intra_suffix() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0, 0, 0]).unwrap();
        let mut contexts = CabacPSliceMbTypeContexts::flat(CabacContext::new(0, false));
        contexts.prefix[0] = CabacContext::new(0, true);

        let mb_type = decode_p_slice_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(
            mb_type,
            PSliceMacroblockType::Intra(ISliceMacroblockType::IntraNxN)
        );
    }

    #[test]
    fn b_slice_macroblock_type_can_decode_direct() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacBSliceMbTypeContexts::flat(CabacContext::new(0, false));

        let mb_type = decode_b_slice_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(mb_type, BSliceMacroblockType::Direct16x16);
    }

    #[test]
    fn b_sub_macroblock_type_can_decode_direct() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacBSubMbTypeContexts::flat(CabacContext::new(0, false));

        let sub_type = decode_b_sub_macroblock_type(&mut decoder, &mut contexts).unwrap();

        assert_eq!(sub_type, BSubMacroblockType::Direct8x8);
    }

    #[test]
    fn p_sub_macroblock_type_decodes_all_partition_shapes() {
        let decode = |mps: [bool; 3]| {
            let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
            let mut contexts = CabacPSubMbTypeContexts {
                bins: mps.map(|value| CabacContext::new(0, value)),
            };
            decode_p_sub_macroblock_type(&mut decoder, &mut contexts).unwrap()
        };

        assert_eq!(decode([true, false, false]), PSubMacroblockType::L0_8x8);
        assert_eq!(decode([false, false, false]), PSubMacroblockType::L0_8x4);
        assert_eq!(decode([false, true, true]), PSubMacroblockType::L0_4x8);
        assert_eq!(decode([false, true, false]), PSubMacroblockType::L0_4x4);
    }

    #[test]
    fn coded_block_pattern_can_decode_full_intra_pattern() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0, 0]).unwrap();
        let mut contexts = CabacCodedBlockPatternContexts::flat(CabacContext::new(0, true));

        let pattern = decode_coded_block_pattern(&mut decoder, &mut contexts).unwrap();

        assert_eq!(
            pattern,
            CodedBlockPattern {
                luma: 15,
                chroma: CodedBlockPatternChroma::DcAndAc,
            }
        );
        assert!(pattern.luma_block_present(3));
        assert!(pattern.chroma_dc_present());
        assert!(pattern.chroma_ac_present());
    }

    #[test]
    fn transform_size_8x8_uses_neighbor_sum_context() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacTransformSize8x8Contexts {
            bins: [
                CabacContext::new(0, true),
                CabacContext::new(0, true),
                CabacContext::new(0, false),
            ],
        };

        let flag = decode_transform_size_8x8_flag(&mut decoder, &mut contexts, true, true).unwrap();

        assert!(!flag);
    }
}
