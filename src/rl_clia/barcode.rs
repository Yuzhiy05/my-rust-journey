use crate::layout::{LABEL_HEIGHT, LABEL_WIDTH, LayoutElementKind, PageLayout};
use ab_glyph::{Font, ScaleFont};
use image::GrayImage;
use zxingcpp::*;

/// 标签中 PDF417 条码区域的默认宽度，单位为像素。
const DEFAULT_BARCODE_W: u32 = 600;

/// 标签中 PDF417 条码区域的默认高度，单位为像素。
const DEFAULT_BARCODE_H: u32 = 300;

/// 单张标签导出图的总宽度，单位为像素。
pub const LABEL_W: u32 = LABEL_WIDTH as u32;

/// 单张标签导出图的总高度，单位为像素。
pub const LABEL_H: u32 = LABEL_HEIGHT as u32;

#[derive(Debug, Clone)]
pub struct LabelContent {
    pub title: String,
    pub subtitle1: Option<String>,
    pub subtitle2: Option<String>,
    pub lot_number: String,
    pub prod_date: String,
    pub expire_date: String,
}

/// 将灰度位图转换为 Slint 可直接显示的预览图。
pub fn gray_to_slint_image(gray: &GrayImage) -> slint::Image {
    let w = gray.width();
    let h = gray.height();
    let pixels: Vec<u8> = gray
        .pixels()
        .flat_map(|p| [p[0], p[0], p[0], 255u8])
        .collect();
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&pixels, w, h);
    slint::Image::from_rgba8(buffer)
}

/// 生成 PDF417 条码灰度图。
pub fn generate_barcode(data: &str) -> Result<GrayImage, String> {
    let barcode = create(BarcodeFormat::PDF417)
        .options("columns:3,eclevel:0")
        .from_str(data)
        .map_err(|e| format!("PDF417编码失败: {e}"))?;
    let img = barcode
        .to_image_with(
            &write()
                .scale(3)
                .add_quiet_zones(true)
                .add_hrt(false)
                .rotate(0),
        )
        .map_err(|e| format!("条码图像生成失败: {e}"))?;
    let mut gray = GrayImage::from(&img);
    if gray.width() != DEFAULT_BARCODE_W || gray.height() != DEFAULT_BARCODE_H {
        gray = image::imageops::resize(
            &gray,
            DEFAULT_BARCODE_W,
            DEFAULT_BARCODE_H,
            image::imageops::FilterType::Nearest,
        );
    }
    Ok(gray)
}

/// 按当前布局配置绘制最终标签图。
pub fn render_label(
    barcode: &GrayImage,
    layout: &PageLayout,
    content: &LabelContent,
) -> image::GrayImage {
    let mut canvas = GrayImage::from_pixel(LABEL_W, LABEL_H, image::Luma([255]));
    let font = load_font();
    for element in &layout.elements {
        match element.kind {
            LayoutElementKind::Barcode => draw_barcode_element(&mut canvas, barcode, element),
            LayoutElementKind::Text => {
                let Some(font) = font.as_ref() else {
                    continue;
                };
                let text = text_of(content, &element.id);
                if text.is_empty() {
                    continue;
                }
                draw_text_in_box(
                    &mut canvas,
                    font,
                    element.font_size,
                    &text,
                    element.x,
                    element.y,
                    element.width,
                    element.bold,
                );
            }
        }
    }
    canvas
}

/// 将多张标签图按 A4 网格排版导出为 PDF。
pub fn generate_pdf(images: &[GrayImage], output_path: &str) -> Result<(), String> {
    use miniz_oxide::deflate::compress_to_vec_zlib;
    use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref};

    if images.is_empty() {
        return Err("没有可生成的图像".into());
    }

    let mut pdf = Pdf::new();
    pdf.set_version(1, 7);

    let catalog_id = Ref::new(1);
    let pages_id = Ref::new(2);
    pdf.catalog(catalog_id).pages(pages_id);

    let per_page = 12usize;
    let cols = 3usize;
    let margin = 28.35f32;
    let cell_w = 155.91f32;
    let cell_h = 127.56f32;
    let page_w = 595.28f32;
    let page_h = 841.89f32;

    let page_count = images.len().div_ceil(per_page);
    let mut next_id: i32 = 3;

    let page_ids: Vec<Ref> = (0..page_count)
        .map(|_| {
            let r = Ref::new(next_id);
            next_id += 1;
            r
        })
        .collect();
    let cont_ids: Vec<Ref> = (0..page_count)
        .map(|_| {
            let r = Ref::new(next_id);
            next_id += 1;
            r
        })
        .collect();
    let xobj_ids: Vec<Ref> = (0..images.len())
        .map(|_| {
            let r = Ref::new(next_id);
            next_id += 1;
            r
        })
        .collect();

    pdf.pages(pages_id)
        .kids(page_ids.iter().copied())
        .count(page_count as i32);

    for pi in 0..page_count {
        let start = pi * per_page;
        let end = (start + per_page).min(images.len());
        let page_imgs = &images[start..end];

        for (i, img) in page_imgs.iter().enumerate() {
            let xobj_id = xobj_ids[start + i];
            let compressed = compress_to_vec_zlib(img.as_raw(), 6);
            let mut xobj = pdf.image_xobject(xobj_id, &compressed);
            xobj.filter(Filter::FlateDecode);
            xobj.width(img.width() as i32);
            xobj.height(img.height() as i32);
            xobj.color_space().device_gray();
            xobj.bits_per_component(8);
        }

        let mut content = Content::new();
        for (i, _) in page_imgs.iter().enumerate() {
            let pos = i % per_page;
            let row = pos / cols;
            let col = pos % cols;
            let x = margin + col as f32 * cell_w;
            let y = page_h - margin - (row as f32 + 1.0) * cell_h;
            content.save_state();
            content.transform([cell_w, 0.0, 0.0, cell_h, x, y]);
            content.x_object(Name(format!("Im{}", start + i).as_bytes()));
            content.restore_state();
        }
        pdf.stream(cont_ids[pi], &content.finish());

        {
            let mut page = pdf.page(page_ids[pi]);
            page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
            page.parent(pages_id);
            page.contents(cont_ids[pi]);
            let mut res = page.resources();
            for (i, _) in page_imgs.iter().enumerate() {
                res.pair(
                    Name(format!("Im{}", start + i).as_bytes()),
                    xobj_ids[start + i],
                );
            }
        }
    }

    let bytes = pdf.finish();
    std::fs::write(output_path, &bytes).map_err(|e| format!("保存PDF失败: {e}"))
}

