use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BitstreamError {
    EndOfRbsp,
    TooManyBits,
    ExpGolombOverflow,
}

impl fmt::Display for BitstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndOfRbsp => write!(f, "unexpected end of RBSP"),
            Self::TooManyBits => write!(f, "bit read exceeds supported width"),
            Self::ExpGolombOverflow => write!(f, "Exp-Golomb value overflow"),
        }
    }
}

impl std::error::Error for BitstreamError {}

#[derive(Clone, Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    pub fn bit_position(&self) -> usize {
        self.bit_pos
    }

    pub fn read_bit(&mut self) -> Result<bool, BitstreamError> {
        if self.bit_pos >= self.data.len().saturating_mul(8) {
            return Err(BitstreamError::EndOfRbsp);
        }
        let byte = self.data[self.bit_pos / 8];
        let bit = 7 - (self.bit_pos % 8);
        self.bit_pos += 1;
        Ok(((byte >> bit) & 1) != 0)
    }

    pub fn read_bits(&mut self, count: u8) -> Result<u32, BitstreamError> {
        if count > 32 {
            return Err(BitstreamError::TooManyBits);
        }
        let mut value = 0_u32;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.read_bit()?);
        }
        Ok(value)
    }

    pub fn read_ue(&mut self) -> Result<u32, BitstreamError> {
        let mut leading_zero_bits = 0_u32;
        while !self.read_bit()? {
            leading_zero_bits += 1;
            if leading_zero_bits >= 32 {
                return Err(BitstreamError::ExpGolombOverflow);
            }
        }
        let suffix = if leading_zero_bits == 0 {
            0
        } else {
            self.read_bits(leading_zero_bits as u8)?
        };
        (1_u32 << leading_zero_bits)
            .checked_sub(1)
            .and_then(|prefix| prefix.checked_add(suffix))
            .ok_or(BitstreamError::ExpGolombOverflow)
    }

    pub fn read_se(&mut self) -> Result<i32, BitstreamError> {
        let code_num = self.read_ue()?;
        let magnitude = code_num.div_ceil(2) as i32;
        if code_num % 2 == 0 {
            Ok(-magnitude)
        } else {
            Ok(magnitude)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_bits_and_exp_golomb_values() {
        let mut bits = BitReader::new(&[0b1010_0110, 0b0100_0000]);

        assert!(bits.read_bit().unwrap());
        assert_eq!(bits.read_bits(3).unwrap(), 0b010);
        assert_eq!(bits.read_ue().unwrap(), 2);
        assert_eq!(bits.read_se().unwrap(), 2);
    }
}
