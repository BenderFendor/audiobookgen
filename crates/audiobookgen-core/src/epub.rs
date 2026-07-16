use crate::model::{
    CaptionMode, Chapter, ChapterReview, EpubLayout, FootnoteMode, FragmentKind, ImportReview,
    ImportSelection, NarrationBlock, TableMode,
};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use roxmltree::Document;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::ZipArchive;

#[derive(Debug, Clone)]
struct ManifestItem {
    id: String,
    href: String,
    media_type: String,
    properties: String,
}

#[derive(Debug)]
pub struct ParsedChapter {
    pub chapter: Chapter,
    pub blocks: Vec<NarrationBlock>,
}

pub fn inspect_epub(path: impl AsRef<Path>) -> Result<ImportReview> {
    let path = path.as_ref();
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("epub"))
        != Some(true)
    {
        bail!("AudiobookGen currently imports EPUB files only");
    }
    let source_sha256 = sha256_file(path)?;
    let file = File::open(path).context("opening EPUB")?;
    let mut archive = ZipArchive::new(file).context("reading EPUB ZIP container")?;
    let mimetype = read_optional_string(&mut archive, "mimetype")?.unwrap_or_default();
    if !mimetype.trim().is_empty() && mimetype.trim() != "application/epub+zip" {
        bail!("the file is a ZIP archive but not an EPUB package");
    }
    let container = read_zip_string(&mut archive, "META-INF/container.xml")
        .context("EPUB container.xml is missing")?;
    let package_path = package_path(&container)?;
    let package_text =
        read_zip_string(&mut archive, &package_path).context("reading EPUB package document")?;
    let package = Document::parse(&package_text).context("parsing EPUB package document")?;
    let package_dir = Path::new(&package_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));

    let title = metadata_text(&package, "title").unwrap_or_else(|| {
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled book")
            .to_owned()
    });
    let authors = metadata_texts(&package, "creator");
    let language = metadata_text(&package, "language");
    let publisher = metadata_text(&package, "publisher");
    let description = metadata_text(&package, "description");
    let identifier = metadata_text(&package, "identifier");
    let package_layout = package
        .descendants()
        .find(|node| {
            node.has_tag_name("meta") && node.attribute("property") == Some("rendition:layout")
        })
        .and_then(|node| node.text())
        .map(layout_from_value)
        .unwrap_or(EpubLayout::Reflowable);

    let manifest = manifest_items(&package, package_dir);
    let manifest_by_id: HashMap<_, _> = manifest
        .iter()
        .map(|item| (item.id.clone(), item.clone()))
        .collect();
    let nav_titles = navigation_titles(&mut archive, &manifest)?;
    let cover_entry = cover_entry(&package, &manifest);
    let mut chapters = Vec::new();
    let mut layouts = HashSet::new();
    for (index, itemref) in package
        .descendants()
        .filter(|node| node.has_tag_name("itemref"))
        .enumerate()
    {
        let Some(idref) = itemref.attribute("idref") else {
            continue;
        };
        let Some(item) = manifest_by_id.get(idref) else {
            continue;
        };
        if !item.media_type.contains("html") {
            continue;
        }
        let html = read_optional_string(&mut archive, &item.href)?.unwrap_or_default();
        let itemref_properties = itemref.attribute("properties").unwrap_or_default();
        let layout = if item.properties.contains("rendition:layout-pre-paginated")
            || itemref_properties.contains("rendition:layout-pre-paginated")
        {
            EpubLayout::Fixed
        } else if item.properties.contains("rendition:layout-reflowable")
            || itemref_properties.contains("rendition:layout-reflowable")
        {
            EpubLayout::Reflowable
        } else {
            package_layout.clone()
        };
        layouts.insert(match layout {
            EpubLayout::Reflowable => 0,
            EpubLayout::Fixed => 1,
            EpubLayout::Mixed => 2,
        });
        let plain = narratable_plain_text(&html);
        let words = plain.split_whitespace().count();
        let title = nav_titles
            .get(&strip_fragment(&item.href))
            .cloned()
            .or_else(|| document_title(&html))
            .unwrap_or_else(|| humanize_filename(&item.href));
        let lower = format!("{} {}", title.to_lowercase(), item.href.to_lowercase());
        let likely_front_matter = [
            "cover",
            "titlepage",
            "title page",
            "copyright",
            "contents",
            "table of contents",
            "navigation",
        ]
        .iter()
        .any(|needle| lower.contains(needle));
        let selected = words >= 8 && !likely_front_matter;
        let mut warnings = Vec::new();
        if layout == EpubLayout::Fixed && words < 8 {
            warnings.push(
                "Fixed-layout page has little or no embedded text; OCR is intentionally not used."
                    .into(),
            );
        }
        chapters.push(ChapterReview {
            index,
            title,
            href: item.href.clone(),
            media_type: item.media_type.clone(),
            layout,
            selected,
            estimated_words: words,
            footnote_count: count_footnotes(&html),
            caption_count: count_tag(&html, "figcaption"),
            table_count: count_tag(&html, "table"),
            warnings,
        });
    }
    if chapters.is_empty() {
        bail!("the EPUB spine contains no readable XHTML chapters");
    }
    if !chapters.iter().any(|chapter| chapter.selected) {
        if let Some(chapter) = chapters
            .iter_mut()
            .max_by_key(|chapter| chapter.estimated_words)
        {
            chapter.selected = true;
        }
    }
    let layout = if layouts.contains(&0) && layouts.contains(&1) {
        EpubLayout::Mixed
    } else if layouts.contains(&1) {
        EpubLayout::Fixed
    } else {
        EpubLayout::Reflowable
    };
    let drm_detected = detect_drm(&mut archive)?;
    let mut warnings = Vec::new();
    if layout != EpubLayout::Reflowable {
        warnings.push("Fixed-layout pages are supported when they contain selectable text; image-only pages are not narrated yet.".into());
    }
    if drm_detected {
        warnings.push(
            "Encrypted EPUB resources were detected. AudiobookGen does not remove DRM.".into(),
        );
    }
    Ok(ImportReview {
        source_path: path.to_path_buf(),
        source_sha256,
        title,
        authors,
        language,
        publisher,
        description,
        identifier,
        layout,
        drm_detected,
        chapters,
        cover_entry,
        warnings,
    })
}

