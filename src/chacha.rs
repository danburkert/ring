// Copyright 2016 Brian Smith.
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND AND THE AUTHORS DISCLAIM ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY
// SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
// OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
// CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! TODO: docs

#![allow(unsafe_code)]

use c;
use core;
use polyfill::slice::u32_from_le_u8;

/// TODO: docs
pub type Key = [u32; KEY_LEN_IN_BYTES / 4];

/// TODO: docs
// TODO: test
pub fn key_from_bytes(key_bytes: &[u8; KEY_LEN_IN_BYTES]) -> Key {
    let mut key = [0u32; KEY_LEN_IN_BYTES / 4];
    for (key_u32, key_u8_4) in key.iter_mut().zip(key_bytes.chunks(4)) {
        *key_u32 = u32_from_le_u8(slice_as_array_ref!(key_u8_4, 4).unwrap());
    }
    key
}

/// TODO: docs
#[inline]
pub fn chacha20_xor(key: &Key, counter: &Counter, input: &[u8],
                    output: &mut [u8]) {
    assert!(input.len() == output.len());
    chacha20_xor_inner(key, counter, input.as_ptr(), input.len(),
                       output.as_mut_ptr());
}

/// TODO: docs
#[inline]
pub fn chacha20_xor_in_place(key: &Key, counter: &Counter, in_out: &mut [u8]) {
    chacha20_xor_inner(key, counter, in_out.as_ptr(), in_out.len(),
                       in_out.as_mut_ptr());
}

#[doc(hidden)]
#[inline]
pub fn chacha20_xor_inner(key: &Key, counter: &Counter, input: *const u8,
                          in_out_len: usize, output: *mut u8) {
    debug_assert!(core::mem::align_of_val(key) >= 4);
    debug_assert!(core::mem::align_of_val(counter) >= 4);
    unsafe {
        GFp_ChaCha20_ctr32(output, input, in_out_len, key, counter);
    }
}

/// TODO: docs
pub type Counter = [u32; 4];

/// TODO: docs.
#[inline]
pub fn make_counter(nonce: &[u8; NONCE_LEN], counter: u32) -> Counter {
    [counter.to_le(),
     u32_from_le_u8(slice_as_array_ref!(&nonce[0..4], 4).unwrap()),
     u32_from_le_u8(slice_as_array_ref!(&nonce[4..8], 4).unwrap()),
     u32_from_le_u8(slice_as_array_ref!(&nonce[8..12], 4).unwrap())]
}

extern {
    fn GFp_ChaCha20_ctr32(out: *mut u8, in_: *const u8, in_len: c::size_t,
                          key: &Key, counter: &Counter);
}

/// TODO: docs
pub const KEY_LEN_IN_BYTES: usize = 256 / 8;

/// TODO: docs
pub const NONCE_LEN: usize = 12; /* 96 bits */

#[cfg(test)]
mod tests {
    use {polyfill, test};
    use super::*;
    use super::GFp_ChaCha20_ctr32;

    // This verifies the encryption functionality provided by ChaCha20_ctr32
    // is successful when either computed on disjoint input/output buffers,
    // or on overlapping input/output buffers. On some branches of the 32-bit
    // x86 and ARM code the in-place operation fails in some situations where
    // the input/output buffers are not exactly overlapping. Such failures are
    // dependent not only on the degree of overlapping but also the length of
    // the data. `open()` works around that by moving the input data to the
    // output location so that the buffers exactly overlap, for those targets.
    // This test exists largely as a canary for detecting if/when that type of
    // problem spreads to other platforms.
    #[test]
    pub fn chacha20_tests() {
        test::from_file("src/chacha_tests.txt", |section, test_case| {
            assert_eq!(section, "");

            let key_bytes = test_case.consume_bytes("Key");
            let mut key = [0u32; KEY_LEN_IN_BYTES / 4];
            for ki in 0..(KEY_LEN_IN_BYTES / 4) {
                let kb =
                    slice_as_array_ref!(&key_bytes[ki * 4..][..4], 4).unwrap();
                key[ki] = polyfill::slice::u32_from_le_u8(kb);
            }

            let ctr = test_case.consume_usize("Ctr");
            let nonce_bytes = test_case.consume_bytes("Nonce");
            let nonce = slice_as_array_ref!(&nonce_bytes, NONCE_LEN).unwrap();
            let ctr = make_counter(&nonce, ctr as u32);
            let input = test_case.consume_bytes("Input");
            let output = test_case.consume_bytes("Output");

            // Pre-allocate buffer for use in test_cases.
            let mut in_out_buf = vec![0u8; input.len() + 276];

            // Run the test case over all prefixes of the input because the
            // behavior of ChaCha20 implementation changes dependent on the
            // length of the input.
            for len in 0..(input.len() + 1) {
                chacha20_test_case_inner(&key, &ctr, &input[..len],
                                         &output[..len], len, &mut in_out_buf);
            }

            Ok(())
        });
    }

    fn chacha20_test_case_inner(key: &Key, ctr: &Counter, input: &[u8],
                                expected: &[u8], len: usize,
                                in_out_buf: &mut [u8]) {
        // Straightforward encryption into disjoint buffers is computed
        // correctly.
        unsafe {
          GFp_ChaCha20_ctr32(in_out_buf.as_mut_ptr(), input[..len].as_ptr(),
                             len, key, &ctr);
        }
        assert_eq!(&in_out_buf[..len], expected);

        // Do not test offset buffers for x86 and ARM architectures (see above
        // for rationale).
        let max_offset =
            if cfg!(any(target_arch = "x86", target_arch = "arm")) {
                0
            } else {
                259
            };

        // Check that in-place encryption works successfully when the pointers
        // to the input/output buffers are (partially) overlapping.
        for alignment in 0..16 {
            for offset in 0..(max_offset + 1) {
              in_out_buf[alignment+offset..][..len].copy_from_slice(input);
              unsafe {
                  GFp_ChaCha20_ctr32(in_out_buf[alignment..].as_mut_ptr(),
                                     in_out_buf[alignment + offset..].as_ptr(),
                                     len, key, ctr);
                  assert_eq!(&in_out_buf[alignment..][..len], expected);
              }
            }
        }
    }
}
