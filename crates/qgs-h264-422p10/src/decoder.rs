use std::collections::BTreeMap;
use std::fmt;

use qgs_codec_h264::{
    H264DecoderState, H264PictureId, H264SliceKind, ParsedH264AccessUnit, ParsedH264Slice,
};

use crate::cabac::{CabacContext, CabacDecoder, CabacInitValue};
use crate::cabac_macroblock::{
    decode_b_slice_macroblock_type, decode_b_sub_macroblock_type, decode_coded_block_pattern,
    decode_coded_block_pattern_with_neighbors, decode_i_slice_macroblock_type_with_context,
    decode_p_slice_macroblock_type, decode_transform_size_8x8_flag, CabacBSliceMbTypeContexts,
    CabacBSubMbTypeContexts, CabacCodedBlockPatternContexts, CabacISliceMbTypeContexts,
    CabacMacroblockError, CabacPSliceMbTypeContexts, CabacTransformSize8x8Contexts,
    CodedBlockPattern,
};
use crate::cabac_motion::{
    decode_motion_vector_difference_with_contexts, CabacMotionError, CabacMotionVectorContexts,
};
use crate::cabac_residual::{
    decode_residual_4x4, decode_residual_8x8, CabacResidual4x4Contexts, CabacResidual8x8Contexts,
    CabacResidualError,
};
use crate::cabac_residual_422::CabacResidualDecoder422;
use crate::frame::{DecodedFrame422P10, Plane422P10};
use crate::macroblock::{
    add_chroma_residual_422, add_luma_residual_16x16, add_luma_residual_8x8,
    reconstruct_chroma_422_intra, reconstruct_intra16x16_luma_dc, reconstruct_intra4x4_luma,
    reconstruct_intra8x8_luma, ChromaPlane, Intra16x16Macroblock, Intra4x4PredictionMode,
    MacroblockReconstructionError,
};
use crate::macroblock_type::{
    BPredictionList, BSliceMacroblockType, BSubMacroblockType, CodedBlockPatternChroma,
    ISliceMacroblockType, MacroblockAddress, MacroblockGrid, PSliceMacroblockType,
};
use crate::motion::{
    predict_bi_inter_16x16, predict_bi_inter_16x8, predict_bi_inter_8x16, predict_bi_inter_region,
    predict_inter_16x16, predict_inter_16x8, predict_inter_8x16, predict_inter_region,
    MotionCompensationError, MotionField, MotionVectorQuarterPel,
};
use crate::residual::{ResidualBlock4x4, ResidualBlock8x8};
use crate::slice::{
    slice_luma_qp, slice_payload_from_parsed, SliceMacroblockCursor, SlicePayloadError,
};
use crate::{
    validate_original_profile, H264422P10Error, H264422P10Frame, H264422P10Plane, H264422P10Profile,
};

#[derive(Debug)]
pub struct H264422P10Decoder {
    state: H264DecoderState,
    expected: H264422P10Profile,
    next_presentation_index: u64,
    decoded: BTreeMap<(u16, i32), DecodedPicture>,
}

#[derive(Clone, Debug)]
struct DecodedPicture {
    presentation_index: u64,
    frame: DecodedFrame422P10,
}

#[derive(Clone, Debug)]
struct CabacIntra4x4PredictionModeContexts {
    prev_intra4x4_pred_mode_flag: CabacContext,
    rem_intra4x4_pred_mode: CabacContext,
}

#[derive(Clone, Debug)]
struct CabacIntraChromaPredModeContexts {
    first: [CabacContext; 3],
    suffix: CabacContext,
}

#[derive(Clone, Debug)]
struct CabacMbQpDeltaContexts {
    bins: [CabacContext; 4],
}

impl Default for H264422P10Decoder {
    fn default() -> Self {
        Self::new(H264422P10Profile::default())
    }
}

impl H264422P10Decoder {
    pub fn new(expected: H264422P10Profile) -> Self {
        Self {
            state: H264DecoderState::new(),
            expected,
            next_presentation_index: 0,
            decoded: BTreeMap::new(),
        }
    }

    pub fn decode_access_unit(
        &mut self,
        access_unit: &[u8],
    ) -> Result<H264422P10Frame, H264422P10PictureDecodeError> {
        let mut outputs = self.submit_access_unit(access_unit)?;
        if outputs.len() != 1 {
            return Err(H264422P10PictureDecodeError::OutputNotReady {
                output_count: outputs.len(),
            });
        }
        Ok(outputs.remove(0))
    }

    pub fn submit_access_unit(
        &mut self,
        access_unit: &[u8],
    ) -> Result<Vec<H264422P10Frame>, H264422P10PictureDecodeError> {
        let parsed = self.state.parse_access_unit(access_unit)?;
        validate_original_profile(&parsed, &self.expected)?;
        let mut frame = DecodedFrame422P10::new(
            parsed.desc.coded_width as usize,
            parsed.desc.coded_height as usize,
            parsed.desc.visible_region.width as usize,
            parsed.desc.visible_region.height as usize,
        );

        decode_picture_into_frame(&parsed, &mut frame, &self.decoded)?;
        let presentation_index = self.next_presentation_index;
        self.next_presentation_index = self.next_presentation_index.saturating_add(1);
        self.decoded.insert(
            picture_key(&parsed.picture.id()),
            DecodedPicture {
                presentation_index,
                frame,
            },
        );
        let update = self.state.finish_picture(&parsed)?;
        let mut outputs = Vec::new();
        for id in update.output_ready {
            let key = picture_key(&id);
            let picture = self.decoded.get(&key).ok_or(
                H264422P10PictureDecodeError::MissingDecodedPicture {
                    frame_num: id.frame_num,
                    poc: id.poc,
                },
            )?;
            outputs.push(frame_to_output(picture.presentation_index, &picture.frame)?);
        }
        for id in update.released {
            self.decoded.remove(&picture_key(&id));
        }
        Ok(outputs)
    }

    pub fn flush(&mut self) -> Result<Vec<H264422P10Frame>, H264422P10PictureDecodeError> {
        let update = self.state.flush();
        let mut outputs = Vec::new();
        for id in update.output_ready {
            let key = picture_key(&id);
            let picture = self.decoded.remove(&key).ok_or(
                H264422P10PictureDecodeError::MissingDecodedPicture {
                    frame_num: id.frame_num,
                    poc: id.poc,
                },
            )?;
            outputs.push(frame_to_output(picture.presentation_index, &picture.frame)?);
        }
        for id in update.released {
            self.decoded.remove(&picture_key(&id));
        }
        Ok(outputs)
    }
}

fn picture_key(id: &H264PictureId) -> (u16, i32) {
    (id.frame_num, id.poc)
}

fn parsed_grid(
    parsed: &ParsedH264AccessUnit,
) -> Result<MacroblockGrid, H264422P10PictureDecodeError> {
    MacroblockGrid::new(parsed.desc.coded_width, parsed.desc.coded_height)
        .map_err(|_| H264422P10PictureDecodeError::Unsupported("invalid macroblock grid"))
}