fn draw_barcode_element(
    canvas: &mut GrayImage,
    barcode: &GrayImage,
    element: &crate::layout::LayoutElement,
) {
    let target_w = element.width.max(1.0).round() as u32;
    let target_h = element.height.max(1.0).round() as u32;
    let resized = if barcode.width() == target_w && barcode.height() == target_h {
        barcode.clone()
    } else {
        image::imageops::resize(
            barcode,
            target_w,
            target_h,
            image::imageops::FilterType::Nearest,
        )
    };

    let start_x = element.x.round() as i32;
    let start_y = element.y.round() as i32;
    for y in 0..resized.height() {
        for x in 0..resized.width() {
            let px = start_x + x as i32;
            let py = start_y + y as i32;
            if px >= 0 && py >= 0 && (px as u32) < LABEL_W && (py as u32) < LABEL_H {
                canvas.put_pixel(px as u32, py as u32, *resized.get_pixel(x, y));
            }
        }
    }
}

fn text_of(content: &LabelContent, id: &str) -> String {
    match id {
        "title" => content.title.clone(),
        "subtitle1" => content.subtitle1.clone().unwrap_or_default(),
        "subtitle2" => content.subtitle2.clone().unwrap_or_default(),
        "lot" => format!("产品批号: {}", content.lot_number),
        "prod_date" => format!("生产日期: {}", content.prod_date),
        "expire_date" => format!("失效日期: {}", content.expire_date),
        _ => String::new(),
    }
}

fn load_font() -> Option<ab_glyph::FontArc> {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:/Windows/Fonts/simhei.ttf",
            "C:/Windows/Fonts/msyh.ttf",
            "C:/Windows/Fonts/arial.ttf",
            "C:/Windows/Fonts/times.ttf",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/System/Library/Fonts/Helvetica.ttc",
        ]
    } else {
        &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]
    };
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(data) {
                return Some(font);
            }
        }
    }
    None
}

fn draw_text_in_box(
    img: &mut GrayImage,
    font: &ab_glyph::FontArc,
    px: f32,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    bold: bool,
) {
    let scaled = font.as_scaled(px);
    let total_w = text_width(font, &scaled, text);
    let left = x + ((width - total_w).max(0.0) / 2.0);
    let baseline = y + scaled.ascent();
    draw_text_once(img, font, &scaled, text, left, baseline);
    if bold {
        draw_text_once(img, font, &scaled, text, left + 0.8, baseline);
        draw_text_once(img, font, &scaled, text, left, baseline + 0.8);
    }
}

fn text_width(
    font: &ab_glyph::FontArc,
    scaled: &ab_glyph::PxScaleFont<&ab_glyph::FontArc>,
    text: &str,
) -> f32 {
    let mut width = 0.0f32;
    let mut last_id = None;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(prev) = last_id {
            width += font.kern_unscaled(prev, gid);
        }
        width += scaled.h_advance(gid);
        last_id = Some(gid);
    }
    width
}

fn draw_text_once(
    img: &mut GrayImage,
    font: &ab_glyph::FontArc,
    scaled: &ab_glyph::PxScaleFont<&ab_glyph::FontArc>,
    text: &str,
    start_x: f32,
    baseline_y: f32,
) {
    let mut cursor = start_x;
    let mut last_id = None;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(prev) = last_id {
            cursor += font.kern_unscaled(prev, gid);
        }
        let advance = scaled.h_advance(gid);
        if let Some(outlined) = scaled.outline_glyph(ab_glyph::Glyph {
            id: gid,
            position: ab_glyph::point(cursor, baseline_y),
            scale: scaled.scale().into(),
        }) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, alpha: f32| {
                let px = gx as i32 + bounds.min.x as i32;
                let py = gy as i32 + bounds.min.y as i32;
                if px >= 0 && (px as u32) < LABEL_W && py >= 0 && (py as u32) < LABEL_H {
                    let old = img.get_pixel(px as u32, py as u32)[0] as f32;
                    img.put_pixel(
                        px as u32,
                        py as u32,
                        image::Luma([((1.0 - alpha) * old) as u8]),
                    );
                }
            });
        }
        cursor += advance;
        last_id = Some(gid);
    }
}
