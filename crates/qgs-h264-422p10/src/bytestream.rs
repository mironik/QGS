use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NalHeader {
    pub nal_ref_idc: u8,
    pub nal_unit_type: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NalUnit {
    pub header: NalHeader,
    pub ebsp: Vec<u8>,
    pub rbsp: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ByteStreamError {
    MissingStartCode,
    EmptyNal,
    ForbiddenZeroBit,
    MalformedEscape,
}

impl fmt::Display for ByteStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStartCode => write!(f, "Annex B stream has no start code"),
            Self::EmptyNal => write!(f, "Annex B stream contains an empty NAL"),
            Self::ForbiddenZeroBit => write!(f, "NAL forbidden_zero_bit is set"),
            Self::MalformedEscape => write!(f, "malformed H.264 emulation-prevention byte"),
        }
    }
}

impl std::error::Error for ByteStreamError {}

pub fn split_annex_b_access_unit(data: &[u8]) -> Result<Vec<NalUnit>, ByteStreamError> {
    let starts = start_code_offsets(data);
    if starts.is_empty() {
        return Err(ByteStreamError::MissingStartCode);
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let payload_start = start + start_code_len(&data[*start..]);
            let end = starts.get(index + 1).copied().unwrap_or(data.len());
            parse_nal(&data[payload_start..end])
        })
        .collect()
}

pub fn rbsp_from_ebsp(ebsp: &[u8]) -> Result<Vec<u8>, ByteStreamError> {
    let mut rbsp = Vec::with_capacity(ebsp.len());
    let mut zero_run = 0_u8;
    let mut index = 0_usize;
    while index < ebsp.len() {
        let byte = ebsp[index];
        if zero_run >= 2 && byte == 0x03 {
            let next = ebsp.get(index + 1).copied().unwrap_or(0);
            if next > 0x03 {
                return Err(ByteStreamError::MalformedEscape);
            }
            zero_run = 0;
            index += 1;
            continue;
        }
        rbsp.push(byte);
        zero_run = if byte == 0 {
            zero_run.saturating_add(1)
        } else {
            0
        };
        index += 1;
    }
    Ok(rbsp)
}

fn parse_nal(payload: &[u8]) -> Result<NalUnit, ByteStreamError> {
    let payload = trim_trailing_zero_bytes(payload);
    let (&header, ebsp) = payload.split_first().ok_or(ByteStreamError::EmptyNal)?;
    if header & 0x80 != 0 {
        return Err(ByteStreamError::ForbiddenZeroBit);
    }
    let header = NalHeader {
        nal_ref_idc: (header >> 5) & 0x03,
        nal_unit_type: header & 0x1f,
    };
    Ok(NalUnit {
        header,
        ebsp: ebsp.to_vec(),
        rbsp: rbsp_from_ebsp(ebsp)?,
    })
}

fn trim_trailing_zero_bytes(mut payload: &[u8]) -> &[u8] {
    while payload.last() == Some(&0) {
        payload = &payload[..payload.len() - 1];
    }
    payload
}

fn start_code_offsets(data: &[u8]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut index = 0_usize;
    while index + 3 <= data.len() {
        if data[index..].starts_with(&[0, 0, 1]) || data[index..].starts_with(&[0, 0, 0, 1]) {
            starts.push(index);
            index += start_code_len(&data[index..]);
        } else {
            index += 1;
        }
    }
    starts
}

fn start_code_len(data: &[u8]) -> usize {
    if data.starts_with(&[0, 0, 0, 1]) {
        4
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_annex_b_extracts_headers_and_rbsp() {
        let data = [0, 0, 0, 1, 0x67, 0, 0, 3, 1, 0, 0, 1, 0x41, 0xaa];

        let nals = split_annex_b_access_unit(&data).unwrap();

        assert_eq!(nals.len(), 2);
        assert_eq!(nals[0].header.nal_unit_type, 7);
        assert_eq!(nals[0].rbsp, vec![0, 0, 1]);
        assert_eq!(nals[1].header.nal_ref_idc, 2);
        assert_eq!(nals[1].header.nal_unit_type, 1);
        assert_eq!(nals[1].rbsp, vec![0xaa]);
    }
}
