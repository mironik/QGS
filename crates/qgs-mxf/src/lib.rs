#![forbid(unsafe_code)]

use std::collections::HashSet;
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
pub const MAX_METADATA_SET_COUNT: usize = 512;
pub const MAX_METADATA_PROPERTY_COUNT: usize = 128;
pub const MAX_REFERENCE_BATCH_COUNT: usize = 128;
pub const MAX_PARTITION_COUNT: usize = 128;
pub const MAX_INDEX_SEGMENTS: usize = 64;
pub const MAX_DELTA_ENTRIES: usize = 64;
pub const MAX_AUDIO_CHANNELS: u16 = 64;

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
const PRIMER_PACK_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x05, 0x01, 0x00,
]);
const INDEX_TABLE_SEGMENT_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x10, 0x01, 0x00,
]);
const RIP_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x11, 0x01, 0x00,
]);
const OP1A_OPERATIONAL_PATTERN: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x01, 0x09, 0x00,
]);

const PREFACE_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x2f, 0x00,
]);
const CONTENT_STORAGE_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x18, 0x00,
]);
const MATERIAL_PACKAGE_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x36, 0x00,
]);
const SOURCE_PACKAGE_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x37, 0x00,
]);
const MULTIPLE_DESCRIPTOR_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x44, 0x00,
]);
const CDCI_DESCRIPTOR_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x28, 0x00,
]);
const WAVE_AUDIO_DESCRIPTOR_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x47, 0x00,
]);
const DATA_ESSENCE_DESCRIPTOR_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x5c, 0x00,
]);
const ESSENCE_CONTAINER_DATA_SET_KEY: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01, 0x23, 0x00,
]);

