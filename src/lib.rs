//! Final-value reduction for a batch of texel contributions. The shader
//! implements this contract only when the caller can replay the whole batch.
pub mod gpu;
pub mod scene;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Fragment {
    texel: u32,
    high: u32,
    low: u32,
    padding: u32,
}

impl Fragment {
    pub fn new(texel: u32, key: u64) -> Self {
        Self {
            texel,
            high: (key >> 32) as u32,
            low: key as u32,
            padding: 0,
        }
    }
}

pub fn validate(texels: u32, fragments: &[Fragment]) -> Result<(), String> {
    if texels == 0 {
        return Err("empty image".into());
    }
    if fragments.iter().any(|f| f.texel >= texels) {
        return Err("fragment addresses a texel outside the image".into());
    }
    Ok(())
}

pub fn reference(texels: u32, fragments: &[Fragment]) -> Result<Vec<u64>, String> {
    validate(texels, fragments)?;
    let mut keys = vec![0u64; texels as usize];
    for f in fragments {
        let key = (u64::from(f.high) << 32) | u64::from(f.low);
        keys[f.texel as usize] = keys[f.texel as usize].max(key);
    }
    Ok(keys)
}

/// Deliberately broken control, showing the error from maximizing both words independently.
pub fn split_control(texels: u32, fragments: &[Fragment]) -> Result<Vec<u64>, String> {
    validate(texels, fragments)?;
    let mut keys = vec![0u64; texels as usize];
    for f in fragments {
        let slot = &mut keys[f.texel as usize];
        *slot = ((*slot >> 32).max(u64::from(f.high)) << 32) | u64::from((*slot as u32).max(f.low));
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_payload_cannot_leak_into_winner() {
        let f = [
            Fragment::new(0, 0x1_ffffffff),
            Fragment::new(0, 0x2_00000000),
        ];
        assert_eq!(reference(2, &f).unwrap(), [0x2_00000000, 0]);
        assert_eq!(split_control(2, &f).unwrap(), [0x2_ffffffff, 0]);
    }

    #[test]
    fn equal_high_words_and_unsigned_extremes() {
        let f = [
            Fragment::new(0, u64::MAX - 1),
            Fragment::new(0, u64::MAX),
            Fragment::new(0, i64::MAX as u64),
            Fragment::new(1, 42),
        ];
        assert_eq!(reference(3, &f).unwrap(), [u64::MAX, 42, 0]);
    }

    #[test]
    fn validates_even_when_the_batch_is_empty() {
        assert_eq!(reference(2, &[]).unwrap(), [0, 0]);
        assert!(reference(0, &[]).is_err());
        assert!(reference(2, &[Fragment::new(2, 0)]).is_err());
    }
}