pub fn parse_selected_chapters(
    path: impl AsRef<Path>,
    review: &ImportReview,
    selection: &ImportSelection,
    book_id: Uuid,
) -> Result<Vec<ParsedChapter>> {
    let selected: HashSet<usize> = selection.selected_chapter_indices.iter().copied().collect();
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut output = Vec::new();
    for chapter_review in review
        .chapters
        .iter()
        .filter(|chapter| selected.contains(&chapter.index))
    {
        let html = read_zip_string(&mut archive, &chapter_review.href)
            .with_context(|| format!("reading {}", chapter_review.href))?;
        let chapter_id = Uuid::new_v4();
        let blocks = parse_blocks(
            &html,
            chapter_id,
            chapter_review.index,
            &chapter_review.href,
            selection,
        );
        let chapter = Chapter {
            id: chapter_id,
            book_id,
            index: chapter_review.index,
            title: chapter_review.title.clone(),
            href: chapter_review.href.clone(),
            media_type: chapter_review.media_type.clone(),
            layout: chapter_review.layout.clone(),
            selected: true,
            fragment_count: 0,
        };
        output.push(ParsedChapter { chapter, blocks });
    }
    Ok(output)
}

pub fn extract_cover(
    path: impl AsRef<Path>,
    entry: &str,
    destination: impl AsRef<Path>,
) -> Result<()> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut source = archive.by_name(entry)?;
    let mut output = File::create(destination)?;
    std::io::copy(&mut source, &mut output)?;
    output.flush()?;
    Ok(())
}