const INSTANCE_UID_UL: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x15, 0x02, 0x00, 0x00, 0x00, 0x00,
]);
const PACKAGE_TRACKS_UL: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x06, 0x01, 0x01, 0x04, 0x06, 0x05, 0x00, 0x00,
]);
const CONTENT_STORAGE_PACKAGES_UL: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x06, 0x01, 0x01, 0x04, 0x05, 0x01, 0x00, 0x00,
]);
const CONTENT_STORAGE_ESSENCE_DATA_UL: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x06, 0x01, 0x01, 0x04, 0x05, 0x02, 0x00, 0x00,
]);
const MULTIPLE_DESCRIPTOR_SUB_DESCRIPTORS_UL: Ul = Ul([
    0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x04, 0x06, 0x01, 0x01, 0x04, 0x06, 0x0b, 0x00, 0x00,
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InstanceUid(pub [u8; 16]);

impl fmt::Display for InstanceUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PackageUid(pub [u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimerPack {
    pub mappings: Vec<PrimerMapping>,
}

impl PrimerPack {
    pub fn resolve(&self, local_tag: u16) -> Option<Ul> {
        self.mappings
            .iter()
            .find(|mapping| mapping.local_tag == local_tag)
            .map(|mapping| mapping.property)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrimerMapping {
    pub local_tag: u16,
    pub property: Ul,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSet {
    pub key: Ul,
    pub instance_uid: Option<InstanceUid>,
    pub properties: Vec<MetadataProperty>,
}

impl MetadataSet {
    pub fn property(&self, property: Ul) -> Option<&MetadataProperty> {
        self.properties
            .iter()
            .find(|item| item.property == property)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataProperty {
    pub local_tag: u16,
    pub property: Ul,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageKind {
    Material,
    Source,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MxfPackage {
    pub uid: InstanceUid,
    pub kind: PackageKind,
    pub track_refs: Vec<InstanceUid>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataDiagnostic {
    pub kind: MetadataDiagnosticKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MetadataDiagnosticKind {
    Mismatch,
    UnsupportedOperationalPattern,
    MalformedIndexFallback,
    MissingReference,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexTableSegment {
    pub index_edit_rate: Option<Rational>,
    pub index_start_position: i64,
    pub index_duration: i64,
    pub edit_unit_byte_count: u32,
    pub index_sid: u32,
    pub body_sid: u32,
    pub slice_count: u8,
    pub pos_table_count: u8,
    pub delta_entries: Vec<DeltaEntry>,
    pub entries: Vec<MxfIndexEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaEntry {
    pub pos_table_index: i8,
    pub slice: u8,
    pub element_delta: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MxfIndexEntry {
    pub temporal_offset: i8,
    pub key_frame_offset: i8,
    pub flags: u8,
    pub stream_offset: u64,
    pub slice_offsets: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RipEntry {
    pub body_sid: u32,
    pub byte_offset: u64,
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
    Data,
    Timecode,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexSource {
    MxfProvided,
    QgsDerived,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DescriptorSource {
    MxfMetadata,
    H264Essence,
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
    pub data: Option<DataEssenceDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoEssenceDescriptor {
    pub source: DescriptorSource,
    pub codec: VideoCodec,
    pub coded_width: u32,
    pub coded_height: u32,
    pub sampled_width: Option<u32>,
    pub sampled_height: Option<u32>,
    pub display_width: u32,
    pub display_height: u32,
    pub display_x_offset: Option<i32>,
    pub display_y_offset: Option<i32>,
    pub frame_layout: Option<u8>,
    pub aspect_ratio: Option<Rational>,
    pub horizontal_subsampling: Option<u32>,
    pub vertical_subsampling: Option<u32>,
    pub bit_depth: u8,
    pub chroma: ChromaSubsampling,
    pub essence_container: Option<Ul>,
    pub compression: Option<Ul>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioEssenceDescriptor {
    pub source: DescriptorSource,
    pub essence: Option<Ul>,
    pub channels: Option<u16>,
    pub sample_rate: Option<Rational>,
    pub bit_depth: Option<u8>,
    pub block_align: Option<u16>,
    pub average_bytes_per_second: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataEssenceDescriptor {
    pub source: DescriptorSource,
    pub essence: Option<Ul>,
    pub sample_rate: Option<Rational>,
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
    pub operational_pattern: Option<Ul>,
    pub primer: Option<PrimerPack>,
    pub packages: Vec<MxfPackage>,
    pub tracks: Vec<MxfTrack>,
    pub index: MediaIndex,
    pub index_segments: Vec<IndexTableSegment>,
    pub rip: Vec<RipEntry>,
    pub diagnostics: Vec<MetadataDiagnostic>,
    pub klv_count: usize,
    pub metadata_set_count: usize,
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
        let mut partitions = Vec::new();
        for triplet in &triplets {
            if matches!(
                triplet.key,
                HEADER_PARTITION_KEY | BODY_PARTITION_KEY | FOOTER_PARTITION_KEY
            ) {
                if partitions.len() >= MAX_PARTITION_COUNT {
                    return Err(MxfError::ExcessivePartitionCount);
                }
                partitions.push(parse_partition_pack(triplet, triplet.value(bytes)?)?);
            }
        }
        if partitions.is_empty() {
            return Err(MxfError::MissingHeaderPartition);
        }
        let operational_pattern = partitions
            .iter()
            .find_map(|partition| partition.operational_pattern);

        let primer = triplets
            .iter()
            .find(|triplet| triplet.key == PRIMER_PACK_KEY)
            .map(|triplet| parse_primer_pack(triplet.value(bytes)?))
            .transpose()?;
        let metadata_sets = parse_metadata_sets(bytes, &triplets, primer.as_ref())?;
        validate_metadata_graph(&metadata_sets)?;
        let mut diagnostics = metadata_graph_diagnostics(&metadata_sets);
        if let Some(pattern) = operational_pattern {
            if pattern != OP1A_OPERATIONAL_PATTERN {
                diagnostics.push(MetadataDiagnostic {
                    kind: MetadataDiagnosticKind::UnsupportedOperationalPattern,
                    message: format!("unsupported operational pattern {pattern}"),
                });
            }
        }
        let packages = parse_packages(&metadata_sets)?;
        let index_segments = parse_index_table_segments(bytes, &triplets, primer.as_ref())?;
        let rip = parse_rip_entries(bytes, &triplets)?;
        let mut tracks = parse_tracks_from_metadata(&metadata_sets)?;
        let video_track_id = tracks
            .iter()
            .find(|track| track.kind == TrackKind::Video)
            .map(|track| track.id)
            .unwrap_or(TrackId(1));

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
                track_id: video_track_id,
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
        let h264_desc = descriptor_from_h264(
            &first_desc.ok_or(MxfError::NoSupportedEssence)?,
            first_profile.ok_or(MxfError::NoSupportedEssence)?,
        );
        if let Some(track) = tracks.iter_mut().find(|track| track.id == video_track_id) {
            if let Some(metadata_desc) = &track.video {
                diagnostics.extend(compare_video_descriptor_to_h264(metadata_desc, &h264_desc));
            }
        } else {
            tracks.push(MxfTrack {
                id: video_track_id,
                track_number: None,
                kind: TrackKind::Video,
                edit_rate: None,
                video: Some(h264_desc.clone()),
                audio: None,
                data: None,
            });
        }
        if let Some(track) = tracks.iter_mut().find(|track| track.id == video_track_id) {
            if track.video.is_none() {
                track.video = Some(h264_desc);
            }
        }
        let duration = Some(video.len() as u64);
        let edit_rate = tracks
            .iter()
            .find(|track| track.kind == TrackKind::Video)
            .and_then(|track| track.edit_rate)
            .or(Some(Rational::new(25, 1)?));
        let timecode = parse_timecode_from_metadata(
            &metadata_sets,
            edit_rate.unwrap_or(Rational::new(25, 1)?),
        )?
        .or(Some(Timecode::new(0, Rational::new(25, 1)?, false)?));
        validate_batch_count(tracks.len())?;
        let mut index = MediaIndex::new(video)?;
        apply_mxf_index_to_derived_entries(&mut index, &index_segments);
        Ok(Self {
            duration,
            edit_rate,
            timecode,
            operational_pattern,
            primer,
            packages,
            tracks,
            index,
            index_segments,
            rip,
            diagnostics,
            klv_count: triplets.len(),
            metadata_set_count: metadata_sets.len(),
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
    pub body_sid: u32,
    pub index_sid: u32,
    pub this_partition: u64,
    pub previous_partition: u64,
    pub footer_partition: u64,
    pub header_byte_count: u64,
    pub index_byte_count: u64,
    pub operational_pattern: Option<Ul>,
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
    ExcessivePartitionCount,
    ExcessiveMetadataSetCount,
    ExcessiveMetadataPropertyCount,
    ExcessiveReferenceBatchCount,
    ExcessiveDeltaEntryCount,
    InvalidPartitionOffset,
    InvalidPartitionPack,
    ExcessiveBatchCount,
    InvalidTrackReference,
    MissingMetadataReference(InstanceUid),
    DuplicateInstanceUid(InstanceUid),
    MalformedPrimerPack,
    MissingPrimerPack,
    MalformedLocalSet,
    InvalidMetadataValue,
    InvalidEssenceOffset,
    TruncatedIndexTable,
    MalformedIndexTable,
    ZeroRationalDenominator,
    InvalidTimecode,
    InvalidAudioDescriptor,
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
            Self::ExcessivePartitionCount => write!(f, "too many MXF partitions"),
            Self::ExcessiveMetadataSetCount => write!(f, "too many MXF metadata sets"),
            Self::ExcessiveMetadataPropertyCount => write!(f, "too many MXF metadata properties"),
            Self::ExcessiveReferenceBatchCount => write!(f, "too many MXF strong references"),
            Self::ExcessiveDeltaEntryCount => write!(f, "too many MXF delta entries"),
            Self::InvalidPartitionOffset => write!(f, "invalid partition offset"),
            Self::InvalidPartitionPack => write!(f, "invalid MXF partition pack"),
            Self::ExcessiveBatchCount => write!(f, "excessive MXF batch count"),
            Self::InvalidTrackReference => write!(f, "invalid track reference"),
            Self::MissingMetadataReference(uid) => {
                write!(f, "missing MXF metadata reference {uid}")
            }
            Self::DuplicateInstanceUid(uid) => write!(f, "duplicate MXF InstanceUID {uid}"),
            Self::MalformedPrimerPack => write!(f, "malformed MXF primer pack"),
            Self::MissingPrimerPack => write!(f, "missing MXF primer pack"),
            Self::MalformedLocalSet => write!(f, "malformed MXF local set"),
            Self::InvalidMetadataValue => write!(f, "invalid MXF metadata value"),
            Self::InvalidEssenceOffset => write!(f, "invalid essence offset"),
            Self::TruncatedIndexTable => write!(f, "truncated index table"),
            Self::MalformedIndexTable => write!(f, "malformed index table"),
            Self::ZeroRationalDenominator => write!(f, "zero rational denominator"),
            Self::InvalidTimecode => write!(f, "invalid timecode metadata"),
            Self::InvalidAudioDescriptor => write!(f, "invalid audio descriptor"),
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

fn parse_partition_pack(triplet: &KlvTriplet, value: &[u8]) -> Result<PartitionInfo, MxfError> {
    if value.len() < 88 {
        return Err(MxfError::InvalidPartitionPack);
    }
    let kind = match triplet.key {
        HEADER_PARTITION_KEY => PartitionKind::Header,
        BODY_PARTITION_KEY => PartitionKind::Body,
        FOOTER_PARTITION_KEY => PartitionKind::Footer,
        _ => return Err(MxfError::InvalidPartitionPack),
    };
    let this_partition = read_u64_at(value, 8)?;
    let previous_partition = read_u64_at(value, 16)?;
    let footer_partition = read_u64_at(value, 24)?;
    let header_byte_count = read_u64_at(value, 32)?;
    let index_byte_count = read_u64_at(value, 40)?;
    let index_sid = read_u32_at(value, 48)?;
    let body_sid = read_u32_at(value, 60)?;
    let operational_pattern = Some(read_ul_at(value, 64)?);
    Ok(PartitionInfo {
        kind,
        offset: triplet.offset,
        body_sid,
        index_sid,
        this_partition,
        previous_partition,
        footer_partition,
        header_byte_count,
        index_byte_count,
        operational_pattern,
    })
}

fn parse_primer_pack(value: &[u8]) -> Result<PrimerPack, MxfError> {
    if value.len() < 8 {
        return Err(MxfError::MalformedPrimerPack);
    }
    let count = read_u32_at(value, 0)? as usize;
    let item_len = read_u32_at(value, 4)? as usize;
    if count > MAX_METADATA_PROPERTY_COUNT {
        return Err(MxfError::ExcessiveMetadataPropertyCount);
    }
    if item_len != 18 {
        return Err(MxfError::MalformedPrimerPack);
    }
    let bytes_len = count
        .checked_mul(item_len)
        .and_then(|len| len.checked_add(8))
        .ok_or(MxfError::OffsetOverflow)?;
    if bytes_len > value.len() {
        return Err(MxfError::MalformedPrimerPack);
    }
    let mut mappings = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        let local_tag = read_u16_at(value, offset)?;
        let property = read_ul_at(value, offset + 2)?;
        mappings.push(PrimerMapping {
            local_tag,
            property,
        });
        offset += item_len;
    }
    Ok(PrimerPack { mappings })
}

fn parse_metadata_sets(
    bytes: &[u8],
    triplets: &[KlvTriplet],
    primer: Option<&PrimerPack>,
) -> Result<Vec<MetadataSet>, MxfError> {
    let primer = primer.ok_or(MxfError::MissingPrimerPack)?;
    let mut sets = Vec::new();
    for triplet in triplets
        .iter()
        .filter(|triplet| is_metadata_set_key(triplet.key))
    {
        if sets.len() >= MAX_METADATA_SET_COUNT {
            return Err(MxfError::ExcessiveMetadataSetCount);
        }
        sets.push(parse_local_metadata_set(
            triplet.key,
            triplet.value(bytes)?,
            primer,
        )?);
    }
    Ok(sets)
}

fn is_metadata_set_key(key: Ul) -> bool {
    matches!(
        key,
        PREFACE_SET_KEY
            | CONTENT_STORAGE_SET_KEY
            | MATERIAL_PACKAGE_SET_KEY
            | SOURCE_PACKAGE_SET_KEY
            | MULTIPLE_DESCRIPTOR_SET_KEY
            | CDCI_DESCRIPTOR_SET_KEY
            | WAVE_AUDIO_DESCRIPTOR_SET_KEY
            | DATA_ESSENCE_DESCRIPTOR_SET_KEY
            | ESSENCE_CONTAINER_DATA_SET_KEY
    ) || key
        .0
        .starts_with(&[0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01])
}

fn parse_local_metadata_set(
    key: Ul,
    value: &[u8],
    primer: &PrimerPack,
) -> Result<MetadataSet, MxfError> {
    let mut offset = 0;
    let mut properties = Vec::new();
    let mut instance_uid = None;
    while offset < value.len() {
        if properties.len() >= MAX_METADATA_PROPERTY_COUNT {
            return Err(MxfError::ExcessiveMetadataPropertyCount);
        }
        if value.len() - offset < 4 {
            return Err(MxfError::MalformedLocalSet);
        }
        let local_tag = read_u16_at(value, offset)?;
        let len = read_u16_at(value, offset + 2)? as usize;
        offset += 4;
        let end = offset.checked_add(len).ok_or(MxfError::OffsetOverflow)?;
        if end > value.len() {
            return Err(MxfError::MalformedLocalSet);
        }
        let property = primer
            .resolve(local_tag)
            .ok_or(MxfError::MalformedLocalSet)?;
        let property_value = value[offset..end].to_vec();
        if property == INSTANCE_UID_UL && property_value.len() == 16 {
            instance_uid = Some(read_instance_uid(&property_value)?);
        }
        properties.push(MetadataProperty {
            local_tag,
            property,
            value: property_value,
        });
        offset = end;
    }
    Ok(MetadataSet {
        key,
        instance_uid,
        properties,
    })
}

fn validate_metadata_graph(sets: &[MetadataSet]) -> Result<(), MxfError> {
    let mut seen = HashSet::new();
    for set in sets {
        if let Some(uid) = set.instance_uid {
            if !seen.insert(uid) {
                return Err(MxfError::DuplicateInstanceUid(uid));
            }
        }
    }
    for set in sets {
        for reference in known_strong_references(set)? {
            if !seen.contains(&reference) {
                return Err(MxfError::MissingMetadataReference(reference));
            }
        }
    }
    Ok(())
}

fn metadata_graph_diagnostics(sets: &[MetadataSet]) -> Vec<MetadataDiagnostic> {
    let mut diagnostics = Vec::new();
    for set in sets {
        if set.instance_uid.is_none() {
            diagnostics.push(MetadataDiagnostic {
                kind: MetadataDiagnosticKind::MissingReference,
                message: format!("metadata set {} has no InstanceUID", set.key),
            });
        }
    }
    diagnostics
}

fn known_strong_references(set: &MetadataSet) -> Result<Vec<InstanceUid>, MxfError> {
    let mut refs = Vec::new();
    for property in &set.properties {
        if matches!(
            property.property,
            PACKAGE_TRACKS_UL
                | CONTENT_STORAGE_PACKAGES_UL
                | CONTENT_STORAGE_ESSENCE_DATA_UL
                | MULTIPLE_DESCRIPTOR_SUB_DESCRIPTORS_UL
        ) {
            refs.extend(parse_strong_reference_batch(&property.value)?);
        }
    }
    Ok(refs)
}

fn parse_strong_reference_batch(value: &[u8]) -> Result<Vec<InstanceUid>, MxfError> {
    if value.len() < 8 {
        return Err(MxfError::InvalidMetadataValue);
    }
    let count = read_u32_at(value, 0)? as usize;
    let item_len = read_u32_at(value, 4)? as usize;
    if count > MAX_REFERENCE_BATCH_COUNT {
        return Err(MxfError::ExcessiveReferenceBatchCount);
    }
    if item_len != 16 {
        return Err(MxfError::InvalidMetadataValue);
    }
    let expected = 8_usize
        .checked_add(
            count
                .checked_mul(item_len)
                .ok_or(MxfError::OffsetOverflow)?,
        )
        .ok_or(MxfError::OffsetOverflow)?;
    if expected > value.len() {
        return Err(MxfError::InvalidMetadataValue);
    }
    let mut refs = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        refs.push(read_instance_uid(&value[offset..offset + 16])?);
        offset += 16;
    }
    Ok(refs)
}

fn parse_packages(sets: &[MetadataSet]) -> Result<Vec<MxfPackage>, MxfError> {
    let mut packages = Vec::new();
    for set in sets {
        let kind = match set.key {
            MATERIAL_PACKAGE_SET_KEY => PackageKind::Material,
            SOURCE_PACKAGE_SET_KEY => PackageKind::Source,
            _ => continue,
        };
        let Some(uid) = set.instance_uid else {
            continue;
        };
        let track_refs = set
            .property(PACKAGE_TRACKS_UL)
            .map(|property| parse_strong_reference_batch(&property.value))
            .transpose()?
            .unwrap_or_default();
        packages.push(MxfPackage {
            uid,
            kind,
            track_refs,
        });
    }
    Ok(packages)
}

fn parse_tracks_from_metadata(sets: &[MetadataSet]) -> Result<Vec<MxfTrack>, MxfError> {
    let mut tracks = Vec::new();
    for set in sets {
        match set.key {
            CDCI_DESCRIPTOR_SET_KEY => {
                if tracks.len() >= MAX_TRACK_COUNT {
                    return Err(MxfError::ExcessiveBatchCount);
                }
                let track_id = descriptor_linked_track_id(set).unwrap_or(1);
                tracks.push(MxfTrack {
                    id: TrackId(track_id),
                    track_number: None,
                    kind: TrackKind::Video,
                    edit_rate: descriptor_sample_rate(set)?,
                    video: Some(parse_cdci_descriptor(set)?),
                    audio: None,
                    data: None,
                });
            }
            WAVE_AUDIO_DESCRIPTOR_SET_KEY => {
                if tracks.len() >= MAX_TRACK_COUNT {
                    return Err(MxfError::ExcessiveBatchCount);
                }
                let track_id = descriptor_linked_track_id(set).unwrap_or((tracks.len() + 1) as u32);
                tracks.push(MxfTrack {
                    id: TrackId(track_id),
                    track_number: None,
                    kind: TrackKind::Audio,
                    edit_rate: descriptor_sample_rate(set)?,
                    video: None,
                    audio: Some(parse_wave_audio_descriptor(set)?),
                    data: None,
                });
            }
            DATA_ESSENCE_DESCRIPTOR_SET_KEY => {
                if tracks.len() >= MAX_TRACK_COUNT {
                    return Err(MxfError::ExcessiveBatchCount);
                }
                let track_id = descriptor_linked_track_id(set).unwrap_or((tracks.len() + 1) as u32);
                tracks.push(MxfTrack {
                    id: TrackId(track_id),
                    track_number: None,
                    kind: TrackKind::Data,
                    edit_rate: descriptor_sample_rate(set)?,
                    video: None,
                    audio: None,
                    data: Some(parse_data_essence_descriptor(set)?),
                });
            }
            _ => {}
        }
    }
    Ok(tracks)
}

fn descriptor_linked_track_id(set: &MetadataSet) -> Option<u32> {
    set.properties
        .iter()
        .find(|property| property.local_tag == 0x3006)
        .and_then(|property| read_u32_at(&property.value, 0).ok())
}

fn descriptor_sample_rate(set: &MetadataSet) -> Result<Option<Rational>, MxfError> {
    set.properties
        .iter()
        .find(|property| property.local_tag == 0x3001)
        .map(|property| read_rational(&property.value))
        .transpose()
}

fn parse_cdci_descriptor(set: &MetadataSet) -> Result<VideoEssenceDescriptor, MxfError> {
    let coded_width = read_optional_u32_tag(set, 0x3203)?.unwrap_or(0);
    let coded_height = read_optional_u32_tag(set, 0x3202)?.unwrap_or(0);
    let sampled_width = read_optional_u32_tag(set, 0x3205)?;
    let sampled_height = read_optional_u32_tag(set, 0x3204)?;
    let display_width = read_optional_u32_tag(set, 0x3209)?.unwrap_or(coded_width);
    let display_height = read_optional_u32_tag(set, 0x3208)?.unwrap_or(coded_height);
    let component_depth = read_optional_u32_tag(set, 0x3301)?.unwrap_or(8) as u8;
    let horizontal_subsampling = read_optional_u32_tag(set, 0x3302)?;
    let vertical_subsampling = read_optional_u32_tag(set, 0x3308)?;
    let chroma = match (
        horizontal_subsampling.unwrap_or(2),
        vertical_subsampling.unwrap_or(2),
    ) {
        (2, 2) => ChromaSubsampling::Cs420,
        (2, 1) | (2, 0) => ChromaSubsampling::Cs422,
        (1, 1) | (1, 0) => ChromaSubsampling::Cs444,
        _ => ChromaSubsampling::Cs420,
    };
    let aspect_ratio = set
        .properties
        .iter()
        .find(|property| property.local_tag == 0x320e)
        .map(|property| read_rational(&property.value))
        .transpose()?;
    Ok(VideoEssenceDescriptor {
        source: DescriptorSource::MxfMetadata,
        codec: VideoCodec::H264,
        coded_width,
        coded_height,
        sampled_width,
        sampled_height,
        display_width,
        display_height,
        display_x_offset: read_optional_i32_tag(set, 0x320a)?,
        display_y_offset: read_optional_i32_tag(set, 0x320b)?,
        frame_layout: read_optional_u8_tag(set, 0x320c)?,
        aspect_ratio,
        horizontal_subsampling,
        vertical_subsampling,
        bit_depth: component_depth,
        chroma,
        essence_container: read_optional_ul_tag(set, 0x3004)?,
        compression: read_optional_ul_tag(set, 0x3201)?,
    })
}

fn parse_wave_audio_descriptor(set: &MetadataSet) -> Result<AudioEssenceDescriptor, MxfError> {
    let channels = read_optional_u32_tag(set, 0x3d07)?.map(|value| value as u16);
    if let Some(channels) = channels {
        if channels == 0 || channels > MAX_AUDIO_CHANNELS {
            return Err(MxfError::InvalidAudioDescriptor);
        }
    }
    Ok(AudioEssenceDescriptor {
        source: DescriptorSource::MxfMetadata,
        essence: read_optional_ul_tag(set, 0x3004)?,
        channels,
        sample_rate: set
            .properties
            .iter()
            .find(|property| property.local_tag == 0x3d03)
            .map(|property| read_rational(&property.value))
            .transpose()?,
        bit_depth: read_optional_u32_tag(set, 0x3d01)?.map(|value| value as u8),
        block_align: read_optional_u16_tag(set, 0x3d0a)?,
        average_bytes_per_second: read_optional_u32_tag(set, 0x3d09)?,
    })
}

fn parse_data_essence_descriptor(set: &MetadataSet) -> Result<DataEssenceDescriptor, MxfError> {
    Ok(DataEssenceDescriptor {
        source: DescriptorSource::MxfMetadata,
        essence: read_optional_ul_tag(set, 0x3004)?,
        sample_rate: descriptor_sample_rate(set)?,
    })
}

fn parse_timecode_from_metadata(
    sets: &[MetadataSet],
    edit_rate: Rational,
) -> Result<Option<Timecode>, MxfError> {
    for set in sets {
        if set.key.0[14] != 0x14 {
            continue;
        }
        let start = read_optional_i64_tag(set, 0x1501)?.unwrap_or(0);
        let rounded_base =
            read_optional_u16_tag(set, 0x1502)?.unwrap_or(edit_rate.numerator as u16);
        let drop_frame = read_optional_u8_tag(set, 0x1503)?.unwrap_or(0) != 0;
        let rate = Rational::new(u32::from(rounded_base), 1)?;
        return Ok(Some(Timecode::new(start, rate, drop_frame)?));
    }
    Ok(None)
}

fn compare_video_descriptor_to_h264(
    metadata: &VideoEssenceDescriptor,
    h264: &VideoEssenceDescriptor,
) -> Vec<MetadataDiagnostic> {
    let mut diagnostics = Vec::new();
    let checks = [
        (
            "display_width",
            metadata.display_width as i64,
            h264.display_width as i64,
        ),
        (
            "display_height",
            metadata.display_height as i64,
            h264.display_height as i64,
        ),
        (
            "bit_depth",
            metadata.bit_depth as i64,
            h264.bit_depth as i64,
        ),
    ];
    for (name, left, right) in checks {
        if left != 0 && left != right {
            diagnostics.push(MetadataDiagnostic {
                kind: MetadataDiagnosticKind::Mismatch,
                message: format!("MXF descriptor {name}={left} disagrees with H.264 SPS {right}"),
            });
        }
    }
    if metadata.chroma != h264.chroma {
        diagnostics.push(MetadataDiagnostic {
            kind: MetadataDiagnosticKind::Mismatch,
            message: format!(
                "MXF descriptor chroma={:?} disagrees with H.264 SPS {:?}",
                metadata.chroma, h264.chroma
            ),
        });
    }
    diagnostics
}

fn parse_index_table_segments(
    bytes: &[u8],
    triplets: &[KlvTriplet],
    primer: Option<&PrimerPack>,
) -> Result<Vec<IndexTableSegment>, MxfError> {
    let primer = primer.ok_or(MxfError::MissingPrimerPack)?;
    let mut segments = Vec::new();
    for triplet in triplets
        .iter()
        .filter(|triplet| triplet.key == INDEX_TABLE_SEGMENT_KEY)
    {
        if segments.len() >= MAX_INDEX_SEGMENTS {
            return Err(MxfError::ExcessiveIndexEntries {
                count: segments.len() + 1,
                max: MAX_INDEX_SEGMENTS,
            });
        }
        let set = parse_local_metadata_set(triplet.key, triplet.value(bytes)?, primer)?;
        segments.push(parse_index_segment_set(&set)?);
    }
    Ok(segments)
}

fn parse_index_segment_set(set: &MetadataSet) -> Result<IndexTableSegment, MxfError> {
    let delta_entries = set
        .properties
        .iter()
        .find(|property| property.local_tag == 0x3f09)
        .map(|property| parse_delta_entry_array(&property.value))
        .transpose()?
        .unwrap_or_default();
    let entries = set
        .properties
        .iter()
        .find(|property| property.local_tag == 0x3f0a)
        .map(|property| parse_index_entry_array(&property.value))
        .transpose()?
        .unwrap_or_default();
    Ok(IndexTableSegment {
        index_edit_rate: set
            .properties
            .iter()
            .find(|property| property.local_tag == 0x3f0b)
            .map(|property| read_rational(&property.value))
            .transpose()?,
        index_start_position: read_optional_i64_tag(set, 0x3f0c)?.unwrap_or(0),
        index_duration: read_optional_i64_tag(set, 0x3f0d)?.unwrap_or(0),
        edit_unit_byte_count: read_optional_u32_tag(set, 0x3f05)?.unwrap_or(0),
        index_sid: read_optional_u32_tag(set, 0x3f06)?.unwrap_or(0),
        body_sid: read_optional_u32_tag(set, 0x3f07)?.unwrap_or(0),
        slice_count: read_optional_u8_tag(set, 0x3f08)?.unwrap_or(0),
        pos_table_count: 0,
        delta_entries,
        entries,
    })
}

fn parse_delta_entry_array(value: &[u8]) -> Result<Vec<DeltaEntry>, MxfError> {
    if value.len() < 8 {
        return Err(MxfError::MalformedIndexTable);
    }
    let count = read_u32_at(value, 0)? as usize;
    let item_len = read_u32_at(value, 4)? as usize;
    if count > MAX_DELTA_ENTRIES {
        return Err(MxfError::ExcessiveDeltaEntryCount);
    }
    if item_len < 6 {
        return Err(MxfError::MalformedIndexTable);
    }
    let expected = 8_usize
        .checked_add(
            count
                .checked_mul(item_len)
                .ok_or(MxfError::OffsetOverflow)?,
        )
        .ok_or(MxfError::OffsetOverflow)?;
    if expected > value.len() {
        return Err(MxfError::TruncatedIndexTable);
    }
    let mut entries = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        entries.push(DeltaEntry {
            pos_table_index: value[offset] as i8,
            slice: value[offset + 1],
            element_delta: read_u32_at(value, offset + 2)?,
        });
        offset += item_len;
    }
    Ok(entries)
}

fn parse_index_entry_array(value: &[u8]) -> Result<Vec<MxfIndexEntry>, MxfError> {
    if value.len() < 8 {
        return Err(MxfError::MalformedIndexTable);
    }
    let count = read_u32_at(value, 0)? as usize;
    let item_len = read_u32_at(value, 4)? as usize;
    validate_index_count(count)?;
    if item_len < 11 {
        return Err(MxfError::MalformedIndexTable);
    }
    let expected = 8_usize
        .checked_add(
            count
                .checked_mul(item_len)
                .ok_or(MxfError::OffsetOverflow)?,
        )
        .ok_or(MxfError::OffsetOverflow)?;
    if expected > value.len() {
        return Err(MxfError::TruncatedIndexTable);
    }
    let mut entries = Vec::with_capacity(count);
    let mut offset = 8;
    for _ in 0..count {
        let remaining = item_len - 11;
        let mut slice_offsets = Vec::new();
        let mut slice_offset = offset + 11;
        while slice_offset + 4 <= offset + 11 + remaining {
            slice_offsets.push(read_u32_at(value, slice_offset)?);
            slice_offset += 4;
        }
        entries.push(MxfIndexEntry {
            temporal_offset: value[offset] as i8,
            key_frame_offset: value[offset + 1] as i8,
            flags: value[offset + 2],
            stream_offset: read_u64_at(value, offset + 3)?,
            slice_offsets,
        });
        offset += item_len;
    }
    Ok(entries)
}

fn apply_mxf_index_to_derived_entries(index: &mut MediaIndex, segments: &[IndexTableSegment]) {
    let Some(segment) = segments.first() else {
        return;
    };
    if segment.entries.len() != index.video.len() {
        return;
    }
    for (entry, mxf_entry) in index.video.iter_mut().zip(&segment.entries) {
        entry.source = IndexSource::MxfProvided;
        entry.random_access = if mxf_entry.key_frame_offset == 0 {
            RandomAccess::Yes
        } else {
            RandomAccess::No
        };
    }
}

fn parse_rip_entries(bytes: &[u8], triplets: &[KlvTriplet]) -> Result<Vec<RipEntry>, MxfError> {
    let Some(triplet) = triplets.iter().find(|triplet| triplet.key == RIP_KEY) else {
        return Ok(Vec::new());
    };
    let value = triplet.value(bytes)?;
    if value.len() < 4 || value.len() % 12 != 4 {
        return Err(MxfError::InvalidMetadataValue);
    }
    let entry_count = (value.len() - 4) / 12;
    if entry_count > MAX_PARTITION_COUNT {
        return Err(MxfError::ExcessivePartitionCount);
    }
    let mut entries = Vec::with_capacity(entry_count);
    let mut offset = 0;
    for _ in 0..entry_count {
        entries.push(RipEntry {
            body_sid: read_u32_at(value, offset)?,
            byte_offset: read_u64_at(value, offset + 4)?,
        });
        offset += 12;
    }
    Ok(entries)
}

fn read_u8_at(value: &[u8], offset: usize) -> Result<u8, MxfError> {
    value
        .get(offset)
        .copied()
        .ok_or(MxfError::InvalidMetadataValue)
}

fn read_u16_at(value: &[u8], offset: usize) -> Result<u16, MxfError> {
    let end = offset.checked_add(2).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_at(value: &[u8], offset: usize) -> Result<u32, MxfError> {
    let end = offset.checked_add(4).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32_at(value: &[u8], offset: usize) -> Result<i32, MxfError> {
    let end = offset.checked_add(4).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64_at(value: &[u8], offset: usize) -> Result<u64, MxfError> {
    let end = offset.checked_add(8).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    Ok(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_i64_at(value: &[u8], offset: usize) -> Result<i64, MxfError> {
    let end = offset.checked_add(8).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    Ok(i64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn read_ul_at(value: &[u8], offset: usize) -> Result<Ul, MxfError> {
    let end = offset.checked_add(16).ok_or(MxfError::OffsetOverflow)?;
    let bytes = value
        .get(offset..end)
        .ok_or(MxfError::InvalidMetadataValue)?;
    let mut ul = [0_u8; 16];
    ul.copy_from_slice(bytes);
    Ok(Ul(ul))
}

fn read_instance_uid(value: &[u8]) -> Result<InstanceUid, MxfError> {
    let bytes = value.get(..16).ok_or(MxfError::InvalidMetadataValue)?;
    let mut uid = [0_u8; 16];
    uid.copy_from_slice(bytes);
    Ok(InstanceUid(uid))
}

fn read_rational(value: &[u8]) -> Result<Rational, MxfError> {
    if value.len() < 8 {
        return Err(MxfError::InvalidMetadataValue);
    }
    Rational::new(read_u32_at(value, 0)?, read_u32_at(value, 4)?)
}

fn property_by_tag(set: &MetadataSet, local_tag: u16) -> Option<&MetadataProperty> {
    set.properties
        .iter()
        .find(|property| property.local_tag == local_tag)
}

fn read_optional_u8_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<u8>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_u8_at(&property.value, 0))
        .transpose()
}

fn read_optional_u16_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<u16>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_u16_at(&property.value, 0))
        .transpose()
}

fn read_optional_u32_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<u32>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_u32_at(&property.value, 0))
        .transpose()
}

fn read_optional_i32_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<i32>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_i32_at(&property.value, 0))
        .transpose()
}

fn read_optional_i64_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<i64>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_i64_at(&property.value, 0))
        .transpose()
}

fn read_optional_ul_tag(set: &MetadataSet, local_tag: u16) -> Result<Option<Ul>, MxfError> {
    property_by_tag(set, local_tag)
        .map(|property| read_ul_at(&property.value, 0))
        .transpose()
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
        source: DescriptorSource::H264Essence,
        codec,
        coded_width: desc.coded_width,
        coded_height: desc.coded_height,
        sampled_width: None,
        sampled_height: None,
        display_width: desc.visible_region.width,
        display_height: desc.visible_region.height,
        display_x_offset: None,
        display_y_offset: None,
        frame_layout: None,
        aspect_ratio: None,
        horizontal_subsampling: None,
        vertical_subsampling: None,
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
    const H264_PCM_STEREO_MXF: &[u8] =
        include_bytes!("../../../tests/fixtures/mxf/h264-8bit-420-long-gop-128x72-pcm-stereo.mxf");
    const H264_TWO_MONO_MXF: &[u8] =
        include_bytes!("../../../tests/fixtures/mxf/h264-8bit-420-long-gop-128x72-two-mono.mxf");

    #[test]
    fn parses_8bit_h264_mxf_and_extracts_access_unit() {
        let source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");

        assert_eq!(source.edit_rate, Some(Rational::new(25, 1).expect("rate")));
        assert_eq!(source.timecode.as_ref().map(|tc| tc.start_frame), Some(0));
        assert_eq!(source.tracks.len(), 1);
        assert_eq!(source.operational_pattern, Some(OP1A_OPERATIONAL_PATTERN));
        assert_eq!(source.packages.len(), 2);
        assert_eq!(source.index_segments.len(), 1);
        assert_eq!(source.rip.len(), 3);
        assert_eq!(source.index.video.len(), 12);
        assert_eq!(source.index.video[0].random_access, RandomAccess::Yes);
        assert_eq!(source.index.video[0].source, IndexSource::MxfProvided);
        assert!(source
            .index
            .video
            .iter()
            .skip(1)
            .any(|entry| entry.random_access == RandomAccess::No));
        let desc = source.tracks[0].video.as_ref().expect("video descriptor");
        assert_eq!(desc.source, DescriptorSource::MxfMetadata);
        assert_eq!(desc.codec, VideoCodec::H264);
        assert_eq!(desc.coded_width, 128);
        assert_eq!(desc.coded_height, 80);
        assert_eq!(desc.display_height, 72);
        assert_eq!(desc.bit_depth, 8);
        assert_eq!(desc.chroma, ChromaSubsampling::Cs420);
        assert!(source.diagnostics.is_empty());

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
        assert_eq!(desc.source, DescriptorSource::MxfMetadata);
        assert_eq!(desc.bit_depth, 10);
        assert_eq!(desc.chroma, ChromaSubsampling::Cs422);
        assert!(source.diagnostics.is_empty());
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
            data: None,
        };

        assert!(matches!(
            validate_track_reference(TrackId(8), &[track]),
            Err(MxfError::InvalidTrackReference)
        ));
    }

    #[test]
    fn parses_primer_pack_and_resolves_local_tag() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_be_bytes());
        bytes.extend_from_slice(&18_u32.to_be_bytes());
        bytes.extend_from_slice(&0x1001_u16.to_be_bytes());
        bytes.extend_from_slice(&INSTANCE_UID_UL.0);

        let primer = parse_primer_pack(&bytes).expect("primer");
        assert_eq!(primer.resolve(0x1001), Some(INSTANCE_UID_UL));
    }

    #[test]
    fn different_local_tags_resolve_to_same_ul_semantics() {
        let primer_a = PrimerPack {
            mappings: vec![PrimerMapping {
                local_tag: 0x1001,
                property: INSTANCE_UID_UL,
            }],
        };
        let primer_b = PrimerPack {
            mappings: vec![PrimerMapping {
                local_tag: 0x2002,
                property: INSTANCE_UID_UL,
            }],
        };
        let uid = [7_u8; 16];
        let mut value_a = Vec::new();
        value_a.extend_from_slice(&0x1001_u16.to_be_bytes());
        value_a.extend_from_slice(&16_u16.to_be_bytes());
        value_a.extend_from_slice(&uid);
        let mut value_b = Vec::new();
        value_b.extend_from_slice(&0x2002_u16.to_be_bytes());
        value_b.extend_from_slice(&16_u16.to_be_bytes());
        value_b.extend_from_slice(&uid);

        let set_a = parse_local_metadata_set(PREFACE_SET_KEY, &value_a, &primer_a).expect("set a");
        let set_b = parse_local_metadata_set(PREFACE_SET_KEY, &value_b, &primer_b).expect("set b");

        assert_eq!(set_a.instance_uid, set_b.instance_uid);
    }

    #[test]
    fn resolves_strong_references_and_rejects_missing_reference() {
        let package_uid = InstanceUid([1_u8; 16]);
        let track_uid = InstanceUid([2_u8; 16]);
        let package = MetadataSet {
            key: MATERIAL_PACKAGE_SET_KEY,
            instance_uid: Some(package_uid),
            properties: vec![MetadataProperty {
                local_tag: 0x4403,
                property: PACKAGE_TRACKS_UL,
                value: strong_ref_batch(&[track_uid]),
            }],
        };
        let track = MetadataSet {
            key: Ul([0_u8; 16]),
            instance_uid: Some(track_uid),
            properties: Vec::new(),
        };

        assert!(validate_metadata_graph(&[package.clone(), track]).is_ok());
        assert!(matches!(
            validate_metadata_graph(&[package]),
            Err(MxfError::MissingMetadataReference(_))
        ));
    }

    #[test]
    fn rejects_duplicate_instance_uid() {
        let uid = InstanceUid([9_u8; 16]);
        let first = MetadataSet {
            key: PREFACE_SET_KEY,
            instance_uid: Some(uid),
            properties: Vec::new(),
        };
        let second = MetadataSet {
            key: CONTENT_STORAGE_SET_KEY,
            instance_uid: Some(uid),
            properties: Vec::new(),
        };

        assert!(matches!(
            validate_metadata_graph(&[first, second]),
            Err(MxfError::DuplicateInstanceUid(_))
        ));
    }

    #[test]
    fn parses_pcm_audio_descriptor() {
        let source = MediaSource::parse(H264_PCM_STEREO_MXF).expect("parse MXF");
        let audio_tracks = source
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .collect::<Vec<_>>();

        assert_eq!(audio_tracks.len(), 1);
        let audio = audio_tracks[0].audio.as_ref().expect("audio descriptor");
        assert_eq!(audio.channels, Some(2));
        assert_eq!(
            audio.sample_rate,
            Some(Rational::new(48000, 1).expect("rate"))
        );
        assert_eq!(audio.bit_depth, Some(16));
    }

    #[test]
    fn models_multiple_audio_tracks() {
        let source = MediaSource::parse(H264_TWO_MONO_MXF).expect("parse MXF");
        let audio_tracks = source
            .tracks
            .iter()
            .filter(|track| track.kind == TrackKind::Audio)
            .collect::<Vec<_>>();

        assert_eq!(audio_tracks.len(), 2);
        assert_eq!(audio_tracks[0].id, TrackId(3));
        assert_eq!(audio_tracks[1].id, TrackId(4));
        assert_eq!(
            audio_tracks[0]
                .audio
                .as_ref()
                .and_then(|audio| audio.channels),
            Some(1)
        );
        assert_eq!(
            audio_tracks[1]
                .audio
                .as_ref()
                .and_then(|audio| audio.channels),
            Some(1)
        );
    }

    #[test]
    fn parses_data_essence_descriptor() {
        let set = MetadataSet {
            key: DATA_ESSENCE_DESCRIPTOR_SET_KEY,
            instance_uid: Some(InstanceUid([4_u8; 16])),
            properties: vec![
                property_u32(0x3006, 7),
                MetadataProperty {
                    local_tag: 0x3001,
                    property: Ul([0_u8; 16]),
                    value: [50_u32.to_be_bytes(), 1_u32.to_be_bytes()].concat(),
                },
                MetadataProperty {
                    local_tag: 0x3004,
                    property: Ul([0_u8; 16]),
                    value: [0x06_u8; 16].to_vec(),
                },
            ],
        };
        let tracks = parse_tracks_from_metadata(&[set]).expect("tracks");

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].kind, TrackKind::Data);
        assert_eq!(tracks[0].id, TrackId(7));
        let data = tracks[0].data.as_ref().expect("data descriptor");
        assert_eq!(data.sample_rate, Some(Rational::new(50, 1).expect("rate")));
        assert_eq!(data.essence, Some(Ul([0x06_u8; 16])));
    }

    #[test]
    fn parses_timecode_component_model_with_non_zero_start() {
        let set = MetadataSet {
            key: Ul([
                0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x01, 0x01, 0x01, 0x01,
                0x14, 0x00,
            ]),
            instance_uid: Some(InstanceUid([3_u8; 16])),
            properties: vec![
                property_i64(0x1501, 90_000),
                property_u16(0x1502, 30),
                MetadataProperty {
                    local_tag: 0x1503,
                    property: Ul([0_u8; 16]),
                    value: vec![1],
                },
            ],
        };
        let timecode = parse_timecode_from_metadata(&[set], Rational::new(30, 1).expect("rate"))
            .expect("parse")
            .expect("timecode");

        assert_eq!(timecode.start_frame, 90_000);
        assert_eq!(timecode.edit_rate, Rational::new(30, 1).expect("rate"));
        assert!(timecode.drop_frame);
    }

    #[test]
    fn parses_index_table_signed_offsets_and_body_index_sid() {
        let source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");
        let segment = source.index_segments.first().expect("index segment");

        assert_eq!(segment.body_sid, 1);
        assert_eq!(segment.index_sid, 2);
        assert_eq!(segment.entries.len(), 12);
        assert!(segment
            .entries
            .iter()
            .any(|entry| entry.key_frame_offset < 0));
    }

    #[test]
    fn parses_signed_temporal_and_key_frame_offsets() {
        let mut value = Vec::new();
        value.extend_from_slice(&1_u32.to_be_bytes());
        value.extend_from_slice(&15_u32.to_be_bytes());
        value.push(0xff);
        value.push(0xfe);
        value.push(0);
        value.extend_from_slice(&123_u64.to_be_bytes());
        value.extend_from_slice(&0_u32.to_be_bytes());

        let entries = parse_index_entry_array(&value).expect("index entries");

        assert_eq!(entries[0].temporal_offset, -1);
        assert_eq!(entries[0].key_frame_offset, -2);
    }

    #[test]
    fn parses_rip_entries() {
        let source = MediaSource::parse(H264_8BIT_MXF).expect("parse MXF");

        assert_eq!(source.rip.len(), 3);
        assert_eq!(source.rip[1].body_sid, 1);
        assert_eq!(source.rip[1].byte_offset, 5120);
        assert_eq!(source.rip[2].body_sid, 0);
        assert_eq!(source.rip[2].byte_offset, 22016);
    }

    #[test]
    fn reports_metadata_h264_mismatch_without_overwriting() {
        let mut metadata = descriptor_from_h264(
            &parse_annex_b_access_unit(
                MediaSource::parse(H264_8BIT_MXF)
                    .expect("parse")
                    .extract_video_access_unit(H264_8BIT_MXF, 0)
                    .expect("extract")
                    .as_slice(),
            )
            .expect("h264")
            .desc,
            H264Profile::Main,
        );
        let h264 = metadata.clone();
        metadata.bit_depth = 10;

        let diagnostics = compare_video_descriptor_to_h264(&metadata, &h264);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].kind, MetadataDiagnosticKind::Mismatch);
    }

    #[test]
    fn rejects_excessive_reference_batch_count() {
        let mut value = Vec::new();
        value.extend_from_slice(&((MAX_REFERENCE_BATCH_COUNT + 1) as u32).to_be_bytes());
        value.extend_from_slice(&16_u32.to_be_bytes());

        assert!(matches!(
            parse_strong_reference_batch(&value),
            Err(MxfError::ExcessiveReferenceBatchCount)
        ));
    }

    #[test]
    fn rejects_truncated_index_entry_array() {
        let mut value = Vec::new();
        value.extend_from_slice(&1_u32.to_be_bytes());
        value.extend_from_slice(&15_u32.to_be_bytes());
        value.extend_from_slice(&[0_u8; 4]);

        assert!(matches!(
            parse_index_entry_array(&value),
            Err(MxfError::TruncatedIndexTable)
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

    fn strong_ref_batch(refs: &[InstanceUid]) -> Vec<u8> {
        let mut value = Vec::new();
        value.extend_from_slice(&(refs.len() as u32).to_be_bytes());
        value.extend_from_slice(&16_u32.to_be_bytes());
        for reference in refs {
            value.extend_from_slice(&reference.0);
        }
        value
    }

    fn property_u16(local_tag: u16, value: u16) -> MetadataProperty {
        MetadataProperty {
            local_tag,
            property: Ul([0_u8; 16]),
            value: value.to_be_bytes().to_vec(),
        }
    }

    fn property_u32(local_tag: u16, value: u32) -> MetadataProperty {
        MetadataProperty {
            local_tag,
            property: Ul([0_u8; 16]),
            value: value.to_be_bytes().to_vec(),
        }
    }

    fn property_i64(local_tag: u16, value: i64) -> MetadataProperty {
        MetadataProperty {
            local_tag,
            property: Ul([0_u8; 16]),
            value: value.to_be_bytes().to_vec(),
        }
    }
}
