//! `data:` URI images of an SVG import. No network, no file access, no decoder:
//! raster images are only validated structurally (signature, dimensions, chunk/marker
//! layout, checksums where the format has them), nested SVG images are returned as text.
use crate::svg_import_report::limits;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageFormat {
    Png,
    Jpeg,
    Svg,
}

impl ImageFormat {
    pub(crate) fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Svg => "image/svg+xml",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DataImage {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
    /// Pixel size of PNG/JPEG; zero for SVG.
    pub width: u32,
    pub height: u32,
}

/// Why an `<image>` href was refused. All variants cancel the import.
#[derive(Clone, Debug)]
pub(crate) enum ImageError {
    /// Not a `data:` URI (file path, `http(s):`, `file:` …): never fetched.
    External(String),
    Malformed(String),
    TooLarge(String),
    Unsupported(String),
}

impl ImageError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::External(_) => "image-external",
            Self::Malformed(_) => "image-malformed",
            Self::TooLarge(_) => "image-too-large",
            Self::Unsupported(_) => "image-unsupported-type",
        }
    }
    pub(crate) fn message(&self) -> String {
        match self {
            Self::External(h) => format!(
                "External image reference rejected (nothing is ever fetched): {}",
                shorten(h)
            ),
            Self::Malformed(m) | Self::TooLarge(m) | Self::Unsupported(m) => m.clone(),
        }
    }
}

fn shorten(s: &str) -> String {
    let mut out: String = s.chars().take(60).collect();
    if s.chars().count() > 60 {
        out.push('…');
    }
    out
}

/// Parse, size-check and validate an image href. `budget` is the remaining decoded-byte allowance.
pub(crate) fn parse_data_uri(href: &str, budget: &mut usize) -> Result<DataImage, ImageError> {
    let href = href.trim();
    let Some(rest) = href
        .get(..5)
        .filter(|p| p.eq_ignore_ascii_case("data:"))
        .map(|_| &href[5..])
    else {
        return Err(ImageError::External(href.into()));
    };
    let (header, payload) = rest
        .split_once(',')
        .ok_or_else(|| ImageError::Malformed("data URI has no ',' separator".into()))?;
    let mut parts = header.split(';');
    let mime = parts.next().unwrap_or("").trim().to_ascii_lowercase();
    let mut base64 = false;
    for p in parts {
        let p = p.trim().to_ascii_lowercase();
        if p == "base64" {
            base64 = true;
        } else if let Some(cs) = p.strip_prefix("charset=") {
            if !matches!(cs, "utf-8" | "utf8" | "us-ascii") {
                return Err(ImageError::Unsupported(format!(
                    "data URI charset {cs} unsupported (UTF-8 only)"
                )));
            }
        }
    }
    let format = match mime.as_str() {
        "image/png" => ImageFormat::Png,
        "image/jpeg" | "image/jpg" => ImageFormat::Jpeg,
        "image/svg+xml" => ImageFormat::Svg,
        "" => {
            return Err(ImageError::Unsupported(
                "data URI without a media type; only image/png, image/jpeg and image/svg+xml are accepted".into(),
            ))
        }
        other => {
            return Err(ImageError::Unsupported(format!(
                "image type {other} unsupported; only image/png, image/jpeg and image/svg+xml are accepted"
            )))
        }
    };
    let max = if format == ImageFormat::Svg {
        limits::NESTED_SVG_BYTES
    } else {
        limits::IMAGE_BYTES
    };
    // Check the encoded size before allocating anything (percent escapes triple the size).
    if payload.len() > (max * 4 / 3 + 1024) * 3 {
        return Err(ImageError::TooLarge(format!(
            "image data exceeds the {max} byte limit"
        )));
    }
    let bytes = if base64 {
        let clean: String = payload
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        decode_base64(&clean)
            .ok_or_else(|| ImageError::Malformed("invalid base64 image data".into()))?
    } else if format == ImageFormat::Svg {
        percent_decode(payload)?
    } else {
        return Err(ImageError::Malformed(
            "raster images must be base64 encoded".into(),
        ));
    };
    if bytes.len() > max {
        return Err(ImageError::TooLarge(format!(
            "image data is {} bytes; the limit is {max}",
            bytes.len()
        )));
    }
    if bytes.len() > *budget {
        return Err(ImageError::TooLarge(format!(
            "images exceed the total limit of {} bytes",
            limits::IMAGES_TOTAL_BYTES
        )));
    }
    *budget -= bytes.len();
    let (width, height) = match format {
        ImageFormat::Png => validate_png(&bytes)?,
        ImageFormat::Jpeg => validate_jpeg(&bytes)?,
        ImageFormat::Svg => {
            std::str::from_utf8(&bytes)
                .map_err(|_| ImageError::Malformed("SVG image is not UTF-8".into()))?;
            (0, 0)
        }
    };
    Ok(DataImage {
        format,
        bytes,
        width,
        height,
    })
}

