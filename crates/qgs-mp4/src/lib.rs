#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::path::Path;

use mp4::{MediaType, Mp4Reader};
use qgs_protocol::{ChromaSubsampling, H264Profile, VideoCodec};

pub const MAX_MP4_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_MP4_TRACKS: usize = 32;
pub const MAX_MP4_VIDEO_SAMPLES: u32 = 240_000;
pub const MAX_MP4_SAMPLE_BYTES: usize = 16 * 1024 * 1024;

type AvcParameterSets<'a> = (&'a [Vec<u8>], &'a [Vec<u8>]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

impl Rational {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, Mp4Error> {
        if denominator == 0 {
            return Err(Mp4Error::InvalidRational);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Mp4TrackKind {
    Video,
    Audio,
    Data,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mp4TrackInfo {
    pub track_id: u32,
    pub handler: String,
    pub kind: Mp4TrackKind,
    pub codec: String,
    pub timescale: u32,
    pub duration_units: u64,
    pub sample_count: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct Mp4VideoSample {
    pub sample_index: u32,
    pub dts: u64,
    pub pts: i64,
    pub duration: u32,
    pub composition_offset: i32,
    pub is_sync: bool,
    pub annex_b: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Mp4VideoTrack {
    pub track_id: u32,
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    pub duration_units: u64,
    pub frame_rate: Rational,
    pub nal_length_size: usize,
    pub sps_count: usize,
    pub pps_count: usize,
    pub samples: Vec<Mp4VideoSample>,
}

#[derive(Clone, Debug)]
pub struct Mp4Source {
    pub major_brand: String,
    pub compatible_brands: Vec<String>,
    pub movie_timescale: u32,
    pub tracks: Vec<Mp4TrackInfo>,
    pub video: Option<Mp4VideoTrack>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaHealth {
    Valid,
    ValidButUnsupported,
    DamagedRecoverable,
    DamagedUnrecoverable,
    FatalContainerError,
}

impl MediaHealth {
    pub fn from_open_error(error: &Mp4Error) -> Self {
        match error {
            Mp4Error::SampleNormalization { .. }
            | Mp4Error::MalformedSample(_)
            | Mp4Error::MalformedSampleOwned(_)
            | Mp4Error::H264(_) => Self::DamagedUnrecoverable,
            Mp4Error::MissingAvcConfig
            | Mp4Error::InvalidNalLengthSize { .. }
            | Mp4Error::NoVideoSamples => Self::ValidButUnsupported,
            Mp4Error::Io(_)
            | Mp4Error::Parse(_)
            | Mp4Error::FileTooLarge { .. }
            | Mp4Error::TooManyTracks { .. }
            | Mp4Error::TooManySamples { .. }
            | Mp4Error::SampleTooLarge { .. }
            | Mp4Error::MissingTrack { .. }
            | Mp4Error::MissingSample { .. }
            | Mp4Error::InvalidRational
            | Mp4Error::TimestampOverflow => Self::FatalContainerError,
        }
    }
}

impl Mp4Source {
    pub fn open(path: &Path) -> Result<Self, Mp4Error> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        if size > MAX_MP4_FILE_BYTES {
            return Err(Mp4Error::FileTooLarge { size });
        }
        let mut reader = Mp4Reader::read_header(BufReader::new(file), size)?;
        let major_brand = fourcc_to_string(reader.major_brand());
        let compatible_brands = reader
            .compatible_brands()
            .iter()
            .map(fourcc_to_string)
            .collect::<Vec<_>>();
        let movie_timescale = reader.timescale();

        let mut tracks = Vec::new();
        let mut video_track_id = None;
        let track_ids = {
            let map = reader.tracks();
            if map.len() > MAX_MP4_TRACKS {
                return Err(Mp4Error::TooManyTracks { count: map.len() });
            }
            map.keys().copied().collect::<Vec<_>>()
        };

        for track_id in track_ids.iter().copied() {
            let track = reader
                .tracks()
                .get(&track_id)
                .ok_or(Mp4Error::MissingTrack { track_id })?;
            let handler = fourcc_to_string(&track.trak.mdia.hdlr.handler_type);
            let kind = match handler.as_str() {
                "vide" => Mp4TrackKind::Video,
                "soun" => Mp4TrackKind::Audio,
                "meta" | "mdir" | "mdta" => Mp4TrackKind::Data,
                _ => Mp4TrackKind::Other,
            };
            let codec = match track.media_type() {
                Ok(MediaType::H264) => "H264".to_string(),
                Ok(MediaType::AAC) => "AAC".to_string(),
                Ok(other) => format!("{other:?}"),
                Err(_) => track
                    .box_type()
                    .map(|fourcc| fourcc_to_string(&fourcc))
                    .unwrap_or_else(|_| "unknown".to_string()),
            };
            let sample_count = track.sample_count();
            let duration_units = track.trak.mdia.mdhd.duration;
            let sample_rate = if kind == Mp4TrackKind::Audio {
                track.sample_freq_index().ok().map(|freq| freq.freq())
            } else {
                None
            };
            let channels = if kind == Mp4TrackKind::Audio {
                track
                    .channel_config()
                    .ok()
                    .and_then(channel_count_from_config)
            } else {
                None
            };
            if kind == Mp4TrackKind::Video && codec == "H264" {
                video_track_id = Some(track_id);
            }
            tracks.push(Mp4TrackInfo {
                track_id,
                handler,
                kind,
                codec,
                timescale: track.timescale(),
                duration_units,
                sample_count,
                width: (track.width() > 0).then(|| u32::from(track.width())),
                height: (track.height() > 0).then(|| u32::from(track.height())),
                sample_rate,
                channels,
            });
        }
        tracks.sort_by_key(|track| track.track_id);

        let video = if let Some(track_id) = video_track_id {
            Some(read_video_track(&mut reader, track_id)?)
        } else {
            None
        };

        Ok(Self {
            major_brand,
            compatible_brands,
            movie_timescale,
            tracks,
            video,
        })
    }
}

pub fn nearest_random_access_before(video: &Mp4VideoTrack, target_index: u32) -> Option<u32> {
    video
        .samples
        .iter()
        .filter(|sample| sample.sample_index <= target_index && sample.is_sync)
        .map(|sample| sample.sample_index)
        .next_back()
}

pub fn classify_video_track(video: &Mp4VideoTrack) -> Result<Mp4H264Summary, Mp4Error> {
    let mut state = qgs_codec_h264::H264DecoderState::new();
    let mut picture_counts = BTreeMap::new();
    let mut idr_positions = Vec::new();
    let mut first = None;
    for sample in &video.samples {
        let parsed = state
            .parse_access_unit(&sample.annex_b)
            .map_err(Mp4Error::H264)?;
        if sample.sample_index == 0 {
            first = Some((parsed.profile, parsed.desc.clone()));
        }
        if parsed.slices.iter().any(|slice| slice.idr) {
            idr_positions.push(sample.sample_index);
        }
        if let Some(kind) = parsed
            .slices
            .first()
            .map(|slice| format!("{:?}", slice.kind))
        {
            *picture_counts.entry(kind).or_insert(0_usize) += 1;
        }
        state.finish_picture(&parsed).map_err(Mp4Error::H264)?;
    }
    let (profile, desc) = first.ok_or(Mp4Error::NoVideoSamples)?;
    Ok(Mp4H264Summary {
        codec: VideoCodec::H264,
        profile,
        bit_depth: desc.bit_depth.get(),
        chroma: desc.chroma,
        width: desc.visible_region.width,
        height: desc.visible_region.height,
        coded_width: desc.coded_width,
        coded_height: desc.coded_height,
        picture_counts,
        idr_positions,
    })
}

#[derive(Clone, Debug)]
pub struct Mp4H264Summary {
    pub codec: VideoCodec,
    pub profile: H264Profile,
    pub bit_depth: u8,
    pub chroma: ChromaSubsampling,
    pub width: u32,
    pub height: u32,
    pub coded_width: u32,
    pub coded_height: u32,
    pub picture_counts: BTreeMap<String, usize>,
    pub idr_positions: Vec<u32>,
}

fn read_video_track<R: std::io::Read + std::io::Seek>(
    reader: &mut Mp4Reader<R>,
    track_id: u32,
) -> Result<Mp4VideoTrack, Mp4Error> {
    let (width, height, timescale, duration_units, nal_length_size, sps, pps, sample_count) = {
        let track = reader
            .tracks()
            .get(&track_id)
            .ok_or(Mp4Error::MissingTrack { track_id })?;
        let avc1 = track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()
            .ok_or(Mp4Error::MissingAvcConfig)?;
        let nal_length_size = usize::from(avc1.avcc.length_size_minus_one) + 1;
        if !(1..=4).contains(&nal_length_size) {
            return Err(Mp4Error::InvalidNalLengthSize { nal_length_size });
        }
        let sps = avc1
            .avcc
            .sequence_parameter_sets
            .iter()
            .map(|nal| nal.bytes.clone())
            .collect::<Vec<_>>();
        let pps = avc1
            .avcc
            .picture_parameter_sets
            .iter()
            .map(|nal| nal.bytes.clone())
            .collect::<Vec<_>>();
        if sps.is_empty() || pps.is_empty() {
            return Err(Mp4Error::MissingAvcConfig);
        }
        (
            u32::from(track.width()),
            u32::from(track.height()),
            track.timescale(),
            track.trak.mdia.mdhd.duration,
            nal_length_size,
            sps,
            pps,
            track.sample_count(),
        )
    };
    if sample_count > MAX_MP4_VIDEO_SAMPLES {
        return Err(Mp4Error::TooManySamples {
            count: sample_count,
        });
    }

    let mut samples = Vec::with_capacity(sample_count as usize);
    for sample_id in 1..=sample_count {
        let Some(sample) = reader.read_sample(track_id, sample_id)? else {
            return Err(Mp4Error::MissingSample { sample_id });
        };
        if sample.bytes.len() > MAX_MP4_SAMPLE_BYTES {
            return Err(Mp4Error::SampleTooLarge {
                sample_id,
                size: sample.bytes.len(),
            });
        }
        let sample_index = sample_id - 1;
        let include_parameter_sets = sample.is_sync;
        let annex_b = avc_sample_to_annex_b(
            sample.bytes.as_ref(),
            nal_length_size,
            include_parameter_sets.then_some((&sps, &pps)),
        )
        .map_err(|err| Mp4Error::SampleNormalization {
            sample_id,
            reason: err.to_string(),
        })?;
        let pts = i64::try_from(sample.start_time)
            .map_err(|_| Mp4Error::TimestampOverflow)?
            .checked_add(i64::from(sample.rendering_offset))
            .ok_or(Mp4Error::TimestampOverflow)?;
        samples.push(Mp4VideoSample {
            sample_index,
            dts: sample.start_time,
            pts,
            duration: sample.duration,
            composition_offset: sample.rendering_offset,
            is_sync: sample.is_sync,
            annex_b,
        });
    }

    let frame_rate = if sample_count == 0 || duration_units == 0 {
        Rational::new(0, 1)?
    } else {
        Rational::new(
            sample_count
                .checked_mul(timescale)
                .ok_or(Mp4Error::TimestampOverflow)?,
            u32::try_from(duration_units).map_err(|_| Mp4Error::TimestampOverflow)?,
        )?
    };

    Ok(Mp4VideoTrack {
        track_id,
        width,
        height,
        timescale,
        duration_units,
        frame_rate,
        nal_length_size,
        sps_count: sps.len(),
        pps_count: pps.len(),
        samples,
    })
}

pub fn avc_sample_to_annex_b(
    sample: &[u8],
    nal_length_size: usize,
    parameter_sets: Option<AvcParameterSets<'_>>,
) -> Result<Vec<u8>, Mp4Error> {
    if !(1..=4).contains(&nal_length_size) {
        return Err(Mp4Error::InvalidNalLengthSize { nal_length_size });
    }
    let mut output = Vec::new();
    if let Some((sps, pps)) = parameter_sets {
        for nal in sps.iter().chain(pps) {
            append_annex_b_nal(&mut output, nal)?;
        }
    }
    let mut offset = 0_usize;
    while offset < sample.len() {
        let length_end = offset
            .checked_add(nal_length_size)
            .ok_or(Mp4Error::MalformedSample("NAL length overflow"))?;
        if length_end > sample.len() {
            return Err(Mp4Error::MalformedSample("truncated NAL length"));
        }
        let mut nal_len = 0_usize;
        for byte in &sample[offset..length_end] {
            nal_len = nal_len
                .checked_shl(8)
                .ok_or(Mp4Error::MalformedSample("NAL length overflow"))?
                | usize::from(*byte);
        }
        if nal_len == 0 {
            return Err(Mp4Error::MalformedSample("zero-length NAL"));
        }
        let nal_end = length_end
            .checked_add(nal_len)
            .ok_or(Mp4Error::MalformedSample("NAL payload overflow"))?;
        if nal_end > sample.len() {
            return Err(Mp4Error::MalformedSampleOwned(format!(
                "truncated NAL payload at offset {offset}, nal_len {nal_len}, sample_len {}",
                sample.len()
            )));
        }
        append_annex_b_nal(&mut output, &sample[length_end..nal_end])?;
        offset = nal_end;
    }
    Ok(output)
}

fn append_annex_b_nal(output: &mut Vec<u8>, nal: &[u8]) -> Result<(), Mp4Error> {
    if nal.is_empty() {
        return Err(Mp4Error::MalformedSample("empty parameter set"));
    }
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(nal);
    Ok(())
}

fn channel_count_from_config(config: mp4::ChannelConfig) -> Option<u16> {
    match config {
        mp4::ChannelConfig::Mono => Some(1),
        mp4::ChannelConfig::Stereo => Some(2),
        mp4::ChannelConfig::Three => Some(3),
        mp4::ChannelConfig::Four => Some(4),
        mp4::ChannelConfig::Five => Some(5),
        mp4::ChannelConfig::FiveOne => Some(6),
        mp4::ChannelConfig::SevenOne => Some(8),
    }
}

fn fourcc_to_string(fourcc: &mp4::FourCC) -> String {
    String::from_utf8_lossy(&fourcc.value).to_string()
}

#[derive(Debug)]
pub enum Mp4Error {
    Io(std::io::Error),
    Parse(mp4::Error),
    H264(qgs_codec_h264::H264Error),
    FileTooLarge { size: u64 },
    TooManyTracks { count: usize },
    TooManySamples { count: u32 },
    SampleTooLarge { sample_id: u32, size: usize },
    MissingTrack { track_id: u32 },
    MissingSample { sample_id: u32 },
    MissingAvcConfig,
    InvalidNalLengthSize { nal_length_size: usize },
    MalformedSample(&'static str),
    MalformedSampleOwned(String),
    SampleNormalization { sample_id: u32, reason: String },
    InvalidRational,
    NoVideoSamples,
    TimestampOverflow,
}

impl std::fmt::Display for Mp4Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "MP4 I/O error: {err}"),
            Self::Parse(err) => write!(f, "MP4 parse error: {err}"),
            Self::H264(err) => write!(f, "MP4 H.264 classification error: {err}"),
            Self::FileTooLarge { size } => write!(f, "MP4 file too large: {size} bytes"),
            Self::TooManyTracks { count } => write!(f, "too many MP4 tracks: {count}"),
            Self::TooManySamples { count } => write!(f, "too many MP4 video samples: {count}"),
            Self::SampleTooLarge { sample_id, size } => {
                write!(f, "MP4 sample {sample_id} too large: {size} bytes")
            }
            Self::MissingTrack { track_id } => write!(f, "missing MP4 track {track_id}"),
            Self::MissingSample { sample_id } => write!(f, "missing MP4 sample {sample_id}"),
            Self::MissingAvcConfig => write!(f, "missing MP4 avcC configuration"),
            Self::InvalidNalLengthSize { nal_length_size } => {
                write!(f, "invalid MP4 AVC NAL length size: {nal_length_size}")
            }
            Self::MalformedSample(reason) => write!(f, "malformed MP4 AVC sample: {reason}"),
            Self::MalformedSampleOwned(reason) => write!(f, "malformed MP4 AVC sample: {reason}"),
            Self::SampleNormalization { sample_id, reason } => {
                write!(f, "malformed MP4 AVC sample {sample_id}: {reason}")
            }
            Self::InvalidRational => write!(f, "invalid rational"),
            Self::NoVideoSamples => write!(f, "MP4 video track has no samples"),
            Self::TimestampOverflow => write!(f, "MP4 timestamp overflow"),
        }
    }
}

impl std::error::Error for Mp4Error {}

impl From<std::io::Error> for Mp4Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<mp4::Error> for Mp4Error {
    fn from(value: mp4::Error) -> Self {
        Self::Parse(value)
    }
}

pub fn sniff_top_level_boxes(path: &Path) -> Result<Vec<String>, Mp4Error> {
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    if size > MAX_MP4_FILE_BYTES {
        return Err(Mp4Error::FileTooLarge { size });
    }
    let mut boxes = Vec::new();
    let mut offset = 0_u64;
    while offset.checked_add(8).ok_or(Mp4Error::TimestampOverflow)? <= size {
        let mut header = [0_u8; 8];
        std::io::Read::read_exact(&mut file, &mut header)?;
        let box_size = u64::from(u32::from_be_bytes([
            header[0], header[1], header[2], header[3],
        ]));
        let name = String::from_utf8_lossy(&header[4..8]).to_string();
        boxes.push(name);
        let real_size = if box_size == 1 {
            let mut large = [0_u8; 8];
            std::io::Read::read_exact(&mut file, &mut large)?;
            u64::from_be_bytes(large)
        } else if box_size == 0 {
            break;
        } else {
            box_size
        };
        if real_size < 8 {
            return Err(Mp4Error::MalformedSample("invalid box size"));
        }
        offset = offset
            .checked_add(real_size)
            .ok_or(Mp4Error::TimestampOverflow)?;
        if offset > size {
            return Err(Mp4Error::MalformedSample("box extends beyond EOF"));
        }
        file.seek(SeekFrom::Start(offset))?;
    }
    Ok(boxes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avc_sample_normalization_prepends_parameter_sets() {
        let sps = vec![vec![0x67, 1, 2]];
        let pps = vec![vec![0x68, 3]];
        let sample = [0, 0, 0, 2, 0x65, 0xaa];

        let annex_b = avc_sample_to_annex_b(&sample, 4, Some((&sps, &pps))).expect("annex b");

        assert_eq!(
            annex_b,
            vec![0, 0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 0xaa]
        );
    }

    #[test]
    fn avc_sample_normalization_rejects_truncated_nal() {
        let sample = [0, 0, 0, 4, 0x65];

        let error = avc_sample_to_annex_b(&sample, 4, None).expect_err("truncated NAL");

        assert!(error.to_string().contains("truncated NAL payload"));
    }

    #[test]
    fn avc_sample_normalization_rejects_zero_length_nal() {
        let sample = [0, 0];

        assert!(matches!(
            avc_sample_to_annex_b(&sample, 2, None),
            Err(Mp4Error::MalformedSample("zero-length NAL"))
        ));
    }

    #[test]
    fn zero_length_nal_maps_to_damaged_unrecoverable_health() {
        let error = Mp4Error::SampleNormalization {
            sample_id: 97,
            reason: Mp4Error::MalformedSample("zero-length NAL").to_string(),
        };

        assert_eq!(
            MediaHealth::from_open_error(&error),
            MediaHealth::DamagedUnrecoverable
        );
    }

    #[test]
    fn rational_rejects_zero_denominator() {
        assert!(matches!(
            Rational::new(1, 0),
            Err(Mp4Error::InvalidRational)
        ));
    }
}
