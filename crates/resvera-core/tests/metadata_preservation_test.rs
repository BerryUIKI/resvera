use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, RgbImage};
use resvera_core::{
    atomic_save_image_with_metadata, load_image_with_alpha, sanitize_exif, strip_gps_from_xmp,
    MetadataPolicy, OutputFormat,
};
use std::fs::File;
use std::io::BufWriter;
use tempfile::tempdir;

fn create_test_exif(orientation: u16, include_gps: bool) -> Vec<u8> {
    let mut buf = Vec::new();
    // TIFF Header: Little-endian
    buf.extend_from_slice(b"II");
    buf.extend_from_slice(&42u16.to_le_bytes());
    buf.extend_from_slice(&8u32.to_le_bytes()); // Offset to IFD0

    let num_entries: u16 = if include_gps { 3 } else { 2 };
    buf.extend_from_slice(&num_entries.to_le_bytes());

    let make_offset = 8 + 2 + (num_entries as u32 * 12) + 4;
    // Entry 1: Make (Tag 0x010F, ASCII, count 8)
    buf.extend_from_slice(&0x010Fu16.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // ASCII
    buf.extend_from_slice(&8u32.to_le_bytes()); // Count
    buf.extend_from_slice(&make_offset.to_le_bytes());

    // Entry 2: Orientation (Tag 0x0112, SHORT, count 1)
    buf.extend_from_slice(&0x0112u16.to_le_bytes());
    buf.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    buf.extend_from_slice(&1u32.to_le_bytes()); // Count 1
    buf.extend_from_slice(&orientation.to_le_bytes());
    buf.extend_from_slice(&[0u8, 0u8]); // Pad to 4 bytes

    if include_gps {
        let gps_ifd_offset = make_offset + 8;
        // Entry 3: GPSInfo (Tag 0x8825, LONG, count 1)
        buf.extend_from_slice(&0x8825u16.to_le_bytes());
        buf.extend_from_slice(&4u16.to_le_bytes()); // LONG
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&gps_ifd_offset.to_le_bytes());
    }

    // Next IFD: 0
    buf.extend_from_slice(&0u32.to_le_bytes());

    // Make string bytes: "Resvera\0"
    buf.extend_from_slice(b"Resvera\0");

    if include_gps {
        // GPS Sub-IFD:
        buf.extend_from_slice(&1u16.to_le_bytes()); // 1 entry
        buf.extend_from_slice(&0x0000u16.to_le_bytes()); // Tag 0
        buf.extend_from_slice(&1u16.to_le_bytes()); // BYTE
        buf.extend_from_slice(&4u32.to_le_bytes()); // Count 4
        buf.extend_from_slice(&[2, 3, 0, 0]); // Value
        buf.extend_from_slice(&0u32.to_le_bytes()); // Next IFD: 0
    }

    buf
}

fn create_test_icc() -> Vec<u8> {
    let mut icc = vec![0u8; 128];
    icc[0..4].copy_from_slice(&128u32.to_be_bytes());
    icc[12..16].copy_from_slice(b"mntr");
    icc[36..40].copy_from_slice(b"acsp");
    icc[40..44].copy_from_slice(b"APPL");
    icc
}

fn create_test_xmp() -> Vec<u8> {
    b"<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"><rdf:Description rdf:about=\"\" xmlns:exif=\"http://ns.adobe.com/exif/1.0/\"><exif:GPSLatitude>37,46.49N</exif:GPSLatitude><exif:Make>ResveraCam</exif:Make></rdf:Description></rdf:RDF></x:xmpmeta>".to_vec()
}

fn has_tag(exif_bytes: &[u8], target_tag: u16) -> bool {
    let tiff_offset = if exif_bytes.starts_with(b"Exif\0\0") {
        6
    } else {
        0
    };
    if exif_bytes.len() < tiff_offset + 10 {
        return false;
    }
    let tiff = &exif_bytes[tiff_offset..];
    let is_le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return false,
    };
    let ifd0 = if is_le {
        u32::from_le_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    } else {
        u32::from_be_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    };
    if ifd0 + 2 > tiff.len() {
        return false;
    }
    let entries = if is_le {
        u16::from_le_bytes([tiff[ifd0], tiff[ifd0 + 1]]) as usize
    } else {
        u16::from_be_bytes([tiff[ifd0], tiff[ifd0 + 1]]) as usize
    };
    let mut offset = ifd0 + 2;
    for _ in 0..entries {
        if offset + 12 > tiff.len() {
            break;
        }
        let tag = if is_le {
            u16::from_le_bytes([tiff[offset], tiff[offset + 1]])
        } else {
            u16::from_be_bytes([tiff[offset], tiff[offset + 1]])
        };
        if tag == target_tag {
            return true;
        }
        offset += 12;
    }
    false
}