fn decode_picture_into_frame(
    parsed: &ParsedH264AccessUnit,
    frame: &mut DecodedFrame422P10,
    decoded: &BTreeMap<(u16, i32), DecodedPicture>,
) -> Result<(), H264422P10PictureDecodeError> {
    if parsed.slices.is_empty() {
        return Err(H264422P10PictureDecodeError::Unsupported(
            "picture has no slices",
        ));
    }
    if !parsed.picture.entropy_coding_mode_flag {
        return Err(H264422P10PictureDecodeError::Unsupported(
            "CAVLC slice decoding is not implemented",
        ));
    }

    let grid = parsed_grid(parsed)?;
    let mut motion_field = MotionField::new(grid)?;
    let mut mvd_syntax = MvdSyntaxField::new(grid)?;
    let mut intra16_or_pcm = vec![None; grid.macroblock_count() as usize];
    let mut chroma_pred_modes = vec![None; grid.macroblock_count() as usize];
    let mut intra_luma_prediction_modes = vec![None; grid.macroblock_count() as usize];
    let mut coded_block_patterns = vec![0_u8; grid.macroblock_count() as usize];
    let mut transform_size_8x8_flags = vec![false; grid.macroblock_count() as usize];
    let mut skip_flags = vec![false; grid.macroblock_count() as usize];
    let mut reconstructed_macroblocks = 0_u32;
    let mut slice_reports = Vec::new();
    for (slice_index, slice) in parsed.slices.iter().enumerate() {
        let expected_slice_macroblocks =
            expected_slice_macroblock_count(parsed, slice_index, grid.macroblock_count())?;
        let progress = match slice.kind {
            H264SliceKind::I => decode_i_slice_into_frame(
                parsed,
                slice,
                expected_slice_macroblocks,
                frame,
                &mut motion_field,
                &mut intra16_or_pcm,
                &mut chroma_pred_modes,
                &mut intra_luma_prediction_modes,
                &mut coded_block_patterns,
                &mut transform_size_8x8_flags,
            )?,
            H264SliceKind::P => decode_p_slice_into_frame(
                parsed,
                slice,
                expected_slice_macroblocks,
                frame,
                decoded,
                &mut motion_field,
                &mut mvd_syntax,
                &mut skip_flags,
                &mut coded_block_patterns,
            )?,
            H264SliceKind::B => decode_b_slice_into_frame(
                parsed,
                slice,
                expected_slice_macroblocks,
                frame,
                decoded,
                &mut motion_field,
                &mut mvd_syntax,
                &mut skip_flags,
                &mut coded_block_patterns,
            )?,
        };
        reconstructed_macroblocks =
            reconstructed_macroblocks.saturating_add(progress.reconstructed_macroblocks);
        slice_reports.push(H264422P10SliceDecodeReport {
            first_macroblock: u32::from(slice.first_mb_in_slice),
            reconstructed_macroblocks: progress.reconstructed_macroblocks,
            expected_macroblocks: expected_slice_macroblocks,
            stop_reason: progress.stop_reason,
            last_macroblock: progress.last_macroblock,
            last_macroblock_type: progress.last_macroblock_type,
            last_coded_block_pattern: progress.last_coded_block_pattern,
            cabac_bit_position: progress.cabac_bit_position,
            cabac_payload_bits: progress.cabac_payload_bits,
        });
    }

    let expected_macroblocks =
        (parsed.desc.coded_width / 16).saturating_mul(parsed.desc.coded_height / 16);
    if reconstructed_macroblocks != expected_macroblocks {
        return Err(H264422P10PictureDecodeError::IncompletePicture {
            reconstructed_macroblocks,
            expected_macroblocks,
            slice_reports,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SliceDecodeProgress {
    reconstructed_macroblocks: u32,
    stop_reason: &'static str,
    last_macroblock: Option<u32>,
    last_macroblock_type: &'static str,
    last_coded_block_pattern: Option<u8>,
    cabac_bit_position: Option<usize>,
    cabac_payload_bits: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MotionList {
    L0,
    L1,
}

const ALL_16X16_BLOCKS: [usize; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
const P_16X8_BLOCKS: [&[usize]; 2] = [&[0, 1, 2, 3, 4, 5, 6, 7], &[8, 9, 10, 11, 12, 13, 14, 15]];
const P_8X16_BLOCKS: [&[usize]; 2] = [&[0, 1, 2, 3, 8, 9, 10, 11], &[4, 5, 6, 7, 12, 13, 14, 15]];
const B_8X8_BLOCKS: [&[usize]; 4] = [
    &[0, 1, 2, 3],
    &[4, 5, 6, 7],
    &[8, 9, 10, 11],
    &[12, 13, 14, 15],
];

#[derive(Clone, Debug)]
struct MvdSyntaxField {
    width_in_mbs: u32,
    height_in_mbs: u32,
    l0: Vec<[Option<MotionVectorQuarterPel>; 16]>,
    l1: Vec<[Option<MotionVectorQuarterPel>; 16]>,
}

impl MvdSyntaxField {
    fn new(grid: MacroblockGrid) -> Result<Self, H264422P10PictureDecodeError> {
        let count = usize::try_from(grid.macroblock_count()).map_err(|_| {
            H264422P10PictureDecodeError::Unsupported("macroblock grid is too large")
        })?;
        Ok(Self {
            width_in_mbs: grid.width_in_mbs,
            height_in_mbs: grid.height_in_mbs,
            l0: vec![[None; 16]; count],
            l1: vec![[None; 16]; count],
        })
    }

    fn context_indices(
        &self,
        address: MacroblockAddress,
        block_index: usize,
        list: MotionList,
    ) -> (usize, usize) {
        let (left, top) = (
            self.neighbor_mvd(address, block_index, list, -1, 0),
            self.neighbor_mvd(address, block_index, list, 0, -1),
        );
        (
            mvd_context_index(left.map(|mvd| mvd.x), top.map(|mvd| mvd.x)),
            mvd_context_index(left.map(|mvd| mvd.y), top.map(|mvd| mvd.y)),
        )
    }

    fn fill(
        &mut self,
        address: MacroblockAddress,
        blocks: &[usize],
        list: MotionList,
        mvd: MotionVectorQuarterPel,
    ) {
        let target = match list {
            MotionList::L0 => &mut self.l0,
            MotionList::L1 => &mut self.l1,
        };
        let Some(macroblock) = target.get_mut(address.address as usize) else {
            return;
        };
        for &block in blocks {
            if let Some(slot) = macroblock.get_mut(block) {
                *slot = Some(mvd);
            }
        }
    }

    fn mark_zero(&mut self, address: MacroblockAddress, blocks: &[usize], list: MotionList) {
        self.fill(address, blocks, list, MotionVectorQuarterPel::ZERO);
    }

    fn mark_intra(&mut self, address: MacroblockAddress) {
        if let Some(macroblock) = self.l0.get_mut(address.address as usize) {
            *macroblock = [None; 16];
        }
        if let Some(macroblock) = self.l1.get_mut(address.address as usize) {
            *macroblock = [None; 16];
        }
    }

    fn neighbor_mvd(
        &self,
        address: MacroblockAddress,
        block_index: usize,
        list: MotionList,
        dx: i32,
        dy: i32,
    ) -> Option<MotionVectorQuarterPel> {
        let (block_x, block_y) = luma4x4_position(block_index);
        let block_x = i32::try_from(block_x).ok()?;
        let block_y = i32::try_from(block_y).ok()?;
        let mut macroblock_x = i32::try_from(address.x).ok()?;
        let mut macroblock_y = i32::try_from(address.y).ok()?;
        let mut neighbor_block_x = block_x + dx;
        let mut neighbor_block_y = block_y + dy;

        if neighbor_block_x < 0 {
            macroblock_x -= 1;
            neighbor_block_x = 3;
        } else if neighbor_block_x >= 4 {
            macroblock_x += 1;
            neighbor_block_x = 0;
        }
        if neighbor_block_y < 0 {
            macroblock_y -= 1;
            neighbor_block_y = 3;
        } else if neighbor_block_y >= 4 {
            macroblock_y += 1;
            neighbor_block_y = 0;
        }
        if macroblock_x < 0
            || macroblock_y < 0
            || macroblock_x >= i32::try_from(self.width_in_mbs).ok()?
            || macroblock_y >= i32::try_from(self.height_in_mbs).ok()?
        {
            return None;
        }
        let macroblock_index = usize::try_from(
            u32::try_from(macroblock_y)
                .ok()?
                .saturating_mul(self.width_in_mbs)
                .saturating_add(u32::try_from(macroblock_x).ok()?),
        )
        .ok()?;
        let neighbor_block_index = luma4x4_index(
            usize::try_from(neighbor_block_x).ok()?,
            usize::try_from(neighbor_block_y).ok()?,
        );
        let source = match list {
            MotionList::L0 => &self.l0,
            MotionList::L1 => &self.l1,
        };
        source
            .get(macroblock_index)
            .and_then(|macroblock| macroblock.get(neighbor_block_index))
            .copied()
            .flatten()
    }
}

fn mvd_context_index(left: Option<i32>, top: Option<i32>) -> usize {
    let sum = left.unwrap_or(0).unsigned_abs() + top.unwrap_or(0).unsigned_abs();
    if sum > 32 {
        2
    } else if sum > 2 {
        1
    } else {
        0
    }
}

fn b_sub_partition_blocks(
    subblock_index: usize,
    shape: BSubPartitionShape,
    sub_partition_index: usize,
) -> &'static [usize] {
    match shape {
        BSubPartitionShape::Full8x8 => B_8X8_BLOCKS[subblock_index],
        BSubPartitionShape::Horizontal8x4 => match (subblock_index, sub_partition_index) {
            (0, 0) => &[0, 1],
            (0, 1) => &[2, 3],
            (1, 0) => &[4, 5],
            (1, 1) => &[6, 7],
            (2, 0) => &[8, 9],
            (2, 1) => &[10, 11],
            (3, 0) => &[12, 13],
            (3, 1) => &[14, 15],
            _ => &[],
        },
        BSubPartitionShape::Vertical4x8 => match (subblock_index, sub_partition_index) {
            (0, 0) => &[0, 2],
            (0, 1) => &[1, 3],
            (1, 0) => &[4, 6],
            (1, 1) => &[5, 7],
            (2, 0) => &[8, 10],
            (2, 1) => &[9, 11],
            (3, 0) => &[12, 14],
            (3, 1) => &[13, 15],
            _ => &[],
        },
        BSubPartitionShape::Square4x4 => match (subblock_index, sub_partition_index) {
            (0, 0) => &[0],
            (0, 1) => &[1],
            (0, 2) => &[2],
            (0, 3) => &[3],
            (1, 0) => &[4],
            (1, 1) => &[5],
            (1, 2) => &[6],
            (1, 3) => &[7],
            (2, 0) => &[8],
            (2, 1) => &[9],
            (2, 2) => &[10],
            (2, 3) => &[11],
            (3, 0) => &[12],
            (3, 1) => &[13],
            (3, 2) => &[14],
            (3, 3) => &[15],
            _ => &[],
        },
    }
}

fn decode_partition_mvd(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacMotionVectorContexts,
    mvd_syntax: &mut MvdSyntaxField,
    address: MacroblockAddress,
    blocks: &[usize],
    list: MotionList,
) -> Result<MotionVectorQuarterPel, H264422P10PictureDecodeError> {
    let block_index = blocks.first().copied().unwrap_or(0);
    let (ctx_inc_x, ctx_inc_y) = mvd_syntax.context_indices(address, block_index, list);
    let mvd = decode_motion_vector_difference_with_contexts(cabac, contexts, ctx_inc_x, ctx_inc_y)?;
    mvd_syntax.fill(address, blocks, list, mvd);
    Ok(mvd)
}

fn expected_slice_macroblock_count(
    parsed: &ParsedH264AccessUnit,
    slice_index: usize,
    picture_macroblocks: u32,
) -> Result<u32, H264422P10PictureDecodeError> {
    let Some(slice) = parsed.slices.get(slice_index) else {
        return Err(H264422P10PictureDecodeError::Unsupported(
            "slice index out of parsed access unit range",
        ));
    };
    let start = u32::from(slice.first_mb_in_slice);
    let end = parsed
        .slices
        .get(slice_index.saturating_add(1))
        .map(|next| u32::from(next.first_mb_in_slice))
        .unwrap_or(picture_macroblocks);
    if end < start || end > picture_macroblocks {
        return Err(H264422P10PictureDecodeError::Unsupported(
            "non-monotonic slice first_mb layout is not supported by this decoder brick",
        ));
    }
    Ok(end - start)
}

fn inter_skip_context_index(
    address: MacroblockAddress,
    width_in_mbs: u32,
    slice_first_mb: u32,
    skip_flags: &[bool],
) -> usize {
    let left = if address.x > 0 {
        let left_address = address.address.saturating_sub(1);
        (left_address >= slice_first_mb)
            .then(|| skip_flags.get(left_address as usize).copied())
            .flatten()
    } else {
        None
    };
    let top = if address.y > 0 {
        let top_address = address.address.saturating_sub(width_in_mbs);
        (top_address >= slice_first_mb)
            .then(|| skip_flags.get(top_address as usize).copied())
            .flatten()
    } else {
        None
    };
    usize::from(matches!(left, Some(false))) + usize::from(matches!(top, Some(false)))
}

fn decode_p_slice_into_frame(
    parsed: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
    expected_slice_macroblocks: u32,
    frame: &mut DecodedFrame422P10,
    decoded: &BTreeMap<(u16, i32), DecodedPicture>,
    motion_field: &mut MotionField,
    mvd_syntax: &mut MvdSyntaxField,
    skip_flags: &mut [bool],
    coded_block_patterns: &mut [u8],
) -> Result<SliceDecodeProgress, H264422P10PictureDecodeError> {
    let ref_id = slice
        .ref_pic_list0
        .first()
        .ok_or(H264422P10PictureDecodeError::Unsupported(
            "P slice has no L0 reference picture",
        ))?;
    let reference = decoded.get(&picture_key(ref_id)).ok_or(
        H264422P10PictureDecodeError::MissingDecodedPicture {
            frame_num: ref_id.frame_num,
            poc: ref_id.poc,
        },
    )?;
    let payload = slice_payload_from_parsed(parsed, slice)?;
    let payload = payload.cabac_payload_bytes()?;
    let cabac_payload_bits = payload.len().saturating_mul(8);
    let mut cabac = CabacDecoder::new(payload)?;
    let qp_y = slice_luma_qp(parsed, slice).0.clamp(0, 51);
    let mut skip_contexts: [CabacContext; 3] =
        std::array::from_fn(|index| pb_cabac_context(slice.cabac_init_idc, 11 + index, qp_y));
    let mut mb_type_contexts =
        CabacPSliceMbTypeContexts::flat(CabacInitValue::new(7, 34).initialize(qp_y));
    let mut intra4x4_prediction_contexts = i_slice_intra4x4_prediction_contexts(qp_y);
    let mut chroma_pred_mode_contexts = i_slice_chroma_pred_mode_contexts(qp_y);
    let mut mb_qp_delta_contexts = i_slice_mb_qp_delta_contexts(qp_y);
    let mut last_qscale_diff_nonzero = false;
    let mut mvd_contexts = pb_mvd_contexts(slice.cabac_init_idc, qp_y);
    let mut cbp_contexts =
        CabacCodedBlockPatternContexts::flat(CabacInitValue::new(9, 43).initialize(qp_y));
    let mut p_luma_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut p_luma8x8_residual_contexts = CabacResidual8x8Contexts::i_slice(qp_y as u8);
    let mut p_cb_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut p_cr_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut transform_size_8x8_contexts = i_slice_transform_size_8x8_contexts(qp_y);
    let grid = parsed_grid(parsed)?;
    let mut transform_size_8x8_flags = vec![false; grid.macroblock_count() as usize];
    let mut current_qp_y = qp_y;
    let mut cursor = SliceMacroblockCursor::new(parsed, slice)?;
    let mut count = 0_u32;
    let mut stop_reason = "slice-cursor-ended";
    let mut last_macroblock = None;
    let mut last_macroblock_type = "none";
    let mut last_coded_block_pattern = None;
    let mut cabac_bit_position = None;

    while count < expected_slice_macroblocks {
        let Some(address) = cursor.next_macroblock() else {
            break;
        };
        let predicted_motion = motion_field.predict_l0_16x16(address, 0)?;
        let skip_context_index = inter_skip_context_index(
            address,
            grid.width_in_mbs,
            u32::from(slice.first_mb_in_slice),
            skip_flags,
        );
        let skipped = cabac.decode_decision(&mut skip_contexts[skip_context_index])?;
        let (prediction, luma_residuals, chroma_residuals) = if skipped {
            last_macroblock_type = "P_Skip";
            last_coded_block_pattern = Some(0);
            (
                InterPrediction::Full16x16(predicted_motion),
                InterLumaResidual::Absent,
                None,
            )
        } else {
            let mb_type = decode_p_slice_macroblock_type(&mut cabac, &mut mb_type_contexts)?;
            last_macroblock_type = p_macroblock_type_name(mb_type);
            match mb_type {
                PSliceMacroblockType::L0_16x16 => {
                    let mvd = decode_partition_mvd(
                        &mut cabac,
                        &mut mvd_contexts,
                        mvd_syntax,
                        address,
                        &ALL_16X16_BLOCKS,
                        MotionList::L0,
                    )?;
                    let motion = predicted_motion.checked_add(mvd)?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut p_luma_residual_contexts,
                        &mut p_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut p_cb_residual_contexts,
                        &mut p_cr_residual_contexts,
                    )?;
                    (
                        InterPrediction::Full16x16(motion),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
                PSliceMacroblockType::L0L0_16x8 => {
                    let first = predicted_motion.checked_add(decode_partition_mvd(
                        &mut cabac,
                        &mut mvd_contexts,
                        mvd_syntax,
                        address,
                        P_16X8_BLOCKS[0],
                        MotionList::L0,
                    )?)?;
                    let second = first.checked_add(decode_partition_mvd(
                        &mut cabac,
                        &mut mvd_contexts,
                        mvd_syntax,
                        address,
                        P_16X8_BLOCKS[1],
                        MotionList::L0,
                    )?)?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut p_luma_residual_contexts,
                        &mut p_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut p_cb_residual_contexts,
                        &mut p_cr_residual_contexts,
                    )?;
                    (
                        InterPrediction::Horizontal16x8([first, second]),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
                PSliceMacroblockType::L0L0_8x16 => {
                    let first = predicted_motion.checked_add(decode_partition_mvd(
                        &mut cabac,
                        &mut mvd_contexts,
                        mvd_syntax,
                        address,
                        P_8X16_BLOCKS[0],
                        MotionList::L0,
                    )?)?;
                    let second = first.checked_add(decode_partition_mvd(
                        &mut cabac,
                        &mut mvd_contexts,
                        mvd_syntax,
                        address,
                        P_8X16_BLOCKS[1],
                        MotionList::L0,
                    )?)?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut p_luma_residual_contexts,
                        &mut p_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut p_cb_residual_contexts,
                        &mut p_cr_residual_contexts,
                    )?;
                    (
                        InterPrediction::Vertical8x16([first, second]),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
                PSliceMacroblockType::Intra(intra) => {
                    let chroma_context = if !matches!(intra, ISliceMacroblockType::Pcm) {
                        Some(0)
                    } else {
                        None
                    };
                    let predecoded_chroma_pred_mode = if !matches!(
                        intra,
                        ISliceMacroblockType::Pcm | ISliceMacroblockType::IntraNxN
                    ) {
                        Some(decode_intra_chroma_pred_mode(
                            &mut cabac,
                            &mut chroma_pred_mode_contexts,
                            chroma_context.unwrap_or(0),
                        )?)
                    } else {
                        None
                    };
                    let mut intra_luma_prediction_modes = [];
                    reconstruct_supported_i_macroblock(
                        frame,
                        address,
                        intra,
                        qp_y as u8,
                        &mut cabac,
                        &mut intra4x4_prediction_contexts,
                        &mut chroma_pred_mode_contexts,
                        chroma_context,
                        predecoded_chroma_pred_mode,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        &mut cbp_contexts,
                        &mut transform_size_8x8_contexts,
                        parsed.picture.transform_8x8_mode_flag,
                        grid.width_in_mbs,
                        0,
                        &mut intra_luma_prediction_modes,
                        &mut transform_size_8x8_flags,
                        &[],
                        None,
                        &mut p_luma_residual_contexts,
                        &mut p_luma8x8_residual_contexts,
                        &mut p_cb_residual_contexts,
                        &mut p_cr_residual_contexts,
                    )?;
                    motion_field.set_intra(address)?;
                    mvd_syntax.mark_intra(address);
                    if let Some(slot) = skip_flags.get_mut(address.address as usize) {
                        *slot = false;
                    }
                    if let Some(slot) = coded_block_patterns.get_mut(address.address as usize) {
                        *slot = 0;
                    }
                    last_macroblock = Some(address.address);
                    last_coded_block_pattern = Some(0);
                    count = count.saturating_add(1);
                    if cabac.decode_terminate()? {
                        cabac_bit_position = Some(cabac.bit_position());
                        stop_reason = if count == expected_slice_macroblocks {
                            "cabac-terminate-at-expected-slice-end"
                        } else {
                            "cabac-terminate-before-expected-slice-end"
                        };
                        break;
                    }
                    continue;
                }
                _ => {
                    return Err(H264422P10PictureDecodeError::Unsupported(
                        "P_8x8 sub-partition macroblock prediction is not implemented yet",
                    ));
                }
            }
        };
        prediction.copy_from_reference(&reference.frame, frame, address)?;
        match luma_residuals {
            InterLumaResidual::Absent => {}
            InterLumaResidual::Blocks4x4(residuals) => {
                if !add_luma_residual_16x16(
                    frame,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp_from_qpy(current_qp_y),
                    &residuals,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            }
            InterLumaResidual::Blocks8x8(residuals) => {
                if !add_luma_residual_8x8(
                    frame,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp_from_qpy(current_qp_y),
                    &residuals,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            }
        }
        if let Some((cb, cr)) = chroma_residuals {
            if !add_chroma_residual_422(
                frame,
                ChromaPlane::Cb,
                address.x as usize,
                address.y as usize,
                reconstruction_qp_from_qpy(current_qp_y),
                &cb,
            ) || !add_chroma_residual_422(
                frame,
                ChromaPlane::Cr,
                address.x as usize,
                address.y as usize,
                reconstruction_qp_from_qpy(current_qp_y),
                &cr,
            ) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
        }
        motion_field.set_inter(address, prediction.representative_motion(), 0)?;
        if skipped {
            mvd_syntax.mark_zero(address, &ALL_16X16_BLOCKS, MotionList::L0);
        }
        if let Some(slot) = skip_flags.get_mut(address.address as usize) {
            *slot = skipped;
        }
        if let Some(slot) = coded_block_patterns.get_mut(address.address as usize) {
            *slot = last_coded_block_pattern.unwrap_or(0);
        }
        last_macroblock = Some(address.address);
        count = count.saturating_add(1);

        if cabac.decode_terminate()? {
            cabac_bit_position = Some(cabac.bit_position());
            stop_reason = if count == expected_slice_macroblocks {
                "cabac-terminate-at-expected-slice-end"
            } else {
                "cabac-terminate-before-expected-slice-end"
            };
            break;
        }
    }
    if count == expected_slice_macroblocks && stop_reason == "slice-cursor-ended" {
        stop_reason = "expected-slice-end-without-terminate";
    }
    Ok(SliceDecodeProgress {
        reconstructed_macroblocks: count,
        stop_reason,
        last_macroblock,
        last_macroblock_type,
        last_coded_block_pattern,
        cabac_bit_position,
        cabac_payload_bits: Some(cabac_payload_bits),
    })
}

fn decode_b_slice_into_frame(
    parsed: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
    expected_slice_macroblocks: u32,
    frame: &mut DecodedFrame422P10,
    decoded: &BTreeMap<(u16, i32), DecodedPicture>,
    motion_field: &mut MotionField,
    mvd_syntax: &mut MvdSyntaxField,
    skip_flags: &mut [bool],
    coded_block_patterns: &mut [u8],
) -> Result<SliceDecodeProgress, H264422P10PictureDecodeError> {
    let list0_id = slice
        .ref_pic_list0
        .first()
        .ok_or(H264422P10PictureDecodeError::Unsupported(
            "B slice has no L0 reference picture",
        ))?;
    let list1_id = slice
        .ref_pic_list1
        .first()
        .ok_or(H264422P10PictureDecodeError::Unsupported(
            "B slice has no L1 reference picture",
        ))?;
    let list0 = decoded.get(&picture_key(list0_id)).ok_or(
        H264422P10PictureDecodeError::MissingDecodedPicture {
            frame_num: list0_id.frame_num,
            poc: list0_id.poc,
        },
    )?;
    let list1 = decoded.get(&picture_key(list1_id)).ok_or(
        H264422P10PictureDecodeError::MissingDecodedPicture {
            frame_num: list1_id.frame_num,
            poc: list1_id.poc,
        },
    )?;
    let payload = slice_payload_from_parsed(parsed, slice)?;
    let payload = payload.cabac_payload_bytes()?;
    let cabac_payload_bits = payload.len().saturating_mul(8);
    let mut cabac = CabacDecoder::new(payload)?;
    let qp_y = slice_luma_qp(parsed, slice).0.clamp(0, 51);
    let mut skip_contexts: [CabacContext; 3] =
        std::array::from_fn(|index| pb_cabac_context(slice.cabac_init_idc, 24 + index, qp_y));
    let mut mb_type_contexts = CabacBSliceMbTypeContexts {
        prefix: std::array::from_fn(|index| {
            pb_cabac_context(slice.cabac_init_idc, 27 + index, qp_y)
        }),
        intra: CabacISliceMbTypeContexts {
            branch: std::array::from_fn(|index| {
                pb_cabac_context(slice.cabac_init_idc, 32 + index, qp_y)
            }),
            suffix: std::array::from_fn(|index| {
                pb_cabac_context(slice.cabac_init_idc, 33 + index, qp_y)
            }),
        },
    };
    let mut intra4x4_prediction_contexts = i_slice_intra4x4_prediction_contexts(qp_y);
    let mut chroma_pred_mode_contexts = i_slice_chroma_pred_mode_contexts(qp_y);
    let mut mb_qp_delta_contexts = i_slice_mb_qp_delta_contexts(qp_y);
    let mut last_qscale_diff_nonzero = false;
    let mut mvd_contexts = pb_mvd_contexts(slice.cabac_init_idc, qp_y);
    let mut cbp_contexts =
        CabacCodedBlockPatternContexts::flat(CabacInitValue::new(9, 43).initialize(qp_y));
    let mut b_luma_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut b_luma8x8_residual_contexts = CabacResidual8x8Contexts::i_slice(qp_y as u8);
    let mut b_cb_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut b_cr_residual_contexts = luma_residual_contexts(qp_y as u8);
    let mut transform_size_8x8_contexts = i_slice_transform_size_8x8_contexts(qp_y);
    let grid = parsed_grid(parsed)?;
    let mut transform_size_8x8_flags = vec![false; grid.macroblock_count() as usize];
    let mut current_qp_y = qp_y;
    let mut b_sub_mb_contexts = CabacBSubMbTypeContexts {
        bins: std::array::from_fn(|index| pb_cabac_context(slice.cabac_init_idc, 36 + index, qp_y)),
    };
    let mut cursor = SliceMacroblockCursor::new(parsed, slice)?;
    let mut count = 0_u32;
    let mut stop_reason = "slice-cursor-ended";
    let mut last_macroblock = None;
    let mut last_macroblock_type = "none";
    let mut last_coded_block_pattern = None;
    let mut cabac_bit_position = None;

    while count < expected_slice_macroblocks {
        let Some(address) = cursor.next_macroblock() else {
            break;
        };
        let predicted_l0 = motion_field.predict_l0_16x16(address, 0)?;
        let predicted_l1 = predicted_l0;
        let skip_context_index = inter_skip_context_index(
            address,
            grid.width_in_mbs,
            u32::from(slice.first_mb_in_slice),
            skip_flags,
        );
        let skipped = cabac.decode_decision(&mut skip_contexts[skip_context_index])?;
        let (prediction, luma_residuals, chroma_residuals) = if skipped {
            last_macroblock_type = "B_Skip";
            last_coded_block_pattern = Some(0);
            (
                BInterPrediction::Bi16x16 {
                    l0: MotionVectorQuarterPel::ZERO,
                    l1: MotionVectorQuarterPel::ZERO,
                },
                InterLumaResidual::Absent,
                None,
            )
        } else {
            let mb_type = decode_b_slice_macroblock_type(&mut cabac, &mut mb_type_contexts)?;
            last_macroblock_type = b_macroblock_type_name(mb_type);
            match mb_type {
                BSliceMacroblockType::Direct16x16 => (
                    BInterPrediction::Bi16x16 {
                        l0: MotionVectorQuarterPel::ZERO,
                        l1: MotionVectorQuarterPel::ZERO,
                    },
                    InterLumaResidual::Absent,
                    None,
                ),
                BSliceMacroblockType::Pred16x16(list) => {
                    let prediction = decode_b_prediction_16x16(
                        list,
                        predicted_l0,
                        predicted_l1,
                        mvd_syntax,
                        address,
                        &ALL_16X16_BLOCKS,
                        &mut cabac,
                        &mut mvd_contexts,
                    )?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut b_luma_residual_contexts,
                        &mut b_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut b_cb_residual_contexts,
                        &mut b_cr_residual_contexts,
                    )?;
                    (prediction, luma_residuals, chroma_residuals)
                }
                BSliceMacroblockType::Pred16x8(lists) => {
                    let first = decode_b_partition_motion(
                        lists[0],
                        predicted_l0,
                        predicted_l1,
                        mvd_syntax,
                        address,
                        P_16X8_BLOCKS[0],
                        &mut cabac,
                        &mut mvd_contexts,
                    )?;
                    let second = decode_b_partition_motion(
                        lists[1],
                        predicted_l0,
                        predicted_l1,
                        mvd_syntax,
                        address,
                        P_16X8_BLOCKS[1],
                        &mut cabac,
                        &mut mvd_contexts,
                    )?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut b_luma_residual_contexts,
                        &mut b_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut b_cb_residual_contexts,
                        &mut b_cr_residual_contexts,
                    )?;
                    (
                        BInterPrediction::Horizontal16x8([first, second]),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
                BSliceMacroblockType::Pred8x16(lists) => {
                    let first = decode_b_partition_motion(
                        lists[0],
                        predicted_l0,
                        predicted_l1,
                        mvd_syntax,
                        address,
                        P_8X16_BLOCKS[0],
                        &mut cabac,
                        &mut mvd_contexts,
                    )?;
                    let second = decode_b_partition_motion(
                        lists[1],
                        predicted_l0,
                        predicted_l1,
                        mvd_syntax,
                        address,
                        P_8X16_BLOCKS[1],
                        &mut cabac,
                        &mut mvd_contexts,
                    )?;
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        true,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut b_luma_residual_contexts,
                        &mut b_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut b_cb_residual_contexts,
                        &mut b_cr_residual_contexts,
                    )?;
                    (
                        BInterPrediction::Vertical8x16([first, second]),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
                BSliceMacroblockType::Intra(intra) => {
                    let chroma_context = if !matches!(intra, ISliceMacroblockType::Pcm) {
                        Some(0)
                    } else {
                        None
                    };
                    let predecoded_chroma_pred_mode = if !matches!(
                        intra,
                        ISliceMacroblockType::Pcm | ISliceMacroblockType::IntraNxN
                    ) {
                        Some(decode_intra_chroma_pred_mode(
                            &mut cabac,
                            &mut chroma_pred_mode_contexts,
                            chroma_context.unwrap_or(0),
                        )?)
                    } else {
                        None
                    };
                    let mut intra_luma_prediction_modes = [];
                    reconstruct_supported_i_macroblock(
                        frame,
                        address,
                        intra,
                        qp_y as u8,
                        &mut cabac,
                        &mut intra4x4_prediction_contexts,
                        &mut chroma_pred_mode_contexts,
                        chroma_context,
                        predecoded_chroma_pred_mode,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        &mut cbp_contexts,
                        &mut transform_size_8x8_contexts,
                        parsed.picture.transform_8x8_mode_flag,
                        grid.width_in_mbs,
                        0,
                        &mut intra_luma_prediction_modes,
                        &mut transform_size_8x8_flags,
                        &[],
                        None,
                        &mut b_luma_residual_contexts,
                        &mut b_luma8x8_residual_contexts,
                        &mut b_cb_residual_contexts,
                        &mut b_cr_residual_contexts,
                    )?;
                    motion_field.set_intra(address)?;
                    mvd_syntax.mark_intra(address);
                    if let Some(slot) = skip_flags.get_mut(address.address as usize) {
                        *slot = false;
                    }
                    if let Some(slot) = coded_block_patterns.get_mut(address.address as usize) {
                        *slot = 0;
                    }
                    last_macroblock = Some(address.address);
                    last_coded_block_pattern = Some(0);
                    count = count.saturating_add(1);
                    if cabac.decode_terminate()? {
                        cabac_bit_position = Some(cabac.bit_position());
                        stop_reason = if count == expected_slice_macroblocks {
                            "cabac-terminate-at-expected-slice-end"
                        } else {
                            "cabac-terminate-before-expected-slice-end"
                        };
                        break;
                    }
                    continue;
                }
                BSliceMacroblockType::B8x8 => {
                    let mut partitions = Vec::new();
                    for subblock_index in 0..4 {
                        let sub_type =
                            decode_b_sub_macroblock_type(&mut cabac, &mut b_sub_mb_contexts)?;
                        decode_b_sub_macroblock_partitions(
                            sub_type,
                            subblock_index,
                            predicted_l0,
                            predicted_l1,
                            mvd_syntax,
                            address,
                            &mut cabac,
                            &mut mvd_contexts,
                            &mut partitions,
                        )?;
                    }
                    let cbp = decode_inter_coded_block_pattern(
                        &mut cabac,
                        &mut cbp_contexts,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        coded_block_patterns,
                    )?;
                    last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
                    let allow_8x8_transform = partitions
                        .iter()
                        .all(|partition| partition.shape == BSubPartitionShape::Full8x8);
                    let transform_size_8x8 = decode_inter_transform_size_8x8_if_present(
                        &mut cabac,
                        &mut transform_size_8x8_contexts,
                        cbp,
                        parsed.picture.transform_8x8_mode_flag,
                        allow_8x8_transform,
                        address,
                        grid.width_in_mbs,
                        u32::from(slice.first_mb_in_slice),
                        &mut transform_size_8x8_flags,
                    )?;
                    decode_inter_mb_qp_delta_if_needed(
                        &mut cabac,
                        &mut mb_qp_delta_contexts,
                        &mut last_qscale_diff_nonzero,
                        &mut current_qp_y,
                        address,
                        cbp,
                    )?;
                    let luma_residuals = decode_inter_luma_residual(
                        &mut cabac,
                        cbp,
                        transform_size_8x8,
                        address,
                        &mut transform_size_8x8_flags,
                        &mut b_luma_residual_contexts,
                        &mut b_luma8x8_residual_contexts,
                    )?;
                    let chroma_residuals = decode_chroma_residual_blocks(
                        &mut cabac,
                        cbp.chroma,
                        &mut b_cb_residual_contexts,
                        &mut b_cr_residual_contexts,
                    )?;
                    (
                        BInterPrediction::SubPartitions(partitions),
                        luma_residuals,
                        chroma_residuals,
                    )
                }
            }
        };
        let representative_motion = prediction.representative_motion();
        prediction.copy_from_references(&list0.frame, &list1.frame, frame, address)?;
        match luma_residuals {
            InterLumaResidual::Absent => {}
            InterLumaResidual::Blocks4x4(residuals) => {
                if !add_luma_residual_16x16(
                    frame,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp_from_qpy(current_qp_y),
                    &residuals,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            }
            InterLumaResidual::Blocks8x8(residuals) => {
                if !add_luma_residual_8x8(
                    frame,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp_from_qpy(current_qp_y),
                    &residuals,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            }
        }
        if let Some((cb, cr)) = chroma_residuals {
            if !add_chroma_residual_422(
                frame,
                ChromaPlane::Cb,
                address.x as usize,
                address.y as usize,
                reconstruction_qp_from_qpy(current_qp_y),
                &cb,
            ) || !add_chroma_residual_422(
                frame,
                ChromaPlane::Cr,
                address.x as usize,
                address.y as usize,
                reconstruction_qp_from_qpy(current_qp_y),
                &cr,
            ) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
        }
        motion_field.set_inter(address, representative_motion, 0)?;
        if skipped || matches!(last_macroblock_type, "B_Direct16x16") {
            mvd_syntax.mark_zero(address, &ALL_16X16_BLOCKS, MotionList::L0);
            mvd_syntax.mark_zero(address, &ALL_16X16_BLOCKS, MotionList::L1);
        }
        if let Some(slot) = skip_flags.get_mut(address.address as usize) {
            *slot = skipped;
        }
        if let Some(slot) = coded_block_patterns.get_mut(address.address as usize) {
            *slot = last_coded_block_pattern.unwrap_or(0);
        }
        last_macroblock = Some(address.address);
        count = count.saturating_add(1);

        if cabac.decode_terminate()? {
            cabac_bit_position = Some(cabac.bit_position());
            stop_reason = if count == expected_slice_macroblocks {
                "cabac-terminate-at-expected-slice-end"
            } else {
                "cabac-terminate-before-expected-slice-end"
            };
            break;
        }
    }
    if count == expected_slice_macroblocks && stop_reason == "slice-cursor-ended" {
        stop_reason = "expected-slice-end-without-terminate";
    }
    Ok(SliceDecodeProgress {
        reconstructed_macroblocks: count,
        stop_reason,
        last_macroblock,
        last_macroblock_type,
        last_coded_block_pattern,
        cabac_bit_position,
        cabac_payload_bits: Some(cabac_payload_bits),
    })
}

fn decode_b_prediction_16x16(
    list: BPredictionList,
    predicted_l0: MotionVectorQuarterPel,
    predicted_l1: MotionVectorQuarterPel,
    mvd_syntax: &mut MvdSyntaxField,
    address: MacroblockAddress,
    blocks: &[usize],
    cabac: &mut CabacDecoder<'_>,
    mvd_contexts: &mut CabacMotionVectorContexts,
) -> Result<BInterPrediction, H264422P10PictureDecodeError> {
    Ok(
        match decode_b_partition_motion(
            list,
            predicted_l0,
            predicted_l1,
            mvd_syntax,
            address,
            blocks,
            cabac,
            mvd_contexts,
        )? {
            BPartitionMotion::L0(motion) => BInterPrediction::L0Full16x16(motion),
            BPartitionMotion::L1(motion) => BInterPrediction::L1Full16x16(motion),
            BPartitionMotion::Bi { l0, l1 } => BInterPrediction::Bi16x16 { l0, l1 },
        },
    )
}

fn decode_b_partition_motion(
    list: BPredictionList,
    predicted_l0: MotionVectorQuarterPel,
    predicted_l1: MotionVectorQuarterPel,
    mvd_syntax: &mut MvdSyntaxField,
    address: MacroblockAddress,
    blocks: &[usize],
    cabac: &mut CabacDecoder<'_>,
    mvd_contexts: &mut CabacMotionVectorContexts,
) -> Result<BPartitionMotion, H264422P10PictureDecodeError> {
    Ok(match list {
        BPredictionList::L0 => {
            let motion = predicted_l0.checked_add(decode_partition_mvd(
                cabac,
                mvd_contexts,
                mvd_syntax,
                address,
                blocks,
                MotionList::L0,
            )?)?;
            BPartitionMotion::L0(motion)
        }
        BPredictionList::L1 => {
            let motion = predicted_l1.checked_add(decode_partition_mvd(
                cabac,
                mvd_contexts,
                mvd_syntax,
                address,
                blocks,
                MotionList::L1,
            )?)?;
            BPartitionMotion::L1(motion)
        }
        BPredictionList::Bi => {
            let l0 = predicted_l0.checked_add(decode_partition_mvd(
                cabac,
                mvd_contexts,
                mvd_syntax,
                address,
                blocks,
                MotionList::L0,
            )?)?;
            let l1 = predicted_l1.checked_add(decode_partition_mvd(
                cabac,
                mvd_contexts,
                mvd_syntax,
                address,
                blocks,
                MotionList::L1,
            )?)?;
            BPartitionMotion::Bi { l0, l1 }
        }
    })
}

fn decode_b_sub_macroblock_partitions(
    sub_type: BSubMacroblockType,
    subblock_index: usize,
    predicted_l0: MotionVectorQuarterPel,
    predicted_l1: MotionVectorQuarterPel,
    mvd_syntax: &mut MvdSyntaxField,
    address: MacroblockAddress,
    cabac: &mut CabacDecoder<'_>,
    mvd_contexts: &mut CabacMotionVectorContexts,
    out: &mut Vec<BSubPartition>,
) -> Result<(), H264422P10PictureDecodeError> {
    match sub_type {
        BSubMacroblockType::Direct8x8 => {
            mvd_syntax.mark_zero(address, B_8X8_BLOCKS[subblock_index], MotionList::L0);
            mvd_syntax.mark_zero(address, B_8X8_BLOCKS[subblock_index], MotionList::L1);
            out.push(BSubPartition {
                subblock_index,
                sub_partition_index: 0,
                shape: BSubPartitionShape::Full8x8,
                motion: BPartitionMotion::Bi {
                    l0: MotionVectorQuarterPel::ZERO,
                    l1: MotionVectorQuarterPel::ZERO,
                },
            });
            Ok(())
        }
        BSubMacroblockType::Pred8x8(list) => {
            out.push(BSubPartition {
                subblock_index,
                sub_partition_index: 0,
                shape: BSubPartitionShape::Full8x8,
                motion: decode_b_partition_motion(
                    list,
                    predicted_l0,
                    predicted_l1,
                    mvd_syntax,
                    address,
                    B_8X8_BLOCKS[subblock_index],
                    cabac,
                    mvd_contexts,
                )?,
            });
            Ok(())
        }
        BSubMacroblockType::Pred8x4(list) => decode_b_split_sub_macroblock_partitions(
            list,
            subblock_index,
            BSubPartitionShape::Horizontal8x4,
            2,
            predicted_l0,
            predicted_l1,
            mvd_syntax,
            address,
            cabac,
            mvd_contexts,
            out,
        ),
        BSubMacroblockType::Pred4x8(list) => decode_b_split_sub_macroblock_partitions(
            list,
            subblock_index,
            BSubPartitionShape::Vertical4x8,
            2,
            predicted_l0,
            predicted_l1,
            mvd_syntax,
            address,
            cabac,
            mvd_contexts,
            out,
        ),
        BSubMacroblockType::Pred4x4(list) => decode_b_split_sub_macroblock_partitions(
            list,
            subblock_index,
            BSubPartitionShape::Square4x4,
            4,
            predicted_l0,
            predicted_l1,
            mvd_syntax,
            address,
            cabac,
            mvd_contexts,
            out,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_b_split_sub_macroblock_partitions(
    list: BPredictionList,
    subblock_index: usize,
    shape: BSubPartitionShape,
    count: usize,
    predicted_l0: MotionVectorQuarterPel,
    predicted_l1: MotionVectorQuarterPel,
    mvd_syntax: &mut MvdSyntaxField,
    address: MacroblockAddress,
    cabac: &mut CabacDecoder<'_>,
    mvd_contexts: &mut CabacMotionVectorContexts,
    out: &mut Vec<BSubPartition>,
) -> Result<(), H264422P10PictureDecodeError> {
    for sub_partition_index in 0..count {
        out.push(BSubPartition {
            subblock_index,
            sub_partition_index,
            shape,
            motion: decode_b_partition_motion(
                list,
                predicted_l0,
                predicted_l1,
                mvd_syntax,
                address,
                b_sub_partition_blocks(subblock_index, shape, sub_partition_index),
                cabac,
                mvd_contexts,
            )?,
        });
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BPartitionMotion {
    L0(MotionVectorQuarterPel),
    L1(MotionVectorQuarterPel),
    Bi {
        l0: MotionVectorQuarterPel,
        l1: MotionVectorQuarterPel,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InterPrediction {
    Full16x16(MotionVectorQuarterPel),
    Horizontal16x8([MotionVectorQuarterPel; 2]),
    Vertical8x16([MotionVectorQuarterPel; 2]),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum InterLumaResidual {
    Absent,
    Blocks4x4(Box<[ResidualBlock4x4; 16]>),
    Blocks8x8(Box<[ResidualBlock8x8; 4]>),
}

impl InterPrediction {
    fn copy_from_reference(
        self,
        reference: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        address: MacroblockAddress,
    ) -> Result<(), H264422P10PictureDecodeError> {
        match self {
            Self::Full16x16(motion) => predict_inter_16x16(
                reference,
                target,
                address.x as usize,
                address.y as usize,
                motion,
            )?,
            Self::Horizontal16x8(motions) => {
                for (index, motion) in motions.into_iter().enumerate() {
                    predict_inter_16x8(
                        reference,
                        target,
                        address.x as usize,
                        address.y as usize,
                        index,
                        motion,
                    )?;
                }
            }
            Self::Vertical8x16(motions) => {
                for (index, motion) in motions.into_iter().enumerate() {
                    predict_inter_8x16(
                        reference,
                        target,
                        address.x as usize,
                        address.y as usize,
                        index,
                        motion,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn representative_motion(self) -> MotionVectorQuarterPel {
        match self {
            Self::Full16x16(motion) => motion,
            Self::Horizontal16x8([first, second]) | Self::Vertical8x16([first, second]) => {
                MotionVectorQuarterPel {
                    x: (first.x + second.x) / 2,
                    y: (first.y + second.y) / 2,
                }
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum BInterPrediction {
    L0Full16x16(MotionVectorQuarterPel),
    L1Full16x16(MotionVectorQuarterPel),
    Bi16x16 {
        l0: MotionVectorQuarterPel,
        l1: MotionVectorQuarterPel,
    },
    Horizontal16x8([BPartitionMotion; 2]),
    Vertical8x16([BPartitionMotion; 2]),
    SubPartitions(Vec<BSubPartition>),
}

impl BInterPrediction {
    fn copy_from_references(
        self,
        list0: &DecodedFrame422P10,
        list1: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        address: MacroblockAddress,
    ) -> Result<(), H264422P10PictureDecodeError> {
        match self {
            Self::L0Full16x16(motion) => predict_inter_16x16(
                list0,
                target,
                address.x as usize,
                address.y as usize,
                motion,
            )?,
            Self::L1Full16x16(motion) => predict_inter_16x16(
                list1,
                target,
                address.x as usize,
                address.y as usize,
                motion,
            )?,
            Self::Bi16x16 { l0, l1 } => predict_bi_inter_16x16(
                list0,
                list1,
                target,
                address.x as usize,
                address.y as usize,
                l0,
                l1,
            )?,
            Self::Horizontal16x8(partitions) => {
                for (index, partition) in partitions.into_iter().enumerate() {
                    partition.copy_16x8(list0, list1, target, address, index)?;
                }
            }
            Self::Vertical8x16(partitions) => {
                for (index, partition) in partitions.into_iter().enumerate() {
                    partition.copy_8x16(list0, list1, target, address, index)?;
                }
            }
            Self::SubPartitions(partitions) => {
                for partition in partitions {
                    partition.copy_from_references(list0, list1, target, address)?;
                }
            }
        }
        Ok(())
    }

    fn representative_motion(&self) -> MotionVectorQuarterPel {
        match self {
            Self::L0Full16x16(motion) | Self::L1Full16x16(motion) => *motion,
            Self::Bi16x16 { l0, l1 } => average_motion(*l0, *l1),
            Self::Horizontal16x8([first, second]) | Self::Vertical8x16([first, second]) => {
                average_motion(
                    first.representative_motion(),
                    second.representative_motion(),
                )
            }
            Self::SubPartitions(partitions) => {
                if partitions.is_empty() {
                    MotionVectorQuarterPel::ZERO
                } else {
                    let mut sum_x = 0_i32;
                    let mut sum_y = 0_i32;
                    for partition in partitions {
                        let motion = partition.motion.representative_motion();
                        sum_x += motion.x;
                        sum_y += motion.y;
                    }
                    MotionVectorQuarterPel {
                        x: sum_x / partitions.len() as i32,
                        y: sum_y / partitions.len() as i32,
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BSubPartition {
    subblock_index: usize,
    sub_partition_index: usize,
    shape: BSubPartitionShape,
    motion: BPartitionMotion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BSubPartitionShape {
    Full8x8,
    Horizontal8x4,
    Vertical4x8,
    Square4x4,
}

impl BSubPartition {
    fn copy_from_references(
        self,
        list0: &DecodedFrame422P10,
        list1: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        address: MacroblockAddress,
    ) -> Result<(), H264422P10PictureDecodeError> {
        let (x_offset, y_offset, width, height) = self.region();
        let base_x = address.x as usize * 16 + x_offset;
        let base_y = address.y as usize * 16 + y_offset;
        self.motion
            .copy_region(list0, list1, target, base_x, base_y, width, height)
    }

    fn region(self) -> (usize, usize, usize, usize) {
        let sub_x = (self.subblock_index % 2) * 8;
        let sub_y = (self.subblock_index / 2) * 8;
        match self.shape {
            BSubPartitionShape::Full8x8 => (sub_x, sub_y, 8, 8),
            BSubPartitionShape::Horizontal8x4 => {
                (sub_x, sub_y + self.sub_partition_index * 4, 8, 4)
            }
            BSubPartitionShape::Vertical4x8 => (sub_x + self.sub_partition_index * 4, sub_y, 4, 8),
            BSubPartitionShape::Square4x4 => (
                sub_x + (self.sub_partition_index % 2) * 4,
                sub_y + (self.sub_partition_index / 2) * 4,
                4,
                4,
            ),
        }
    }
}

impl BPartitionMotion {
    fn copy_16x8(
        self,
        list0: &DecodedFrame422P10,
        list1: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        address: MacroblockAddress,
        partition_index: usize,
    ) -> Result<(), H264422P10PictureDecodeError> {
        match self {
            Self::L0(motion) => predict_inter_16x8(
                list0,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                motion,
            )?,
            Self::L1(motion) => predict_inter_16x8(
                list1,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                motion,
            )?,
            Self::Bi { l0, l1 } => predict_bi_inter_16x8(
                list0,
                list1,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                l0,
                l1,
            )?,
        }
        Ok(())
    }

    fn copy_8x16(
        self,
        list0: &DecodedFrame422P10,
        list1: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        address: MacroblockAddress,
        partition_index: usize,
    ) -> Result<(), H264422P10PictureDecodeError> {
        match self {
            Self::L0(motion) => predict_inter_8x16(
                list0,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                motion,
            )?,
            Self::L1(motion) => predict_inter_8x16(
                list1,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                motion,
            )?,
            Self::Bi { l0, l1 } => predict_bi_inter_8x16(
                list0,
                list1,
                target,
                address.x as usize,
                address.y as usize,
                partition_index,
                l0,
                l1,
            )?,
        }
        Ok(())
    }

    fn copy_region(
        self,
        list0: &DecodedFrame422P10,
        list1: &DecodedFrame422P10,
        target: &mut DecodedFrame422P10,
        dst_luma_x: usize,
        dst_luma_y: usize,
        luma_width: usize,
        luma_height: usize,
    ) -> Result<(), H264422P10PictureDecodeError> {
        match self {
            Self::L0(motion) => predict_inter_region(
                list0,
                target,
                dst_luma_x,
                dst_luma_y,
                luma_width,
                luma_height,
                motion,
            )?,
            Self::L1(motion) => predict_inter_region(
                list1,
                target,
                dst_luma_x,
                dst_luma_y,
                luma_width,
                luma_height,
                motion,
            )?,
            Self::Bi { l0, l1 } => predict_bi_inter_region(
                list0,
                list1,
                target,
                dst_luma_x,
                dst_luma_y,
                luma_width,
                luma_height,
                l0,
                l1,
            )?,
        }
        Ok(())
    }

    fn representative_motion(self) -> MotionVectorQuarterPel {
        match self {
            Self::L0(motion) | Self::L1(motion) => motion,
            Self::Bi { l0, l1 } => average_motion(l0, l1),
        }
    }
}

fn average_motion(
    left: MotionVectorQuarterPel,
    right: MotionVectorQuarterPel,
) -> MotionVectorQuarterPel {
    MotionVectorQuarterPel {
        x: (left.x + right.x) / 2,
        y: (left.y + right.y) / 2,
    }
}

fn pb_cabac_context(cabac_init_idc: u8, context_index: usize, qp_y: i16) -> CabacContext {
    pb_cabac_init_value(cabac_init_idc, context_index)
        .unwrap_or(CabacInitValue::new(7, 34))
        .initialize(qp_y)
}

fn i_cabac_context(context_index: usize, qp_y: i16) -> CabacContext {
    i_cabac_init_value(context_index)
        .unwrap_or(CabacInitValue::new(20, -15))
        .initialize(qp_y)
}

fn i_cabac_init_value(context_index: usize) -> Option<CabacInitValue> {
    let (m, n) = match context_index {
        3 => (20, -15),
        4 => (2, 54),
        5 => (3, 74),
        6 => (-28, 127),
        7 => (-23, 104),
        8 => (-6, 53),
        9 => (-1, 54),
        10 => (7, 51),
        60 => (0, 41),
        61 => (0, 63),
        62 => (0, 63),
        63 => (0, 63),
        64 => (-9, 83),
        65 => (4, 86),
        66 => (0, 97),
        67 => (-7, 72),
        68 => (13, 41),
        69 => (3, 62),
        73 => (-17, 127),
        74 => (-13, 102),
        75 => (0, 82),
        76 => (-7, 74),
        77 => (-21, 107),
        78 => (-27, 127),
        79 => (-31, 127),
        80 => (-24, 127),
        81 => (-18, 95),
        82 => (-27, 127),
        83 => (-21, 114),
        84 => (-30, 127),
        399 => (31, 21),
        400 => (31, 31),
        401 => (25, 50),
        _ => return None,
    };
    Some(CabacInitValue::new(m, n))
}

fn i_slice_mb_type_contexts(qp_y: i16) -> CabacISliceMbTypeContexts {
    CabacISliceMbTypeContexts {
        branch: std::array::from_fn(|index| i_cabac_context(3 + index, qp_y)),
        suffix: std::array::from_fn(|index| i_cabac_context(6 + index, qp_y)),
    }
}

fn i_slice_intra4x4_prediction_contexts(qp_y: i16) -> CabacIntra4x4PredictionModeContexts {
    CabacIntra4x4PredictionModeContexts {
        prev_intra4x4_pred_mode_flag: i_cabac_context(68, qp_y),
        rem_intra4x4_pred_mode: i_cabac_context(69, qp_y),
    }
}

fn i_slice_chroma_pred_mode_contexts(qp_y: i16) -> CabacIntraChromaPredModeContexts {
    CabacIntraChromaPredModeContexts {
        first: std::array::from_fn(|index| i_cabac_context(64 + index, qp_y)),
        suffix: i_cabac_context(67, qp_y),
    }
}

fn i_slice_mb_qp_delta_contexts(qp_y: i16) -> CabacMbQpDeltaContexts {
    CabacMbQpDeltaContexts {
        bins: std::array::from_fn(|index| i_cabac_context(60 + index, qp_y)),
    }
}

fn i_slice_transform_size_8x8_contexts(qp_y: i16) -> CabacTransformSize8x8Contexts {
    CabacTransformSize8x8Contexts {
        bins: std::array::from_fn(|index| i_cabac_context(399 + index, qp_y)),
    }
}

fn i_slice_coded_block_pattern_contexts(qp_y: i16) -> CabacCodedBlockPatternContexts {
    CabacCodedBlockPatternContexts {
        luma: std::array::from_fn(|index| i_cabac_context(73 + index, qp_y)),
        chroma_dc: std::array::from_fn(|index| i_cabac_context(77 + index, qp_y)),
        chroma_ac: std::array::from_fn(|index| i_cabac_context(81 + index, qp_y)),
    }
}

fn pb_mvd_contexts(cabac_init_idc: u8, qp_y: i16) -> CabacMotionVectorContexts {
    let _ = cabac_init_idc;
    CabacMotionVectorContexts::flat(CabacInitValue::new(5, 45).initialize(qp_y))
}

fn pb_cabac_init_value(cabac_init_idc: u8, context_index: usize) -> Option<CabacInitValue> {
    if cabac_init_idc != 0 {
        return None;
    }
    let (m, n) = match context_index {
        11 => (23, 33),
        12 => (23, 2),
        13 => (21, 0),
        24 => (18, 64),
        25 => (9, 43),
        26 => (29, 0),
        27 => (26, 67),
        28 => (16, 90),
        29 => (9, 104),
        30 => (-46, 127),
        31 => (-20, 104),
        32 => (1, 67),
        33 => (-13, 78),
        34 => (-11, 65),
        35 => (1, 62),
        36 => (-6, 86),
        37 => (-17, 95),
        38 => (-6, 61),
        39 => (9, 45),
        73 => (-27, 126),
        74 => (-28, 98),
        75 => (-25, 101),
        76 => (-23, 67),
        77 => (-28, 82),
        78 => (-20, 94),
        79 => (-16, 83),
        80 => (-22, 110),
        81 => (-21, 91),
        82 => (-18, 102),
        83 => (-13, 93),
        84 => (-29, 127),
        40 => (-3, 69),
        41 => (-6, 81),
        42 => (-11, 96),
        43 => (6, 55),
        44 => (7, 67),
        45 => (-5, 86),
        46 => (2, 88),
        47 => (0, 58),
        48 => (-3, 76),
        49 => (-10, 94),
        50 => (5, 54),
        51 => (4, 69),
        52 => (-3, 81),
        53 => (0, 88),
        _ => return None,
    };
    Some(CabacInitValue::new(m, n))
}

fn decode_luma_residual_blocks(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacResidual4x4Contexts,
) -> Result<[ResidualBlock4x4; 16], H264422P10PictureDecodeError> {
    let mut residuals = std::array::from_fn(|_| ResidualBlock4x4::zero());
    for block in &mut residuals {
        *block = decode_residual_4x4(cabac, contexts)?.block;
    }
    Ok(residuals)
}

fn decode_inter_mb_qp_delta_if_needed(
    _cabac: &mut CabacDecoder<'_>,
    _contexts: &mut CabacMbQpDeltaContexts,
    last_qscale_diff_nonzero: &mut bool,
    _current_qp_y: &mut i16,
    _address: MacroblockAddress,
    _cbp: CodedBlockPattern,
) -> Result<(), H264422P10PictureDecodeError> {
    *last_qscale_diff_nonzero = false;
    Ok(())
}

fn decode_inter_transform_size_8x8_if_present(
    _cabac: &mut CabacDecoder<'_>,
    _contexts: &mut CabacTransformSize8x8Contexts,
    _cbp: CodedBlockPattern,
    _transform_8x8_mode_flag: bool,
    _allow_8x8_transform: bool,
    address: MacroblockAddress,
    _width_in_mbs: u32,
    _slice_first_mb: u32,
    transform_size_8x8_flags: &mut [bool],
) -> Result<bool, H264422P10PictureDecodeError> {
    if let Some(slot) = transform_size_8x8_flags.get_mut(address.address as usize) {
        *slot = false;
    }
    Ok(false)
}

fn decode_chroma_residual_blocks(
    cabac: &mut CabacDecoder<'_>,
    coded_block_pattern_chroma: CodedBlockPatternChroma,
    cb_contexts: &mut CabacResidual4x4Contexts,
    cr_contexts: &mut CabacResidual4x4Contexts,
) -> Result<Option<([ResidualBlock4x4; 8], [ResidualBlock4x4; 8])>, H264422P10PictureDecodeError> {
    if coded_block_pattern_chroma == CodedBlockPatternChroma::Zero {
        return Ok(None);
    }
    let mut cb = std::array::from_fn(|_| ResidualBlock4x4::zero());
    let mut cr = std::array::from_fn(|_| ResidualBlock4x4::zero());
    for block in &mut cb {
        *block = decode_residual_4x4(cabac, cb_contexts)?.block;
    }
    for block in &mut cr {
        *block = decode_residual_4x4(cabac, cr_contexts)?.block;
    }
    Ok(Some((cb, cr)))
}

#[allow(clippy::too_many_arguments)]
fn decode_inter_luma_residual(
    cabac: &mut CabacDecoder<'_>,
    cbp: CodedBlockPattern,
    transform_size_8x8: bool,
    address: MacroblockAddress,
    transform_size_8x8_flags: &mut [bool],
    luma_residual_contexts: &mut CabacResidual4x4Contexts,
    luma8x8_residual_contexts: &mut CabacResidual8x8Contexts,
) -> Result<InterLumaResidual, H264422P10PictureDecodeError> {
    if cbp.luma == 0 && cbp.chroma == CodedBlockPatternChroma::Zero {
        if let Some(slot) = transform_size_8x8_flags.get_mut(address.address as usize) {
            *slot = false;
        }
        return Ok(InterLumaResidual::Absent);
    }

    if cbp.luma == 0 {
        if let Some(slot) = transform_size_8x8_flags.get_mut(address.address as usize) {
            *slot = false;
        }
        return Ok(InterLumaResidual::Absent);
    }

    if let Some(slot) = transform_size_8x8_flags.get_mut(address.address as usize) {
        *slot = transform_size_8x8;
    }

    if transform_size_8x8 {
        let mut luma = std::array::from_fn(|_| ResidualBlock8x8::zero());
        for block_index in 0..4 {
            if cbp.luma_block_present(block_index) {
                luma[block_index] = decode_residual_8x8(cabac, luma8x8_residual_contexts)?.block;
            }
        }
        Ok(InterLumaResidual::Blocks8x8(Box::new(luma)))
    } else {
        Ok(InterLumaResidual::Blocks4x4(Box::new(
            decode_luma_residual_blocks(cabac, luma_residual_contexts)?,
        )))
    }
}

fn decode_i_slice_into_frame(
    parsed: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
    expected_slice_macroblocks: u32,
    frame: &mut DecodedFrame422P10,
    motion_field: &mut MotionField,
    intra16_or_pcm: &mut [Option<bool>],
    chroma_pred_modes: &mut [Option<u8>],
    intra_luma_prediction_modes: &mut [Option<[Intra4x4PredictionMode; 16]>],
    coded_block_patterns: &mut [u8],
    transform_size_8x8_flags: &mut [bool],
) -> Result<SliceDecodeProgress, H264422P10PictureDecodeError> {
    let payload = slice_payload_from_parsed(parsed, slice)?;
    let payload = payload.cabac_payload_bytes()?;
    let cabac_payload_bits = payload.len().saturating_mul(8);
    let mut cabac = crate::cabac::CabacDecoder::new(payload)?;
    let qp_y = slice_luma_qp(parsed, slice).0.clamp(0, 51) as u8;
    let mut cursor = SliceMacroblockCursor::new(parsed, slice)?;
    let mut mb_type_contexts = i_slice_mb_type_contexts(i16::from(qp_y));
    let mut intra4x4_prediction_contexts = i_slice_intra4x4_prediction_contexts(i16::from(qp_y));
    let mut chroma_pred_mode_contexts = i_slice_chroma_pred_mode_contexts(i16::from(qp_y));
    let mut mb_qp_delta_contexts = i_slice_mb_qp_delta_contexts(i16::from(qp_y));
    let mut last_qscale_diff_nonzero = false;
    let mut cbp_contexts = i_slice_coded_block_pattern_contexts(i16::from(qp_y));
    let mut transform_size_8x8_contexts = i_slice_transform_size_8x8_contexts(i16::from(qp_y));
    let grid = parsed_grid(parsed)?;
    let mut i_residual_decoder = CabacResidualDecoder422::new_i_slice(grid, qp_y);
    i_residual_decoder.set_slice_first_mb(u32::from(slice.first_mb_in_slice));
    let mut i_luma_residual_contexts = luma_residual_contexts(qp_y);
    let mut i_luma8x8_residual_contexts = CabacResidual8x8Contexts::i_slice(qp_y);
    let mut i_cb_residual_contexts = luma_residual_contexts(qp_y);
    let mut i_cr_residual_contexts = luma_residual_contexts(qp_y);
    let mut current_qp_y = slice_luma_qp(parsed, slice).0;

    let mut count = 0_u32;
    let mut stop_reason = "slice-cursor-ended";
    let mut last_macroblock = None;
    let mut last_macroblock_type = "none";
    let mut last_coded_block_pattern = None;
    let mut cabac_bit_position = None;
    let trace_i_slice = std::env::var_os("QGS_H264_422P10_TRACE").is_some();
    while count < expected_slice_macroblocks {
        let Some(address) = cursor.next_macroblock() else {
            break;
        };
        let macroblock_bit_start = cabac.bit_position();
        let branch_context = i_slice_mb_type_branch_context(
            parsed,
            u32::from(slice.first_mb_in_slice),
            address,
            intra16_or_pcm,
        )?;
        let mb_type = decode_i_slice_macroblock_type_with_context(
            &mut cabac,
            &mut mb_type_contexts,
            branch_context,
        )?;
        let chroma_context = if matches!(mb_type, ISliceMacroblockType::Pcm) {
            None
        } else {
            Some(intra_chroma_pred_mode_context(
                parsed,
                u32::from(slice.first_mb_in_slice),
                address,
                chroma_pred_modes,
            )?)
        };
        let predecoded_chroma_pred_mode = if !matches!(
            mb_type,
            ISliceMacroblockType::Pcm | ISliceMacroblockType::IntraNxN
        ) {
            Some(decode_intra_chroma_pred_mode(
                &mut cabac,
                &mut chroma_pred_mode_contexts,
                chroma_context.unwrap_or(0),
            )?)
        } else {
            None
        };
        let (cbp, decoded_chroma_pred_mode) = reconstruct_supported_i_macroblock(
            frame,
            address,
            mb_type,
            qp_y,
            &mut cabac,
            &mut intra4x4_prediction_contexts,
            &mut chroma_pred_mode_contexts,
            chroma_context,
            predecoded_chroma_pred_mode,
            &mut mb_qp_delta_contexts,
            &mut last_qscale_diff_nonzero,
            &mut current_qp_y,
            &mut cbp_contexts,
            &mut transform_size_8x8_contexts,
            parsed.picture.transform_8x8_mode_flag,
            parsed.desc.coded_width / 16,
            u32::from(slice.first_mb_in_slice),
            intra_luma_prediction_modes,
            transform_size_8x8_flags,
            coded_block_patterns,
            Some(&mut i_residual_decoder),
            &mut i_luma_residual_contexts,
            &mut i_luma8x8_residual_contexts,
            &mut i_cb_residual_contexts,
            &mut i_cr_residual_contexts,
        )?;
        if trace_i_slice {
            let transform_size_8x8 = transform_size_8x8_flags
                .get(address.address as usize)
                .copied()
                .unwrap_or(false);
            eprintln!(
                "qgs-h264-422p10 trace: slice_first={} mb={} count={} bits={}..{} type={} t8x8={} cbp=0x{:02x} qp={}",
                slice.first_mb_in_slice,
                address.address,
                count,
                macroblock_bit_start,
                cabac.bit_position(),
                i_macroblock_type_name(mb_type),
                transform_size_8x8,
                coded_block_pattern_to_u8(cbp),
                reconstruction_qp_from_qpy(current_qp_y),
            );
        }
        last_macroblock = Some(address.address);
        last_macroblock_type = i_macroblock_type_name(mb_type);
        last_coded_block_pattern = Some(coded_block_pattern_to_u8(cbp));
        if let Some(slot) = coded_block_patterns.get_mut(address.address as usize) {
            *slot = coded_block_pattern_to_u8(cbp);
        }
        motion_field.set_intra(address)?;
        if let Some(slot) = intra16_or_pcm.get_mut(address.address as usize) {
            *slot = Some(matches!(
                mb_type,
                ISliceMacroblockType::Intra16x16 { .. } | ISliceMacroblockType::Pcm
            ));
        }
        if let (Some(mode), Some(slot)) = (
            decoded_chroma_pred_mode,
            chroma_pred_modes.get_mut(address.address as usize),
        ) {
            *slot = Some(mode);
        }
        count = count.saturating_add(1);

        let pre_terminate_range = cabac.range();
        let pre_terminate_offset = cabac.offset();
        let pre_terminate_bits = cabac.bit_position();
        if cabac.decode_terminate()? {
            if trace_i_slice {
                eprintln!(
                    "qgs-h264-422p10 trace: slice_first={} terminate_after_mb={} pre_bits={} range={} offset={}",
                    slice.first_mb_in_slice,
                    address.address,
                    pre_terminate_bits,
                    pre_terminate_range,
                    pre_terminate_offset,
                );
            }
            cabac_bit_position = Some(cabac.bit_position());
            stop_reason = if count == expected_slice_macroblocks {
                "cabac-terminate-at-expected-slice-end"
            } else {
                "cabac-terminate-before-expected-slice-end"
            };
            break;
        }
    }
    if count == expected_slice_macroblocks && stop_reason == "slice-cursor-ended" {
        stop_reason = "expected-slice-end-without-terminate";
    }
    Ok(SliceDecodeProgress {
        reconstructed_macroblocks: count,
        stop_reason,
        last_macroblock,
        last_macroblock_type,
        last_coded_block_pattern,
        cabac_bit_position,
        cabac_payload_bits: Some(cabac_payload_bits),
    })
}

fn i_macroblock_type_name(mb_type: ISliceMacroblockType) -> &'static str {
    match mb_type {
        ISliceMacroblockType::IntraNxN => "I_NxN",
        ISliceMacroblockType::Intra16x16 { .. } => "I_16x16",
        ISliceMacroblockType::Pcm => "I_PCM",
    }
}

fn p_macroblock_type_name(mb_type: PSliceMacroblockType) -> &'static str {
    match mb_type {
        PSliceMacroblockType::L0_16x16 => "P_L0_16x16",
        PSliceMacroblockType::L0L0_16x8 => "P_L0L0_16x8",
        PSliceMacroblockType::L0L0_8x16 => "P_L0L0_8x16",
        PSliceMacroblockType::P8x8 => "P_8x8",
        PSliceMacroblockType::P8x8Ref0 => "P_8x8ref0",
        PSliceMacroblockType::Intra(_) => "P_Intra",
    }
}

fn b_macroblock_type_name(mb_type: BSliceMacroblockType) -> &'static str {
    match mb_type {
        BSliceMacroblockType::Direct16x16 => "B_Direct16x16",
        BSliceMacroblockType::Pred16x16(_) => "B_Pred16x16",
        BSliceMacroblockType::Pred16x8(_) => "B_Pred16x8",
        BSliceMacroblockType::Pred8x16(_) => "B_Pred8x16",
        BSliceMacroblockType::B8x8 => "B_8x8",
        BSliceMacroblockType::Intra(_) => "B_Intra",
    }
}

fn i_slice_mb_type_branch_context(
    parsed: &ParsedH264AccessUnit,
    slice_first_mb: u32,
    address: MacroblockAddress,
    intra16_or_pcm: &[Option<bool>],
) -> Result<usize, H264422P10PictureDecodeError> {
    let grid = MacroblockGrid::new(parsed.desc.coded_width, parsed.desc.coded_height)
        .map_err(|_| H264422P10PictureDecodeError::Unsupported("invalid macroblock grid"))?;
    let left = if address.x > 0 && address.address.saturating_sub(1) >= slice_first_mb {
        intra16_or_pcm
            .get(address.address.saturating_sub(1) as usize)
            .copied()
            .flatten()
            .unwrap_or(false)
    } else {
        false
    };
    let top_address = address.address.saturating_sub(grid.width_in_mbs);
    let top = if address.y > 0 && top_address >= slice_first_mb {
        intra16_or_pcm
            .get(top_address as usize)
            .copied()
            .flatten()
            .unwrap_or(false)
    } else {
        false
    };
    Ok(usize::from(left) + usize::from(top))
}

fn intra_chroma_pred_mode_context(
    parsed: &ParsedH264AccessUnit,
    slice_first_mb: u32,
    address: MacroblockAddress,
    chroma_pred_modes: &[Option<u8>],
) -> Result<usize, H264422P10PictureDecodeError> {
    let grid = MacroblockGrid::new(parsed.desc.coded_width, parsed.desc.coded_height)
        .map_err(|_| H264422P10PictureDecodeError::Unsupported("invalid macroblock grid"))?;
    let left = if address.x > 0 && address.address.saturating_sub(1) >= slice_first_mb {
        chroma_pred_modes
            .get(address.address.saturating_sub(1) as usize)
            .copied()
            .flatten()
            .map(|mode| mode != 0)
            .unwrap_or(false)
    } else {
        false
    };
    let top_address = address.address.saturating_sub(grid.width_in_mbs);
    let top = if address.y > 0 && top_address >= slice_first_mb {
        chroma_pred_modes
            .get(top_address as usize)
            .copied()
            .flatten()
            .map(|mode| mode != 0)
            .unwrap_or(false)
    } else {
        false
    };
    Ok(usize::from(left) + usize::from(top))
}

fn transform_size_8x8_context(
    width_in_mbs: u32,
    slice_first_mb: u32,
    address: MacroblockAddress,
    transform_size_8x8_flags: &[bool],
) -> (bool, bool) {
    let left = if address.x > 0 && address.address.saturating_sub(1) >= slice_first_mb {
        transform_size_8x8_flags
            .get(address.address.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(false)
    } else {
        false
    };
    let top_address = address.address.saturating_sub(width_in_mbs);
    let top = if address.y > 0 && top_address >= slice_first_mb {
        transform_size_8x8_flags
            .get(top_address as usize)
            .copied()
            .unwrap_or(false)
    } else {
        false
    };
    (left, top)
}

fn decode_intra_coded_block_pattern(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacCodedBlockPatternContexts,
    address: MacroblockAddress,
    width_in_mbs: u32,
    slice_first_mb: u32,
    coded_block_patterns: &[u8],
) -> Result<CodedBlockPattern, H264422P10PictureDecodeError> {
    let left = if address.x > 0 && address.address.saturating_sub(1) >= slice_first_mb {
        coded_block_patterns
            .get(address.address.saturating_sub(1) as usize)
            .copied()
    } else {
        Some(0x0f)
    };
    let top_address = address.address.saturating_sub(width_in_mbs);
    let top = if address.y > 0 && top_address >= slice_first_mb {
        coded_block_patterns.get(top_address as usize).copied()
    } else {
        Some(0x0f)
    };
    Ok(decode_coded_block_pattern_with_neighbors(
        cabac, contexts, left, top,
    )?)
}

fn decode_inter_coded_block_pattern(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacCodedBlockPatternContexts,
    _address: MacroblockAddress,
    _width_in_mbs: u32,
    _slice_first_mb: u32,
    _coded_block_patterns: &[u8],
) -> Result<CodedBlockPattern, H264422P10PictureDecodeError> {
    Ok(decode_coded_block_pattern(cabac, contexts)?)
}

fn reconstruct_supported_i_macroblock(
    frame: &mut DecodedFrame422P10,
    address: MacroblockAddress,
    mb_type: ISliceMacroblockType,
    _qp_y: u8,
    cabac: &mut CabacDecoder<'_>,
    intra4x4_prediction_contexts: &mut CabacIntra4x4PredictionModeContexts,
    chroma_pred_mode_contexts: &mut CabacIntraChromaPredModeContexts,
    chroma_context: Option<usize>,
    predecoded_chroma_pred_mode: Option<u8>,
    mb_qp_delta_contexts: &mut CabacMbQpDeltaContexts,
    last_qscale_diff_nonzero: &mut bool,
    current_qp_y: &mut i16,
    cbp_contexts: &mut CabacCodedBlockPatternContexts,
    transform_size_8x8_contexts: &mut CabacTransformSize8x8Contexts,
    transform_8x8_mode_flag: bool,
    width_in_mbs: u32,
    slice_first_mb: u32,
    intra_luma_prediction_modes: &mut [Option<[Intra4x4PredictionMode; 16]>],
    transform_size_8x8_flags: &mut [bool],
    coded_block_patterns: &[u8],
    mut residual_decoder_422: Option<&mut CabacResidualDecoder422>,
    luma_residual_contexts: &mut CabacResidual4x4Contexts,
    luma8x8_residual_contexts: &mut CabacResidual8x8Contexts,
    cb_residual_contexts: &mut CabacResidual4x4Contexts,
    cr_residual_contexts: &mut CabacResidual4x4Contexts,
) -> Result<(CodedBlockPattern, Option<u8>), H264422P10PictureDecodeError> {
    match mb_type {
        ISliceMacroblockType::Intra16x16 {
            coded_block_pattern_chroma,
            coded_block_pattern_luma,
            ..
        } => {
            let mb_qp_delta = decode_mb_qp_delta(
                cabac,
                mb_qp_delta_contexts,
                last_qscale_diff_nonzero,
                address.address,
                "Intra16x16 CABAC mb_qp_delta exceeded bounded decoder range",
            )?;
            apply_i_mb_qp_delta(current_qp_y, mb_qp_delta);
            let reconstruction_qp = reconstruction_qp_from_qpy(*current_qp_y);
            let mut macroblock = Intra16x16Macroblock::from_type(mb_type, reconstruction_qp)?;
            if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                let luma = residual_decoder.decode_intra16x16_luma(
                    cabac,
                    address,
                    coded_block_pattern_luma,
                )?;
                macroblock.luma = luma.into_reconstruction_blocks(reconstruction_qp);
                let chroma = residual_decoder.decode_chroma_422(
                    cabac,
                    address,
                    coded_block_pattern_chroma,
                )?;
                let [cb, cr] = chroma.into_reconstruction_blocks(reconstruction_qp);
                macroblock.cb = cb;
                macroblock.cr = cr;
            } else {
                if coded_block_pattern_luma != 0 {
                    for block in &mut macroblock.luma {
                        *block = decode_residual_4x4(cabac, luma_residual_contexts)?.block;
                    }
                }
                if let Some((cb, cr)) = decode_chroma_residual_blocks(
                    cabac,
                    coded_block_pattern_chroma,
                    cb_residual_contexts,
                    cr_residual_contexts,
                )? {
                    macroblock.cb = cb;
                    macroblock.cr = cr;
                }
            }
            let top = top_luma_samples(frame, address);
            let left = left_luma_samples(frame, address);
            if !reconstruct_intra16x16_luma_dc(
                frame,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                macroblock.prediction,
                &macroblock.luma,
                top,
                left,
            ) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
            let chroma_pred_mode = predecoded_chroma_pred_mode.unwrap_or(0);
            if !reconstruct_chroma_422_intra(
                frame,
                ChromaPlane::Cb,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                chroma_pred_mode,
                &macroblock.cb,
            ) || !reconstruct_chroma_422_intra(
                frame,
                ChromaPlane::Cr,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                chroma_pred_mode,
                &macroblock.cr,
            ) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
            Ok((
                CodedBlockPattern {
                    luma: coded_block_pattern_luma,
                    chroma: coded_block_pattern_chroma,
                },
                predecoded_chroma_pred_mode,
            ))
        }
        ISliceMacroblockType::IntraNxN => {
            let transform_size_8x8 = if transform_8x8_mode_flag {
                let (left, top) = transform_size_8x8_context(
                    width_in_mbs,
                    slice_first_mb,
                    address,
                    transform_size_8x8_flags,
                );
                decode_transform_size_8x8_flag(cabac, transform_size_8x8_contexts, left, top)?
            } else {
                false
            };
            if let Some(slot) = transform_size_8x8_flags.get_mut(address.address as usize) {
                *slot = transform_size_8x8;
            }
            if transform_size_8x8 {
                let modes = decode_intra8x8_prediction_modes(
                    cabac,
                    intra4x4_prediction_contexts,
                    address,
                    width_in_mbs,
                    slice_first_mb,
                    intra_luma_prediction_modes,
                )?;
                if let Some(slot) = intra_luma_prediction_modes.get_mut(address.address as usize) {
                    *slot = Some(expand_intra8x8_modes(modes));
                }
                let chroma_pred_mode = Some(decode_intra_chroma_pred_mode(
                    cabac,
                    chroma_pred_mode_contexts,
                    chroma_context.unwrap_or(0),
                )?);
                let cbp = decode_intra_coded_block_pattern(
                    cabac,
                    cbp_contexts,
                    address,
                    width_in_mbs,
                    slice_first_mb,
                    coded_block_patterns,
                )?;
                if cbp.luma != 0 || cbp.chroma != CodedBlockPatternChroma::Zero {
                    let mb_qp_delta = decode_mb_qp_delta(
                        cabac,
                        mb_qp_delta_contexts,
                        last_qscale_diff_nonzero,
                        address.address,
                        "Intra8x8 CABAC mb_qp_delta exceeded bounded decoder range",
                    )?;
                    apply_i_mb_qp_delta(current_qp_y, mb_qp_delta);
                } else {
                    *last_qscale_diff_nonzero = false;
                }
                let reconstruction_qp = reconstruction_qp_from_qpy(*current_qp_y);
                let mut luma = std::array::from_fn(|_| ResidualBlock8x8::zero());
                for block_index in 0..4 {
                    if cbp.luma_block_present(block_index) {
                        let residual = decode_residual_8x8(cabac, luma8x8_residual_contexts)?;
                        if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                            residual_decoder.record_luma8x8_count(
                                address,
                                block_index,
                                residual.report.non_zero_coefficients,
                            );
                        }
                        luma[block_index] = residual.block;
                    } else if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                        residual_decoder.record_luma8x8_count(address, block_index, 0);
                    }
                }
                reconstruct_intra8x8_luma(
                    frame,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    &modes,
                    &luma,
                )?;
                let zero_chroma = std::array::from_fn(|_| ResidualBlock4x4::zero());
                if !reconstruct_chroma_422_intra(
                    frame,
                    ChromaPlane::Cb,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    chroma_pred_mode.unwrap_or(0),
                    &zero_chroma,
                ) || !reconstruct_chroma_422_intra(
                    frame,
                    ChromaPlane::Cr,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    chroma_pred_mode.unwrap_or(0),
                    &zero_chroma,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
                if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                    let chroma = residual_decoder.decode_chroma_422(cabac, address, cbp.chroma)?;
                    let [cb, cr] = chroma.into_reconstruction_blocks(reconstruction_qp);
                    if !add_chroma_residual_422(
                        frame,
                        ChromaPlane::Cb,
                        address.x as usize,
                        address.y as usize,
                        reconstruction_qp,
                        &cb,
                    ) || !add_chroma_residual_422(
                        frame,
                        ChromaPlane::Cr,
                        address.x as usize,
                        address.y as usize,
                        reconstruction_qp,
                        &cr,
                    ) {
                        return Err(MacroblockReconstructionError::OutOfBounds.into());
                    }
                } else if let Some((cb, cr)) = decode_chroma_residual_blocks(
                    cabac,
                    cbp.chroma,
                    cb_residual_contexts,
                    cr_residual_contexts,
                )? {
                    if !add_chroma_residual_422(
                        frame,
                        ChromaPlane::Cb,
                        address.x as usize,
                        address.y as usize,
                        reconstruction_qp,
                        &cb,
                    ) || !add_chroma_residual_422(
                        frame,
                        ChromaPlane::Cr,
                        address.x as usize,
                        address.y as usize,
                        reconstruction_qp,
                        &cr,
                    ) {
                        return Err(MacroblockReconstructionError::OutOfBounds.into());
                    }
                }
                return Ok((cbp, chroma_pred_mode));
            }
            let modes = decode_intra4x4_prediction_modes(
                cabac,
                intra4x4_prediction_contexts,
                address,
                width_in_mbs,
                slice_first_mb,
                intra_luma_prediction_modes,
            )?;
            if let Some(slot) = intra_luma_prediction_modes.get_mut(address.address as usize) {
                *slot = Some(modes);
            }
            let chroma_pred_mode = Some(decode_intra_chroma_pred_mode(
                cabac,
                chroma_pred_mode_contexts,
                chroma_context.unwrap_or(0),
            )?);
            let cbp = decode_intra_coded_block_pattern(
                cabac,
                cbp_contexts,
                address,
                width_in_mbs,
                slice_first_mb,
                coded_block_patterns,
            )?;
            if cbp.luma != 0 || cbp.chroma != CodedBlockPatternChroma::Zero {
                let mb_qp_delta = decode_mb_qp_delta(
                    cabac,
                    mb_qp_delta_contexts,
                    last_qscale_diff_nonzero,
                    address.address,
                    "Intra4x4 CABAC mb_qp_delta exceeded bounded decoder range",
                )?;
                apply_i_mb_qp_delta(current_qp_y, mb_qp_delta);
            } else {
                *last_qscale_diff_nonzero = false;
            }
            let reconstruction_qp = reconstruction_qp_from_qpy(*current_qp_y);
            let luma = if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                residual_decoder
                    .decode_intra4x4_luma(cabac, address, cbp.luma)?
                    .0
            } else if cbp.luma == 0 {
                std::array::from_fn(|_| ResidualBlock4x4::zero())
            } else {
                decode_luma_residual_blocks(cabac, luma_residual_contexts)?
            };
            reconstruct_intra4x4_luma(
                frame,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                &modes,
                &luma,
            )?;
            let zero_chroma = std::array::from_fn(|_| ResidualBlock4x4::zero());
            if !reconstruct_chroma_422_intra(
                frame,
                ChromaPlane::Cb,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                chroma_pred_mode.unwrap_or(0),
                &zero_chroma,
            ) || !reconstruct_chroma_422_intra(
                frame,
                ChromaPlane::Cr,
                address.x as usize,
                address.y as usize,
                reconstruction_qp,
                chroma_pred_mode.unwrap_or(0),
                &zero_chroma,
            ) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
            if let Some(residual_decoder) = residual_decoder_422.as_deref_mut() {
                let chroma = residual_decoder.decode_chroma_422(cabac, address, cbp.chroma)?;
                let [cb, cr] = chroma.into_reconstruction_blocks(reconstruction_qp);
                if !add_chroma_residual_422(
                    frame,
                    ChromaPlane::Cb,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    &cb,
                ) || !add_chroma_residual_422(
                    frame,
                    ChromaPlane::Cr,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    &cr,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            } else if let Some((cb, cr)) = decode_chroma_residual_blocks(
                cabac,
                cbp.chroma,
                cb_residual_contexts,
                cr_residual_contexts,
            )? {
                if !add_chroma_residual_422(
                    frame,
                    ChromaPlane::Cb,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    &cb,
                ) || !add_chroma_residual_422(
                    frame,
                    ChromaPlane::Cr,
                    address.x as usize,
                    address.y as usize,
                    reconstruction_qp,
                    &cr,
                ) {
                    return Err(MacroblockReconstructionError::OutOfBounds.into());
                }
            }
            Ok((cbp, chroma_pred_mode))
        }
        ISliceMacroblockType::Pcm => {
            decode_pcm_macroblock(frame, address, cabac)?;
            Ok((
                CodedBlockPattern {
                    luma: 15,
                    chroma: CodedBlockPatternChroma::DcAndAc,
                },
                None,
            ))
        }
    }
}

fn coded_block_pattern_to_u8(cbp: CodedBlockPattern) -> u8 {
    let chroma = match cbp.chroma {
        CodedBlockPatternChroma::Zero => 0,
        CodedBlockPatternChroma::Dc => 1,
        CodedBlockPatternChroma::DcAndAc => 2,
    };
    cbp.luma | (chroma << 4)
}

fn decode_pcm_macroblock(
    frame: &mut DecodedFrame422P10,
    address: MacroblockAddress,
    cabac: &mut CabacDecoder<'_>,
) -> Result<(), H264422P10PictureDecodeError> {
    let base_x = address.x as usize * 16;
    let base_y = address.y as usize * 16;
    cabac.align_raw_byte()?;
    for y in 0..16 {
        for x in 0..16 {
            let sample = cabac.read_raw_bits(10)? as u16;
            if !frame.y.set(base_x + x, base_y + y, sample) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
        }
    }
    for y in 0..16 {
        for x in 0..8 {
            let sample = cabac.read_raw_bits(10)? as u16;
            if !frame.cb.set(base_x / 2 + x, base_y + y, sample) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
        }
    }
    for y in 0..16 {
        for x in 0..8 {
            let sample = cabac.read_raw_bits(10)? as u16;
            if !frame.cr.set(base_x / 2 + x, base_y + y, sample) {
                return Err(MacroblockReconstructionError::OutOfBounds.into());
            }
        }
    }
    cabac.align_raw_byte()?;
    cabac.reinitialize_from_current_position()?;
    Ok(())
}

fn decode_intra4x4_prediction_modes(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacIntra4x4PredictionModeContexts,
    address: MacroblockAddress,
    width_in_mbs: u32,
    slice_first_mb: u32,
    stored_modes: &[Option<[Intra4x4PredictionMode; 16]>],
) -> Result<[Intra4x4PredictionMode; 16], H264422P10PictureDecodeError> {
    let mut modes = [Intra4x4PredictionMode::Dc; 16];
    for index in 0..16 {
        let predicted = predicted_intra4x4_mode(
            &modes,
            index,
            address,
            width_in_mbs,
            slice_first_mb,
            stored_modes,
        );
        let mode_index = if cabac.decode_decision(&mut contexts.prev_intra4x4_pred_mode_flag)? {
            predicted
        } else {
            let rem = decode_intra4x4_rem_mode(cabac, &mut contexts.rem_intra4x4_pred_mode)?;
            if rem >= predicted {
                rem.saturating_add(1)
            } else {
                rem
            }
        };
        modes[index] = Intra4x4PredictionMode::from_index(mode_index).ok_or(
            H264422P10PictureDecodeError::Unsupported("invalid Intra4x4 prediction mode"),
        )?;
    }
    Ok(modes)
}

fn decode_intra8x8_prediction_modes(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacIntra4x4PredictionModeContexts,
    address: MacroblockAddress,
    width_in_mbs: u32,
    slice_first_mb: u32,
    stored_modes: &[Option<[Intra4x4PredictionMode; 16]>],
) -> Result<[Intra4x4PredictionMode; 4], H264422P10PictureDecodeError> {
    let mut modes = [Intra4x4PredictionMode::Dc; 4];
    let mut expanded_modes = [Intra4x4PredictionMode::Dc; 16];
    const ANCHORS: [usize; 4] = [0, 4, 8, 12];
    const BLOCKS: [[usize; 4]; 4] = [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]];
    for index in 0..4 {
        let predicted = predicted_intra4x4_mode(
            &expanded_modes,
            ANCHORS[index],
            address,
            width_in_mbs,
            slice_first_mb,
            stored_modes,
        );
        let mode_index = if cabac.decode_decision(&mut contexts.prev_intra4x4_pred_mode_flag)? {
            predicted
        } else {
            let rem = decode_intra4x4_rem_mode(cabac, &mut contexts.rem_intra4x4_pred_mode)?;
            if rem >= predicted {
                rem.saturating_add(1)
            } else {
                rem
            }
        };
        modes[index] = Intra4x4PredictionMode::from_index(mode_index).ok_or(
            H264422P10PictureDecodeError::Unsupported("invalid Intra8x8 prediction mode"),
        )?;
        for block_index in BLOCKS[index] {
            expanded_modes[block_index] = modes[index];
        }
    }
    Ok(modes)
}

fn decode_intra4x4_rem_mode(
    cabac: &mut CabacDecoder<'_>,
    context: &mut CabacContext,
) -> Result<u8, H264422P10PictureDecodeError> {
    let b0 = u8::from(cabac.decode_decision(context)?);
    let b1 = u8::from(cabac.decode_decision(context)?);
    let b2 = u8::from(cabac.decode_decision(context)?);
    Ok((b0 << 2) | (b1 << 1) | b2)
}

fn decode_intra_chroma_pred_mode(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacIntraChromaPredModeContexts,
    context_index: usize,
) -> Result<u8, H264422P10PictureDecodeError> {
    let context_index = context_index.min(contexts.first.len().saturating_sub(1));
    if !cabac.decode_decision(&mut contexts.first[context_index])? {
        return Ok(0);
    }
    if !cabac.decode_decision(&mut contexts.suffix)? {
        return Ok(1);
    }
    if !cabac.decode_decision(&mut contexts.suffix)? {
        return Ok(2);
    }
    Ok(3)
}

fn decode_mb_qp_delta(
    cabac: &mut CabacDecoder<'_>,
    contexts: &mut CabacMbQpDeltaContexts,
    last_qscale_diff_nonzero: &mut bool,
    macroblock_address: u32,
    overflow_reason: &'static str,
) -> Result<i16, H264422P10PictureDecodeError> {
    let first_context = usize::from(*last_qscale_diff_nonzero);
    if !cabac.decode_decision(&mut contexts.bins[first_context])? {
        *last_qscale_diff_nonzero = false;
        return Ok(0);
    }

    let mut value = 1_i16;
    let mut context_index = 2_usize;
    while cabac.decode_decision(&mut contexts.bins[context_index])? {
        context_index = 3;
        value = value.saturating_add(1);
        if value > 512 {
            return Err(H264422P10PictureDecodeError::MacroblockSyntax {
                address: macroblock_address,
                reason: overflow_reason,
            });
        }
    }

    let delta = if value & 1 != 0 {
        (value + 1) / 2
    } else {
        -((value + 1) / 2)
    };
    *last_qscale_diff_nonzero = delta != 0;
    Ok(delta)
}

fn apply_i_mb_qp_delta(current_qp_y: &mut i16, delta: i16) {
    let qpy_min = -12_i16;
    let qpy_max = 51_i16;
    let modulus = qpy_max - qpy_min + 1;
    let mut next = (*current_qp_y + delta - qpy_min) % modulus;
    if next < 0 {
        next += modulus;
    }
    *current_qp_y = (next + qpy_min).clamp(qpy_min, qpy_max);
}

fn reconstruction_qp_from_qpy(qp_y: i16) -> u8 {
    (qp_y + 12).clamp(0, 63) as u8
}

fn predicted_intra4x4_mode(
    modes: &[Intra4x4PredictionMode; 16],
    index: usize,
    address: MacroblockAddress,
    width_in_mbs: u32,
    slice_first_mb: u32,
    stored_modes: &[Option<[Intra4x4PredictionMode; 16]>],
) -> u8 {
    let (x, y) = luma4x4_position(index);
    let left = if x > 0 {
        Some(modes[luma4x4_index(x - 1, y)] as u8)
    } else {
        stored_left_intra_mode(address, luma4x4_index(3, y), slice_first_mb, stored_modes)
    };
    let top = if y > 0 {
        Some(modes[luma4x4_index(x, y - 1)] as u8)
    } else {
        stored_top_intra_mode(
            address,
            luma4x4_index(x, 3),
            width_in_mbs,
            slice_first_mb,
            stored_modes,
        )
    };
    left.zip(top)
        .map(|(left, top)| left.min(top))
        .unwrap_or(Intra4x4PredictionMode::Dc as u8)
}

fn stored_left_intra_mode(
    address: MacroblockAddress,
    block_index: usize,
    slice_first_mb: u32,
    stored_modes: &[Option<[Intra4x4PredictionMode; 16]>],
) -> Option<u8> {
    if address.x == 0 || address.address == slice_first_mb {
        return None;
    }
    let left_address = address.address.saturating_sub(1);
    if left_address < slice_first_mb {
        return None;
    }
    stored_modes
        .get(left_address as usize)
        .copied()
        .flatten()
        .and_then(|modes| modes.get(block_index).copied())
        .map(|mode| mode as u8)
}

fn stored_top_intra_mode(
    address: MacroblockAddress,
    block_index: usize,
    width_in_mbs: u32,
    slice_first_mb: u32,
    stored_modes: &[Option<[Intra4x4PredictionMode; 16]>],
) -> Option<u8> {
    if address.y == 0 {
        return None;
    }
    let top_address = address.address.saturating_sub(width_in_mbs);
    if top_address < slice_first_mb {
        return None;
    }
    stored_modes
        .get(top_address as usize)
        .copied()
        .flatten()
        .and_then(|modes| modes.get(block_index).copied())
        .map(|mode| mode as u8)
}

fn expand_intra8x8_modes(modes: [Intra4x4PredictionMode; 4]) -> [Intra4x4PredictionMode; 16] {
    [
        modes[0], modes[0], modes[0], modes[0], modes[1], modes[1], modes[1], modes[1], modes[2],
        modes[2], modes[2], modes[2], modes[3], modes[3], modes[3], modes[3],
    ]
}

fn luma4x4_position(block_index: usize) -> (usize, usize) {
    let x = (block_index & 1) + ((block_index >> 2) & 1) * 2;
    let y = ((block_index >> 1) & 1) + ((block_index >> 3) & 1) * 2;
    (x, y)
}

fn luma4x4_index(x: usize, y: usize) -> usize {
    (x & 1) + (y & 1) * 2 + (x >> 1) * 4 + (y >> 1) * 8
}

fn luma_residual_contexts(qp_y: u8) -> CabacResidual4x4Contexts {
    let qp = i16::from(qp_y);
    CabacResidual4x4Contexts {
        coded_block_flag: CabacInitValue::new(4, 39).initialize(qp),
        significant_coeff_flag: [CabacInitValue::new(15, 33).initialize(qp); 15],
        last_significant_coeff_flag: [CabacInitValue::new(7, 54).initialize(qp); 15],
        coeff_abs_level_greater1: CabacInitValue::new(5, 45).initialize(qp),
        coeff_abs_level_greater2: CabacContext::new(0, false),
    }
}

fn top_luma_samples(frame: &DecodedFrame422P10, address: MacroblockAddress) -> Option<[u16; 16]> {
    if address.y == 0 {
        return None;
    }
    let y = address.y as usize * 16 - 1;
    let x = address.x as usize * 16;
    let mut samples = [0_u16; 16];
    for (index, sample) in samples.iter_mut().enumerate() {
        *sample = frame.y.get(x + index, y)?;
    }
    Some(samples)
}

fn left_luma_samples(frame: &DecodedFrame422P10, address: MacroblockAddress) -> Option<[u16; 16]> {
    if address.x == 0 {
        return None;
    }
    let x = address.x as usize * 16 - 1;
    let y = address.y as usize * 16;
    let mut samples = [0_u16; 16];
    for (index, sample) in samples.iter_mut().enumerate() {
        *sample = frame.y.get(x, y + index)?;
    }
    Some(samples)
}

fn frame_to_output(
    presentation_index: u64,
    frame: &DecodedFrame422P10,
) -> Result<H264422P10Frame, H264422P10PictureDecodeError> {
    let out = H264422P10Frame {
        presentation_index,
        coded_width: frame.coded_width as u32,
        coded_height: frame.coded_height as u32,
        visible_width: frame.visible_width as u32,
        visible_height: frame.visible_height as u32,
        y: plane_to_output(&frame.y),
        cb: plane_to_output(&frame.cb),
        cr: plane_to_output(&frame.cr),
    };
    out.validate_layout()?;
    Ok(out)
}

fn plane_to_output(plane: &Plane422P10) -> H264422P10Plane {
    H264422P10Plane {
        width_samples: plane.width as u32,
        height: plane.height as u32,
        stride_bytes: plane.width * 2,
        data: plane.to_le_bytes(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H264422P10SliceDecodeReport {
    pub first_macroblock: u32,
    pub reconstructed_macroblocks: u32,
    pub expected_macroblocks: u32,
    pub stop_reason: &'static str,
    pub last_macroblock: Option<u32>,
    pub last_macroblock_type: &'static str,
    pub last_coded_block_pattern: Option<u8>,
    pub cabac_bit_position: Option<usize>,
    pub cabac_payload_bits: Option<usize>,
}

#[derive(Debug)]
pub enum H264422P10PictureDecodeError {
    H264(qgs_codec_h264::H264Error),
    Profile(H264422P10Error),
    Slice(SlicePayloadError),
    Cabac(crate::cabac::CabacError),
    CabacMacroblock(CabacMacroblockError),
    CabacMotion(CabacMotionError),
    CabacResidual(CabacResidualError),
    Macroblock(MacroblockReconstructionError),
    Motion(MotionCompensationError),
    IncompletePicture {
        reconstructed_macroblocks: u32,
        expected_macroblocks: u32,
        slice_reports: Vec<H264422P10SliceDecodeReport>,
    },
    MissingDecodedPicture {
        frame_num: u16,
        poc: i32,
    },
    OutputNotReady {
        output_count: usize,
    },
    MacroblockSyntax {
        address: u32,
        reason: &'static str,
    },
    Unsupported(&'static str),
}

impl fmt::Display for H264422P10PictureDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::H264(error) => write!(f, "{error}"),
            Self::Profile(error) => write!(f, "{error}"),
            Self::Slice(error) => write!(f, "{error}"),
            Self::Cabac(error) => write!(f, "{error}"),
            Self::CabacMacroblock(error) => write!(f, "{error}"),
            Self::CabacMotion(error) => write!(f, "{error}"),
            Self::CabacResidual(error) => write!(f, "{error}"),
            Self::Macroblock(error) => write!(f, "{error}"),
            Self::Motion(error) => write!(f, "{error}"),
            Self::IncompletePicture {
                reconstructed_macroblocks,
                expected_macroblocks,
                ..
            } => write!(
                f,
                "incomplete H.264 4:2:2 10-bit picture: reconstructed {reconstructed_macroblocks}/{expected_macroblocks} macroblocks"
            ),
            Self::MissingDecodedPicture { frame_num, poc } => write!(
                f,
                "decoded H.264 picture missing from native DPB store: frame_num={frame_num} poc={poc}"
            ),
            Self::OutputNotReady { output_count } => write!(
                f,
                "H.264 access unit did not produce exactly one output picture: outputs={output_count}"
            ),
            Self::MacroblockSyntax { address, reason } => write!(
                f,
                "H.264 4:2:2 10-bit macroblock syntax boundary at {address}: {reason}"
            ),
            Self::Unsupported(reason) => write!(
                f,
                "unsupported H.264 4:2:2 10-bit picture decode path: {reason}"
            ),
        }
    }
}

impl std::error::Error for H264422P10PictureDecodeError {}

impl From<qgs_codec_h264::H264Error> for H264422P10PictureDecodeError {
    fn from(value: qgs_codec_h264::H264Error) -> Self {
        Self::H264(value)
    }
}

impl From<H264422P10Error> for H264422P10PictureDecodeError {
    fn from(value: H264422P10Error) -> Self {
        Self::Profile(value)
    }
}

impl From<SlicePayloadError> for H264422P10PictureDecodeError {
    fn from(value: SlicePayloadError) -> Self {
        Self::Slice(value)
    }
}

impl From<crate::cabac::CabacError> for H264422P10PictureDecodeError {
    fn from(value: crate::cabac::CabacError) -> Self {
        Self::Cabac(value)
    }
}

impl From<CabacMacroblockError> for H264422P10PictureDecodeError {
    fn from(value: CabacMacroblockError) -> Self {
        Self::CabacMacroblock(value)
    }
}

impl From<CabacMotionError> for H264422P10PictureDecodeError {
    fn from(value: CabacMotionError) -> Self {
        Self::CabacMotion(value)
    }
}

impl From<CabacResidualError> for H264422P10PictureDecodeError {
    fn from(value: CabacResidualError) -> Self {
        Self::CabacResidual(value)
    }
}

impl From<MacroblockReconstructionError> for H264422P10PictureDecodeError {
    fn from(value: MacroblockReconstructionError) -> Self {
        Self::Macroblock(value)
    }
}

impl From<MotionCompensationError> for H264422P10PictureDecodeError {
    fn from(value: MotionCompensationError) -> Self {
        Self::Motion(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SONY_FX6_CODED_HEIGHT;
    use crate::SONY_FX6_CODED_WIDTH;
    use crate::SONY_FX6_VISIBLE_HEIGHT;
    use crate::SONY_FX6_VISIBLE_WIDTH;

    #[test]
    fn frame_to_output_exports_owned_yuv422p10le_planes() {
        let mut frame = DecodedFrame422P10::new(
            SONY_FX6_CODED_WIDTH as usize,
            SONY_FX6_CODED_HEIGHT as usize,
            SONY_FX6_VISIBLE_WIDTH as usize,
            SONY_FX6_VISIBLE_HEIGHT as usize,
        );
        assert!(frame.y.set(0, 0, 1));
        assert!(frame.cb.set(0, 0, 2));
        assert!(frame.cr.set(0, 0, 3));

        let output = frame_to_output(7, &frame).unwrap();

        assert_eq!(output.presentation_index, 7);
        assert_eq!(output.y.data[0..2], [1, 0]);
        assert_eq!(output.cb.data[0..2], [2, 0]);
        assert_eq!(output.cr.data[0..2], [3, 0]);
        assert_eq!(
            output.owned_bytes(),
            (SONY_FX6_CODED_WIDTH * SONY_FX6_CODED_HEIGHT * 2
                + (SONY_FX6_CODED_WIDTH / 2) * SONY_FX6_CODED_HEIGHT * 2 * 2) as usize
        );
    }

    #[test]
    fn i_slice_transform_size_8x8_contexts_use_high_profile_init_values() {
        let contexts = i_slice_transform_size_8x8_contexts(26);

        assert_eq!(
            contexts.bins,
            [
                CabacInitValue::new(31, 21).initialize(26),
                CabacInitValue::new(31, 31).initialize(26),
                CabacInitValue::new(25, 50).initialize(26),
            ]
        );
    }

    #[test]
    fn transform_size_8x8_context_reads_left_and_top_flags() {
        let flags = [false, true, false, true, false, false];
        let address = MacroblockAddress {
            address: 4,
            x: 1,
            y: 1,
        };

        let (left, top) = transform_size_8x8_context(3, 0, address, &flags);

        assert!(left);
        assert!(top);
    }
}
