use crate::error::{DiffError, Result};
use crate::image_export::{write_heatmap, write_overlay, write_side_by_side};
use base64::Engine;
use printpdf::{Mm, Op, PdfDocument, PdfPage, PdfSaveOptions, Pt, RawImage, XObjectTransform};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct PdfVisualOptions {
    pub pdftoppm: PathBuf,
    pub dpi: u32,
    pub max_pages: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PdfVisualReport {
    pub left: PathBuf,
    pub right: PathBuf,
    pub left_pages: usize,
    pub right_pages: usize,
    pub pages_compared: usize,
    pub pages_added: usize,
    pub pages_removed: usize,
    pub changed_pages: usize,
    pub output_dir: PathBuf,
    pub report_html: PathBuf,
    pub report_pdf: PathBuf,
    pub report_overlay_pdf: PathBuf,
    pub report_vector_pdf: PathBuf,
    pub pages: Vec<PdfPageDiff>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PdfPageDiff {
    pub page: usize,
    pub left_image: PathBuf,
    pub right_image: PathBuf,
    pub heatmap: PathBuf,
    pub side_by_side: PathBuf,
    pub overlay: PathBuf,
    pub differing_pixels: u64,
    pub total_pixels: u64,
    pub diff_ratio: f64,
}

impl PdfVisualReport {
    pub fn has_changes(&self) -> bool {
        self.pages_added > 0
            || self.pages_removed > 0
            || self.pages.iter().any(|page| page.differing_pixels > 0)
    }

    pub fn summary_text(&self) -> String {
        let mut out = String::new();
        out.push_str("PDF visual diff\n");
        out.push_str(&format!(
            "left   : {} ({} pages)\n",
            self.left.display(),
            self.left_pages
        ));
        out.push_str(&format!(
            "right  : {} ({} pages)\n",
            self.right.display(),
            self.right_pages
        ));
        out.push_str(&format!(
            "compared: {} page(s), {} changed, {} added, {} removed\n",
            self.pages_compared, self.changed_pages, self.pages_added, self.pages_removed
        ));
        for page in &self.pages {
            if page.differing_pixels == 0 {
                continue;
            }
            out.push_str(&format!(
                "page {}: {:.2}% pixels differ ({} / {})\n",
                page.page,
                page.diff_ratio * 100.0,
                page.differing_pixels,
                page.total_pixels
            ));
            out.push_str(&format!("  heatmap     : {}\n", page.heatmap.display()));
            out.push_str(&format!(
                "  side-by-side: {}\n",
                page.side_by_side.display()
            ));
        }
        out.push_str(&format!("report: {}\n", self.report_html.display()));
        out.push_str(&format!("pdf   : {}\n", self.report_pdf.display()));
        out.push_str(&format!("overlay: {}\n", self.report_overlay_pdf.display()));
        out.push_str(&format!("vector : {}\n", self.report_vector_pdf.display()));
        out
    }
}

pub fn visual_pdf_diff(
    left: &Path,
    right: &Path,
    output_dir: &Path,
    options: &PdfVisualOptions,
) -> Result<PdfVisualReport> {
    if !options.pdftoppm.exists() {
        return Err(DiffError::Invalid(format!(
            "pdftoppm not found at {}",
            options.pdftoppm.display()
        )));
    }
    let left_pages = pdf_page_count(left)?;
    let right_pages = pdf_page_count(right)?;
    if left_pages == 0 || right_pages == 0 {
        return Err(DiffError::Invalid(
            "could not determine PDF page count".to_string(),
        ));
    }
    let max_pages = options.max_pages.min(left_pages.max(right_pages)).max(1);

    if output_dir.exists() {
        fs::remove_dir_all(output_dir)?;
    }
    let left_dir = output_dir.join("left");
    let right_dir = output_dir.join("right");
    let heatmap_dir = output_dir.join("heatmaps");
    let side_dir = output_dir.join("side-by-side");
    let overlay_dir = output_dir.join("overlays");
    for dir in [&left_dir, &right_dir, &heatmap_dir, &side_dir, &overlay_dir] {
        fs::create_dir_all(dir)?;
    }

    let left_images = render_pdf_pages(left, &left_dir, &options.pdftoppm, options.dpi, max_pages)?;
    let right_images =
        render_pdf_pages(right, &right_dir, &options.pdftoppm, options.dpi, max_pages)?;
    let compared = left_images.len().min(right_images.len());
    let mut pages = Vec::with_capacity(compared);

    for index in 0..compared {
        let left_image = &left_images[index];
        let right_image = &right_images[index];
        let left_pixels = image::open(left_image)?.to_rgba8();
        let right_pixels = image::open(right_image)?.to_rgba8();
        let diff = crate::diff::image::compare_rgba(&left_pixels, &right_pixels);
        let page_number = index + 1;
        let heatmap = heatmap_dir.join(format!("page-{page_number}.png"));
        let side_by_side = side_dir.join(format!("page-{page_number}.png"));
        let overlay = overlay_dir.join(format!("page-{page_number}.png"));
        write_heatmap(left_image, right_image, &heatmap)?;
        write_side_by_side(left_image, right_image, &side_by_side)?;
        write_overlay(left_image, right_image, &overlay)?;
        pages.push(PdfPageDiff {
            page: page_number,
            left_image: left_image.clone(),
            right_image: right_image.clone(),
            heatmap,
            side_by_side,
            overlay,
            differing_pixels: diff.differing_pixels,
            total_pixels: diff.total_pixels,
            diff_ratio: diff.diff_ratio,
        });
    }

    let pages_added = right_pages.saturating_sub(left_pages);
    let pages_removed = left_pages.saturating_sub(right_pages);
    let changed_pages = pages
        .iter()
        .filter(|page| page.differing_pixels > 0)
        .count();
    let report_html = output_dir.join("report.html");
    write_html_report(
        left,
        right,
        &pages,
        pages_added,
        pages_removed,
        &report_html,
    )?;
    let report_pdf = output_dir.join("report.pdf");
    write_pdf_report(&pages, &report_pdf)?;
    let report_overlay_pdf = output_dir.join("report-overlay.pdf");
    let overlay_images: Vec<PathBuf> = pages.iter().map(|page| page.overlay.clone()).collect();
    write_image_pdf(&overlay_images, &report_overlay_pdf)?;
    let report_vector_pdf = output_dir.join("report-vector.pdf");
    write_vector_report(left, &pages, &report_vector_pdf)?;

    Ok(PdfVisualReport {
        left: left.to_path_buf(),
        right: right.to_path_buf(),
        left_pages,
        right_pages,
        pages_compared: compared,
        pages_added,
        pages_removed,
        changed_pages,
        output_dir: output_dir.to_path_buf(),
        report_html,
        report_pdf,
        report_overlay_pdf,
        report_vector_pdf,
        pages,
    })
}

/// Render PDF pages to PNG files. This is public so OCR can reuse the same
/// poppler-based renderer.
pub fn render_pdf_pages(
    pdf: &Path,
    output_dir: &Path,
    pdftoppm: &Path,
    dpi: u32,
    max_pages: usize,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(output_dir)?;
    let prefix = output_dir.join("page");
    let output = Command::new(pdftoppm)
        .arg("-png")
        .arg("-r")
        .arg(dpi.to_string())
        .arg("-f")
        .arg("1")
        .arg("-l")
        .arg(max_pages.to_string())
        .arg(pdf)
        .arg(&prefix)
        .output()
        .map_err(|error| DiffError::Invalid(format!("cannot run pdftoppm: {error}")))?;
    if !output.status.success() {
        return Err(DiffError::Invalid(format!(
            "pdftoppm failed for {}: {}",
            pdf.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let mut pages = Vec::new();
    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("png") {
            pages.push(path);
        }
    }
    pages.sort_by_key(|path| page_number(path).unwrap_or(u32::MAX));
    Ok(pages)
}

fn page_number(path: &Path) -> Option<u32> {
    path.file_stem()?
        .to_string_lossy()
        .rsplit('-')
        .next()?
        .parse()
        .ok()
}

fn pdf_page_count(path: &Path) -> Result<usize> {
    let document = lopdf::Document::load(path).map_err(|error| {
        DiffError::Invalid(format!("cannot read PDF {}: {error}", path.display()))
    })?;
    Ok(document.get_pages().len())
}

fn write_pdf_report(pages: &[PdfPageDiff], report_path: &Path) -> Result<()> {
    let mut document = PdfDocument::new("DiffEverything PDF visual diff");
    let mut pdf_pages = Vec::new();

    if pages.is_empty() {
        pdf_pages.push(PdfPage::new(Mm(297.0), Mm(210.0), Vec::new()));
    }

    for page in pages {
        let side_bytes = std::fs::read(&page.side_by_side)?;
        let heat_bytes = std::fs::read(&page.heatmap)?;
        let mut warnings = Vec::new();
        let side_image = RawImage::decode_from_bytes(&side_bytes, &mut warnings)
            .map_err(|error| DiffError::Invalid(format!("PDF image: {error}")))?;
        let heat_image = RawImage::decode_from_bytes(&heat_bytes, &mut warnings)
            .map_err(|error| DiffError::Invalid(format!("PDF image: {error}")))?;
        let side_id = document.add_image(&side_image);
        let heat_id = document.add_image(&heat_image);

        let ops = vec![
            place_pdf_image(side_id, &side_image, 277.0, 90.0, 10.0, 110.0),
            place_pdf_image(heat_id, &heat_image, 277.0, 90.0, 10.0, 10.0),
        ];
        pdf_pages.push(PdfPage::new(Mm(297.0), Mm(210.0), ops));
    }

    let bytes = document
        .with_pages(pdf_pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    std::fs::write(report_path, bytes)?;
    Ok(())
}

fn write_vector_report(left_pdf: &Path, pages: &[PdfPageDiff], report_path: &Path) -> Result<()> {
    let mut document = lopdf::Document::load(left_pdf).map_err(|error| {
        DiffError::Invalid(format!("cannot load PDF for vector report: {error}"))
    })?;
    let page_ids: Vec<lopdf::ObjectId> = document.get_pages().values().copied().collect();
    let page_sizes = pdf_page_sizes(&document, &page_ids);

    for (index, page) in pages.iter().enumerate() {
        let Some(page_id) = page_ids.get(index).copied() else {
            break;
        };
        let left_pixels = image::open(&page.left_image)?.to_rgba8();
        let right_pixels = image::open(&page.right_image)?.to_rgba8();
        let rectangles = crate::diff::image::diff_rectangles(&left_pixels, &right_pixels, 12);
        let (page_width, page_height) = page_sizes.get(index).copied().unwrap_or((595.0, 842.0));
        let content = vector_rectangles(
            &rectangles,
            left_pixels.width(),
            left_pixels.height(),
            page_width,
            page_height,
        );
        document
            .add_page_contents(page_id, content.into_bytes())
            .map_err(|error| DiffError::Invalid(format!("cannot add PDF overlay: {error}")))?;
    }

    document
        .save(report_path)
        .map_err(|error| DiffError::Invalid(format!("cannot save vector PDF: {error}")))?;
    Ok(())
}

fn pdf_page_sizes(document: &lopdf::Document, page_ids: &[lopdf::ObjectId]) -> Vec<(f32, f32)> {
    page_ids
        .iter()
        .map(|page_id| {
            document
                .get_object(*page_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(|dictionary| dictionary.get(b"MediaBox").ok())
                .and_then(|object| object.as_array().ok())
                .and_then(|values| {
                    if values.len() != 4 {
                        return None;
                    }
                    let numbers: Vec<f32> = values
                        .iter()
                        .map(|value| {
                            value
                                .as_float()
                                .or_else(|_| value.as_i64().map(|value| value as f32))
                                .ok()
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some((
                        (numbers[2] - numbers[0]).abs(),
                        (numbers[3] - numbers[1]).abs(),
                    ))
                })
                .unwrap_or((595.0, 842.0))
        })
        .collect()
}

fn vector_rectangles(
    rectangles: &[crate::model::BoundingBox],
    image_width: u32,
    image_height: u32,
    page_width: f32,
    page_height: f32,
) -> String {
    use std::fmt::Write;
    let mut content = String::from("q\n1 0 0 RG\n1.5 w\n");
    let scale_x = page_width / image_width.max(1) as f32;
    let scale_y = page_height / image_height.max(1) as f32;
    for rectangle in rectangles {
        let x = rectangle.x as f32 * scale_x;
        let y = page_height - (rectangle.y + rectangle.height) as f32 * scale_y;
        let width = rectangle.width as f32 * scale_x;
        let height = rectangle.height as f32 * scale_y;
        let _ = writeln!(content, "{x} {y} {width} {height} re S");
    }
    content.push_str("Q\n");
    content
}

fn write_image_pdf(images: &[PathBuf], report_path: &Path) -> Result<()> {
    let mut document = PdfDocument::new("DiffEverything PDF overlay diff");
    let mut pdf_pages = Vec::new();
    if images.is_empty() {
        pdf_pages.push(PdfPage::new(Mm(297.0), Mm(210.0), Vec::new()));
    }
    for image_path in images {
        let bytes = std::fs::read(image_path)?;
        let mut warnings = Vec::new();
        let image = RawImage::decode_from_bytes(&bytes, &mut warnings)
            .map_err(|error| DiffError::Invalid(format!("PDF overlay image: {error}")))?;
        let id = document.add_image(&image);
        let ops = vec![place_pdf_image(id, &image, 277.0, 190.0, 10.0, 10.0)];
        pdf_pages.push(PdfPage::new(Mm(297.0), Mm(210.0), ops));
    }
    let bytes = document
        .with_pages(pdf_pages)
        .save(&PdfSaveOptions::default(), &mut Vec::new());
    std::fs::write(report_path, bytes)?;
    Ok(())
}

fn place_pdf_image(
    id: printpdf::XObjectId,
    image: &RawImage,
    max_width_mm: f32,
    max_height_mm: f32,
    x_mm: f32,
    y_mm: f32,
) -> Op {
    let width_points = image.width as f32;
    let height_points = image.height as f32;
    let width_mm = points_to_mm(width_points);
    let height_mm = points_to_mm(height_points);
    let scale = (max_width_mm / width_mm).min(max_height_mm / height_mm);
    Op::UseXobject {
        id,
        transform: XObjectTransform {
            translate_x: Some(Pt(mm_to_points(x_mm))),
            translate_y: Some(Pt(mm_to_points(y_mm))),
            scale_x: Some(scale),
            scale_y: Some(scale),
            dpi: Some(72.0),
            ..Default::default()
        },
    }
}

fn mm_to_points(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

fn points_to_mm(points: f32) -> f32 {
    points * 25.4 / 72.0
}

fn write_html_report(
    left: &Path,
    right: &Path,
    pages: &[PdfPageDiff],
    pages_added: usize,
    pages_removed: usize,
    report_path: &Path,
) -> Result<()> {
    let mut html = String::new();
    html.push_str(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>PDF visual diff</title>",
    );
    html.push_str("<style>body{font-family:system-ui,sans-serif;margin:24px;background:#f7f7f5;color:#222}img{max-width:100%;border:1px solid #ddd;border-radius:6px;background:#fff}.page{background:#fff;border:1px solid #ddd;border-radius:10px;padding:16px;margin:18px 0}.grid{display:grid;grid-template-columns:1fr 1fr;gap:10px}code{overflow-wrap:anywhere}</style></head><body>");
    html.push_str("<h1>PDF visual diff</h1>");
    html.push_str(&format!(
        "<p><code>{}</code> vs <code>{}</code></p>",
        escape_html(&left.display().to_string()),
        escape_html(&right.display().to_string())
    ));
    html.push_str(&format!(
        "<p>{} compared, {} changed, {} added, {} removed</p>",
        pages.len(),
        pages
            .iter()
            .filter(|page| page.differing_pixels > 0)
            .count(),
        pages_added,
        pages_removed
    ));
    for page in pages {
        html.push_str(&format!(
            "<section class=\"page\"><h2>Page {}</h2><p>{:.2}% pixels differ</p><div class=\"grid\">",
            page.page,
            page.diff_ratio * 100.0
        ));
        for (label, path) in [
            ("Left", &page.left_image),
            ("Right", &page.right_image),
            ("Heatmap", &page.heatmap),
            ("Side by side", &page.side_by_side),
        ] {
            let source =
                embedded_image_source(path, report_path.parent().unwrap_or(Path::new(".")));
            html.push_str(&format!(
                "<figure><figcaption>{}</figcaption><img src=\"{}\"></figure>",
                label,
                escape_html(&source)
            ));
        }
        html.push_str("</div></section>");
    }
    html.push_str("</body></html>");
    fs::write(report_path, html)?;
    Ok(())
}

fn embedded_image_source(path: &Path, report_dir: &Path) -> String {
    match fs::read(path) {
        Ok(bytes) => format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ),
        Err(_) => path
            .strip_prefix(report_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned(),
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
#[path = "../tests/unit/pdf_visual.rs"]
mod tests;