fn check_dims(w: u32, h: u32) -> Result<(u32, u32), ImageError> {
    if w == 0 || h == 0 {
        return Err(ImageError::Malformed(
            "image has zero width or height".into(),
        ));
    }
    if w > limits::IMAGE_EDGE
        || h > limits::IMAGE_EDGE
        || w as u64 * h as u64 > limits::IMAGE_PIXELS
    {
        return Err(ImageError::TooLarge(format!(
            "image is {w}x{h}; limits are {} per edge and {} pixels",
            limits::IMAGE_EDGE,
            limits::IMAGE_PIXELS
        )));
    }
    Ok((w, h))
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn validate_png(b: &[u8]) -> Result<(u32, u32), ImageError> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let bad = |m: &str| ImageError::Malformed(format!("invalid PNG: {m}"));
    if b.len() < 8 || b[..8] != SIG {
        return Err(bad("missing signature (data does not match image/png)"));
    }
    let mut pos = 8;
    let (mut dims, mut idat, mut first) = (None, false, true);
    loop {
        if pos + 12 > b.len() {
            return Err(bad("truncated chunk or missing IEND"));
        }
        let len = u32::from_be_bytes(b[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &b[pos + 4..pos + 8];
        if !kind.iter().all(u8::is_ascii_alphabetic) {
            return Err(bad("bad chunk type"));
        }
        let end = pos
            .checked_add(12)
            .and_then(|v| v.checked_add(len))
            .filter(|e| *e <= b.len())
            .ok_or_else(|| bad("chunk length exceeds data"))?;
        let crc = u32::from_be_bytes(b[end - 4..end].try_into().unwrap());
        if crc32(&b[pos + 4..end - 4]) != crc {
            return Err(bad("chunk checksum mismatch"));
        }
        if first {
            if kind != b"IHDR" || len != 13 {
                return Err(bad("first chunk is not IHDR"));
            }
            let w = u32::from_be_bytes(b[pos + 8..pos + 12].try_into().unwrap());
            let h = u32::from_be_bytes(b[pos + 12..pos + 16].try_into().unwrap());
            let (depth, color) = (b[pos + 16], b[pos + 17]);
            let ok = match color {
                0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
                3 => matches!(depth, 1 | 2 | 4 | 8),
                2 | 4 | 6 => matches!(depth, 8 | 16),
                _ => false,
            };
            if !ok {
                return Err(bad("invalid bit depth / color type"));
            }
            dims = Some(check_dims(w, h)?);
            first = false;
        }
        idat |= kind == b"IDAT";
        pos = end;
        if kind == b"IEND" {
            break;
        }
    }
    if pos != b.len() {
        return Err(bad("data after IEND"));
    }
    if !idat {
        return Err(bad("no IDAT chunk"));
    }
    dims.ok_or_else(|| bad("missing IHDR"))
}

fn validate_jpeg(b: &[u8]) -> Result<(u32, u32), ImageError> {
    let bad = |m: &str| ImageError::Malformed(format!("invalid JPEG: {m}"));
    if b.len() < 4 || b[..3] != [0xff, 0xd8, 0xff] {
        return Err(bad("missing SOI (data does not match image/jpeg)"));
    }
    let mut pos = 2;
    let mut dims = None;
    loop {
        if pos >= b.len() || b[pos] != 0xff {
            return Err(bad("expected a marker"));
        }
        while pos < b.len() && b[pos] == 0xff {
            pos += 1;
        }
        let Some(&m) = b.get(pos) else {
            return Err(bad("truncated"));
        };
        pos += 1;
        match m {
            0x00 => return Err(bad("stray stuffed byte")),
            0x01 | 0xd0..=0xd7 | 0xd8 => continue,
            0xd9 => return Err(bad("EOI before image data")),
            _ => {}
        }
        if pos + 2 > b.len() {
            return Err(bad("truncated segment"));
        }
        let len = u16::from_be_bytes([b[pos], b[pos + 1]]) as usize;
        if len < 2 || pos + len > b.len() {
            return Err(bad("segment length exceeds data"));
        }
        if matches!(m, 0xc0..=0xcf) && !matches!(m, 0xc4 | 0xc8 | 0xcc) {
            if len < 8 {
                return Err(bad("short frame header"));
            }
            let h = u16::from_be_bytes([b[pos + 3], b[pos + 4]]) as u32;
            let w = u16::from_be_bytes([b[pos + 5], b[pos + 6]]) as u32;
            dims = Some(check_dims(w, h)?);
        }
        if m == 0xda {
            let dims = dims.ok_or_else(|| bad("no frame header before scan data"))?;
            let mut end = b.len();
            while end > 0 && b[end - 1] == 0 {
                end -= 1;
            }
            if end < 2 || b[end - 2..end] != [0xff, 0xd9] {
                return Err(bad("missing EOI"));
            }
            return Ok(dims);
        }
        pos += len;
    }
}

fn percent_decode(s: &str) -> Result<Vec<u8>, ImageError> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = b
                .get(i + 1..i + 3)
                .and_then(|h| std::str::from_utf8(h).ok())
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| ImageError::Malformed("invalid %-escape in data URI".into()))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    let mut s = s.trim_end_matches('=').as_bytes().to_vec();
    if s.len() % 4 == 1 {
        return None;
    }
    s.iter_mut().for_each(|c| {
        *c = match *c {
            b'A'..=b'Z' => *c - b'A',
            b'a'..=b'z' => *c - b'a' + 26,
            b'0'..=b'9' => *c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => 255,
        }
    });
    if s.contains(&255) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for chunk in s.chunks(4) {
        let mut v = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            v |= (*c as u32) << (18 - 6 * i);
        }
        out.push((v >> 16) as u8);
        if chunk.len() > 2 {
            out.push((v >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(v as u8);
        }
    }
    Some(out)
}
