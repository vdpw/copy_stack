use crate::event::{event_encoded_size, MAX_EVENT_BLOB_BYTES};
use crate::pasteboard_protocol::{REMOTE_CLIPBOARD_TYPE, SOURCE_TYPE};
use copy_event_listener::event::{Data, Event, Item};

pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_RTF_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PNG_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FILE_URL_BYTES: usize = 64 * 1024;
pub const MAX_DISPLAY_BYTES: usize = 1024 * 1024;
pub const MAX_TRAY_PREVIEW_BYTES: usize = 64 * 1024;
pub const MAX_PREVIEW_IMAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PREVIEW_IMAGE_PIXELS: u64 = 20_000_000;
pub const MAX_PREVIEW_SEGMENTS: usize = 32;
pub const MAX_DETAIL_IPC_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_HISTORY_BYTES: u64 = 256 * 1024 * 1024;

const INLINE_ATTACHMENT_PLACEHOLDER: char = '\u{fffc}';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureResourceKind {
    Event,
    Text,
    FormattedText,
    Image,
    FileReference,
}

impl CaptureResourceKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Event => "capture.event_too_large",
            Self::Text => "capture.text_too_large",
            Self::FormattedText => "capture.formatted_text_too_large",
            Self::Image => "capture.image_too_large",
            Self::FileReference => "capture.file_reference_too_large",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SizeBucket {
    UnderOneMiB,
    OneToFourMiB,
    FourToSixteenMiB,
    SixteenToThirtyTwoMiB,
    OverThirtyTwoMiB,
}

impl SizeBucket {
    pub fn code(self) -> &'static str {
        match self {
            Self::UnderOneMiB => "under_1_mib",
            Self::OneToFourMiB => "1_to_4_mib",
            Self::FourToSixteenMiB => "4_to_16_mib",
            Self::SixteenToThirtyTwoMiB => "16_to_32_mib",
            Self::OverThirtyTwoMiB => "over_32_mib",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureResourceRejection {
    pub kind: CaptureResourceKind,
    pub size_bucket: SizeBucket,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapturePreparation {
    Accepted,
    DegradedToPlainText,
}

#[derive(Clone, Debug)]
pub struct PreparedCaptureEvent {
    pub event: Event,
    #[cfg(test)]
    pub preparation: CapturePreparation,
}

pub fn prepare_capture_event(
    event: Event,
) -> Result<PreparedCaptureEvent, CaptureResourceRejection> {
    let encoded_size = event_encoded_size(&event).unwrap_or(MAX_EVENT_BLOB_BYTES + 1);
    let mut rejection = (encoded_size > MAX_EVENT_BLOB_BYTES).then_some(CaptureResourceRejection {
        kind: CaptureResourceKind::Event,
        size_bucket: size_bucket(encoded_size),
    });

    for data in event.items.iter().flat_map(|item| item.data_list.iter()) {
        let (kind, limit) = match data.r#type.as_str() {
            "public.utf8-plain-text" => (CaptureResourceKind::Text, MAX_TEXT_BYTES),
            "public.html" => (CaptureResourceKind::FormattedText, MAX_HTML_BYTES),
            "public.rtf" => (CaptureResourceKind::FormattedText, MAX_RTF_BYTES),
            "public.png" | "public.tiff" | "public.jpeg" | "public.jpg" => {
                (CaptureResourceKind::Image, MAX_PNG_BYTES)
            }
            "public.file-url" => (CaptureResourceKind::FileReference, MAX_FILE_URL_BYTES),
            _ => continue,
        };

        if data.data.len() > limit {
            rejection = Some(CaptureResourceRejection {
                kind,
                size_bucket: size_bucket(data.data.len()),
            });
            break;
        }
    }

    let Some(rejection) = rejection else {
        return Ok(PreparedCaptureEvent {
            event,
            #[cfg(test)]
            preparation: CapturePreparation::Accepted,
        });
    };

    if let Some(event) = safe_plain_text_projection(&event) {
        return Ok(PreparedCaptureEvent {
            event,
            #[cfg(test)]
            preparation: CapturePreparation::DegradedToPlainText,
        });
    }

    Err(rejection)
}

pub fn size_bucket(bytes: usize) -> SizeBucket {
    match bytes {
        0..=1_048_575 => SizeBucket::UnderOneMiB,
        1_048_576..=4_194_303 => SizeBucket::OneToFourMiB,
        4_194_304..=16_777_215 => SizeBucket::FourToSixteenMiB,
        16_777_216..=33_554_432 => SizeBucket::SixteenToThirtyTwoMiB,
        _ => SizeBucket::OverThirtyTwoMiB,
    }
}

pub fn safe_png_preview_dimensions(bytes: &[u8]) -> bool {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
        return false;
    }

    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) as u64;
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]) as u64;
    width > 0
        && height > 0
        && width
            .checked_mul(height)
            .is_some_and(|pixels| pixels <= MAX_PREVIEW_IMAGE_PIXELS)
}

