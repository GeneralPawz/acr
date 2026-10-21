# Bink videos (`Video\*.bik`)

Implementation: `crates/acr-video` (pure Rust, no `unsafe`, builds for `wasm32`). Verified
with `acr bik-check <game-dir>`, which decodes every frame and the whole soundtrack of all
three shipped files without an error, and with `acr bik-frames`, whose PNGs match the
original movies (the GSC logo over blue clouds, the stone calendar, the storm at sea).
`acr bik-play <game-dir> Video\3.bik` plays a soundtrack.

The format is RAD Game Tools' Bink 1. The original plays it through `binkw32.dll`; the
open-source descriptions used to understand it are FFmpeg's `libavcodec/bink.c`,
`binkaudio.c`, `binkdsp.c`, `libavformat/bink.c` and the MultimediaWiki "Bink Container"
page. The code here is an independent implementation; the numeric tables in
`tables.rs` (scan order, Huffman codes, run patterns, dequantization matrices) are
constants of the format and must be bit-exact.

## The shipped files

All three are revision **`BIKi`**, 25 fps, one 44.1 kHz stereo RDFT audio track
(flags `0xE000`: 16-bit, stereo, bit 15 unknown), no alpha.

| file | size | frames | length | content |
|---|---|---|---|---|
| `1.bik` | 640x480 | 501 | 20.0 s | stone Aztec calendar (publisher logo) |
| `2.bik` | 640x360 | 120 | 4.8 s | GSC Game World logo |
| `3.bik` | 640x360 | 1669 | 66.8 s | intro movie (ships, storm, burning arrow) |

`dmcr.exe` first checks that `Video\1.bik` exists and then plays `Video\1.bik`,
`2.bik`, `3.bik`, `4.bik`, `5.bik` in that order at startup (pushes at `0x4A5EB5`..`0x4A5EE9`,
missing files are skipped), followed by optional `cdv.avi`/`logo.avi`/`kz.avi`.
`6.bik`..`10.bik` are referenced elsewhere and do not ship with this version.

## Container

Little-endian. Header:

| offset | size | field |
|---|---|---|
| 0 | 3 | `"BIK"` |
| 3 | 1 | revision letter (`'i'`) |
| 4 | 4 | file size - 8 |
| 8 | 4 | frame count |
| 12 | 4 | largest frame packet in bytes |
| 16 | 4 | frame count again |
| 20 | 4 | width |
| 24 | 4 | height |
| 28 | 4 | frame rate numerator |
| 32 | 4 | frame rate denominator |
| 36 | 4 | video flags (`0x00100000` alpha plane) |
| 40 | 4 | audio track count `n` |
| 44 | 4n | per track: largest decoded audio packet |
| .. | 4n | per track: sample rate (u16), flags (u16: `0x4000` 16-bit, `0x2000` stereo, `0x1000` DCT instead of RDFT) |
| .. | 4n | per track: track id |
| .. | 4f | frame offsets; bit 0 marks a keyframe. The last frame ends at the file size |

A frame packet holds, per audio track, `u32 size` + `size` bytes of audio, then the video
data up to the end of the packet.

## Video

YUV 4:2:0 (BT.601 limited range), 8x8 blocks. A frame (from revision `i`) starts with a
32-bit word that is skipped, then the planes Y, V, U (revisions before `h`: Y, U, V),
each starting at a 32-bit boundary. Bits are read least significant first.

Per plane, 23 Huffman "trees" are read: each picks one of 16 fixed code sets (4 bits;
set 0 = raw nibbles) and a symbol permutation (an explicit list of up to 8 symbols, or up
to 4 rounds of bit-driven merging). They belong to nine bundles: block types, 16x16 sub
block types, colours (plus 16 trees for the high nibble, chosen by the previous high
nibble), pattern masks, motion x, motion y, intra DC, inter DC (no tree) and run lengths.

Before each row of blocks every bundle is refilled unless values from an earlier refill
remain: a count (width derived from the plane width), then either one value repeated
(flag bit) or one coded value per entry. Block types use symbols 12..15 as runs of 4, 8,
12 or 32 copies of the previous type. DC bundles store an 11-bit start value and groups of
up to 8 deltas sharing a 4-bit width.

Block types: 0 skip (copy previous frame), 1 scaled (a 16x16 block coded at 8x8 and
doubled; its sub type is run, intra, fill, pattern or raw), 2 motion copy, 3 runs in one
of 16 scan patterns, 4 motion + residue (bit planes added to pixels), 5 intra DCT,
6 fill, 7 motion + DCT residue, 8 two-colour pattern, 9 raw 64 bytes.

DCT coefficients are coded as bit planes over a work list of coefficient groups; the
quantizer index (4 bits) follows the coefficients. The inverse DCT is a separable integer
butterfly, rounded `(x + 127) >> 8`; results wrap at 8 bits instead of being clamped.

## Audio

RDFT flavour: stereo samples are interleaved before the transform, so a 44.1 kHz stereo
track uses one 4096-point real transform per block (2048 for mono; 1024/512 below
44.1 kHz; 512/256 below 22.05 kHz). Each packet starts with the decoded size in bytes,
then 32-bit-aligned blocks: two coefficients as custom floats (5-bit exponent, 23-bit
mantissa, sign), an 8-bit quantizer per critical band (25 bands, WMA band edges), then
runs of 8..512 coefficients sharing a 4-bit width, each non-zero value followed by a sign.
The spectrum (`c[2k]` cosine, `c[2k+1]` sine) is turned into samples with an inverse real
FFT (`y = c0/2 + c1/2 (-1)^t + sum c2k cos + c2k+1 sin`). The first 1/16 of each block is
cross-faded linearly with the last 1/16 of the previous one, and each block outputs
`15/16` of its length.

The sign convention of the sine terms was checked on the game files: the overlapping
block ends then agree to 1-2 % of their energy, against ~100 % with the opposite sign.
Peaks reach 1.17 (clipped to 16 bits), RMS 0.20-0.24.

## Performance

Release build, x64: 1.bik decodes at ~4500 fps, 3.bik at ~1700 fps (68x real time);
the whole 67 s soundtrack decodes in 0.06 s.

## In the game

`crates/acr-game/src/intro.rs` plays `Video.bik`..`10.bik` (those present) at startup
and again from the main menu's "Intro" entry; a click or Esc/Space/Enter skips the current
movie. The soundtrack replaces the menu music while a movie plays. In the browser the
first click only enables audio and restarts the current movie with sound. Which movies
the original's "Intro" entry plays has not been checked yet.

## Open issues

- DCT audio (flag `0x1000`), revision `b` and revisions `k`/`KB2` (Bink 1 successors with
  extra header fields and a per-plane fill flag) are rejected; none occur in the game.
- Video timing follows a wall clock started together with the audio; there is no
  resynchronisation against the mixer position.
- Videos are converted to RGBA on the CPU and uploaded as a new texture per frame.
