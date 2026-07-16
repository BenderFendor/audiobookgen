use crate::model::{BookDetail, Fragment, GeneratedSegment, NarrationProfile};
use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use roxmltree::Document;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFragment {
    pub fragment: Fragment,
    pub segment: GeneratedSegment,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportChapter {
    pub index: usize,
    pub title: String,
    pub href: String,
    pub fragments: Vec<ExportFragment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub book: BookDetail,
    pub profile: NarrationProfile,
    pub chapters: Vec<ExportChapter>,
}

pub fn export_m4a_chapters(
    manifest: &ExportManifest,
    ffmpeg: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    require_ffmpeg(ffmpeg)?;
    std::fs::create_dir_all(output_dir)?;
    let temp = TempDir::new()?;
    let mut outputs = Vec::new();
    for chapter in &manifest.chapters {
        let wav = temp.path().join(format!("{:03}.wav", chapter.index + 1));
        render_chapter_wav(chapter, &wav)?;
        let output = output_dir.join(format!(
            "{:03}-{}.m4a",
            chapter.index + 1,
            safe_name(&chapter.title)
        ));
        run_ffmpeg(
            ffmpeg,
            vec![
                "-y".into(),
                "-i".into(),
                wav.to_string_lossy().into_owned(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "64k".into(),
                "-ar".into(),
                "24000".into(),
                "-ac".into(),
                "1".into(),
                output.to_string_lossy().into_owned(),
            ],
        )?;
        outputs.push(output);
    }
    Ok(outputs)
}

pub fn export_m4b(manifest: &ExportManifest, ffmpeg: &Path, output: &Path) -> Result<()> {
    require_ffmpeg(ffmpeg)?;
    if manifest.chapters.is_empty() {
        bail!("no generated chapters to export");
    }
    let temp = TempDir::new()?;
    let full_wav = temp.path().join("book.wav");
    render_book_wav(manifest, &full_wav)?;
    let metadata = temp.path().join("metadata.txt");
    let mut body = String::from(";FFMETADATA1\n");
    body.push_str(&format!(
        "title={}\n",
        ffmetadata_escape(&manifest.book.summary.title)
    ));
    body.push_str(&format!(
        "artist={}\n",
        ffmetadata_escape(&manifest.book.summary.authors.join(", "))
    ));
    body.push_str(&format!(
        "album={}\n",
        ffmetadata_escape(&manifest.book.summary.title)
    ));
    body.push_str("comment=Generated locally with AudiobookGen and Kokoro\n");
    let mut current = 0u64;
    for chapter in &manifest.chapters {
        let duration = chapter_duration_ms(chapter);
        body.push_str("[CHAPTER]\nTIMEBASE=1/1000\n");
        body.push_str(&format!(
            "START={current}\nEND={}\ntitle={}\n",
            current + duration,
            ffmetadata_escape(&chapter.title)
        ));
        current += duration;
    }
    std::fs::write(&metadata, body)?;
    run_ffmpeg(
        ffmpeg,
        vec![
            "-y".into(),
            "-i".into(),
            full_wav.to_string_lossy().into_owned(),
            "-i".into(),
            metadata.to_string_lossy().into_owned(),
            "-map".into(),
            "0:a".into(),
            "-map_metadata".into(),
            "1".into(),
            "-map_chapters".into(),
            "1".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "64k".into(),
            "-movflags".into(),
            "+faststart".into(),
            output.to_string_lossy().into_owned(),
        ],
    )
}

pub fn export_narrated_epub(
    source_epub: &Path,
    manifest: &ExportManifest,
    ffmpeg: &Path,
    output: &Path,
) -> Result<()> {
    require_ffmpeg(ffmpeg)?;
    let source = File::open(source_epub)?;
    let mut archive = ZipArchive::new(source)?;
    let mut entries = BTreeMap::<String, Vec<u8>>::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        entries.insert(file.name().to_owned(), bytes);
    }
    let opf_path = package_path_from_entries(&entries)?;
    let original_opf = String::from_utf8(
        entries
            .get(&opf_path)
            .cloned()
            .ok_or_else(|| anyhow!("package document missing"))?,
    )?;
    let opf_dir = Path::new(&opf_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let href_to_item = package_href_items(&original_opf, opf_dir)?;
    let temp = TempDir::new()?;
    let mut additions = Vec::new();
    let mut overlay_links = Vec::new();
    let mut total_duration = 0u64;
    for chapter in &manifest.chapters {
        let audio_name = format!("AudiobookGen/audio/{:03}.m4a", chapter.index + 1);
        let smil_name = format!("AudiobookGen/overlays/{:03}.smil", chapter.index + 1);
        let wav = temp.path().join(format!("{:03}.wav", chapter.index + 1));
        let audio = temp.path().join(format!("{:03}.m4a", chapter.index + 1));
        render_chapter_wav(chapter, &wav)?;
        run_ffmpeg(
            ffmpeg,
            vec![
                "-y".into(),
                "-i".into(),
                wav.to_string_lossy().into_owned(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "64k".into(),
                "-ar".into(),
                "24000".into(),
                "-ac".into(),
                "1".into(),
                audio.to_string_lossy().into_owned(),
            ],
        )?;
        entries.insert(audio_name.clone(), std::fs::read(audio)?);
        let duration = chapter_duration_ms(chapter);
        total_duration += duration;
        entries.insert(
            smil_name.clone(),
            render_smil(chapter, &relative_from(&smil_name, &audio_name), duration).into_bytes(),
        );
        additions.push(format!(
            "<item id=\"ag-audio-{}\" href=\"{}\" media-type=\"audio/mp4\"/>",
            chapter.index,
            relative_from(&opf_path, &audio_name)
        ));
        additions.push(format!(
            "<item id=\"ag-smil-{}\" href=\"{}\" media-type=\"application/smil+xml\"/>",
            chapter.index,
            relative_from(&opf_path, &smil_name)
        ));
        if let Some(item_id) = href_to_item.get(&chapter.href) {
            overlay_links.push((item_id.clone(), format!("ag-smil-{}", chapter.index)));
        }
        if let Some(source) = entries.get(&chapter.href).cloned() {
            let updated =
                inject_sentence_targets(&String::from_utf8_lossy(&source), &chapter.fragments);
            entries.insert(chapter.href.clone(), updated.into_bytes());
        }
    }
    let updated_opf = update_package_document(
        &original_opf,
        &additions,
        &overlay_links,
        total_duration,
        &manifest.profile.name,
    )?;
    entries.insert(opf_path, updated_opf.into_bytes());
    entries.insert(
        "AudiobookGen/manifest.json".into(),
        serde_json::to_vec_pretty(manifest)?,
    );
    let destination = File::create(output)?;
    let mut writer = ZipWriter::new(destination);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    if let Some(mimetype) = entries.remove("mimetype") {
        writer.start_file("mimetype", stored)?;
        writer.write_all(&mimetype)?;
    }
    for (name, bytes) in entries {
        writer.start_file(name, deflated)?;
        writer.write_all(&bytes)?;
    }
    writer.finish()?;
    Ok(())
}

fn render_chapter_wav(chapter: &ExportChapter, output: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output, spec)?;
    for item in &chapter.fragments {
        let mut reader = hound::WavReader::open(&item.segment.audio_path)
            .with_context(|| format!("reading {}", item.segment.audio_path.display()))?;
        let input = reader.spec();
        if input.channels != 1 || input.sample_rate != 24_000 || input.bits_per_sample != 16 {
            bail!(
                "generated segment has unsupported WAV format: {}",
                item.segment.audio_path.display()
            );
        }
        for sample in reader.samples::<i16>() {
            writer.write_sample(sample?)?;
        }
        for _ in 0..(u64::from(item.fragment.pause_after_ms) * 24) {
            writer.write_sample(0i16)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

fn render_book_wav(manifest: &ExportManifest, output: &Path) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(output, spec)?;
    for chapter in &manifest.chapters {
        for item in &chapter.fragments {
            let mut reader = hound::WavReader::open(&item.segment.audio_path)?;
            for sample in reader.samples::<i16>() {
                writer.write_sample(sample?)?;
            }
            for _ in 0..(u64::from(item.fragment.pause_after_ms) * 24) {
                writer.write_sample(0i16)?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}

fn render_smil(chapter: &ExportChapter, audio_href: &str, duration_ms: u64) -> String {
    let overlay_path = format!("AudiobookGen/overlays/{:03}.smil", chapter.index + 1);
    let mut current = 0u64;
    let mut pars = String::new();
    for (index, item) in chapter.fragments.iter().enumerate() {
        let start = current;
        let end = start + item.segment.duration_ms;
        current = end + u64::from(item.fragment.pause_after_ms);
        let target = format!(
            "{}#ag-{}",
            relative_from(&overlay_path, &chapter.href),
            &item.fragment.locator.source_text_hash[..12]
        );
        pars.push_str(&format!("<par id=\"ag-par-{index}\"><text src=\"{}\"/><audio src=\"{}\" clipBegin=\"{}\" clipEnd=\"{}\"/></par>", xml_escape(&target), xml_escape(audio_href), clock(start), clock(end)));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><smil xmlns=\"http://www.w3.org/ns/SMIL\" version=\"3.0\"><body><seq epub:textref=\"{}\" xmlns:epub=\"http://www.idpf.org/2007/ops\" dur=\"{}\">{pars}</seq></body></smil>",
        xml_escape(&relative_from(&overlay_path, &chapter.href)),
        clock(duration_ms)
    )
}

fn inject_sentence_targets(source: &str, fragments: &[ExportFragment]) -> String {
    let mut output = source.to_owned();
    for item in fragments {
        let id = format!("ag-{}", &item.fragment.locator.source_text_hash[..12]);
        if output.contains(&format!("id=\"{id}\"")) {
            continue;
        }
        let plain = &item.fragment.source_text;
        let escaped = xml_escape_text(plain);
        if output.contains(plain) {
            output = output.replacen(
                plain,
                &format!("<span id=\"{id}\">{}</span>", xml_escape_text(plain)),
                1,
            );
        } else if output.contains(&escaped) {
            output = output.replacen(&escaped, &format!("<span id=\"{id}\">{escaped}</span>"), 1);
        }
    }
    if let Some(position) = output.find("</head>") {
        output.insert_str(
            position,
            "<style>.-epub-media-overlay-active{background:#d7ff39;color:#11110e;}</style>",
        );
    }
    output
}

fn update_package_document(
    source: &str,
    additions: &[String],
    overlays: &[(String, String)],
    duration_ms: u64,
    narrator: &str,
) -> Result<String> {
    let mut output = source.to_owned();
    for (item_id, overlay_id) in overlays {
        let pattern = Regex::new(&format!(
            r#"(?is)<item\b[^>]*\bid\s*=\s*[\"']{}[\"'][^>]*/?>"#,
            regex::escape(item_id)
        ))?;
        output = pattern
            .replace(&output, |capture: &regex::Captures| {
                let whole = capture.get(0).expect("whole match").as_str();
                if whole.contains("media-overlay=") {
                    return whole.to_owned();
                }
                if let Some(prefix) = whole.strip_suffix("/>") {
                    format!("{prefix} media-overlay=\"{overlay_id}\"/>")
                } else {
                    format!(
                        "{} media-overlay=\"{overlay_id}\">",
                        whole.trim_end_matches('>')
                    )
                }
            })
            .into_owned();
    }
    let manifest_end = output
        .find("</manifest>")
        .ok_or_else(|| anyhow!("package document has no manifest"))?;
    output.insert_str(manifest_end, &additions.join(""));
    let metadata_end = output
        .find("</metadata>")
        .ok_or_else(|| anyhow!("package document has no metadata"))?;
    output.insert_str(metadata_end, &format!("<meta property=\"media:duration\">{}</meta><meta property=\"media:narrator\">{}</meta><meta property=\"media:active-class\">-epub-media-overlay-active</meta>", clock(duration_ms), xml_escape(narrator)));
    Ok(output)
}

fn package_path_from_entries(entries: &BTreeMap<String, Vec<u8>>) -> Result<String> {
    let source = entries
        .get("META-INF/container.xml")
        .ok_or_else(|| anyhow!("container.xml missing"))?;
    let text = String::from_utf8_lossy(source);
    let document = Document::parse(&text)?;
    document
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("container.xml has no rootfile"))
}
fn package_href_items(source: &str, opf_dir: &Path) -> Result<HashMap<String, String>> {
    let doc = Document::parse(source)?;
    Ok(doc
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| {
            Some((
                normalize_join(opf_dir, node.attribute("href")?),
                node.attribute("id")?.to_owned(),
            ))
        })
        .collect())
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
fn relative_from(from_file: &str, to_file: &str) -> String {
    let from: Vec<_> = Path::new(from_file)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
        .filter_map(|c| {
            if let Component::Normal(v) = c {
                Some(v.to_os_string())
            } else {
                None
            }
        })
        .collect();
    let to: Vec<_> = Path::new(to_file)
        .components()
        .filter_map(|c| {
            if let Component::Normal(v) = c {
                Some(v.to_os_string())
            } else {
                None
            }
        })
        .collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for value in &to[common..] {
        result.push(value);
    }
    result.to_string_lossy().replace('\\', "/")
}
fn run_ffmpeg(ffmpeg: &Path, args: Vec<String>) -> Result<()> {
    let status = Command::new(ffmpeg)
        .args(args)
        .status()
        .context("running FFmpeg")?;
    if status.success() {
        Ok(())
    } else {
        bail!("FFmpeg failed with status {status}")
    }
}
fn require_ffmpeg(ffmpeg: &Path) -> Result<()> {
    let status = Command::new(ffmpeg)
        .arg("-version")
        .status()
        .context("FFmpeg is required for exports")?;
    if status.success() {
        Ok(())
    } else {
        bail!("FFmpeg executable is unavailable")
    }
}
fn chapter_duration_ms(chapter: &ExportChapter) -> u64 {
    chapter
        .fragments
        .iter()
        .map(|item| item.segment.duration_ms + u64::from(item.fragment.pause_after_ms))
        .sum()
}
fn clock(ms: u64) -> String {
    format!(
        "{}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1_000) % 60,
        ms % 1_000
    )
}
fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .replace(' ', "-")
}
fn ffmetadata_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace(';', "\\;")
        .replace('#', "\\#")
        .replace('\n', " ")
}
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