pub fn allow_image_preview(bytes: &[u8], media_type: &str) -> bool {
    if bytes.len() > MAX_PREVIEW_IMAGE_BYTES {
        return false;
    }

    match media_type {
        "image/png" => safe_png_preview_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes).is_some_and(safe_dimensions),
        "image/gif" => gif_dimensions_and_frames(bytes)
            .is_some_and(|(dimensions, frames)| safe_dimensions(dimensions) && frames == 1),
        "image/webp" => webp_dimensions(bytes).is_some_and(safe_dimensions),
        "image/bmp" => bmp_dimensions(bytes).is_some_and(safe_dimensions),
        _ => false,
    }
}

fn safe_dimensions((width, height): (u64, u64)) -> bool {
    width > 0
        && height > 0
        && width
            .checked_mul(height)
            .is_some_and(|pixels| pixels <= MAX_PREVIEW_IMAGE_PIXELS)
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.get(..2)? != b"\xff\xd8" {
        return None;
    }
    let mut cursor = 2;
    while cursor < bytes.len() {
        while bytes.get(cursor) == Some(&0xff) {
            cursor += 1;
        }
        let marker = *bytes.get(cursor)?;
        cursor += 1;
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        let segment_length =
            u16::from_be_bytes([*bytes.get(cursor)?, *bytes.get(cursor + 1)?]) as usize;
        if segment_length < 2 || cursor.checked_add(segment_length)? > bytes.len() {
            return None;
        }
        if matches!(
            marker,
            0xc0 | 0xc1
                | 0xc2
                | 0xc3
                | 0xc5
                | 0xc6
                | 0xc7
                | 0xc9
                | 0xca
                | 0xcb
                | 0xcd
                | 0xce
                | 0xcf
        ) && segment_length >= 7
        {
            let height = u16::from_be_bytes([bytes[cursor + 3], bytes[cursor + 4]]) as u64;
            let width = u16::from_be_bytes([bytes[cursor + 5], bytes[cursor + 6]]) as u64;
            return Some((width, height));
        }
        cursor += segment_length;
    }
    None
}

fn gif_dimensions_and_frames(bytes: &[u8]) -> Option<((u64, u64), usize)> {
    if bytes.len() < 13 || !matches!(&bytes[..6], b"GIF87a" | b"GIF89a") {
        return None;
    }
    let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u64;
    let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u64;
    let mut cursor = 13usize;
    let packed = bytes[10];
    if packed & 0x80 != 0 {
        let color_table_bytes = 3usize.checked_mul(1usize << ((packed & 0x07) + 1))?;
        cursor = cursor.checked_add(color_table_bytes)?;
    }
    if cursor > bytes.len() {
        return None;
    }

    let mut frames = 0usize;
    loop {
        match *bytes.get(cursor)? {
            0x3b => return Some(((width, height), frames)),
            0x21 => {
                cursor = cursor.checked_add(2)?;
                cursor = skip_gif_sub_blocks(bytes, cursor)?;
            }
            0x2c => {
                if cursor.checked_add(10)? > bytes.len() {
                    return None;
                }
                let frame_width = u16::from_le_bytes([bytes[cursor + 5], bytes[cursor + 6]]) as u64;
                let frame_height =
                    u16::from_le_bytes([bytes[cursor + 7], bytes[cursor + 8]]) as u64;
                if !safe_dimensions((frame_width, frame_height)) {
                    return None;
                }
                frames = frames.checked_add(1)?;
                let frame_packed = bytes[cursor + 9];
                cursor += 10;
                if frame_packed & 0x80 != 0 {
                    let color_table_bytes =
                        3usize.checked_mul(1usize << ((frame_packed & 0x07) + 1))?;
                    cursor = cursor.checked_add(color_table_bytes)?;
                }
                cursor = cursor.checked_add(1)?;
                cursor = skip_gif_sub_blocks(bytes, cursor)?;
            }
            _ => return None,
        }
    }
}

