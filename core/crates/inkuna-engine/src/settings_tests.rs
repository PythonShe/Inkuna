use super::{FontFamily, LayoutSettings};

#[test]
fn fingerprint_is_stable_and_sensitive() {
    let base = LayoutSettings::default();
    assert_eq!(base.fingerprint(), LayoutSettings::default().fingerprint());

    let variants = [
        LayoutSettings {
            reading_font: "noto-sans".to_string(),
            ..base.clone()
        },
        LayoutSettings {
            reading_bold: true,
            ..base.clone()
        },
        LayoutSettings {
            text_size_step: 3,
            ..base.clone()
        },
        LayoutSettings {
            line_spacing: 1.80,
            ..base.clone()
        },
        LayoutSettings {
            letter_spacing: 0.02,
            ..base.clone()
        },
        LayoutSettings {
            word_spacing: 0.10,
            ..base.clone()
        },
        LayoutSettings {
            reading_margins: 32,
            ..base.clone()
        },
    ];
    for changed in &variants {
        assert_ne!(
            changed.fingerprint(),
            base.fingerprint(),
            "field change went unnoticed: {changed:?}"
        );
    }
}

#[test]
fn fingerprint_hashes_clamped_values() {
    // 5.0 clamps to the 2.10 ceiling: both settings lay out identically,
    // so they must fingerprint identically (no spurious cache
    // invalidation).
    let ceiling = LayoutSettings {
        line_spacing: 2.10,
        ..LayoutSettings::default()
    };
    let over = LayoutSettings {
        line_spacing: 5.0,
        ..LayoutSettings::default()
    };
    assert_eq!(over.fingerprint(), ceiling.fingerprint());

    // Distinct in-range values still fingerprint apart.
    let a = LayoutSettings {
        line_spacing: 1.80,
        ..LayoutSettings::default()
    };
    let b = LayoutSettings {
        line_spacing: 1.90,
        ..LayoutSettings::default()
    };
    assert_ne!(a.fingerprint(), b.fingerprint());
}

#[test]
fn clamps_out_of_range() {
    let clamped = LayoutSettings {
        text_size_step: 9,
        line_spacing: 5.0,
        letter_spacing: -1.0,
        word_spacing: 0.9,
        reading_margins: 200,
        ..LayoutSettings::default()
    }
    .clamped();

    assert_eq!(clamped.text_size_step, 4);
    assert_eq!(clamped.line_spacing, 2.10);
    assert_eq!(clamped.letter_spacing, 0.0);
    assert_eq!(clamped.word_spacing, 0.30);
    assert_eq!(clamped.reading_margins, 48);

    let low = LayoutSettings {
        line_spacing: 0.5,
        reading_margins: 1,
        ..LayoutSettings::default()
    }
    .clamped();
    assert_eq!(low.line_spacing, 1.30);
    assert_eq!(low.reading_margins, 16);

    let poisoned = LayoutSettings {
        line_spacing: f64::NAN,
        ..LayoutSettings::default()
    }
    .clamped();
    assert_eq!(poisoned.line_spacing, 1.65);
}

#[test]
fn unknown_font_maps_to_serif() {
    for id in ["publisher", "garamond-galaxy", "", "noto-serif", "system-serif"] {
        let settings = LayoutSettings {
            reading_font: id.to_string(),
            ..LayoutSettings::default()
        };
        assert_eq!(settings.font_family(), FontFamily::NotoSerif, "id {id:?}");
    }
    for id in ["noto-sans", "system-sans", "Noto-Sans"] {
        let settings = LayoutSettings {
            reading_font: id.to_string(),
            ..LayoutSettings::default()
        };
        assert_eq!(settings.font_family(), FontFamily::NotoSans, "id {id:?}");
    }
}

#[test]
fn typography_matches_transcription() {
    // Step 2 is the shells' default: 17 pt body (ReadingTextSize.medium /
    // TEXT_SIZE_STEPS[2]), 1.65 line spacing → 28.05 pt line height.
    let typography = LayoutSettings::default().typography();
    assert_eq!(typography.font_size, 17.0);
    assert!((typography.line_height - 28.05).abs() < 1e-9);
    assert_eq!(typography.paragraph_spacing, 17.0);
    assert_eq!(typography.paragraph_indent, 0.0);
    assert_eq!(typography.heading_scale, [2.0, 1.5, 1.17, 1.0, 0.83, 0.67]);
    assert_eq!(typography.ruby_scale, (1, 2));
    assert!(!typography.bold_base);

    // The full transcribed step table.
    for (step, expected) in [14.4, 15.6, 17.0, 18.4, 20.0].iter().enumerate() {
        let settings = LayoutSettings {
            text_size_step: step as u8,
            ..LayoutSettings::default()
        };
        assert_eq!(settings.typography().font_size, *expected);
    }
}
