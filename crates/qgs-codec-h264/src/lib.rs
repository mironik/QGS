#![forbid(unsafe_code)]

use std::fmt;
use std::io;

use h264_reader::nal::pps::{PicParameterSet, SliceGroup};
use h264_reader::nal::slice::{FieldPic, SliceFamily, SliceHeader};
use h264_reader::nal::sps::{ChromaFormat, FrameMbsFlags, PicOrderCntType, Profile};
use h264_reader::nal::{Nal, RefNal, UnitType};
use h264_reader::rbsp::{BitRead, BitReaderError, Integer, Primitive};
use h264_reader::Context;
use qgs_protocol::{
    BitDepth, ChromaSubsampling, FieldOrder, H264Profile, ScanMode, VideoCodec, VideoProfile,
    VideoSurfaceDesc, VideoSurfaceFormat, VisibleRegion,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedH264AccessUnit {
    pub desc: VideoSurfaceDesc,
    pub profile: H264Profile,
    pub level_idc: u8,
    pub picture: ParsedH264Picture,
    pub slices: Vec<ParsedH264Slice>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedH264Picture {
    pub picture_width_in_mbs_minus1: u16,
    pub picture_height_in_mbs_minus1: u16,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,
    pub num_ref_frames: u8,
    pub chroma_format_idc: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: bool,
    pub direct_8x8_inference_flag: bool,
    pub log2_max_frame_num_minus4: u8,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: u32,
    pub delta_pic_order_always_zero_flag: bool,
    pub num_slice_groups_minus1: u8,
    pub slice_group_map_type: u8,
    pub slice_group_change_rate_minus1: u16,
    pub entropy_coding_mode_flag: bool,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u8,
    pub transform_8x8_mode_flag: bool,
    pub field_pic_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub pic_order_present_flag: bool,
    pub deblocking_filter_control_present_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
    pub reference_pic_flag: bool,
    pub pic_init_qp_minus26: i8,
    pub pic_init_qs_minus26: i8,
    pub chroma_qp_index_offset: i8,
    pub second_chroma_qp_index_offset: i8,
    pub frame_num: u16,
    pub top_field_order_cnt: i32,
    pub bottom_field_order_cnt: i32,
    pub scaling_list4x4: [[u8; 16]; 6],
    pub scaling_list8x8: [[u8; 64]; 2],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedH264Slice {
    pub nal_bytes: Vec<u8>,
    pub nal_ref_idc: u8,
    pub idr: bool,
    pub slice_data_bit_offset: u16,
    pub first_mb_in_slice: u16,
    pub slice_type: u8,
    pub direct_spatial_mv_pred_flag: u8,
    pub num_ref_idx_l0_active_minus1: u8,
    pub num_ref_idx_l1_active_minus1: u8,
    pub cabac_init_idc: u8,
    pub slice_qp_delta: i8,
    pub disable_deblocking_filter_idc: u8,
    pub slice_alpha_c0_offset_div2: i8,
    pub slice_beta_offset_div2: i8,
}

#[derive(Debug)]
pub enum H264Error {
    MalformedAnnexB,
    MissingSps,
    MissingPps,
    MissingSlice,
    UnsupportedFeature(&'static str),
    Parser(String),
}

impl fmt::Display for H264Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedAnnexB => write!(f, "malformed Annex B H.264 data"),
            Self::MissingSps => write!(f, "H.264 access unit is missing SPS"),
            Self::MissingPps => write!(f, "H.264 access unit is missing PPS"),
            Self::MissingSlice => write!(f, "H.264 access unit is missing a slice"),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported H.264 feature: {feature}"),
            Self::Parser(error) => write!(f, "H.264 parser error: {error}"),
        }
    }
}

impl std::error::Error for H264Error {}

pub fn parse_annex_b_access_unit(data: &[u8]) -> Result<ParsedH264AccessUnit, H264Error> {
    let nals = split_annex_b(data)?;
    let mut context = Context::new();
    let mut parsed_sps = None;
    let mut parsed_pps = None;
    let mut slices = Vec::new();

    for nal_bytes in nals {
        let nal = RefNal::new(nal_bytes, &[], true);
        let header = nal
            .header()
            .map_err(|error| H264Error::Parser(format!("{error:?}")))?;
        match header.nal_unit_type() {
            UnitType::SeqParameterSet => {
                let sps = h264_reader::nal::sps::SeqParameterSet::from_bits(nal.rbsp_bits())
                    .map_err(|error| H264Error::Parser(format!("{error:?}")))?;
                context.put_seq_param_set(sps.clone());
                parsed_sps = Some(sps);
            }
            UnitType::PicParameterSet => {
                let pps = PicParameterSet::from_bits(&context, nal.rbsp_bits())
                    .map_err(|error| H264Error::Parser(format!("{error:?}")))?;
                context.put_pic_param_set(pps.clone());
                parsed_pps = Some(pps);
            }
            UnitType::SliceLayerWithoutPartitioningIdr
            | UnitType::SliceLayerWithoutPartitioningNonIdr => {
                let mut reader = CountingBitReader::new(nal.rbsp_bits());
                let (slice_header, _, _) =
                    SliceHeader::from_bits(&context, &mut reader, header, None)
                        .map_err(|error| H264Error::Parser(format!("{error:?}")))?;
                slices.push(parsed_slice(
                    nal_bytes,
                    header.nal_ref_idc(),
                    &slice_header,
                    reader.bits_read(),
                )?);
            }
            UnitType::AccessUnitDelimiter
            | UnitType::SEI
            | UnitType::EndOfSeq
            | UnitType::EndOfStream => {}
            _ => return Err(H264Error::UnsupportedFeature("unsupported NAL unit type")),
        }
    }

    let sps = parsed_sps.ok_or(H264Error::MissingSps)?;
    let pps = parsed_pps.ok_or(H264Error::MissingPps)?;
    if slices.is_empty() {
        return Err(H264Error::MissingSlice);
    }
    if slices.iter().any(|slice| !slice.idr) {
        return Err(H264Error::UnsupportedFeature("non-IDR slices"));
    }

    validate_supported_sps_pps(&sps, &pps)?;
    let desc = surface_desc_from_sps(&sps)?;
    let picture = parsed_picture(&sps, &pps, &slices[0])?;

    Ok(ParsedH264AccessUnit {
        desc,
        profile: map_profile(sps.profile())?,
        level_idc: sps.level_idc,
        picture,
        slices,
    })
}

fn parsed_slice(
    nal_bytes: &[u8],
    nal_ref_idc: u8,
    header: &SliceHeader,
    bits_after_nal_header: u32,
) -> Result<ParsedH264Slice, H264Error> {
    if header.field_pic != FieldPic::Frame {
        return Err(H264Error::UnsupportedFeature("field pictures"));
    }
    if header.slice_type.family != SliceFamily::I {
        return Err(H264Error::UnsupportedFeature("P/B/SP/SI slices"));
    }
    let offset = 8_u32
        .checked_add(bits_after_nal_header)
        .ok_or(H264Error::UnsupportedFeature("oversized slice header"))?;
    Ok(ParsedH264Slice {
        nal_bytes: nal_bytes.to_vec(),
        nal_ref_idc,
        idr: header.idr_pic_id.is_some(),
        slice_data_bit_offset: u16::try_from(offset)
            .map_err(|_| H264Error::UnsupportedFeature("oversized slice header"))?,
        first_mb_in_slice: u16::try_from(header.first_mb_in_slice)
            .map_err(|_| H264Error::UnsupportedFeature("large first_mb_in_slice"))?,
        slice_type: 2,
        direct_spatial_mv_pred_flag: header.direct_spatial_mv_pred_flag.unwrap_or(false) as u8,
        num_ref_idx_l0_active_minus1: 0,
        num_ref_idx_l1_active_minus1: 0,
        cabac_init_idc: header.cabac_init_idc.unwrap_or(0) as u8,
        slice_qp_delta: i8::try_from(header.slice_qp_delta)
            .map_err(|_| H264Error::UnsupportedFeature("slice_qp_delta out of range"))?,
        disable_deblocking_filter_idc: header.disable_deblocking_filter_idc,
        slice_alpha_c0_offset_div2: i8::try_from(header.slice_alpha_c0_offset_div2.unwrap_or(0))
            .map_err(|_| H264Error::UnsupportedFeature("deblocking alpha out of range"))?,
        slice_beta_offset_div2: i8::try_from(header.slice_beta_offset_div2.unwrap_or(0))
            .map_err(|_| H264Error::UnsupportedFeature("deblocking beta out of range"))?,
    })
}

fn validate_supported_sps_pps(
    sps: &h264_reader::nal::sps::SeqParameterSet,
    pps: &PicParameterSet,
) -> Result<(), H264Error> {
    match sps.profile() {
        Profile::Baseline | Profile::ConstrainedBaseline | Profile::Main | Profile::High => {}
        Profile::High422 | Profile::High422Intra => {
            return Err(H264Error::UnsupportedFeature("H.264 4:2:2 profile"));
        }
        Profile::High10 | Profile::High10Intra => {
            return Err(H264Error::UnsupportedFeature("H.264 10-bit profile"));
        }
        _ => return Err(H264Error::UnsupportedFeature("H.264 profile")),
    }
    if sps.chroma_info.bit_depth_luma_minus8 != 0 || sps.chroma_info.bit_depth_chroma_minus8 != 0 {
        return Err(H264Error::UnsupportedFeature("bit depth greater than 8"));
    }
    if sps.chroma_info.chroma_format != ChromaFormat::YUV420 {
        return Err(H264Error::UnsupportedFeature("non-4:2:0 chroma"));
    }
    if sps.chroma_info.separate_colour_plane_flag {
        return Err(H264Error::UnsupportedFeature("separate colour plane"));
    }
    if !matches!(sps.frame_mbs_flags, FrameMbsFlags::Frames) {
        return Err(H264Error::UnsupportedFeature("interlaced or MBAFF stream"));
    }
    if pps.slice_groups.is_some() {
        return Err(H264Error::UnsupportedFeature("slice groups"));
    }
    Ok(())
}

fn parsed_picture(
    sps: &h264_reader::nal::sps::SeqParameterSet,
    pps: &PicParameterSet,
    first_slice: &ParsedH264Slice,
) -> Result<ParsedH264Picture, H264Error> {
    let (pic_order_cnt_type, log2_max_pic_order_cnt_lsb_minus4, delta_pic_order_always_zero_flag) =
        match sps.pic_order_cnt {
            PicOrderCntType::TypeZero {
                log2_max_pic_order_cnt_lsb_minus4,
            } => (0, u32::from(log2_max_pic_order_cnt_lsb_minus4), false),
            PicOrderCntType::TypeOne {
                delta_pic_order_always_zero_flag,
                ..
            } => (1, 0, delta_pic_order_always_zero_flag),
            PicOrderCntType::TypeTwo => (2, 0, false),
        };
    let transform_8x8_mode_flag = pps
        .extension
        .as_ref()
        .map(|extension| extension.transform_8x8_mode_flag)
        .unwrap_or(false);
    let second_chroma_qp_index_offset = pps
        .extension
        .as_ref()
        .map(|extension| extension.second_chroma_qp_index_offset)
        .unwrap_or(pps.chroma_qp_index_offset);

    Ok(ParsedH264Picture {
        picture_width_in_mbs_minus1: u16::try_from(sps.pic_width_in_mbs_minus1)
            .map_err(|_| H264Error::UnsupportedFeature("large width"))?,
        picture_height_in_mbs_minus1: u16::try_from(sps.pic_height_in_map_units_minus1)
            .map_err(|_| H264Error::UnsupportedFeature("large height"))?,
        bit_depth_luma_minus8: sps.chroma_info.bit_depth_luma_minus8,
        bit_depth_chroma_minus8: sps.chroma_info.bit_depth_chroma_minus8,
        num_ref_frames: u8::try_from(sps.max_num_ref_frames)
            .map_err(|_| H264Error::UnsupportedFeature("too many reference frames"))?,
        chroma_format_idc: sps.chroma_info.chroma_format.to_u32(),
        gaps_in_frame_num_value_allowed_flag: sps.gaps_in_frame_num_value_allowed_flag,
        frame_mbs_only_flag: true,
        mb_adaptive_frame_field_flag: false,
        direct_8x8_inference_flag: sps.direct_8x8_inference_flag,
        log2_max_frame_num_minus4: sps.log2_max_frame_num_minus4,
        pic_order_cnt_type,
        log2_max_pic_order_cnt_lsb_minus4,
        delta_pic_order_always_zero_flag,
        num_slice_groups_minus1: match &pps.slice_groups {
            None => 0,
            Some(SliceGroup::Interleaved { .. })
            | Some(SliceGroup::Dispersed { .. })
            | Some(SliceGroup::ForegroundAndLeftover { .. })
            | Some(SliceGroup::Changing { .. })
            | Some(SliceGroup::ExplicitAssignment { .. }) => {
                return Err(H264Error::UnsupportedFeature("slice groups"));
            }
        },
        slice_group_map_type: 0,
        slice_group_change_rate_minus1: 0,
        entropy_coding_mode_flag: pps.entropy_coding_mode_flag,
        weighted_pred_flag: pps.weighted_pred_flag,
        weighted_bipred_idc: pps.weighted_bipred_idc,
        transform_8x8_mode_flag,
        field_pic_flag: false,
        constrained_intra_pred_flag: pps.constrained_intra_pred_flag,
        pic_order_present_flag: pps.bottom_field_pic_order_in_frame_present_flag,
        deblocking_filter_control_present_flag: pps.deblocking_filter_control_present_flag,
        redundant_pic_cnt_present_flag: pps.redundant_pic_cnt_present_flag,
        reference_pic_flag: first_slice.nal_ref_idc != 0,
        pic_init_qp_minus26: i8::try_from(pps.pic_init_qp_minus26)
            .map_err(|_| H264Error::UnsupportedFeature("pic_init_qp_minus26 out of range"))?,
        pic_init_qs_minus26: i8::try_from(pps.pic_init_qs_minus26)
            .map_err(|_| H264Error::UnsupportedFeature("pic_init_qs_minus26 out of range"))?,
        chroma_qp_index_offset: i8::try_from(pps.chroma_qp_index_offset)
            .map_err(|_| H264Error::UnsupportedFeature("chroma qp offset out of range"))?,
        second_chroma_qp_index_offset: i8::try_from(second_chroma_qp_index_offset)
            .map_err(|_| H264Error::UnsupportedFeature("second chroma qp offset out of range"))?,
        frame_num: 0,
        top_field_order_cnt: 0,
        bottom_field_order_cnt: 0,
        scaling_list4x4: [[16; 16]; 6],
        scaling_list8x8: [[16; 64]; 2],
    })
}

fn surface_desc_from_sps(
    sps: &h264_reader::nal::sps::SeqParameterSet,
) -> Result<VideoSurfaceDesc, H264Error> {
    let (width, height) = sps
        .pixel_dimensions()
        .map_err(|error| H264Error::Parser(format!("{error:?}")))?;
    let desc = VideoSurfaceDesc {
        coded_width: width,
        coded_height: height,
        visible_region: VisibleRegion {
            x: 0,
            y: 0,
            width,
            height,
        },
        format: VideoSurfaceFormat::Nv12,
        bit_depth: BitDepth::new(8).map_err(|error| H264Error::Parser(error.to_string()))?,
        chroma: ChromaSubsampling::Cs420,
        scan_mode: ScanMode::Progressive,
        field_order: FieldOrder::Unknown,
    };
    desc.validate()
        .map_err(|error| H264Error::Parser(error.to_string()))?;
    Ok(desc)
}

fn map_profile(profile: Profile) -> Result<H264Profile, H264Error> {
    match profile {
        Profile::Baseline | Profile::ConstrainedBaseline => Ok(H264Profile::Baseline),
        Profile::Main => Ok(H264Profile::Main),
        Profile::High | Profile::ProgressiveHigh | Profile::ConstrainedHigh => {
            Ok(H264Profile::High)
        }
        Profile::High422 | Profile::High422Intra => Ok(H264Profile::High422),
        Profile::High10 | Profile::High10Intra => {
            Err(H264Error::UnsupportedFeature("H.264 10-bit profile"))
        }
        _ => Err(H264Error::UnsupportedFeature("H.264 profile")),
    }
}

pub fn decoder_profile(parsed: &ParsedH264AccessUnit) -> VideoProfile {
    VideoProfile::H264(parsed.profile)
}

pub const fn decoder_codec() -> VideoCodec {
    VideoCodec::H264
}

fn split_annex_b(data: &[u8]) -> Result<Vec<&[u8]>, H264Error> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i..].starts_with(&[0, 0, 1]) {
            starts.push((i, 3));
            i += 3;
        } else if i + 4 <= data.len() && data[i..].starts_with(&[0, 0, 0, 1]) {
            starts.push((i, 4));
            i += 4;
        } else {
            i += 1;
        }
    }
    if starts.is_empty() {
        return Err(H264Error::MalformedAnnexB);
    }
    let mut nals = Vec::new();
    for (index, (start, prefix_len)) in starts.iter().copied().enumerate() {
        let payload_start = start + prefix_len;
        let payload_end = starts
            .get(index + 1)
            .map(|(next, _)| *next)
            .unwrap_or(data.len());
        if payload_start >= payload_end {
            return Err(H264Error::MalformedAnnexB);
        }
        nals.push(&data[payload_start..payload_end]);
    }
    Ok(nals)
}