fn skip_gif_sub_blocks(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    loop {
        let size = *bytes.get(cursor)? as usize;
        cursor = cursor.checked_add(1)?;
        if size == 0 {
            return Some(cursor);
        }
        cursor = cursor.checked_add(size)?;
        if cursor > bytes.len() {
            return None;
        }
    }
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.len() < 20 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let declared_size = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize + 8;
    if declared_size > bytes.len() {
        return None;
    }

    let mut cursor = 12usize;
    while cursor.checked_add(8)? <= declared_size {
        let chunk_type = bytes.get(cursor..cursor + 4)?;
        let chunk_size = u32::from_le_bytes([
            bytes[cursor + 4],
            bytes[cursor + 5],
            bytes[cursor + 6],
            bytes[cursor + 7],
        ]) as usize;
        let data_start = cursor + 8;
        let data_end = data_start.checked_add(chunk_size)?;
        if data_end > declared_size {
            return None;
        }
        let data = &bytes[data_start..data_end];
        match chunk_type {
            b"ANIM" | b"ANMF" => return None,
            b"VP8X" if data.len() >= 10 => {
                if data[0] & 0x02 != 0 {
                    return None;
                }
                let width = read_u24_le(&data[4..7])? as u64 + 1;
                let height = read_u24_le(&data[7..10])? as u64 + 1;
                return Some((width, height));
            }
            b"VP8 " if data.len() >= 10 && data[3..6] == [0x9d, 0x01, 0x2a] => {
                let width = u16::from_le_bytes([data[6], data[7]]) & 0x3fff;
                let height = u16::from_le_bytes([data[8], data[9]]) & 0x3fff;
                return Some((width as u64, height as u64));
            }
            b"VP8L" if data.len() >= 5 && data[0] == 0x2f => {
                let bits = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
                let width = (bits & 0x3fff) as u64 + 1;
                let height = ((bits >> 14) & 0x3fff) as u64 + 1;
                return Some((width, height));
            }
            _ => {}
        }
        cursor = data_end.checked_add(chunk_size & 1)?;
    }
    None
}

fn read_u24_le(bytes: &[u8]) -> Option<u32> {
    Some(*bytes.first()? as u32 | ((*bytes.get(1)? as u32) << 8) | ((*bytes.get(2)? as u32) << 16))
}

fn bmp_dimensions(bytes: &[u8]) -> Option<(u64, u64)> {
    if bytes.len() < 26 || &bytes[..2] != b"BM" {
        return None;
    }
    let dib_size = u32::from_le_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
    if dib_size == 12 {
        let width = u16::from_le_bytes([bytes[18], bytes[19]]) as u64;
        let height = u16::from_le_bytes([bytes[20], bytes[21]]) as u64;
        return Some((width, height));
    }
    if dib_size < 40 {
        return None;
    }
    let width =
        i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]).unsigned_abs() as u64;
    let height =
        i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]).unsigned_abs() as u64;
    Some((width, height))
}