fn parse_blocks(
    html: &str,
    chapter_id: Uuid,
    chapter_index: usize,
    href: &str,
    selection: &ImportSelection,
) -> Vec<NarrationBlock> {
    let sanitized = remove_sections(html, &["script", "style", "nav"]);
    let pattern = Regex::new(r"(?is)<(h[1-6]|p|blockquote|li|figcaption|table|aside)\b([^>]*)>(.*?)</(?:h[1-6]|p|blockquote|li|figcaption|table|aside)\s*>").expect("valid block regex");
    let mut blocks = Vec::new();
    let mut deferred_notes = Vec::new();
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    for capture in pattern.captures_iter(&sanitized) {
        let tag = capture
            .get(1)
            .map(|value| value.as_str().to_ascii_lowercase())
            .unwrap_or_default();
        let attrs = capture
            .get(2)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let body = capture
            .get(3)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let is_footnote = tag == "aside"
            || attrs.to_ascii_lowercase().contains("footnote")
            || attrs.to_ascii_lowercase().contains("doc-note");
        if is_footnote && selection.footnote_mode == FootnoteMode::Skip {
            continue;
        }
        if tag == "figcaption" && selection.caption_mode == CaptionMode::Skip {
            continue;
        }
        if tag == "table" && selection.table_mode == TableMode::Skip {
            continue;
        }
        let (text, kind) = if tag == "table" {
            (table_text(body, selection.table_mode), FragmentKind::Table)
        } else {
            let text = clean_markup_text(body);
            let kind = if tag.starts_with('h') {
                FragmentKind::Heading
            } else if tag == "figcaption" {
                FragmentKind::Caption
            } else if is_footnote {
                FragmentKind::Footnote
            } else if is_scene_break(&text) {
                FragmentKind::SceneBreak
            } else {
                FragmentKind::Sentence
            };
            (text, kind)
        };
        if text.trim().is_empty() {
            continue;
        }
        let key = hex::encode(Sha256::digest(text.as_bytes()));
        let occurrence = *occurrences
            .entry(key)
            .and_modify(|value| *value += 1)
            .or_insert(0);
        let block = NarrationBlock {
            chapter_id,
            chapter_index,
            href: href.to_owned(),
            text,
            kind,
            occurrence,
        };
        if is_footnote && selection.footnote_mode == FootnoteMode::EndOfChapter {
            deferred_notes.push(block);
        } else {
            blocks.push(block);
        }
    }
    if !deferred_notes.is_empty() {
        blocks.push(NarrationBlock {
            chapter_id,
            chapter_index,
            href: href.to_owned(),
            text: "Footnotes.".into(),
            kind: FragmentKind::Heading,
            occurrence: 0,
        });
        blocks.extend(deferred_notes);
    }
    blocks
}

fn package_path(container: &str) -> Result<String> {
    let document = Document::parse(container)?;
    document
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("container.xml has no rootfile"))
}

