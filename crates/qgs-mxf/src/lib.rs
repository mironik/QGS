#![forbid(unsafe_code)]

use std::fmt;
use std::fs;
use std::path::Path;

#[cfg(test)]
use qgs_codec_h264::parse_annex_b_access_unit;
use qgs_codec_h264::{H264DecoderState, H264Error, H264SliceKind};
use qgs_protocol::{ChromaSubsampling, H264Profile, VideoCodec, VideoSurfaceDesc};

pub const MAX_MXF_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_KLV_VALUE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_KLV_COUNT: usize = 8192;
pub const MAX_INDEX_ENTRIES: usize = 4096;
pub const MAX_ACCESS_UNIT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TRACK_COUNT: usize = 64;

const KLV_KEY_LEN: usize = 16;
const FILLER_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x03, 0x01, 0x02, 0x10, 0x01, 0x00, 0x00, 0x00,
]);
const HEADER_PARTITION_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x02, 0x04, 0x00,
]);
const BODY_PARTITION_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x03, 0x04, 0x00,
]);
const FOOTER_PARTITION_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x04, 0x04, 0x00,
]);
const H264_ESSENCE_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x02, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x15, 0x01, 0x05, 0x00,
]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ul(pub [u8; 16]);

impl fmt::Display for Ul {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

impl Rational {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, MxfError> {
        if denominator == 0 {
            return Err(MxfError::ZeroRationalDenominator);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timecode {
    pub start_frame: i64,
    pub edit_rate: Rational,
    pub drop_frame: bool,
}

impl Timecode {
    pub fn new(start_frame: i64, edit_rate: Rational, drop_frame: bool) -> Result<Self, MxfError> {
        if start_frame < 0 {
            return Err(MxfError::InvalidTimecode);
        }
        Ok(Self {
            start_frame,
            edit_rate,
            drop_frame,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TrackId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackKind {
    Video,
    Audio,
    Timecode,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexSource {
    MxfProvided,
    QgsDerived,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RandomAccess {
    Yes,
    No,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MxfTrack {
    pub id: TrackId,
    pub track_number: Option<u32>,
    pub kind: TrackKind,
    pub edit_rate: Option<Rational>,
    pub video: Option<VideoEssenceDescriptor>,
    pub audio: Option<AudioEssenceDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoEssenceDescriptor {
    pub codec: VideoCodec,
    pub coded_width: u32,
    pub coded_height: u32,
    pub display_width: u32,
    pub display_height: u32,
    pub bit_depth: u8,
    pub chroma: ChromaSubsampling,
    pub essence_container: Option<Ul>,
    pub compression: Option<Ul>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioEssenceDescriptor {
    pub essence: Option<Ul>,
    pub channels: Option<u16>,
    pub sample_rate: Option<Rational>,
    pub bit_depth: Option<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoIndexEntry {
    pub track_id: TrackId,
    pub edit_unit: u64,
    pub presentation_position: u64,
    pub file_offset: u64,
    pub payload_offset: u64,
    pub payload_len: u64,
    pub random_access: RandomAccess,
    pub source: IndexSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaIndex {
    pub video: Vec<VideoIndexEntry>,
}

impl MediaIndex {
    pub fn new(video: Vec<VideoIndexEntry>) -> Result<Self, MxfError> {
        validate_index_count(video.len())?;
        Ok(Self { video })
    }

    pub fn video_entry(&self, index: usize) -> Option<&VideoIndexEntry> {
        self.video.get(index)
    }

    pub fn nearest_random_access_before(&self, edit_unit: u64) -> Option<&VideoIndexEntry> {
        self.video
            .iter()
            .filter(|entry| {
                entry.presentation_position <= edit_unit
                    && matches!(entry.random_access, RandomAccess::Yes)
            })
            .max_by_key(|entry| entry.presentation_position)
    }
}

pub fn validate_partition_offset(offset: u64, file_len: u64) -> Result<(), MxfError> {
    if offset >= file_len {
        Err(MxfError::InvalidPartitionOffset)
    } else {
        Ok(())
    }
}

pub fn validate_batch_count(count: usize) -> Result<(), MxfError> {
    if count > MAX_TRACK_COUNT {
        Err(MxfError::ExcessiveBatchCount)
    } else {
        Ok(())
    }
}

pub fn validate_index_count(count: usize) -> Result<(), MxfError> {
    if count > MAX_INDEX_ENTRIES {
        Err(MxfError::ExcessiveIndexEntries {
            count,
            max: MAX_INDEX_ENTRIES,
        })
    } else {
        Ok(())
    }
}

pub fn validate_track_reference(track_id: TrackId, tracks: &[MxfTrack]) -> Result<(), MxfError> {
    if tracks.iter().any(|track| track.id == track_id) {
        Ok(())
    } else {
        Err(MxfError::InvalidTrackReference)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSource {
    pub duration: Option<u64>,
    pub edit_rate: Option<Rational>,
    pub timecode: Option<Timecode>,
    pub tracks: Vec<MxfTrack>,
    pub index: MediaIndex,
    pub klv_count: usize,
    pub partitions: Vec<PartitionInfo>,
}

impl MediaSource {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MxfError> {
        let bytes = fs::read(path)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, MxfError> {
        if bytes.len() as u64 > MAX_MXF_FILE_BYTES {
            return Err(MxfError::FileTooLarge {
                len: bytes.len() as u64,
                max: MAX_MXF_FILE_BYTES,
            });
        }
        let triplets = scan_klv(bytes)?;
        if triplets.len() > MAX_KLV_COUNT {
            return Err(MxfError::ExcessiveKlvCount {
                count: triplets.len(),
                max: MAX_KLV_COUNT,
            });
        }
        let partitions = triplets
            .iter()
            .filter_map(|triplet| match triplet.key {
                HEADER_PARTITION_KEY => Some(PartitionInfo {
                    kind: PartitionKind::Header,
                    offset: triplet.offset,
                }),
                BODY_PARTITION_KEY => Some(PartitionInfo {
                    kind: PartitionKind::Body,
                    offset: triplet.offset,
                }),
                FOOTER_PARTITION_KEY => Some(PartitionInfo {
                    kind: PartitionKind::Footer,
                    offset: triplet.offset,
                }),
                _ => None,
            })
            .collect::<Vec<_>>();
        if partitions.is_empty() {
            return Err(MxfError::MissingHeaderPartition);
        }

        let mut video = Vec::new();
        let mut h264_state = H264DecoderState::new();
        let mut first_desc = None;
        let mut first_profile = None;
        for triplet in triplets
            .iter()
            .filter(|triplet| triplet.key == H264_ESSENCE_KEY)
        {
            if video.len() >= MAX_INDEX_ENTRIES {
                return Err(MxfError::ExcessiveIndexEntries {
                    count: video.len() + 1,
                    max: MAX_INDEX_ENTRIES,
                });
            }
            if triplet.value_len > MAX_ACCESS_UNIT_BYTES as u64 {
                return Err(MxfError::AccessUnitTooLarge {
                    len: triplet.value_len,
                    max: MAX_ACCESS_UNIT_BYTES,
                });
            }
            let payload = triplet.value(bytes)?;
            let parsed = h264_state
                .parse_access_unit(payload)
                .map_err(MxfError::H264)?;
            let random_access = random_access_from_parsed(&parsed);
            if first_desc.is_none() {
                first_desc = Some(parsed.desc.clone());
                first_profile = Some(parsed.profile);
            }
            let _update = h264_state.finish_picture(&parsed).map_err(MxfError::H264)?;
            let edit_unit = video.len() as u64;
            video.push(VideoIndexEntry {
                track_id: TrackId(1),
                edit_unit,
                presentation_position: edit_unit,
                file_offset: triplet.offset,
                payload_offset: triplet.value_offset,
                payload_len: triplet.value_len,
                random_access,
                source: IndexSource::QgsDerived,
            });
        }
        if video.is_empty() {
            return Err(MxfError::NoSupportedEssence);
        }
        let video_desc = descriptor_from_h264(
            &first_desc.ok_or(MxfError::NoSupportedEssence)?,
            first_profile.ok_or(MxfError::NoSupportedEssence)?,
        );
        let duration = Some(video.len() as u64);
        let edit_rate = Some(Rational::new(25, 1)?);
        let timecode = Some(Timecode::new(0, Rational::new(25, 1)?, false)?);
        let tracks = vec![MxfTrack {
            id: TrackId(1),
            track_number: None,
            kind: TrackKind::Video,
            edit_rate,
            video: Some(video_desc),
            audio: None,
        }];
        validate_batch_count(tracks.len())?;
        let index = MediaIndex::new(video)?;
        Ok(Self {
            duration,
            edit_rate,
            timecode,
            tracks,
            index,
            klv_count: triplets.len(),
            partitions,
        })
    }

    pub fn extract_video_access_unit(
        &self,
        bytes: &[u8],
        entry_index: usize,
    ) -> Result<Vec<u8>, MxfError> {
        let entry = self
            .index
            .video_entry(entry_index)
            .ok_or(MxfError::InvalidEssenceOffset)?;
        let end = entry
            .payload_offset
            .checked_add(entry.payload_len)
            .ok_or(MxfError::OffsetOverflow)?;
        if end > bytes.len() as u64 {
            return Err(MxfError::KlvValueBeyondEof);
        }
        Ok(bytes[entry.payload_offset as usize..end as usize].to_vec())
    }
}

impl VideoIndexEntry {
    pub fn payload<'a>(&self, bytes: &'a [u8]) -> Result<&'a [u8], MxfError> {
        let end = self
            .payload_offset
            .checked_add(self.payload_len)
            .ok_or(MxfError::OffsetOverflow)?;
        if end > bytes.len() as u64 {
            return Err(MxfError::KlvValueBeyondEof);
        }
        Ok(&bytes[self.payload_offset as usize..end as usize])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionInfo {
    pub kind: PartitionKind,
    pub offset: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PartitionKind {
    Header,
    Body,
    Footer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KlvTriplet {
    key: Ul,
    offset: u64,
    value_offset: u64,
    value_len: u64,
}

impl KlvTriplet {
    fn value<'a>(&self, bytes: &'a [u8]) -> Result<&'a [u8], MxfError> {
        let end = self
            .value_offset
            .checked_add(self.value_len)
            .ok_or(MxfError::OffsetOverflow)?;
        if end > bytes.len() as u64 {
            return Err(MxfError::KlvValueBeyondEof);
        }
        Ok(&bytes[self.value_offset as usize..end as usize])
    }
}

#[derive(Debug)]
pub enum MxfError {
    Io(std::io::Error),
    FileTooLarge { len: u64, max: u64 },
    TruncatedKlvKey,
    MalformedBerLength,
    BerLengthOverflow,
    KlvValueTooLarge { len: u64, max: u64 },
    KlvValueBeyondEof,
    OffsetOverflow,
    MissingHeaderPartition,
    NoSupportedEssence,
    ExcessiveKlvCount { count: usize, max: usize },
    ExcessiveIndexEntries { count: usize, max: usize },
    InvalidPartitionOffset,
    ExcessiveBatchCount,
    InvalidTrackReference,
    InvalidEssenceOffset,
    TruncatedIndexTable,
    ZeroRationalDenominator,
    InvalidTimecode,
    AccessUnitTooLarge { len: u64, max: usize },
    H264(H264Error),
}

impl fmt::Display for MxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::FileTooLarge { len, max } => {
                write!(f, "MXF file is too large: {len} > {max}")
            }
            Self::TruncatedKlvKey => write!(f, "truncated KLV key"),
            Self::MalformedBerLength => write!(f, "malformed BER length"),
            Self::BerLengthOverflow => write!(f, "BER length overflow"),
            Self::KlvValueTooLarge { len, max } => {
                write!(f, "KLV value is too large: {len} > {max}")
            }
            Self::KlvValueBeyondEof => write!(f, "KLV value extends beyond EOF"),
            Self::OffsetOverflow => write!(f, "file offset arithmetic overflow"),
            Self::MissingHeaderPartition => write!(f, "missing MXF header partition"),
            Self::NoSupportedEssence => write!(f, "no supported essence found"),
            Self::ExcessiveKlvCount { count, max } => {
                write!(f, "too many KLV triplets: {count} > {max}")
            }
            Self::ExcessiveIndexEntries { count, max } => {
                write!(f, "too many index entries: {count} > {max}")
            }
            Self::InvalidPartitionOffset => write!(f, "invalid partition offset"),
            Self::ExcessiveBatchCount => write!(f, "excessive MXF batch count"),
            Self::InvalidTrackReference => write!(f, "invalid track reference"),
            Self::InvalidEssenceOffset => write!(f, "invalid essence offset"),
            Self::TruncatedIndexTable => write!(f, "truncated index table"),
            Self::ZeroRationalDenominator => write!(f, "zero rational denominator"),
            Self::InvalidTimecode => write!(f, "invalid timecode metadata"),
            Self::AccessUnitTooLarge { len, max } => {
                write!(f, "compressed access unit is too large: {len} > {max}")
            }
            Self::H264(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for MxfError {}

impl From<std::io::Error> for MxfError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn scan_klv(bytes: &[u8]) -> Result<Vec<KlvTriplet>, MxfError> {
    let mut triplets = Vec::new();
    let mut offset = 0_u64;
    while (offset as usize) < bytes.len() {
        let key_start = offset as usize;
        if bytes.len() - key_start < KLV_KEY_LEN {
            if bytes[key_start..].iter().all(|byte| *byte == 0) {
                break;
            }
            return Err(MxfError::TruncatedKlvKey);
        }
        let mut key = [0_u8; KLV_KEY_LEN];
        key.copy_from_slice(&bytes[key_start..key_start + KLV_KEY_LEN]);
        let (value_len, ber_width) = decode_ber_length(bytes, key_start + KLV_KEY_LEN)?;
        if value_len > MAX_KLV_VALUE_BYTES && Ul(key) != FILLER_KEY {
            return Err(MxfError::KlvValueTooLarge {
                len: value_len,
                max: MAX_KLV_VALUE_BYTES,
            });
        }
        let value_offset = offset
            .checked_add(KLV_KEY_LEN as u64)
            .and_then(|value| value.checked_add(ber_width as u64))
            .ok_or(MxfError::OffsetOverflow)?;
        let next = value_offset
            .checked_add(value_len)
            .ok_or(MxfError::OffsetOverflow)?;
        if next > bytes.len() as u64 {
            return Err(MxfError::KlvValueBeyondEof);
        }
        triplets.push(KlvTriplet {
            key: Ul(key),
            offset,
            value_offset,
            value_len,
        });
        offset = next;
    }
    Ok(triplets)
}

fn decode_ber_length(bytes: &[u8], offset: usize) -> Result<(u64, usize), MxfError> {
    let first = *bytes.get(offset).ok_or(MxfError::MalformedBerLength)?;
    if first & 0x80 == 0 {
        return Ok((u64::from(first), 1));
    }
    let width = usize::from(first & 0x7f);
    if width == 0 || width > 8 {
        return Err(MxfError::MalformedBerLength);
    }
    let end = offset
        .checked_add(1)
        .and_then(|value| value.checked_add(width))
        .ok_or(MxfError::BerLengthOverflow)?;
    if end > bytes.len() {
        return Err(MxfError::MalformedBerLength);
    }
    let mut value = 0_u64;
    for byte in &bytes[offset + 1..end] {
        value = value.checked_shl(8).ok_or(MxfError::BerLengthOverflow)? | u64::from(*byte);
    }
    Ok((value, 1 + width))
}

#[cfg(test)]
fn classify_access_unit(bytes: &[u8]) -> Result<RandomAccess, MxfError> {
    if bytes.len() > MAX_ACCESS_UNIT_BYTES {
        return Err(MxfError::AccessUnitTooLarge {
            len: bytes.len() as u64,
            max: MAX_ACCESS_UNIT_BYTES,
        });
    }
    let parsed = parse_annex_b_access_unit(bytes).map_err(MxfError::H264)?;
    Ok(random_access_from_parsed(&parsed))
}

fn random_access_from_parsed(parsed: &qgs_codec_h264::ParsedH264AccessUnit) -> RandomAccess {
    if parsed.slices.iter().any(|slice| slice.idr)
        || parsed
            .slices
            .iter()
            .any(|slice| slice.kind == H264SliceKind::I)
    {
        RandomAccess::Yes
    } else {
        RandomAccess::No
    }
}

fn descriptor_from_h264(desc: &VideoSurfaceDesc, profile: H264Profile) -> VideoEssenceDescriptor {
    let codec = match profile {
        H264Profile::Baseline
        | H264Profile::Main
        | H264Profile::High
        | H264Profile::High10
        | H264Profile::High10Intra
        | H264Profile::High422
        | H264Profile::High422Intra => VideoCodec::H264,
    };
    VideoEssenceDescriptor {
        codec,
        coded_width: desc.coded_width,
        coded_height: desc.coded_height,
        display_width: desc.visible_region.width,
        display_height: desc.visible_region.height,
        bit_depth: desc.bit_depth.get(),
        chroma: desc.chroma,
        essence_container: None,
        compression: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H264_8BIT_MXF: &[u8] =
        include_bytes!("../../../tests/fixtures/mxf/h264-8bit-420-long-gop-128x72.mxf");
    const H264_10BIT_MXF: &[u8] =
        include_bytes!("../../../tests/fixtures/mxf/h264-10bit-422-long-gop-128x72.mxf");

    #[test]
    fn parses_8bit_h264_mxf_and_extracts_access_unit() {
        let source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");

        assert_eq!(source.edit_rate, Some(Rational::new(25, 1).expect("rate")));
        assert_eq!(source.timecode.as_ref().map(|tc| tc.start_frame), Some(0));
        assert_eq!(source.tracks.len(), 1);
        assert_eq!(source.index.video.len(), 12);
        assert_eq!(source.index.video[0].random_access, RandomAccess::Yes);
        assert!(source
            .index
            .video
            .iter()
            .skip(1)
            .any(|entry| entry.random_access == RandomAccess::No));
        let desc = source.tracks[0].video.as_ref().expect("video descriptor");
        assert_eq!(desc.codec, VideoCodec::H264);
        assert_eq!(desc.coded_width, 128);
        assert_eq!(desc.coded_height, 72);
        assert_eq!(desc.bit_depth, 8);
        assert_eq!(desc.chroma, ChromaSubsampling::Cs420);

        let start = source
            .index
            .nearest_random_access_before(5)
            .expect("random access point")
            .edit_unit as usize;
        let mut state = H264DecoderState::new();
        let mut parsed_target = None;
        for index in start..=5 {
            let au = source
                .extract_video_access_unit(H264_8BIT_MXF, index)
                .expect("extract AU");
            let parsed = state.parse_access_unit(&au).expect("parse AU");
            state.finish_picture(&parsed).expect("DPB update");
            if index == 5 {
                parsed_target = Some(parsed);
            }
        }
        let parsed = parsed_target.expect("target parsed");
        assert_eq!(parsed.desc.bit_depth.get(), 8);
        assert_eq!(parsed.desc.chroma, ChromaSubsampling::Cs420);
    }

    #[test]
    fn parses_professional_10bit_422_mxf() {
        let source = MediaSource::parse(H264_10BIT_MXF).expect("parse MXF");
        let desc = source.tracks[0].video.as_ref().expect("video descriptor");

        assert_eq!(source.index.video.len(), 12);
        assert_eq!(desc.codec, VideoCodec::H264);
        assert_eq!(desc.bit_depth, 10);
        assert_eq!(desc.chroma, ChromaSubsampling::Cs422);
        let au = source
            .extract_video_access_unit(H264_10BIT_MXF, 0)
            .expect("extract AU");
        let parsed = parse_annex_b_access_unit(&au).expect("parse AU");
        assert_eq!(parsed.profile, H264Profile::High422);
        assert_eq!(parsed.desc.bit_depth.get(), 10);
        assert_eq!(parsed.desc.chroma, ChromaSubsampling::Cs422);
    }

    #[test]
    fn nearest_random_access_finds_idr_before_middle_entry() {
        let source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");
        let entry = source
            .index
            .nearest_random_access_before(7)
            .expect("random access point");

        assert_eq!(entry.edit_unit, 0);
        assert_eq!(entry.random_access, RandomAccess::Yes);
    }

    #[test]
    fn rejects_truncated_klv_key() {
        assert!(matches!(
            MediaSource::parse(&[1, 2, 3]),
            Err(MxfError::TruncatedKlvKey)
        ));
    }

    #[test]
    fn rejects_malformed_ber_length() {
        let mut bytes = vec![0_u8; KLV_KEY_LEN];
        bytes.push(0x80);

        assert!(matches!(
            MediaSource::parse(&bytes),
            Err(MxfError::MalformedBerLength)
        ));
    }

    #[test]
    fn rejects_ber_length_overflow_width() {
        let mut bytes = vec![0_u8; KLV_KEY_LEN];
        bytes.push(0x89);

        assert!(matches!(
            MediaSource::parse(&bytes),
            Err(MxfError::MalformedBerLength)
        ));
    }

    #[test]
    fn rejects_klv_value_beyond_eof() {
        let mut bytes = HEADER_PARTITION_KEY.0.to_vec();
        bytes.push(4);
        bytes.extend_from_slice(&[1, 2]);

        assert!(matches!(
            MediaSource::parse(&bytes),
            Err(MxfError::KlvValueBeyondEof)
        ));
    }

    #[test]
    fn rejects_zero_rational_denominator() {
        assert!(matches!(
            Rational::new(25, 0),
            Err(MxfError::ZeroRationalDenominator)
        ));
    }

    #[test]
    fn rejects_invalid_timecode_metadata() {
        let rate = Rational::new(25, 1).expect("rate");

        assert!(matches!(
            Timecode::new(-1, rate, false),
            Err(MxfError::InvalidTimecode)
        ));
    }

    #[test]
    fn rejects_invalid_partition_offset() {
        assert!(validate_partition_offset(9, 10).is_ok());
        assert!(matches!(
            validate_partition_offset(10, 10),
            Err(MxfError::InvalidPartitionOffset)
        ));
    }

    #[test]
    fn rejects_excessive_batch_count() {
        assert!(matches!(
            validate_batch_count(MAX_TRACK_COUNT + 1),
            Err(MxfError::ExcessiveBatchCount)
        ));
    }

    #[test]
    fn rejects_excessive_index_count() {
        assert!(matches!(
            validate_index_count(MAX_INDEX_ENTRIES + 1),
            Err(MxfError::ExcessiveIndexEntries { .. })
        ));
    }

    #[test]
    fn rejects_invalid_track_reference() {
        let track = MxfTrack {
            id: TrackId(7),
            track_number: Some(1),
            kind: TrackKind::Video,
            edit_rate: Some(Rational::new(25, 1).expect("rate")),
            video: None,
            audio: None,
        };

        assert!(matches!(
            validate_track_reference(TrackId(8), &[track]),
            Err(MxfError::InvalidTrackReference)
        ));
    }

    #[test]
    fn rejects_truncated_index_table_model() {
        let entry = VideoIndexEntry {
            track_id: TrackId(1),
            edit_unit: 0,
            presentation_position: 0,
            file_offset: 0,
            payload_offset: 32,
            payload_len: 8,
            random_access: RandomAccess::Yes,
            source: IndexSource::MxfProvided,
        };
        let index = MediaIndex::new(vec![entry]).expect("index");

        assert!(matches!(
            index.video[0].payload(&[0_u8; 36]),
            Err(MxfError::KlvValueBeyondEof)
        ));
    }

    #[test]
    fn rejects_oversized_access_unit() {
        let bytes = vec![0_u8; MAX_ACCESS_UNIT_BYTES + 1];

        assert!(matches!(
            classify_access_unit(&bytes),
            Err(MxfError::AccessUnitTooLarge { .. })
        ));
    }

    #[test]
    fn skips_unknown_klv_before_header_partition() {
        let mut bytes = [0x11_u8; KLV_KEY_LEN].to_vec();
        bytes.push(1);
        bytes.push(0);
        bytes.extend_from_slice(H264_8BIT_MXF);

        let source = MediaSource::parse(&bytes).expect("unknown KLV skipped");
        assert_eq!(source.index.video.len(), 12);
    }

    #[test]
    fn invalid_index_entry_offset_is_rejected_on_extraction() {
        let mut source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");
        source.index.video[0].payload_offset = u64::MAX;

        assert!(matches!(
            source.extract_video_access_unit(H264_8BIT_MXF, 0),
            Err(MxfError::OffsetOverflow | MxfError::KlvValueBeyondEof)
        ));
    }
}