fn safe_plain_text_projection(event: &Event) -> Option<Event> {
    let text = event
        .items
        .iter()
        .flat_map(|item| item.data_list.iter())
        .find(|data| data.r#type == "public.utf8-plain-text")?;
    let value = std::str::from_utf8(&text.data).ok()?;
    if text.data.len() > MAX_TEXT_BYTES
        || value.trim().is_empty()
        || value.contains(INLINE_ATTACHMENT_PLACEHOLDER)
    {
        return None;
    }

    let mut data_list = vec![Data {
        r#type: "public.utf8-plain-text".to_string(),
        data: text.data.clone(),
    }];
    data_list.extend(
        event
            .items
            .iter()
            .flat_map(|item| item.data_list.iter())
            .filter(|data| data.r#type == SOURCE_TYPE || data.r#type == REMOTE_CLIPBOARD_TYPE)
            .cloned(),
    );

    Some(Event {
        items: vec![Item { data_list }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(data_type: &str, size: usize) -> Data {
        Data {
            r#type: data_type.to_string(),
            data: vec![b'x'; size],
        }
    }

    fn event(data_list: Vec<Data>) -> Event {
        Event {
            items: vec![Item { data_list }],
        }
    }

    #[test]
    fn oversized_formatted_content_degrades_to_safe_plain_text() {
        let prepared = prepare_capture_event(event(vec![
            Data {
                r#type: "public.utf8-plain-text".to_string(),
                data: b"synthetic safe fallback".to_vec(),
            },
            data("public.html", MAX_HTML_BYTES + 1),
            Data {
                r#type: SOURCE_TYPE.to_string(),
                data: b"com.example.synthetic".to_vec(),
            },
        ]))
        .expect("plain-text fallback should be accepted");

        assert_eq!(
            prepared.preparation,
            CapturePreparation::DegradedToPlainText
        );
        assert_eq!(prepared.event.items[0].data_list.len(), 2);
        assert!(prepared.event.items[0]
            .data_list
            .iter()
            .any(|data| data.r#type == SOURCE_TYPE));
    }

    #[test]
    fn oversized_binary_content_without_text_is_rejected_safely() {
        let rejection = prepare_capture_event(event(vec![data("public.png", MAX_PNG_BYTES + 1)]))
            .expect_err("oversized image should be rejected");

        assert_eq!(rejection.kind, CaptureResourceKind::Image);
        assert_eq!(rejection.kind.code(), "capture.image_too_large");
        assert_ne!(rejection.size_bucket.code(), "");
    }

    #[test]
    fn png_pixel_budget_rejects_decompression_bomb_dimensions() {
        let mut png = vec![0; 24];
        png[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
        png[12..16].copy_from_slice(b"IHDR");
        png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        png[20..24].copy_from_slice(&100_000u32.to_be_bytes());

        assert!(!allow_image_preview(&png, "image/png"));

        png[16..20].copy_from_slice(&1920u32.to_be_bytes());
        png[20..24].copy_from_slice(&1080u32.to_be_bytes());
        assert!(allow_image_preview(&png, "image/png"));
    }

    #[test]
    fn every_supported_preview_format_enforces_dimensions_and_animation_bounds() {
        let jpeg = |width: u16, height: u16| {
            vec![
                0xff,
                0xd8,
                0xff,
                0xc0,
                0x00,
                0x08,
                0x08,
                (height >> 8) as u8,
                height as u8,
                (width >> 8) as u8,
                width as u8,
                0x01,
            ]
        };
        assert!(allow_image_preview(&jpeg(1920, 1080), "image/jpeg"));
        assert!(!allow_image_preview(
            &jpeg(u16::MAX, u16::MAX),
            "image/jpeg"
        ));

        let gif_frame = [0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 0x01, 0];
        let mut gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00".to_vec();
        gif.extend_from_slice(&gif_frame);
        gif.push(0x3b);
        assert!(allow_image_preview(&gif, "image/gif"));
        let trailer = gif.pop().expect("GIF should have a trailer");
        gif.extend_from_slice(&gif_frame);
        gif.push(trailer);
        assert!(!allow_image_preview(&gif, "image/gif"));

        let webp = |width: u32, height: u32, animated: bool| {
            let width = width - 1;
            let height = height - 1;
            let mut bytes = b"RIFF\x16\x00\x00\x00WEBPVP8X\x0a\x00\x00\x00".to_vec();
            bytes.push(if animated { 0x02 } else { 0 });
            bytes.extend_from_slice(&[0, 0, 0]);
            bytes.extend_from_slice(&[
                width as u8,
                (width >> 8) as u8,
                (width >> 16) as u8,
                height as u8,
                (height >> 8) as u8,
                (height >> 16) as u8,
            ]);
            bytes
        };
        assert!(allow_image_preview(&webp(1920, 1080, false), "image/webp"));
        assert!(!allow_image_preview(
            &webp(10_000, 10_000, false),
            "image/webp"
        ));
        assert!(!allow_image_preview(&webp(1920, 1080, true), "image/webp"));

        let bmp = |width: i32, height: i32| {
            let mut bytes = vec![0; 54];
            bytes[..2].copy_from_slice(b"BM");
            bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
            bytes[18..22].copy_from_slice(&width.to_le_bytes());
            bytes[22..26].copy_from_slice(&height.to_le_bytes());
            bytes
        };
        assert!(allow_image_preview(&bmp(1920, 1080), "image/bmp"));
        assert!(!allow_image_preview(&bmp(100_000, 100_000), "image/bmp"));

        for media_type in ["image/jpeg", "image/gif", "image/webp", "image/bmp"] {
            assert!(!allow_image_preview(b"malformed", media_type));
        }
    }
}
