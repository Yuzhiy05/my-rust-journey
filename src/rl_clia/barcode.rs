use ab_glyph::{Font, ScaleFont};
use image::GrayImage;
use zxingcpp::*;

/// 标签中 PDF417 条码区域的目标宽度，单位为像素。
const BARCODE_W: u32 = 600;

/// 标签中 PDF417 条码区域的目标高度，单位为像素。
const BARCODE_H: u32 = 300;

/// 单张标签导出图的总宽度，单位为像素。
pub const LABEL_W: u32 = 660;

/// 单张标签导出图的总高度，单位为像素。
pub const LABEL_H: u32 = 580;

/// 底部“产品批号 / 生产日期 / 失效日期”三行文字的字号。
const FOOTER_FONT_PX: f32 = 23.0;

/// 不同标签类型对应的版式参数。
struct LabelLayout {
    /// 条码区域的起始纵坐标。
    barcode_y: u32,
    /// 底部说明文字首行的纵坐标。
    footer_y: i32,
}

/// 标签图像的业务类型。
#[derive(Debug, Clone, Copy)]
pub enum ImageType {
    ReagentInformation,
    ExcitationFluidA,
    ExcitationFluidB,
    QualityControl,
    CalibrationProduct,
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
    if gray.width() != BARCODE_W || gray.height() != BARCODE_H {
        gray = image::imageops::resize(
            &gray,
            BARCODE_W,
            BARCODE_H,
            image::imageops::FilterType::Nearest,
        );
    }
    Ok(gray)
}

/// 将条码与说明文字合成为最终标签图。
pub fn draw_barcode_with_text(
    barcode: &GrayImage,
    image_type: ImageType,
    project_name: &str,
    lot_number: &str,
    prod_date: &str,
    expire_date: &str,
    test_counts: &str,
) -> GrayImage {
    let mut canvas = GrayImage::from_pixel(LABEL_W, LABEL_H, image::Luma([255]));
    let layout = layout_for(image_type);
    // Paste barcode centered horizontally at the per-label vertical position.
    let bx = (LABEL_W - BARCODE_W) / 2;
    let by = layout.barcode_y;
    for y in 0..barcode.height().min(LABEL_H.saturating_sub(by)) {
        for x in 0..barcode.width().min(LABEL_W.saturating_sub(bx)) {
            canvas.put_pixel(bx + x, by + y, *barcode.get_pixel(x, y));
        }
    }
    if let Some(font) = load_font() {
        render_labels(
            &mut canvas,
            &font,
            image_type,
            project_name,
            lot_number,
            prod_date,
            expire_date,
            test_counts,
            &layout,
        );
    }
    canvas
}

/// 根据业务类型返回标签布局参数。
fn layout_for(image_type: ImageType) -> LabelLayout {
    let barcode_y = match image_type {
        ImageType::ReagentInformation => 130,
        ImageType::QualityControl | ImageType::CalibrationProduct => 98,
        ImageType::ExcitationFluidA | ImageType::ExcitationFluidB => 82,
    };
    LabelLayout {
        barcode_y,
        footer_y: (barcode_y + BARCODE_H + 20) as i32,
    }
}

/// 尝试从系统字体中加载可用字体。
fn load_font() -> Option<ab_glyph::FontArc> {
    // Try TTF files first (ab_glyph handles single TTF better than TTC)
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

/// 在标签画布上绘制标题与底部业务说明。
fn render_labels(
    canvas: &mut GrayImage,
    font: &ab_glyph::FontArc,
    image_type: ImageType,
    project_name: &str,
    lot_number: &str,
    prod_date: &str,
    expire_date: &str,
    test_counts: &str,
    layout: &LabelLayout,
) {
    let black = image::Luma([0]);

    let title = match image_type {
        ImageType::ReagentInformation => "试剂二维码信息",
        ImageType::ExcitationFluidA => "激发液A二维码",
        ImageType::ExcitationFluidB => "激发液B二维码",
        ImageType::QualityControl => "质控品二维码",
        ImageType::CalibrationProduct => "校准品二维码",
    };
    draw_centered_bold(canvas, font, 32.0, title, 38, black);

    if matches!(image_type, ImageType::ReagentInformation) {
        draw_centered(
            canvas,
            font,
            22.0,
            &format!("{project_name} 测定试剂盒"),
            75,
            black,
        );
        draw_centered(
            canvas,
            font,
            18.0,
            &format!("(化学发光免疫分析法)  {test_counts} 测试/盒"),
            100,
            black,
        );
    } else if matches!(
        image_type,
        ImageType::QualityControl | ImageType::CalibrationProduct
    ) {
        draw_centered(canvas, font, 24.0, project_name, 72, black);
    }

    let y0 = layout.footer_y;
    draw_centered(
        canvas,
        font,
        FOOTER_FONT_PX,
        &format!("产品批号: {lot_number}"),
        y0,
        black,
    );
    draw_centered(
        canvas,
        font,
        FOOTER_FONT_PX,
        &format!("生产日期: {prod_date}"),
        y0 + 40,
        black,
    );
    draw_centered(
        canvas,
        font,
        FOOTER_FONT_PX,
        &format!("失效日期: {expire_date}"),
        y0 + 80,
        black,
    );
}

/// 在画布水平居中位置绘制单行文本。
fn draw_centered(
    img: &mut GrayImage,
    font: &ab_glyph::FontArc,
    px: f32,
    text: &str,
    y: i32,
    color: image::Luma<u8>,
) {
    let scaled = font.as_scaled(px);
    let mut total_w = 0.0f32;
    let mut last_id = None;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(prev) = last_id {
            total_w += font.kern_unscaled(prev, gid);
        }
        total_w += scaled.h_advance(gid);
        last_id = Some(gid);
    }
    let mut cursor = (LABEL_W as f32 - total_w) / 2.0;
    last_id = None;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        if let Some(prev) = last_id {
            cursor += font.kern_unscaled(prev, gid);
        }
        let advance = scaled.h_advance(gid);
        if let Some(outlined) = scaled.outline_glyph(ab_glyph::Glyph {
            id: gid,
            position: ab_glyph::point(cursor, y as f32),
            scale: px.into(),
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
                        image::Luma([((1.0 - alpha) * old + alpha * color[0] as f32) as u8]),
                    );
                }
            });
        }
        cursor += advance;
        last_id = Some(gid);
    }
}

/// 通过重复绘制与轻微偏移，生成较粗的文本效果。
fn draw_centered_bold(
    img: &mut GrayImage,
    font: &ab_glyph::FontArc,
    px: f32,
    text: &str,
    y: i32,
    color: image::Luma<u8>,
) {
    draw_centered(img, font, px, text, y, color);
    draw_centered(img, font, px, text, y, color);
    draw_centered(img, font, px, text, y + 1, color);
    draw_centered(img, font, px, text, y, color);
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

    let page_count = (images.len() + per_page - 1) / per_page;
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

        // Write compressed image XObjects
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

        // Build content stream
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

        // Write page
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
