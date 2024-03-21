pub const U32_SIZE: usize = (u32::BITS >> 3) as usize;

#[inline]
pub fn munch_u32_be(buf: &[u8]) -> (u32, &[u8]) {
    let (word_bytes, tail_bytes) = buf.split_at(U32_SIZE);
    // opt-compiler merges these two lines
    let mut word_sized = [0u8; U32_SIZE];
    word_sized.copy_from_slice(word_bytes);
    let decoded_word = u32::from_be_bytes(word_sized);
    (decoded_word, tail_bytes)
}

#[inline]
pub fn emit_u32_be(buf: &mut [u8], word: u32) -> &mut [u8] {
    let (word_bytes, tail_bytes) = buf.split_at_mut(U32_SIZE);
    word_bytes.copy_from_slice(&word.to_be_bytes());
    tail_bytes
}