fn get_orientation_value(exif_bytes: &[u8]) -> Option<u16> {
    let tiff_offset = if exif_bytes.starts_with(b"Exif\0\0") {
        6
    } else {
        0
    };
    if exif_bytes.len() < tiff_offset + 10 {
        return None;
    }
    let tiff = &exif_bytes[tiff_offset..];
    let is_le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let ifd0 = if is_le {
        u32::from_le_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    } else {
        u32::from_be_bytes([tiff[4], tiff[5], tiff[6], tiff[7]]) as usize
    };
    if ifd0 + 2 > tiff.len() {
        return None;
    }
    let entries = if is_le {
        u16::from_le_bytes([tiff[ifd0], tiff[ifd0 + 1]]) as usize
    } else {
        u16::from_be_bytes([tiff[ifd0], tiff[ifd0 + 1]]) as usize
    };
    let mut offset = ifd0 + 2;
    for _ in 0..entries {
        if offset + 12 > tiff.len() {
            break;
        }
        let tag = if is_le {
            u16::from_le_bytes([tiff[offset], tiff[offset + 1]])
        } else {
            u16::from_be_bytes([tiff[offset], tiff[offset + 1]])
        };
        if tag == 0x0112 {
            let val = if is_le {
                u16::from_le_bytes([tiff[offset + 8], tiff[offset + 9]])
            } else {
                u16::from_be_bytes([tiff[offset + 8], tiff[offset + 9]])
            };
            return Some(val);
        }
        offset += 12;
    }
    None
}

#[test]
fn test_sanitize_exif_gps_and_orientation() {
    let original = create_test_exif(6, true);
    assert!(has_tag(&original, 0x0112)); // Orientation tag exists
    assert_eq!(get_orientation_value(&original), Some(6));
    assert!(has_tag(&original, 0x8825)); // GPS tag exists

    // 1. PreserveSafe: orientation reset to 1, GPS stripped
    let safe = sanitize_exif(&original, false, true);
    assert!(has_tag(&safe, 0x0112));
    assert_eq!(get_orientation_value(&safe), Some(1)); // Reset to 1
    assert!(!has_tag(&safe, 0x8825)); // GPS tag stripped

    // 2. PreserveAll: orientation reset to 1, GPS preserved
    let all = sanitize_exif(&original, true, true);
    assert!(has_tag(&all, 0x0112));
    assert_eq!(get_orientation_value(&all), Some(1));
    assert!(has_tag(&all, 0x8825)); // GPS tag preserved
}

#[test]
fn test_strip_gps_from_xmp_content() {
    let xmp = create_test_xmp();
    let cleaned = strip_gps_from_xmp(&xmp);
    let s = std::str::from_utf8(&cleaned).unwrap();
    assert!(!s.contains("GPSLatitude"));
    assert!(s.contains("ResveraCam"));
}