struct CountingBitReader<R> {
    inner: R,
    bits_read: u32,
}

impl<R> CountingBitReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            bits_read: 0,
        }
    }

    const fn bits_read(&self) -> u32 {
        self.bits_read
    }

    fn add_bits(&mut self, bits: u32) -> Result<(), BitReaderError> {
        self.bits_read = self
            .bits_read
            .checked_add(bits)
            .ok_or_else(|| io_error("bit counter overflow"))?;
        Ok(())
    }
}

impl<R: BitRead> BitRead for CountingBitReader<R> {
    fn read_ue(&mut self, name: &'static str) -> Result<u32, BitReaderError> {
        let mut zeros = 0_u32;
        while !self.read_bit(name)? {
            zeros += 1;
            if zeros > 31 {
                return Err(BitReaderError::ExpGolombTooLarge(name));
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let suffix: u32 = self.read_var(zeros, name)?;
        Ok((1 << zeros) - 1 + suffix)
    }

    fn read_se(&mut self, name: &'static str) -> Result<i32, BitReaderError> {
        let code_num = self.read_ue(name)?;
        if code_num % 2 == 0 {
            Ok(-i32::try_from(code_num / 2).unwrap_or(i32::MAX))
        } else {
            Ok(i32::try_from(code_num.div_ceil(2)).unwrap_or(i32::MAX))
        }
    }

    fn read_bit(&mut self, name: &'static str) -> Result<bool, BitReaderError> {
        let value = self.inner.read_bit(name)?;
        self.add_bits(1)?;
        Ok(value)
    }

    fn read<const BITS: u32, I: Integer>(
        &mut self,
        name: &'static str,
    ) -> Result<I, BitReaderError> {
        let value = self.inner.read::<BITS, I>(name)?;
        self.add_bits(BITS)?;
        Ok(value)
    }

    fn read_var<I: Integer>(
        &mut self,
        bit_count: u32,
        name: &'static str,
    ) -> Result<I, BitReaderError> {
        let value = self.inner.read_var(bit_count, name)?;
        self.add_bits(bit_count)?;
        Ok(value)
    }

    fn read_to<V: Primitive>(&mut self, name: &'static str) -> Result<V, BitReaderError> {
        let value = self.inner.read_to(name)?;
        self.add_bits((std::mem::size_of::<V>() * 8) as u32)?;
        Ok(value)
    }

    fn skip(&mut self, bit_count: u32, name: &'static str) -> Result<(), BitReaderError> {
        self.inner.skip(bit_count, name)?;
        self.add_bits(bit_count)?;
        Ok(())
    }

    fn byte_aligned(&self) -> bool {
        self.inner.byte_aligned()
    }

    fn has_more_rbsp_data(&mut self, name: &'static str) -> Result<bool, BitReaderError> {
        self.inner.has_more_rbsp_data(name)
    }

    fn finish_rbsp(self) -> Result<(), BitReaderError> {
        self.inner.finish_rbsp()
    }

    fn finish_sei_payload(self) -> Result<(), BitReaderError> {
        self.inner.finish_sei_payload()
    }
}

fn io_error(message: &'static str) -> BitReaderError {
    BitReaderError::ReaderError(
        "qgs-codec-h264",
        io::Error::new(io::ErrorKind::InvalidData, message),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/h264/idr-64x64-baseline.h264");

    #[test]
    fn parses_fixture_access_unit() {
        let parsed = parse_annex_b_access_unit(FIXTURE).expect("fixture parses");

        assert_eq!(parsed.profile, H264Profile::Baseline);
        assert_eq!(parsed.desc.coded_width, 64);
        assert_eq!(parsed.desc.coded_height, 64);
        assert_eq!(parsed.desc.format, VideoSurfaceFormat::Nv12);
        assert_eq!(parsed.desc.bit_depth.get(), 8);
        assert_eq!(parsed.desc.chroma, ChromaSubsampling::Cs420);
        assert_eq!(parsed.desc.scan_mode, ScanMode::Progressive);
        assert_eq!(parsed.slices.len(), 1);
        assert!(parsed.slices[0].idr);
        assert!(parsed.slices[0].slice_data_bit_offset > 8);
    }

    #[test]
    fn rejects_malformed_annex_b() {
        assert!(matches!(
            parse_annex_b_access_unit(&[1, 2, 3]),
            Err(H264Error::MalformedAnnexB)
        ));
    }
}
