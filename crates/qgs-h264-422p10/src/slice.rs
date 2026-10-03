use std::fmt;

use qgs_codec_h264::{H264SliceKind, ParsedH264AccessUnit, ParsedH264Slice};

use crate::bytestream::{rbsp_from_ebsp, ByteStreamError};
use crate::cabac::{CabacDecoder, CabacError, CabacInitValue};
use crate::cabac_macroblock::{
    decode_i_slice_macroblock_type, CabacISliceMbTypeContexts, CabacMacroblockError,
};
use crate::macroblock_type::ISliceMacroblockType;
use crate::macroblock_type::{MacroblockAddress, MacroblockGrid, MacroblockTypeError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntropyCodingMode {
    Cavlc,
    Cabac,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlicePayload {
    pub entropy: EntropyCodingMode,
    pub rbsp: Vec<u8>,
    pub payload_bit_offset: usize,
    pub payload_byte_offset: usize,
    pub cabac_alignment_bits: usize,
    pub cabac_alignment_all_ones: bool,
    pub cabac_init_idc: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SliceLumaQp(pub i16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ISliceMbTypeBranch {
    IntraNxN,
    Intra16x16OrPcm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SliceMacroblockCursor {
    grid: MacroblockGrid,
    next_address: u32,
}

impl SliceMacroblockCursor {
    pub fn new(
        parsed: &ParsedH264AccessUnit,
        slice: &ParsedH264Slice,
    ) -> Result<Self, SlicePayloadError> {
        let grid = MacroblockGrid::new(parsed.desc.coded_width, parsed.desc.coded_height)?;
        let next_address = u32::from(slice.first_mb_in_slice);
        if next_address >= grid.macroblock_count() {
            return Err(SlicePayloadError::MacroblockAddressOutOfRange);
        }
        Ok(Self { grid, next_address })
    }

    pub fn next_macroblock(&mut self) -> Option<MacroblockAddress> {
        let address = self.grid.address(self.next_address)?;
        self.next_address = self.next_address.saturating_add(1);
        Some(address)
    }
}

impl SlicePayload {
    pub fn payload_bytes(&self) -> &[u8] {
        &self.rbsp[self.payload_byte_offset.min(self.rbsp.len())..]
    }

    pub fn cabac_payload_bytes(&self) -> Result<&[u8], SlicePayloadError> {
        if self.entropy != EntropyCodingMode::Cabac {
            return Err(SlicePayloadError::WrongEntropyMode);
        }
        Ok(self.payload_bytes())
    }
}

#[derive(Debug)]
pub enum SlicePayloadError {
    MissingNalHeader,
    SliceDataOffsetBeforePayload,
    SliceDataOffsetOutOfRange,
    WrongEntropyMode,
    UnsupportedSliceKind,
    MacroblockAddressOutOfRange,
    ByteStream(ByteStreamError),
    Cabac(CabacError),
    CabacMacroblock(CabacMacroblockError),
    MacroblockType(MacroblockTypeError),
}

impl fmt::Display for SlicePayloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNalHeader => write!(f, "slice NAL is missing its header byte"),
            Self::SliceDataOffsetBeforePayload => {
                write!(f, "slice data offset points before RBSP payload")
            }
            Self::SliceDataOffsetOutOfRange => write!(f, "slice data offset points past RBSP"),
            Self::WrongEntropyMode => write!(f, "slice payload is not CABAC-coded"),
            Self::UnsupportedSliceKind => {
                write!(f, "slice kind is not supported by this syntax decoder")
            }
            Self::MacroblockAddressOutOfRange => {
                write!(f, "slice macroblock address is out of range")
            }
            Self::ByteStream(error) => write!(f, "{error}"),
            Self::Cabac(error) => write!(f, "{error}"),
            Self::CabacMacroblock(error) => write!(f, "{error}"),
            Self::MacroblockType(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SlicePayloadError {}

impl From<ByteStreamError> for SlicePayloadError {
    fn from(value: ByteStreamError) -> Self {
        Self::ByteStream(value)
    }
}

impl From<CabacError> for SlicePayloadError {
    fn from(value: CabacError) -> Self {
        Self::Cabac(value)
    }
}

impl From<CabacMacroblockError> for SlicePayloadError {
    fn from(value: CabacMacroblockError) -> Self {
        Self::CabacMacroblock(value)
    }
}

impl From<MacroblockTypeError> for SlicePayloadError {
    fn from(value: MacroblockTypeError) -> Self {
        Self::MacroblockType(value)
    }
}

pub fn slice_payload_from_parsed(
    access_unit: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
) -> Result<SlicePayload, SlicePayloadError> {
    let (_nal_header, ebsp) = slice
        .nal_bytes
        .split_first()
        .ok_or(SlicePayloadError::MissingNalHeader)?;
    let rbsp = rbsp_from_ebsp(ebsp)?;
    let payload_bit_offset = usize::from(slice.slice_data_bit_offset)
        .checked_sub(8)
        .ok_or(SlicePayloadError::SliceDataOffsetBeforePayload)?;
    if payload_bit_offset > rbsp.len().saturating_mul(8) {
        return Err(SlicePayloadError::SliceDataOffsetOutOfRange);
    }
    let payload_byte_offset = payload_bit_offset.div_ceil(8);
    let cabac_alignment_bits = payload_byte_offset
        .saturating_mul(8)
        .saturating_sub(payload_bit_offset);
    let cabac_alignment_all_ones = (0..cabac_alignment_bits).all(|index| {
        rbsp_bit(&rbsp, payload_bit_offset + index)
            .map(|bit| bit)
            .unwrap_or(false)
    });
    Ok(SlicePayload {
        entropy: if access_unit.picture.entropy_coding_mode_flag {
            EntropyCodingMode::Cabac
        } else {
            EntropyCodingMode::Cavlc
        },
        rbsp,
        payload_bit_offset,
        payload_byte_offset,
        cabac_alignment_bits,
        cabac_alignment_all_ones,
        cabac_init_idc: slice.cabac_init_idc,
    })
}

pub fn slice_luma_qp(parsed: &ParsedH264AccessUnit, slice: &ParsedH264Slice) -> SliceLumaQp {
    SliceLumaQp(
        26 + i16::from(parsed.picture.pic_init_qp_minus26) + i16::from(slice.slice_qp_delta),
    )
}

fn rbsp_bit(rbsp: &[u8], bit_offset: usize) -> Option<bool> {
    let byte = *rbsp.get(bit_offset / 8)?;
    let bit = 7 - (bit_offset % 8);
    Some(((byte >> bit) & 1) != 0)
}

pub fn decode_i_slice_first_mb_type_branch(
    parsed: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
) -> Result<ISliceMbTypeBranch, SlicePayloadError> {
    if slice.kind != H264SliceKind::I {
        return Err(SlicePayloadError::UnsupportedSliceKind);
    }
    let payload = slice_payload_from_parsed(parsed, slice)?;
    let payload = payload.cabac_payload_bytes()?;
    let mut decoder = CabacDecoder::new(payload)?;
    let SliceLumaQp(slice_qp_y) = slice_luma_qp(parsed, slice);
    let mut mb_type_context = CabacInitValue::new(20, -15).initialize(slice_qp_y);
    if decoder.decode_decision(&mut mb_type_context)? {
        Ok(ISliceMbTypeBranch::Intra16x16OrPcm)
    } else {
        Ok(ISliceMbTypeBranch::IntraNxN)
    }
}

pub fn decode_i_slice_first_macroblock_type(
    parsed: &ParsedH264AccessUnit,
    slice: &ParsedH264Slice,
) -> Result<ISliceMacroblockType, SlicePayloadError> {
    if slice.kind != H264SliceKind::I {
        return Err(SlicePayloadError::UnsupportedSliceKind);
    }
    let payload = slice_payload_from_parsed(parsed, slice)?;
    let payload = payload.cabac_payload_bytes()?;
    let mut decoder = CabacDecoder::new(payload)?;
    let SliceLumaQp(slice_qp_y) = slice_luma_qp(parsed, slice);
    let flat_context = CabacInitValue::new(20, -15).initialize(slice_qp_y);
    let mut contexts = CabacISliceMbTypeContexts {
        branch: [flat_context; 3],
        suffix: [flat_context; 5],
    };
    Ok(decode_i_slice_macroblock_type(&mut decoder, &mut contexts)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qgs_codec_h264::H264DecoderState;

    const PROFESSIONAL_LONG_GOP_FIXTURE: &[u8] =
        include_bytes!("../../../tests/fixtures/h264/professional-422-10bit-long-gop-128x72.h264");

    #[test]
    fn extracts_cabac_slice_payload_from_professional_422_fixture() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();
        let payload = slice_payload_from_parsed(&parsed, &parsed.slices[0]).unwrap();

        assert_eq!(payload.entropy, EntropyCodingMode::Cabac);
        assert_eq!(payload.cabac_init_idc, 0);
        assert!(payload.payload_bit_offset > 0);
        assert!(payload.payload_byte_offset * 8 >= payload.payload_bit_offset);
        assert!(payload.cabac_alignment_bits < 8);
        assert_eq!(
            payload.payload_byte_offset * 8 - payload.payload_bit_offset,
            payload.cabac_alignment_bits
        );
        assert!(!payload.cabac_payload_bytes().unwrap().is_empty());
    }

    #[test]
    fn reports_cabac_alignment_bits_between_slice_header_and_payload() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();
        let payload = slice_payload_from_parsed(&parsed, &parsed.slices[0]).unwrap();

        assert!(payload.cabac_alignment_bits < 8);
        if payload.cabac_alignment_bits > 0 {
            assert!(payload.cabac_alignment_all_ones);
        }
    }

    #[test]
    fn computes_slice_luma_qp_from_picture_and_slice_header() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();

        assert_eq!(
            slice_luma_qp(&parsed, &parsed.slices[0]),
            SliceLumaQp(
                26 + i16::from(parsed.picture.pic_init_qp_minus26)
                    + i16::from(parsed.slices[0].slice_qp_delta)
            )
        );
    }

    #[test]
    fn decodes_first_i_slice_mb_type_branch_from_cabac_payload() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();

        if parsed.slices[0].kind == H264SliceKind::I {
            let branch = decode_i_slice_first_mb_type_branch(&parsed, &parsed.slices[0]).unwrap();
            assert!(matches!(
                branch,
                ISliceMbTypeBranch::IntraNxN | ISliceMbTypeBranch::Intra16x16OrPcm
            ));
        }
    }

    #[test]
    fn decodes_first_i_slice_macroblock_type_from_cabac_payload() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();

        if parsed.slices[0].kind == H264SliceKind::I {
            let mb_type = decode_i_slice_first_macroblock_type(&parsed, &parsed.slices[0]).unwrap();
            assert!(matches!(
                mb_type,
                ISliceMacroblockType::IntraNxN
                    | ISliceMacroblockType::Intra16x16 { .. }
                    | ISliceMacroblockType::Pcm
            ));
        }
    }

    #[test]
    fn slice_macroblock_cursor_starts_at_first_mb_in_slice() {
        let access_units = split_access_units_for_test(PROFESSIONAL_LONG_GOP_FIXTURE);
        let mut state = H264DecoderState::new();
        let parsed = state.parse_access_unit(&access_units[0]).unwrap();
        let mut cursor = SliceMacroblockCursor::new(&parsed, &parsed.slices[0]).unwrap();

        let first = cursor.next_macroblock().unwrap();
        assert_eq!(first.address, u32::from(parsed.slices[0].first_mb_in_slice));
        assert_eq!(cursor.next_macroblock().unwrap().address, first.address + 1);
    }

    fn split_access_units_for_test(data: &[u8]) -> Vec<Vec<u8>> {
        let mut starts = Vec::new();
        let mut index = 0_usize;
        while index + 3 <= data.len() {
            let start_code_len = if data[index..].starts_with(&[0, 0, 1]) {
                3
            } else if data[index..].starts_with(&[0, 0, 0, 1]) {
                4
            } else {
                index += 1;
                continue;
            };
            starts.push(index);
            index += start_code_len;
        }
        starts.sort_unstable();
        starts.dedup();
        let nals = starts
            .iter()
            .enumerate()
            .filter_map(|(position, start)| {
                let end = starts.get(position + 1).copied().unwrap_or(data.len());
                let nal_start = if data[*start..].starts_with(&[0, 0, 0, 1]) {
                    start + 4
                } else {
                    start + 3
                };
                let nal = data[nal_start..end].trim_ascii_end();
                (!nal.is_empty()).then_some(nal)
            })
            .collect::<Vec<_>>();

        let mut access_units = Vec::new();
        let mut current = Vec::new();
        let mut seen_vcl = false;
        for nal in nals {
            let nal_type = nal[0] & 0x1f;
            let is_vcl = nal_type == 1 || nal_type == 5;
            if is_vcl && seen_vcl && !current.is_empty() {
                access_units.push(std::mem::take(&mut current));
            }
            current.extend_from_slice(&[0, 0, 0, 1]);
            current.extend_from_slice(nal);
            if is_vcl {
                seen_vcl = true;
            }
        }
        if !current.is_empty() {
            access_units.push(current);
        }
        access_units
    }
}