fn manifest_items(package: &Document, package_dir: &Path) -> Vec<ManifestItem> {
    package
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| {
            Some(ManifestItem {
                id: node.attribute("id")?.to_owned(),
                href: normalize_join(package_dir, node.attribute("href")?),
                media_type: node
                    .attribute("media-type")
                    .unwrap_or("application/octet-stream")
                    .to_owned(),
                properties: node.attribute("properties").unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

fn navigation_titles(
    archive: &mut ZipArchive<File>,
    manifest: &[ManifestItem],
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let Some(nav) = manifest.iter().find(|item| {
        item.properties
            .split_whitespace()
            .any(|property| property == "nav")
    }) else {
        return Ok(map);
    };
    let Some(source) = read_optional_string(archive, &nav.href)? else {
        return Ok(map);
    };
    let nav_dir = Path::new(&nav.href)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let links = Regex::new(r#"(?is)<a\b[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)
        .expect("valid nav regex");
    for capture in links.captures_iter(&source) {
        let target = normalize_join(nav_dir, &strip_fragment(&capture[1]));
        let label = clean_markup_text(&capture[2]);
        if !label.is_empty() {
            map.entry(target).or_insert(label);
        }
    }
    Ok(map)
}

fn cover_entry(package: &Document, manifest: &[ManifestItem]) -> Option<String> {
    if let Some(item) = manifest.iter().find(|item| {
        item.properties
            .split_whitespace()
            .any(|property| property == "cover-image")
    }) {
        return Some(item.href.clone());
    }
    let cover_id = package
        .descendants()
        .find(|node| node.has_tag_name("meta") && node.attribute("name") == Some("cover"))
        .and_then(|node| node.attribute("content"));
    cover_id
        .and_then(|id| manifest.iter().find(|item| item.id == id))
        .map(|item| item.href.clone())
}

fn detect_drm(archive: &mut ZipArchive<File>) -> Result<bool> {
    let Some(source) = read_optional_string(archive, "META-INF/encryption.xml")? else {
        return Ok(false);
    };
    let algorithms =
        Regex::new(r#"(?i)Algorithm\s*=\s*["']([^"']+)["']"#).expect("valid encryption regex");
    Ok(algorithms
        .captures_iter(&source)
        .filter_map(|capture| capture.get(1))
        .any(|algorithm| {
            let value = algorithm.as_str();
            !value.contains("idpf.org/2008/embedding") && !value.contains("ns.adobe.com/pdf/enc#RC")
        }))
}

fn metadata_text(document: &Document, local_name: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.tag_name().name() == local_name)
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
fn metadata_texts(document: &Document, local_name: &str) -> Vec<String> {
    document
        .descendants()
        .filter(|node| node.tag_name().name() == local_name)
        .filter_map(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}
fn document_title(html: &str) -> Option<String> {
    let pattern = Regex::new(r"(?is)<(?:title|h1|h2)\b[^>]*>(.*?)</(?:title|h1|h2)\s*>")
        .expect("valid title regex");
    pattern
        .captures(html)
        .map(|capture| clean_markup_text(&capture[1]))
        .filter(|value| !value.is_empty())
}
fn count_tag(html: &str, tag: &str) -> usize {
    Regex::new(&format!(r"(?i)<{}\b", regex::escape(tag)))
        .expect("valid tag regex")
        .find_iter(html)
        .count()
}
fn count_footnotes(html: &str) -> usize {
    Regex::new(r"(?i)<([a-z][a-z0-9]*)\b([^>]*)>")
        .expect("valid footnote regex")
        .captures_iter(html)
        .filter(|element| {
            let tag = element[1].to_ascii_lowercase();
            let attrs = element[2].to_ascii_lowercase();
            tag == "aside"
                || attrs.contains("footnote")
                || attrs.contains("doc-note")
                || attrs.contains("doc-endnote")
        })
        .count()
}
fn narratable_plain_text(html: &str) -> String {
    clean_markup_text(&remove_sections(html, &["script", "style", "nav", "head"]))
}
fn remove_sections(html: &str, tags: &[&str]) -> String {
    tags.iter().fold(html.to_owned(), |source, tag| {
        Regex::new(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}\s*>"))
            .expect("valid section regex")
            .replace_all(&source, " ")
            .into_owned()
    })
}
fn clean_markup_text(source: &str) -> String {
    let without_tags = Regex::new(r"(?is)<[^>]+>")
        .expect("valid tags regex")
        .replace_all(source, " ");
    decode_entities(&without_tags)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn decode_entities(source: &str) -> String {
    let named = source
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'");
    let decimal = Regex::new(r"&#([0-9]+);")
        .expect("valid entity regex")
        .replace_all(&named, |capture: &regex::Captures| {
            capture[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|value| value.to_string())
                .unwrap_or_else(|| capture[0].to_owned())
        })
        .into_owned();
    Regex::new(r"(?i)&#x([0-9a-f]+);")
        .expect("valid hex entity regex")
        .replace_all(&decimal, |capture: &regex::Captures| {
            u32::from_str_radix(&capture[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|value| value.to_string())
                .unwrap_or_else(|| capture[0].to_owned())
        })
        .into_owned()
}
fn table_text(body: &str, mode: TableMode) -> String {
    let rows = count_tag(body, "tr");
    let row_pattern = Regex::new(r"(?is)<tr\b[^>]*>(.*?)</tr\s*>").expect("valid row regex");
    let cell_pattern =
        Regex::new(r"(?is)<t[hd]\b[^>]*>(.*?)</t[hd]\s*>").expect("valid cell regex");
    let parsed: Vec<Vec<String>> = row_pattern
        .captures_iter(body)
        .map(|row| {
            cell_pattern
                .captures_iter(&row[1])
                .map(|cell| clean_markup_text(&cell[1]))
                .filter(|value| !value.is_empty())
                .collect()
        })
        .collect();
    let columns = parsed.iter().map(Vec::len).max().unwrap_or(0);
    match mode {
        TableMode::Skip => String::new(),
        TableMode::Summary => {
            let headers = parsed.first().map(|row| row.join(", ")).unwrap_or_default();
            if headers.is_empty() {
                format!("Table with {rows} rows and {columns} columns.")
            } else {
                format!("Table with {rows} rows and {columns} columns. First row: {headers}.")
            }
        }
        TableMode::Cells => parsed
            .into_iter()
            .map(|row| row.join("; "))
            .collect::<Vec<_>>()
            .join(". "),
    }
}
fn is_scene_break(text: &str) -> bool {
    matches!(text.trim(), "*" | "**" | "***" | "* * *" | "—" | "---")
}
fn layout_from_value(value: &str) -> EpubLayout {
    if value.contains("pre-paginated") {
        EpubLayout::Fixed
    } else {
        EpubLayout::Reflowable
    }
}
fn humanize_filename(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled chapter")
        .replace(['_', '-'], " ")
}
fn strip_fragment(path: &str) -> String {
    path.split('#').next().unwrap_or(path).to_owned()
}
fn normalize_join(base: &Path, child: &str) -> String {
    let mut output = PathBuf::new();
    for component in base.join(child).components() {
        match component {
            Component::Normal(value) => output.push(value),
            Component::ParentDir => {
                output.pop();
            }
            _ => {}
        }
    }
    output.to_string_lossy().replace('\\', "/")
}
fn read_zip_string(archive: &mut ZipArchive<File>, name: &str) -> Result<String> {
    let mut file = archive
        .by_name(name)
        .with_context(|| format!("ZIP entry {name} not found"))?;
    let mut output = String::new();
    file.read_to_string(&mut output)
        .with_context(|| format!("ZIP entry {name} is not UTF-8 text"))?;
    Ok(output)
}
fn read_optional_string(archive: &mut ZipArchive<File>, name: &str) -> Result<Option<String>> {
    match archive.by_name(name) {
        Ok(mut file) => {
            let mut output = String::new();
            file.read_to_string(&mut output)?;
            Ok(Some(output))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joins_paths_without_escaping_root() {
        assert_eq!(
            normalize_join(Path::new("EPUB/text"), "../images/cover.jpg"),
            "EPUB/images/cover.jpg"
        );
    }
    #[test]
    fn counts_footnote_elements_once_each() {
        // An aside that also carries epub:type="footnote" is one footnote, not two.
        assert_eq!(
            count_footnotes(r#"<aside epub:type="footnote"><p>Note.</p></aside>"#),
            1
        );
        assert_eq!(count_footnotes("<aside><p>Sidebar note.</p></aside>"), 1);
        assert_eq!(
            count_footnotes(r#"<p role="doc-endnote">An endnote.</p>"#),
            1
        );
        assert_eq!(
            count_footnotes(
                r#"<span epub:type="footnote">a</span><aside role="doc-note">b</aside>"#
            ),
            2
        );
        assert_eq!(count_footnotes("<p>No notes here.</p>"), 0);
    }

    #[test]
    fn ignores_font_obfuscation() {
        let source = r#"<EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>"#;
        let algorithms = Regex::new(r#"(?i)Algorithm\s*=\s*["']([^"']+)["']"#).unwrap();
        assert!(
            !algorithms
                .captures_iter(source)
                .any(|capture| !capture[1].contains("idpf.org/2008/embedding"))
        );
    }
}
