use audiobookgen_core::epub::{inspect_epub, parse_selected_chapters};
use audiobookgen_core::model::{
    CaptionMode, EpubLayout, FootnoteMode, ImportSelection, NarrationProfile, TableMode,
};
use audiobookgen_core::narration::{PLANNER_VERSION, plan_fragments};
use audiobookgen_core::normalize::NORMALIZATION_VERSION;
use chrono::Utc;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[test]
fn inspects_and_plans_reflowable_and_fixed_epub_content() {
    let temp = TempDir::new().expect("temp directory");
    let epub = temp.path().join("fixture.epub");
    write_fixture(&epub);

    let review = inspect_epub(&epub).expect("inspect fixture");
    assert_eq!(review.title, "The Test Book");
    assert_eq!(review.layout, EpubLayout::Mixed);
    assert_eq!(review.chapters.len(), 2);
    assert_eq!(review.chapters[0].table_count, 1);
    assert_eq!(review.chapters[0].footnote_count, 1);
    assert!(review.chapters[1].warnings.is_empty());

    let selection = ImportSelection {
        selected_chapter_indices: vec![0, 1],
        footnote_mode: FootnoteMode::EndOfChapter,
        table_mode: TableMode::Summary,
        caption_mode: CaptionMode::Read,
    };
    let book_id = Uuid::new_v4();
    let parsed =
        parse_selected_chapters(&epub, &review, &selection, book_id).expect("parse fixture");
    let profile = NarrationProfile {
        id: Uuid::new_v4(),
        book_id,
        name: "Test".into(),
        voice: "af_heart".into(),
        speed: 1.0,
        model_revision: "test".into(),
        model_sha256: None,
        normalization_version: NORMALIZATION_VERSION.into(),
        planner_version: PLANNER_VERSION.into(),
        created_at: Utc::now(),
    };
    let fragments = parsed
        .into_iter()
        .flat_map(|chapter| plan_fragments(book_id, chapter.blocks, &profile, &[]))
        .collect::<Vec<_>>();
    assert!(
        fragments
            .iter()
            .any(|fragment| fragment.spoken_text.contains("Doctor Reed"))
    );
    assert!(
        fragments
            .iter()
            .any(|fragment| fragment.spoken_text.starts_with("Table with"))
    );
    assert!(
        fragments
            .iter()
            .any(|fragment| fragment.spoken_text == "Footnotes.")
    );
    assert!(
        fragments
            .iter()
            .all(|fragment| !fragment.cache_key.is_empty())
    );
}

fn write_fixture(path: &std::path::Path) {
    let file = File::create(path).expect("fixture file");
    let mut zip = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("EPUB/package.opf", deflated).unwrap();
    zip.write_all(br#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">fixture</dc:identifier><dc:title>The Test Book</dc:title><dc:creator>Example Author</dc:creator><dc:language>en</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2" properties="rendition:layout-pre-paginated"/></spine></package>"#).unwrap();
    zip.start_file("EPUB/nav.xhtml", deflated).unwrap();
    zip.write_all(br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><nav epub:type="toc" xmlns:epub="http://www.idpf.org/2007/ops"><ol><li><a href="chapter1.xhtml">First Chapter</a></li><li><a href="chapter2.xhtml">Fixed Page</a></li></ol></nav></body></html>"#).unwrap();
    zip.start_file("EPUB/chapter1.xhtml", deflated).unwrap();
    zip.write_all(br#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>First Chapter</title></head><body><h1>Chapter I</h1><p>Dr. Reed paid $12.50. Then she spoke.</p><figure><figcaption>A small diagram.</figcaption></figure><table><tr><th>Name</th><th>Value</th></tr><tr><td>Alpha</td><td>10</td></tr></table><aside epub:type="footnote"><p>A source note.</p></aside></body></html>"#).unwrap();
    zip.start_file("EPUB/chapter2.xhtml", deflated).unwrap();
    zip.write_all(br#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Fixed Page</title></head><body><p>This fixed-layout page contains selectable text and can be narrated without OCR.</p></body></html>"#).unwrap();
    zip.finish().unwrap();
}
