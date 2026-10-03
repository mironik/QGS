use std::fmt;

use crate::frame::DecodedFrame422P10;
use crate::macroblock_type::{MacroblockAddress, MacroblockGrid};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionVectorQuarterPel {
    pub x: i32,
    pub y: i32,
}

impl MotionVectorQuarterPel {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub fn is_full_pel(self) -> bool {
        self.x % 4 == 0 && self.y % 4 == 0
    }

    pub fn checked_add(self, delta: Self) -> Result<Self, MotionCompensationError> {
        Ok(Self {
            x: self
                .x
                .checked_add(delta.x)
                .ok_or(MotionCompensationError::MotionVectorOverflow)?,
            y: self
                .y
                .checked_add(delta.y)
                .ok_or(MotionCompensationError::MotionVectorOverflow)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionSample {
    pub vector: MotionVectorQuarterPel,
    pub ref_index: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotionField {
    width_in_mbs: u32,
    height_in_mbs: u32,
    samples: Vec<Option<MotionSample>>,
}

impl MotionField {
    pub fn new(grid: MacroblockGrid) -> Result<Self, MotionCompensationError> {
        let count = usize::try_from(grid.macroblock_count())
            .map_err(|_| MotionCompensationError::OutOfBounds)?;
        Ok(Self {
            width_in_mbs: grid.width_in_mbs,
            height_in_mbs: grid.height_in_mbs,
            samples: vec![None; count],
        })
    }

    pub fn set_inter(
        &mut self,
        address: MacroblockAddress,
        vector: MotionVectorQuarterPel,
        ref_index: u8,
    ) -> Result<(), MotionCompensationError> {
        let index = self.index(address)?;
        self.samples[index] = Some(MotionSample { vector, ref_index });
        Ok(())
    }

    pub fn set_intra(&mut self, address: MacroblockAddress) -> Result<(), MotionCompensationError> {
        let index = self.index(address)?;
        self.samples[index] = None;
        Ok(())
    }

    pub fn predict_l0_16x16(
        &self,
        address: MacroblockAddress,
        ref_index: u8,
    ) -> Result<MotionVectorQuarterPel, MotionCompensationError> {
        let left = self.neighbor(address.x.checked_sub(1), Some(address.y), ref_index)?;
        let top = self.neighbor(Some(address.x), address.y.checked_sub(1), ref_index)?;
        let top_right = self.neighbor(
            address.x.checked_add(1).filter(|x| *x < self.width_in_mbs),
            address.y.checked_sub(1),
            ref_index,
        )?;
        let top_left = self.neighbor(
            address.x.checked_sub(1),
            address.y.checked_sub(1),
            ref_index,
        )?;
        let diagonal = top_right.or(top_left);
        Ok(median_motion_vector(left, top, diagonal))
    }

    fn neighbor(
        &self,
        x: Option<u32>,
        y: Option<u32>,
        ref_index: u8,
    ) -> Result<Option<MotionVectorQuarterPel>, MotionCompensationError> {
        let (Some(x), Some(y)) = (x, y) else {
            return Ok(None);
        };
        if x >= self.width_in_mbs || y >= self.height_in_mbs {
            return Ok(None);
        }
        let index = usize::try_from(y.saturating_mul(self.width_in_mbs).saturating_add(x))
            .map_err(|_| MotionCompensationError::OutOfBounds)?;
        Ok(self
            .samples
            .get(index)
            .copied()
            .flatten()
            .and_then(|sample| (sample.ref_index == ref_index).then_some(sample.vector)))
    }

    fn index(&self, address: MacroblockAddress) -> Result<usize, MotionCompensationError> {
        if address.x >= self.width_in_mbs || address.y >= self.height_in_mbs {
            return Err(MotionCompensationError::OutOfBounds);
        }
        usize::try_from(address.address).map_err(|_| MotionCompensationError::OutOfBounds)
    }
}

fn median_motion_vector(
    left: Option<MotionVectorQuarterPel>,
    top: Option<MotionVectorQuarterPel>,
    diagonal: Option<MotionVectorQuarterPel>,
) -> MotionVectorQuarterPel {
    match (left, top, diagonal) {
        (None, None, None) => MotionVectorQuarterPel::ZERO,
        (Some(value), None, None) | (None, Some(value), None) | (None, None, Some(value)) => value,
        (Some(left), Some(top), None) => median_pair(left, top),
        (Some(left), None, Some(diagonal)) => median_pair(left, diagonal),
        (None, Some(top), Some(diagonal)) => median_pair(top, diagonal),
        (Some(left), Some(top), Some(diagonal)) => MotionVectorQuarterPel {
            x: median3(left.x, top.x, diagonal.x),
            y: median3(left.y, top.y, diagonal.y),
        },
    }
}

fn median_pair(
    first: MotionVectorQuarterPel,
    second: MotionVectorQuarterPel,
) -> MotionVectorQuarterPel {
    MotionVectorQuarterPel {
        x: median3(first.x, second.x, 0),
        y: median3(first.y, second.y, 0),
    }
}

fn median3(a: i32, b: i32, c: i32) -> i32 {
    a + b + c - a.min(b).min(c) - a.max(b).max(c)
}

pub fn predict_inter_16x16(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_luma_prediction(reference, target, dst_luma_x, dst_luma_y, motion)?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_chroma_prediction(reference, target, dst_chroma_x, dst_chroma_y, motion)?;
    Ok(())
}

pub fn predict_bi_inter_16x16(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_luma_prediction_region(
        list0, list1, target, dst_luma_x, dst_luma_y, 16, 16, motion0, motion1,
    )?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_chroma_prediction_region(
        list0,
        list1,
        target,
        dst_chroma_x,
        dst_chroma_y,
        8,
        16,
        motion0,
        motion1,
    )?;
    Ok(())
}

pub fn predict_inter_16x8(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    partition_index: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if partition_index >= 2 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_luma_prediction_region(reference, target, dst_luma_x, dst_luma_y, 16, 8, motion)?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_chroma_prediction_region(reference, target, dst_chroma_x, dst_chroma_y, 8, 8, motion)?;
    Ok(())
}

pub fn predict_bi_inter_16x8(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    partition_index: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if partition_index >= 2 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_luma_prediction_region(
        list0, list1, target, dst_luma_x, dst_luma_y, 16, 8, motion0, motion1,
    )?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_chroma_prediction_region(
        list0,
        list1,
        target,
        dst_chroma_x,
        dst_chroma_y,
        8,
        8,
        motion0,
        motion1,
    )?;
    Ok(())
}

pub fn predict_inter_8x16(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    partition_index: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if partition_index >= 2 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_luma_prediction_region(reference, target, dst_luma_x, dst_luma_y, 8, 16, motion)?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .and_then(|value| value.checked_add(partition_index * 4))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_chroma_prediction_region(reference, target, dst_chroma_x, dst_chroma_y, 4, 16, motion)?;
    Ok(())
}

pub fn predict_inter_8x8(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    subblock_index: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if subblock_index >= 4 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let sub_x = subblock_index % 2;
    let sub_y = subblock_index / 2;
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_x * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_y * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_luma_prediction_region(reference, target, dst_luma_x, dst_luma_y, 8, 8, motion)?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .and_then(|value| value.checked_add(sub_x * 4))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_y * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_chroma_prediction_region(reference, target, dst_chroma_x, dst_chroma_y, 4, 8, motion)?;
    Ok(())
}

pub fn predict_inter_region(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_luma_x: usize,
    dst_luma_y: usize,
    luma_width: usize,
    luma_height: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if !luma_width.is_multiple_of(2) {
        return Err(MotionCompensationError::OutOfBounds);
    }
    copy_luma_prediction_region(
        reference,
        target,
        dst_luma_x,
        dst_luma_y,
        luma_width,
        luma_height,
        motion,
    )?;
    copy_chroma_prediction_region(
        reference,
        target,
        dst_luma_x / 2,
        dst_luma_y,
        luma_width / 2,
        luma_height,
        motion,
    )?;
    Ok(())
}

pub fn predict_bi_inter_8x16(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    partition_index: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if partition_index >= 2 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .and_then(|value| value.checked_add(partition_index * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_luma_prediction_region(
        list0, list1, target, dst_luma_x, dst_luma_y, 8, 16, motion0, motion1,
    )?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .and_then(|value| value.checked_add(partition_index * 4))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_chroma_prediction_region(
        list0,
        list1,
        target,
        dst_chroma_x,
        dst_chroma_y,
        4,
        16,
        motion0,
        motion1,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn predict_bi_inter_8x8(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_mb_x: usize,
    dst_mb_y: usize,
    subblock_index: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if subblock_index >= 4 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let sub_x = subblock_index % 2;
    let sub_y = subblock_index / 2;
    let dst_luma_x = dst_mb_x
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_x * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_luma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_y * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_luma_prediction_region(
        list0, list1, target, dst_luma_x, dst_luma_y, 8, 8, motion0, motion1,
    )?;

    let dst_chroma_x = dst_mb_x
        .checked_mul(8)
        .and_then(|value| value.checked_add(sub_x * 4))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    let dst_chroma_y = dst_mb_y
        .checked_mul(16)
        .and_then(|value| value.checked_add(sub_y * 8))
        .ok_or(MotionCompensationError::OutOfBounds)?;
    copy_bi_chroma_prediction_region(
        list0,
        list1,
        target,
        dst_chroma_x,
        dst_chroma_y,
        4,
        8,
        motion0,
        motion1,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn predict_bi_inter_region(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_luma_x: usize,
    dst_luma_y: usize,
    luma_width: usize,
    luma_height: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    if !luma_width.is_multiple_of(2) {
        return Err(MotionCompensationError::OutOfBounds);
    }
    copy_bi_luma_prediction_region(
        list0,
        list1,
        target,
        dst_luma_x,
        dst_luma_y,
        luma_width,
        luma_height,
        motion0,
        motion1,
    )?;
    copy_bi_chroma_prediction_region(
        list0,
        list1,
        target,
        dst_luma_x / 2,
        dst_luma_y,
        luma_width / 2,
        luma_height,
        motion0,
        motion1,
    )?;
    Ok(())
}

fn copy_luma_prediction(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    copy_luma_prediction_region(reference, target, dst_x, dst_y, 16, 16, motion)
}

fn copy_luma_prediction_region(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    width: usize,
    height: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    for row in 0..height {
        for column in 0..width {
            let sample = luma_quarter_sample(
                reference,
                ((dst_x + column) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion.x))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
                ((dst_y + row) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion.y))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
            )?;
            if !target.y.set(dst_x + column, dst_y + row, sample) {
                return Err(MotionCompensationError::OutOfBounds);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_bi_luma_prediction_region(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    width: usize,
    height: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    for row in 0..height {
        for column in 0..width {
            let sample0 = luma_quarter_sample(
                list0,
                ((dst_x + column) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion0.x))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
                ((dst_y + row) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion0.y))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
            )?;
            let sample1 = luma_quarter_sample(
                list1,
                ((dst_x + column) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion1.x))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
                ((dst_y + row) as i32)
                    .checked_mul(4)
                    .and_then(|value| value.checked_add(motion1.y))
                    .ok_or(MotionCompensationError::OutOfBounds)?,
            )?;
            if !target
                .y
                .set(dst_x + column, dst_y + row, avg_round(sample0, sample1))
            {
                return Err(MotionCompensationError::OutOfBounds);
            }
        }
    }
    Ok(())
}

fn copy_chroma_prediction(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    copy_chroma_prediction_region(reference, target, dst_x, dst_y, 8, 16, motion)
}

fn copy_chroma_prediction_region(
    reference: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    width: usize,
    height: usize,
    motion: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    for row in 0..height {
        for column in 0..width {
            let sample_x = ((dst_x + column) as i32)
                .checked_mul(8)
                .and_then(|value| value.checked_add(motion.x))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let sample_y = ((dst_y + row) as i32)
                .checked_mul(4)
                .and_then(|value| value.checked_add(motion.y))
                .and_then(|value| value.checked_mul(2))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let cb = chroma_eighth_sample(&reference.cb, sample_x, sample_y)?;
            let cr = chroma_eighth_sample(&reference.cr, sample_x, sample_y)?;
            if !target.cb.set(dst_x + column, dst_y + row, cb)
                || !target.cr.set(dst_x + column, dst_y + row, cr)
            {
                return Err(MotionCompensationError::OutOfBounds);
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_bi_chroma_prediction_region(
    list0: &DecodedFrame422P10,
    list1: &DecodedFrame422P10,
    target: &mut DecodedFrame422P10,
    dst_x: usize,
    dst_y: usize,
    width: usize,
    height: usize,
    motion0: MotionVectorQuarterPel,
    motion1: MotionVectorQuarterPel,
) -> Result<(), MotionCompensationError> {
    for row in 0..height {
        for column in 0..width {
            let sample0_x = ((dst_x + column) as i32)
                .checked_mul(8)
                .and_then(|value| value.checked_add(motion0.x))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let sample0_y = ((dst_y + row) as i32)
                .checked_mul(4)
                .and_then(|value| value.checked_add(motion0.y))
                .and_then(|value| value.checked_mul(2))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let sample1_x = ((dst_x + column) as i32)
                .checked_mul(8)
                .and_then(|value| value.checked_add(motion1.x))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let sample1_y = ((dst_y + row) as i32)
                .checked_mul(4)
                .and_then(|value| value.checked_add(motion1.y))
                .and_then(|value| value.checked_mul(2))
                .ok_or(MotionCompensationError::OutOfBounds)?;
            let cb0 = chroma_eighth_sample(&list0.cb, sample0_x, sample0_y)?;
            let cb1 = chroma_eighth_sample(&list1.cb, sample1_x, sample1_y)?;
            let cr0 = chroma_eighth_sample(&list0.cr, sample0_x, sample0_y)?;
            let cr1 = chroma_eighth_sample(&list1.cr, sample1_x, sample1_y)?;
            if !target
                .cb
                .set(dst_x + column, dst_y + row, avg_round(cb0, cb1))
                || !target
                    .cr
                    .set(dst_x + column, dst_y + row, avg_round(cr0, cr1))
            {
                return Err(MotionCompensationError::OutOfBounds);
            }
        }
    }
    Ok(())
}

fn luma_quarter_sample(
    reference: &DecodedFrame422P10,
    quarter_x: i32,
    quarter_y: i32,
) -> Result<u16, MotionCompensationError> {
    let half_x0 = div_floor(quarter_x, 2);
    let half_y0 = div_floor(quarter_y, 2);
    let half_x1 = if quarter_x.rem_euclid(2) == 0 {
        half_x0
    } else {
        half_x0 + 1
    };
    let half_y1 = if quarter_y.rem_euclid(2) == 0 {
        half_y0
    } else {
        half_y0 + 1
    };
    let a = luma_half_grid_sample(reference, half_x0, half_y0)?;
    if half_x0 == half_x1 && half_y0 == half_y1 {
        return Ok(a);
    }
    let b = luma_half_grid_sample(reference, half_x1, half_y0)?;
    if half_y0 == half_y1 {
        return Ok(avg_round(a, b));
    }
    let c = luma_half_grid_sample(reference, half_x0, half_y1)?;
    if half_x0 == half_x1 {
        return Ok(avg_round(a, c));
    }
    let d = luma_half_grid_sample(reference, half_x1, half_y1)?;
    Ok((((u32::from(a) + u32::from(b) + u32::from(c) + u32::from(d)) + 2) >> 2) as u16)
}

fn luma_half_grid_sample(
    reference: &DecodedFrame422P10,
    half_x: i32,
    half_y: i32,
) -> Result<u16, MotionCompensationError> {
    luma_half_sample(
        reference,
        div_floor(half_x, 2),
        div_floor(half_y, 2),
        half_x.rem_euclid(2) != 0,
        half_y.rem_euclid(2) != 0,
    )
}

fn luma_half_sample(
    reference: &DecodedFrame422P10,
    x: i32,
    y: i32,
    half_x: bool,
    half_y: bool,
) -> Result<u16, MotionCompensationError> {
    match (half_x, half_y) {
        (false, false) => sample_luma(reference, x, y),
        (true, false) => {
            let value = six_tap([
                sample_luma(reference, x - 2, y)?,
                sample_luma(reference, x - 1, y)?,
                sample_luma(reference, x, y)?,
                sample_luma(reference, x + 1, y)?,
                sample_luma(reference, x + 2, y)?,
                sample_luma(reference, x + 3, y)?,
            ]);
            Ok(clip10(value))
        }
        (false, true) => {
            let value = six_tap([
                sample_luma(reference, x, y - 2)?,
                sample_luma(reference, x, y - 1)?,
                sample_luma(reference, x, y)?,
                sample_luma(reference, x, y + 1)?,
                sample_luma(reference, x, y + 2)?,
                sample_luma(reference, x, y + 3)?,
            ]);
            Ok(clip10(value))
        }
        (true, true) => {
            let mut tmp = [0_i32; 6];
            for (index, row_offset) in (-2..=3).enumerate() {
                tmp[index] = six_tap([
                    sample_luma(reference, x - 2, y + row_offset)?,
                    sample_luma(reference, x - 1, y + row_offset)?,
                    sample_luma(reference, x, y + row_offset)?,
                    sample_luma(reference, x + 1, y + row_offset)?,
                    sample_luma(reference, x + 2, y + row_offset)?,
                    sample_luma(reference, x + 3, y + row_offset)?,
                ]);
            }
            Ok(clip10(
                (tmp[0] - 5 * tmp[1] + 20 * tmp[2] + 20 * tmp[3] - 5 * tmp[4] + tmp[5] + 512) >> 10,
            ))
        }
    }
}

fn chroma_eighth_sample(
    plane: &crate::frame::Plane422P10,
    eighth_x: i32,
    eighth_y: i32,
) -> Result<u16, MotionCompensationError> {
    let base_x = div_floor(eighth_x, 8);
    let base_y = div_floor(eighth_y, 8);
    let dx = eighth_x.rem_euclid(8);
    let dy = eighth_y.rem_euclid(8);
    let a = i32::from(sample_plane(plane, base_x, base_y)?);
    let b = i32::from(sample_plane(plane, base_x + 1, base_y)?);
    let c = i32::from(sample_plane(plane, base_x, base_y + 1)?);
    let d = i32::from(sample_plane(plane, base_x + 1, base_y + 1)?);
    let value =
        ((8 - dx) * (8 - dy) * a + dx * (8 - dy) * b + (8 - dx) * dy * c + dx * dy * d + 32) >> 6;
    Ok(clip10(value))
}

fn six_tap(samples: [u16; 6]) -> i32 {
    (i32::from(samples[0]) - 5 * i32::from(samples[1])
        + 20 * i32::from(samples[2])
        + 20 * i32::from(samples[3])
        - 5 * i32::from(samples[4])
        + i32::from(samples[5])
        + 16)
        >> 5
}

fn sample_luma(
    reference: &DecodedFrame422P10,
    x: i32,
    y: i32,
) -> Result<u16, MotionCompensationError> {
    sample_plane(&reference.y, x, y)
}

fn sample_plane(
    plane: &crate::frame::Plane422P10,
    x: i32,
    y: i32,
) -> Result<u16, MotionCompensationError> {
    if plane.width == 0 || plane.height == 0 {
        return Err(MotionCompensationError::OutOfBounds);
    }
    let x = x.clamp(0, plane.width.saturating_sub(1) as i32) as usize;
    let y = y.clamp(0, plane.height.saturating_sub(1) as i32) as usize;
    plane.get(x, y).ok_or(MotionCompensationError::OutOfBounds)
}

fn avg_round(a: u16, b: u16) -> u16 {
    ((u32::from(a) + u32::from(b) + 1) >> 1) as u16
}

fn clip10(value: i32) -> u16 {
    value.clamp(0, 1023) as u16
}

fn div_floor(value: i32, divisor: i32) -> i32 {
    let quotient = value / divisor;
    let remainder = value % divisor;
    if remainder != 0 && (remainder > 0) != (divisor > 0) {
        quotient - 1
    } else {
        quotient
    }
}

#[derive(Debug)]
pub enum MotionCompensationError {
    MotionVectorOverflow,
    OutOfBounds,
}

impl fmt::Display for MotionCompensationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MotionVectorOverflow => write!(f, "motion vector arithmetic overflowed"),
            Self::OutOfBounds => write!(f, "motion compensation reference is out of bounds"),
        }
    }
}

impl std::error::Error for MotionCompensationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_pel_inter_prediction_copies_422_macroblock_planes() {
        let mut reference = DecodedFrame422P10::new(32, 16, 32, 16);
        for y in 0..16 {
            for x in 0..32 {
                assert!(reference.y.set(x, y, (x + y) as u16));
            }
            for x in 0..16 {
                assert!(reference.cb.set(x, y, (100 + x + y) as u16));
                assert!(reference.cr.set(x, y, (200 + x + y) as u16));
            }
        }
        let mut target = DecodedFrame422P10::new(32, 16, 32, 16);

        predict_inter_16x16(
            &reference,
            &mut target,
            1,
            0,
            MotionVectorQuarterPel { x: -64, y: 0 },
        )
        .unwrap();

        assert_eq!(target.y.get(16, 0), reference.y.get(0, 0));
        assert_eq!(target.y.get(31, 15), reference.y.get(15, 15));
        assert_eq!(target.cb.get(8, 0), reference.cb.get(0, 0));
        assert_eq!(target.cr.get(15, 15), reference.cr.get(7, 15));
    }

    #[test]
    fn fractional_pel_motion_interpolates_luma_and_chroma() {
        let mut reference = DecodedFrame422P10::new(32, 32, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                assert!(reference.y.set(x, y, (x * 4 + y * 2) as u16));
            }
            for x in 0..16 {
                assert!(reference.cb.set(x, y, (100 + x * 3 + y) as u16));
                assert!(reference.cr.set(x, y, (200 + x * 3 + y) as u16));
            }
        }
        let mut target = DecodedFrame422P10::new(32, 32, 32, 32);

        predict_inter_16x16(
            &reference,
            &mut target,
            0,
            0,
            MotionVectorQuarterPel { x: 2, y: 0 },
        )
        .unwrap();

        assert!(target.y.get(4, 4).unwrap() > reference.y.get(4, 4).unwrap());
        assert!(target.cb.get(4, 4).unwrap() >= reference.cb.get(4, 4).unwrap());
        assert!(target.cr.get(4, 4).unwrap() >= reference.cr.get(4, 4).unwrap());
    }

    #[test]
    fn inter_16x8_prediction_updates_only_selected_partition() {
        let mut reference = DecodedFrame422P10::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                assert!(reference.y.set(x, y, (100 + y) as u16));
            }
            for x in 0..8 {
                assert!(reference.cb.set(x, y, (200 + y) as u16));
                assert!(reference.cr.set(x, y, (300 + y) as u16));
            }
        }
        let mut target = DecodedFrame422P10::new(16, 16, 16, 16);

        predict_inter_16x8(
            &reference,
            &mut target,
            0,
            0,
            1,
            MotionVectorQuarterPel::ZERO,
        )
        .unwrap();

        assert_eq!(target.y.get(0, 7), Some(0));
        assert_eq!(target.y.get(0, 8), Some(108));
        assert_eq!(target.cb.get(0, 7), Some(0));
        assert_eq!(target.cb.get(0, 8), Some(208));
    }

    #[test]
    fn inter_8x16_prediction_updates_only_selected_partition() {
        let mut reference = DecodedFrame422P10::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                assert!(reference.y.set(x, y, (100 + x) as u16));
            }
            for x in 0..8 {
                assert!(reference.cb.set(x, y, (200 + x) as u16));
                assert!(reference.cr.set(x, y, (300 + x) as u16));
            }
        }
        let mut target = DecodedFrame422P10::new(16, 16, 16, 16);

        predict_inter_8x16(
            &reference,
            &mut target,
            0,
            0,
            1,
            MotionVectorQuarterPel::ZERO,
        )
        .unwrap();

        assert_eq!(target.y.get(7, 0), Some(0));
        assert_eq!(target.y.get(8, 0), Some(108));
        assert_eq!(target.cb.get(3, 0), Some(0));
        assert_eq!(target.cb.get(4, 0), Some(204));
    }

    #[test]
    fn bi_inter_prediction_averages_two_reference_frames() {
        let mut list0 = DecodedFrame422P10::new(16, 16, 16, 16);
        let mut list1 = DecodedFrame422P10::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                assert!(list0.y.set(x, y, 100));
                assert!(list1.y.set(x, y, 300));
            }
            for x in 0..8 {
                assert!(list0.cb.set(x, y, 200));
                assert!(list1.cb.set(x, y, 400));
                assert!(list0.cr.set(x, y, 500));
                assert!(list1.cr.set(x, y, 700));
            }
        }
        let mut target = DecodedFrame422P10::new(16, 16, 16, 16);

        predict_bi_inter_16x16(
            &list0,
            &list1,
            &mut target,
            0,
            0,
            MotionVectorQuarterPel::ZERO,
            MotionVectorQuarterPel::ZERO,
        )
        .unwrap();

        assert_eq!(target.y.get(0, 0), Some(200));
        assert_eq!(target.cb.get(0, 0), Some(300));
        assert_eq!(target.cr.get(0, 0), Some(600));
    }

    #[test]
    fn motion_field_predicts_first_macroblock_as_zero() {
        let grid = MacroblockGrid::new(32, 16).unwrap();
        let field = MotionField::new(grid).unwrap();
        let address = grid.address(0).unwrap();

        assert_eq!(
            field.predict_l0_16x16(address, 0).unwrap(),
            MotionVectorQuarterPel::ZERO
        );
    }

    #[test]
    fn motion_field_predicts_from_available_left_neighbor() {
        let grid = MacroblockGrid::new(32, 16).unwrap();
        let mut field = MotionField::new(grid).unwrap();
        field
            .set_inter(
                grid.address(0).unwrap(),
                MotionVectorQuarterPel { x: 8, y: -4 },
                0,
            )
            .unwrap();

        assert_eq!(
            field.predict_l0_16x16(grid.address(1).unwrap(), 0).unwrap(),
            MotionVectorQuarterPel { x: 8, y: -4 }
        );
    }

    #[test]
    fn motion_field_predicts_median_of_three_neighbors() {
        let grid = MacroblockGrid::new(48, 32).unwrap();
        let mut field = MotionField::new(grid).unwrap();
        field
            .set_inter(
                grid.address(3).unwrap(),
                MotionVectorQuarterPel { x: 2, y: 40 },
                0,
            )
            .unwrap();
        field
            .set_inter(
                grid.address(1).unwrap(),
                MotionVectorQuarterPel { x: 10, y: 4 },
                0,
            )
            .unwrap();
        field
            .set_inter(
                grid.address(2).unwrap(),
                MotionVectorQuarterPel { x: 6, y: 8 },
                0,
            )
            .unwrap();

        assert_eq!(
            field.predict_l0_16x16(grid.address(4).unwrap(), 0).unwrap(),
            MotionVectorQuarterPel { x: 6, y: 8 }
        );
    }
}
