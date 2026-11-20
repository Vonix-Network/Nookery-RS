//! Big-endian 16-bit checksum used by the GameCube save blocks.
//!
//! Ported from `pc_checksum_be` in `pc/src/pc_gci.c`: the sum of all
//! big-endian `u16` words, with the previous checksum value subtracted, then
//! negated in two's complement.  A block is valid when the checksum computed
//! over the whole block (including its stored checksum word) is zero.

/// Compute the stored checksum for `data`, replacing an existing checksum.
///
/// Mirrors the C implementation exactly, including its treatment of a
/// trailing odd byte: only whole `u16` words participate.
pub fn checksum_be(data: &[u8], old_checksum: u16) -> u16 {
    let mut sum: u32 = 0;
    for word in data.chunks_exact(2) {
        sum = sum.wrapping_add(u32::from(u16::from_be_bytes([word[0], word[1]])));
    }
    let raw = (sum.wrapping_sub(u32::from(old_checksum)) & 0xFFFF) as u16;
    raw.wrapping_neg()
}

/// Verify the checksum stored inside an even-length block.
///
/// Odd-length blocks never validate: the C readers only ever hash whole
/// sector-sized blocks, so an odd length is malformed input rather than a
/// checksummed payload.
pub fn verify_block(data: &[u8]) -> bool {
    data.len() % 2 == 0 && checksum_be(data, 0) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_matches_hand_computed_vector() {
        // 0x0001 + 0x7FFF = 0x8000, negated == 0x8000.
        let data = [0x00, 0x01, 0x7F, 0xFF];
        assert_eq!(checksum_be(&data, 0), 0x8000);
        // Subtracting the included checksum is what makes a sealed block
        // verify: the same words with old_checksum == stored sum to zero.
        assert_eq!(checksum_be(&data, 0x8000), 0);
    }

    #[test]
    fn stored_checksum_makes_block_verify() {
        let mut block = vec![0u8; 0x100];
        block[2..4].copy_from_slice(&[0x12, 0x34]);
        block[10..12].copy_from_slice(&[0xAB, 0xCD]);
        let stored = checksum_be(&block, 0);
        block[0..2].copy_from_slice(&stored.to_be_bytes());
        assert!(verify_block(&block));
        block[4] ^= 0x01;
        assert!(!verify_block(&block));
    }

    #[test]
    fn odd_length_never_verifies() {
        assert!(!verify_block(&[0, 0, 0]));
        assert!(verify_block(&[0, 0]));
    }
}
