use std::fmt;

use crate::cabac::{CabacContext, CabacDecoder, CabacError};
use crate::motion::MotionVectorQuarterPel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacMvdContexts {
    pub bins: [CabacContext; 7],
}

impl CabacMvdContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self { bins: [context; 7] }
    }

    pub fn new(bins: [CabacContext; 7]) -> Self {
        Self { bins }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CabacMotionVectorContexts {
    pub x: CabacMvdContexts,
    pub y: CabacMvdContexts,
}

impl CabacMotionVectorContexts {
    pub fn flat(context: CabacContext) -> Self {
        Self {
            x: CabacMvdContexts::flat(context),
            y: CabacMvdContexts::flat(context),
        }
    }

    pub fn new(x: [CabacContext; 7], y: [CabacContext; 7]) -> Self {
        Self {
            x: CabacMvdContexts::new(x),
            y: CabacMvdContexts::new(y),
        }
    }
}

pub fn decode_motion_vector_difference(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacMotionVectorContexts,
) -> Result<MotionVectorQuarterPel, CabacMotionError> {
    decode_motion_vector_difference_with_contexts(decoder, contexts, 0, 0)
}

pub fn decode_motion_vector_difference_with_contexts(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacMotionVectorContexts,
    ctx_inc_x: usize,
    ctx_inc_y: usize,
) -> Result<MotionVectorQuarterPel, CabacMotionError> {
    Ok(MotionVectorQuarterPel {
        x: decode_mvd_component(decoder, &mut contexts.x, ctx_inc_x)?,
        y: decode_mvd_component(decoder, &mut contexts.y, ctx_inc_y)?,
    })
}

pub fn decode_mvd_component(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacMvdContexts,
    ctx_inc: usize,
) -> Result<i32, CabacMotionError> {
    let ctx_inc = ctx_inc.min(2);
    if !decoder.decode_decision(&mut contexts.bins[ctx_inc])? {
        return Ok(0);
    }
    let abs = decode_mvd_ueg3_suffix(decoder, contexts)?
        .checked_add(1)
        .ok_or(CabacMotionError::ComponentOverflow)?;
    let sign_negative = decoder.decode_bypass()?;
    let value = i32::try_from(abs).map_err(|_| CabacMotionError::ComponentOverflow)?;
    Ok(if sign_negative { -value } else { value })
}

fn decode_mvd_ueg3_suffix(
    decoder: &mut CabacDecoder<'_>,
    contexts: &mut CabacMvdContexts,
) -> Result<u32, CabacMotionError> {
    const PREFIX_CONTEXT: [usize; 8] = [3, 4, 5, 6, 6, 6, 6, 6];
    let mut code = 0_u32;
    let mut count = 0_usize;
    loop {
        let context_index = PREFIX_CONTEXT[count.min(PREFIX_CONTEXT.len() - 1)];
        let bin = decoder.decode_decision(&mut contexts.bins[context_index])?;
        if !bin {
            return Ok(code);
        }
        code = code
            .checked_add(1)
            .ok_or(CabacMotionError::ComponentOverflow)?;
        count += 1;
        if count == 8 {
            break;
        }
    }
    let suffix = decoder.decode_bypass_exp_golomb(3)?;
    code.checked_add(suffix)
        .ok_or(CabacMotionError::ComponentOverflow)
}

#[derive(Debug)]
pub enum CabacMotionError {
    Cabac(CabacError),
    ComponentOverflow,
}

impl fmt::Display for CabacMotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cabac(error) => write!(f, "{error}"),
            Self::ComponentOverflow => write!(f, "CABAC motion vector component overflowed"),
        }
    }
}

impl std::error::Error for CabacMotionError {}

impl From<CabacError> for CabacMotionError {
    fn from(value: CabacError) -> Self {
        Self::Cabac(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_vector_difference_can_decode_zero_vector() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacMotionVectorContexts::flat(CabacContext::new(0, false));

        let motion = decode_motion_vector_difference(&mut decoder, &mut contexts).unwrap();

        assert_eq!(motion, MotionVectorQuarterPel::ZERO);
    }

    #[test]
    fn mvd_component_decodes_signed_unit_value() {
        let mut decoder = CabacDecoder::new(&[0, 0, 0]).unwrap();
        let mut contexts = CabacMvdContexts {
            bins: [
                CabacContext::new(0, true),
                CabacContext::new(0, false),
                CabacContext::new(0, false),
                CabacContext::new(0, false),
                CabacContext::new(0, false),
                CabacContext::new(0, false),
                CabacContext::new(0, false),
            ],
        };

        assert_eq!(
            decode_mvd_component(&mut decoder, &mut contexts, 0).unwrap(),
            1
        );
    }
}