#[test]
fn test_jpeg_metadata_preservation_and_policies() {
    let temp = tempdir().unwrap();
    let src_path = temp.path().join("input_source.jpg");

    // Write input JPEG with EXIF (orientation 6 + GPS) and ICC profile
    let rgb = RgbImage::new(16, 16);
    let exif = create_test_exif(6, true);
    let icc = create_test_icc();

    {
        let file = File::create(&src_path).unwrap();
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder.set_exif_metadata(exif).unwrap();
        encoder.set_icc_profile(icc.clone()).unwrap();
        encoder
            .write_image(rgb.as_raw(), 16, 16, ExtendedColorType::Rgb8)
            .unwrap();
    }

    // Load with load_image_with_alpha
    let loaded = load_image_with_alpha(&src_path).unwrap();
    assert!(loaded.metadata.exif.is_some());
    assert!(loaded.metadata.icc_profile.is_some());
    assert!(loaded.metadata.orientation_was_applied);

    // Policy 1: PreserveSafe -> ICC preserved, EXIF orientation=1, GPS stripped
    let out_safe = temp.path().join("out_safe.jpg");
    atomic_save_image_with_metadata(
        &loaded.rgb,
        None,
        &out_safe,
        &OutputFormat::Jpeg { quality: 90 },
        Some(&src_path),
        Some(&loaded.metadata),
        &MetadataPolicy::PreserveSafe {
            preserve_gps: false,
        },
    )
    .unwrap();

    let reader = ImageReader::open(&out_safe)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    let mut decoder = reader.into_decoder().unwrap();
    let out_icc = decoder.icc_profile().unwrap();
    let out_exif = decoder.exif_metadata().unwrap();

    assert_eq!(
        out_icc,
        Some(icc.clone()),
        "ICC profile must be preserved in preserveSafe"
    );
    assert!(out_exif.is_some(), "EXIF must be present in preserveSafe");
    let exif_bytes = out_exif.unwrap();
    assert_eq!(
        get_orientation_value(&exif_bytes),
        Some(1),
        "Orientation must be reset to 1"
    );
    assert!(
        !has_tag(&exif_bytes, 0x8825),
        "GPS must be stripped in preserveSafe"
    );

    // Policy 2: PreserveAll -> ICC preserved, EXIF orientation=1, GPS preserved
    let out_all = temp.path().join("out_all.jpg");
    atomic_save_image_with_metadata(
        &loaded.rgb,
        None,
        &out_all,
        &OutputFormat::Jpeg { quality: 90 },
        Some(&src_path),
        Some(&loaded.metadata),
        &MetadataPolicy::PreserveAll,
    )
    .unwrap();

    let reader = ImageReader::open(&out_all)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    let mut decoder = reader.into_decoder().unwrap();
    let all_icc = decoder.icc_profile().unwrap();
    let all_exif = decoder.exif_metadata().unwrap();

    assert_eq!(
        all_icc,
        Some(icc),
        "ICC profile must be preserved in preserveAll"
    );
    let exif_all_bytes = all_exif.unwrap();
    assert_eq!(get_orientation_value(&exif_all_bytes), Some(1));
    assert!(
        has_tag(&exif_all_bytes, 0x8825),
        "GPS must be preserved in preserveAll"
    );

    // Policy 3: StripAll -> No ICC, no EXIF
    let out_strip = temp.path().join("out_strip.jpg");
    atomic_save_image_with_metadata(
        &loaded.rgb,
        None,
        &out_strip,
        &OutputFormat::Jpeg { quality: 90 },
        Some(&src_path),
        Some(&loaded.metadata),
        &MetadataPolicy::Strip,
    )
    .unwrap();

    let reader = ImageReader::open(&out_strip)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    let mut decoder = reader.into_decoder().unwrap();
    assert!(
        decoder.icc_profile().unwrap().is_none(),
        "ICC must be stripped in StripAll"
    );
    assert!(
        decoder.exif_metadata().unwrap().is_none(),
        "EXIF must be stripped in StripAll"
    );
}

#[test]
fn test_png_metadata_preservation_and_policies() {
    let temp = tempdir().unwrap();
    let src_path = temp.path().join("input_source.png");

    let rgb = RgbImage::new(16, 16);
    let exif = create_test_exif(1, true);
    let icc = create_test_icc();

    {
        let file = File::create(&src_path).unwrap();
        let mut writer = BufWriter::new(file);
        let mut encoder = PngEncoder::new(&mut writer);
        encoder.set_exif_metadata(exif).unwrap();
        encoder.set_icc_profile(icc.clone()).unwrap();
        encoder
            .write_image(rgb.as_raw(), 16, 16, ExtendedColorType::Rgb8)
            .unwrap();
    }

    let loaded = load_image_with_alpha(&src_path).unwrap();
    assert!(loaded.metadata.exif.is_some());
    assert!(loaded.metadata.icc_profile.is_some());

    // PreserveSafe in PNG
    let out_safe = temp.path().join("out_safe.png");
    atomic_save_image_with_metadata(
        &loaded.rgb,
        None,
        &out_safe,
        &OutputFormat::Png,
        Some(&src_path),
        Some(&loaded.metadata),
        &MetadataPolicy::PreserveSafe {
            preserve_gps: false,
        },
    )
    .unwrap();

    let reader = ImageReader::open(&out_safe)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    let mut decoder = reader.into_decoder().unwrap();
    assert_eq!(
        decoder.icc_profile().unwrap(),
        Some(icc),
        "PNG ICC profile must be preserved"
    );
    let exif_safe = decoder.exif_metadata().unwrap().unwrap();
    assert!(
        !has_tag(&exif_safe, 0x8825),
        "PNG GPS must be stripped in preserveSafe"
    );

    // StripAll in PNG
    let out_strip = temp.path().join("out_strip.png");
    atomic_save_image_with_metadata(
        &loaded.rgb,
        None,
        &out_strip,
        &OutputFormat::Png,
        Some(&src_path),
        Some(&loaded.metadata),
        &MetadataPolicy::Strip,
    )
    .unwrap();

    let reader = ImageReader::open(&out_strip)
        .unwrap()
        .with_guessed_format()
        .unwrap();
    let mut decoder = reader.into_decoder().unwrap();
    assert!(
        decoder.icc_profile().unwrap().is_none(),
        "PNG ICC stripped in StripAll"
    );
    assert!(
        decoder.exif_metadata().unwrap().is_none(),
        "PNG EXIF stripped in StripAll"
    );
}
