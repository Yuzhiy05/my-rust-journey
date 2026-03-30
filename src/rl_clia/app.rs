use crate::barcode::{
    ImageType, draw_barcode_with_text, generate_barcode, generate_pdf, gray_to_slint_image,
};
use crate::config;
use crate::encryptor;
use chrono::{Duration, Local};
use slint::{ModelRc, VecModel};
use std::sync::{Arc, Mutex};

slint::include_modules!();

/// 按业务类型缓存已生成的标签图，供预览与导出复用。
type ImageStore = Arc<Mutex<Vec<image::GrayImage>>>;

/// 应用运行入口。
///
/// 该函数只负责组织初始化流程，避免把窗口初始化、状态准备和回调绑定
/// 全部堆叠在一个大函数中。
pub fn run() {
    let proj = config::load_project_config();
    let window = RLCLIAWindow::new().expect("创建窗口失败");
    let stores = create_image_stores();

    init_project_models(&window, &proj);
    init_default_dates(&window);
    bind_compute_expiry_callback(&window);
    bind_generate_preview_callback(&window, proj.clone(), stores.clone());
    bind_export_png_callback(&window, stores.clone());
    bind_export_pdf_callback(&window, stores);
    bind_decrypt_callback(&window);

    window.run().expect("运行失败");
}

/// 返回当前日期，格式为 `YYYY-MM-DD`。
fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 根据生产日期与有效天数计算失效日期。
fn compute_expire(date: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d + Duration::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// 为试剂序列号批量生成当天流水号。
fn generate_serials(count: usize) -> Vec<String> {
    let d = Local::now().format("%Y%m%d").to_string();
    (1..=count).map(|i| format!("{d}{:04}", i)).collect()
}

/// 校验一组必填字段是否都已填写。
fn require_fields(fields: &[(&str, &str)]) -> Result<(), String> {
    for (name, val) in fields {
        if val.trim().is_empty() {
            return Err(format!("「{name}」不能为空"));
        }
    }
    Ok(())
}

/// 创建四类业务各自独立的图像缓存。
fn create_image_stores() -> [ImageStore; 4] {
    [
        Arc::default(),
        Arc::default(),
        Arc::default(),
        Arc::default(),
    ]
}

/// 初始化项目名称与项目编号下拉框数据。
fn init_project_models(window: &RLCLIAWindow, proj: &config::ProjectConfig) {
    let names: Vec<slint::SharedString> = proj
        .project_name_list
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    let ids: Vec<slint::SharedString> = proj
        .project_id_list
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    window.set_project_names(ModelRc::new(VecModel::from(names)));
    window.set_project_ids(ModelRc::new(VecModel::from(ids)));
}

/// 初始化各页默认的生产日期。
fn init_default_dates(window: &RLCLIAWindow) {
    let today = today_str();
    window.set_reagent_prod_date(today.clone().into());
    window.set_calib_prod_date(today.clone().into());
    window.set_consumable_prod_date(today.clone().into());
    window.set_quality_prod_date(today.into());
}

/// 绑定失效日期的即时计算回调。
fn bind_compute_expiry_callback(window: &RLCLIAWindow) {
    window.on_compute_expiry(|pd, vd| {
        compute_expire(&pd.to_string(), vd.to_string().parse().unwrap_or(365)).into()
    });
}

/// 绑定“生成预览”回调，并同步刷新预览区与图像缓存。
fn bind_generate_preview_callback(
    window: &RLCLIAWindow,
    proj: config::ProjectConfig,
    stores: [ImageStore; 4],
) {
    let w = window.as_weak();
    let [ir, ic, ico, iq] = stores;
    window.on_generate_preview(move |etype| {
        let w = w.unwrap();
        let typ = etype.to_string();
        match dispatch_generate(&typ, &w, &proj) {
            Ok((imgs, _, label)) => {
                if imgs.is_empty() {
                    w.set_status("没有图像".into());
                    return;
                }
                let preview = gray_to_slint_image(&imgs[0]);
                match typ.as_str() {
                    "reagent" => {
                        w.set_preview_reagent(preview);
                        w.set_has_preview_reagent(true);
                        *ir.lock().unwrap() = imgs;
                    }
                    "calibration" => {
                        w.set_preview_calibration(preview);
                        w.set_has_preview_calibration(true);
                        *ic.lock().unwrap() = imgs;
                    }
                    "consumable" => {
                        w.set_preview_consumable(preview);
                        w.set_has_preview_consumable(true);
                        *ico.lock().unwrap() = imgs;
                    }
                    "quality" => {
                        w.set_preview_quality(preview);
                        w.set_has_preview_quality(true);
                        *iq.lock().unwrap() = imgs;
                    }
                    _ => {}
                }
                w.set_status(format!("{label} 预览已生成").into());
                w.set_toast_msg("预览成功".into());
                w.set_toast_visible(true);
            }
            Err(e) => w.set_status(format!("错误: {e}").into()),
        }
    });
}

/// 绑定 PNG 导出回调。
fn bind_export_png_callback(window: &RLCLIAWindow, stores: [ImageStore; 4]) {
    let w = window.as_weak();
    let [ir, ic, ico, iq] = stores;
    window.on_export_png(move |etype| {
        let w = w.unwrap();
        let typ = etype.to_string();
        let imgs = current_images(&typ, &ir, &ic, &ico, &iq);
        if imgs.is_empty() {
            w.set_status("请先点击「生成预览」".into());
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("保存PNG图片")
            .set_file_name(&format!("{}.png", label_of(&typ)))
            .add_filter("PNG图片", &["png"])
            .save_file()
        {
            match imgs[0].save(&path) {
                Ok(_) => {
                    w.set_status(format!("已保存: {}", path.display()).into());
                    w.set_toast_msg("导出成功".into());
                    w.set_toast_visible(true);
                }
                Err(e) => w.set_status(format!("保存失败: {e}").into()),
            }
        }
    });
}

/// 绑定 PDF 导出回调。
fn bind_export_pdf_callback(window: &RLCLIAWindow, stores: [ImageStore; 4]) {
    let w = window.as_weak();
    let [ir, ic, ico, iq] = stores;
    window.on_export_pdf(move |etype| {
        let w = w.unwrap();
        let typ = etype.to_string();
        let imgs = current_images(&typ, &ir, &ic, &ico, &iq);
        if imgs.is_empty() {
            w.set_status("请先点击「生成预览」".into());
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("保存PDF")
            .set_file_name(&format!("{}.pdf", label_of(&typ)))
            .add_filter("PDF文件", &["pdf"])
            .save_file()
        {
            match generate_pdf(&imgs, path.to_str().unwrap_or("")) {
                Ok(_) => {
                    w.set_status(format!("已保存: {}", path.display()).into());
                    w.set_toast_msg("导出成功".into());
                    w.set_toast_visible(true);
                }
                Err(e) => w.set_status(format!("PDF失败: {e}").into()),
            }
        }
    });
}

/// 绑定解密回调。
fn bind_decrypt_callback(window: &RLCLIAWindow) {
    let w = window.as_weak();
    window.on_decrypt_data(move || {
        let w = w.unwrap();
        let input = w.get_decrypt_input().to_string();
        match encryptor::decrypt(&input) {
            Ok(plain) => w.set_decrypt_output(plain.into()),
            Err(e) => w.set_decrypt_output(format!("错误: {e}").into()),
        }
    });
}

/// 按业务类型读取对应的图像缓存。
fn current_images(
    typ: &str,
    reagent: &ImageStore,
    calibration: &ImageStore,
    consumable: &ImageStore,
    quality: &ImageStore,
) -> Vec<image::GrayImage> {
    match typ {
        "reagent" => reagent.lock().unwrap().clone(),
        "calibration" => calibration.lock().unwrap().clone(),
        "consumable" => consumable.lock().unwrap().clone(),
        "quality" => quality.lock().unwrap().clone(),
        _ => Vec::new(),
    }
}

/// 生成试剂标签图像集合。
fn gen_reagent(
    w: &RLCLIAWindow,
    proj: &config::ProjectConfig,
) -> Result<(Vec<image::GrayImage>, String, String), String> {
    let idx = w.get_reagent_project_index() as usize;
    let name = proj.project_name_list.get(idx).cloned().unwrap_or_default();
    let id = proj.project_id_list.get(idx).cloned().unwrap_or_default();
    let lot = w.get_reagent_lot().to_string();
    let prod = w.get_reagent_prod_date().to_string();
    let days: i64 = w.get_reagent_valid_days().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let counts = w.get_reagent_test_counts().to_string();
    let open = w.get_reagent_open_days().to_string();
    let n: usize = w.get_reagent_serial_count().parse().unwrap_or(1);
    let units = ["pg/mL", "ng/mL", "mg/L", "ng/L", "IU/L"];
    let unit = units
        .get(w.get_reagent_unit_index() as usize)
        .unwrap_or(&"pg/mL");
    let pa = w.get_reagent_param_a().to_string();
    let pb = w.get_reagent_param_b().to_string();
    let pc = w.get_reagent_param_c().to_string();
    let pd = w.get_reagent_param_d().to_string();
    let rl = w.get_reagent_range_low().to_string();
    let ru = w.get_reagent_range_upper().to_string();
    let ll = w.get_reagent_limit_low().to_string();
    let lu = w.get_reagent_limit_upper().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("试剂批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &w.get_reagent_valid_days().to_string()),
        ("测试/盒", &counts),
        ("开瓶天数", &open),
        ("数量", &w.get_reagent_serial_count().to_string()),
        ("曲线参数a", &pa),
        ("曲线参数b", &pb),
        ("曲线参数c", &pc),
        ("曲线参数d", &pd),
        ("范围下限", &rl),
        ("范围上限", &ru),
        ("限值下限", &ll),
        ("限值上限", &lu),
    ])?;

    let serials = generate_serials(n);
    let mut imgs = Vec::new();
    for s in &serials {
        let enc = encryptor::compose_reagent(
            &name, &id, &lot, &prod, &exp, &counts, &open, "direct", s, unit, &pa, &pb, &pc, &pd,
            &rl, &ru, &ll, &lu,
        )?;
        let bc = generate_barcode(&enc)?;
        imgs.push(draw_barcode_with_text(
            &bc,
            ImageType::ReagentInformation,
            &name,
            &lot,
            &prod,
            &exp,
            &counts,
        ));
    }
    Ok((imgs, name, "试剂".into()))
}

/// 生成校准品标签图像集合。
fn gen_calibration(
    w: &RLCLIAWindow,
    proj: &config::ProjectConfig,
) -> Result<(Vec<image::GrayImage>, String, String), String> {
    let idx = w.get_calib_project_index() as usize;
    let name = proj.project_name_list.get(idx).cloned().unwrap_or_default();
    let id = proj.project_id_list.get(idx).cloned().unwrap_or_default();
    let lot = w.get_calib_lot().to_string();
    let prod = w.get_calib_prod_date().to_string();
    let days: i64 = w.get_calib_valid_days().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let n: usize = w.get_calib_quantity().parse().unwrap_or(1);
    let c1 = w.get_calib_c1().to_string();
    let c2 = w.get_calib_c2().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("校准批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &w.get_calib_valid_days().to_string()),
        ("数量", &w.get_calib_quantity().to_string()),
        ("C1发光值", &c1),
        ("C2发光值", &c2),
    ])?;

    let mut imgs = Vec::new();
    for _ in 0..n {
        let enc =
            encryptor::compose_calibration(&name, &id, &lot, &prod, &exp, "direct", &c1, &c2)?;
        let bc = generate_barcode(&enc)?;
        imgs.push(draw_barcode_with_text(
            &bc,
            ImageType::CalibrationProduct,
            &name,
            &lot,
            &prod,
            &exp,
            "",
        ));
    }
    Ok((imgs, name, "校准品".into()))
}

/// 生成耗材标签图像集合。
fn gen_consumable(w: &RLCLIAWindow) -> Result<(Vec<image::GrayImage>, String, String), String> {
    let types = ["激发液A", "激发液B"];
    let ti = w.get_consumable_type_index() as usize;
    let tn = types.get(ti).unwrap_or(&"激发液A");
    let lot = w.get_consumable_lot().to_string();
    let prod = w.get_consumable_prod_date().to_string();
    let days: i64 = w.get_consumable_valid_days().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let freq = w.get_consumable_freq().to_string();
    let open = w.get_consumable_open_days().to_string();
    let n: usize = w.get_consumable_quantity().parse().unwrap_or(1);

    require_fields(&[
        ("耗材批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &w.get_consumable_valid_days().to_string()),
        ("可用频次", &freq),
        ("开瓶天数", &open),
        ("数量", &w.get_consumable_quantity().to_string()),
    ])?;

    let mut imgs = Vec::new();
    for _ in 0..n {
        let enc = encryptor::compose_consumable(tn, &lot, &prod, &exp, &freq, &open)?;
        let bc = generate_barcode(&enc)?;
        let it = if ti == 0 {
            ImageType::ExcitationFluidA
        } else {
            ImageType::ExcitationFluidB
        };
        imgs.push(draw_barcode_with_text(&bc, it, tn, &lot, &prod, &exp, ""));
    }
    Ok((imgs, tn.to_string(), "耗材".into()))
}

/// 生成质控品标签图像集合。
fn gen_quality(
    w: &RLCLIAWindow,
    proj: &config::ProjectConfig,
) -> Result<(Vec<image::GrayImage>, String, String), String> {
    let idx = w.get_quality_project_index() as usize;
    let name = proj.project_name_list.get(idx).cloned().unwrap_or_default();
    let id = proj.project_id_list.get(idx).cloned().unwrap_or_default();
    let lot = w.get_quality_lot().to_string();
    let prod = w.get_quality_prod_date().to_string();
    let days: i64 = w.get_quality_valid_days().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let n: usize = w.get_quality_quantity().parse().unwrap_or(1);
    let q1 = w.get_quality_q1().to_string();
    let sd1 = w.get_quality_sd1().to_string();
    let q2 = w.get_quality_q2().to_string();
    let sd2 = w.get_quality_sd2().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("质控批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &w.get_quality_valid_days().to_string()),
        ("数量", &w.get_quality_quantity().to_string()),
        ("Q1", &q1),
        ("SD1", &sd1),
        ("Q2", &q2),
        ("SD2", &sd2),
    ])?;

    let mut imgs = Vec::new();
    for _ in 0..n {
        let enc = encryptor::compose_quality(
            &name, &id, &lot, &prod, &exp, "direct", &q1, &sd1, &q2, &sd2,
        )?;
        let bc = generate_barcode(&enc)?;
        imgs.push(draw_barcode_with_text(
            &bc,
            ImageType::QualityControl,
            &name,
            &lot,
            &prod,
            &exp,
            "",
        ));
    }
    Ok((imgs, name, "质控品".into()))
}

/// 按页面类型分发到对应的图像生成流程。
fn dispatch_generate(
    typ: &str,
    w: &RLCLIAWindow,
    proj: &config::ProjectConfig,
) -> Result<(Vec<image::GrayImage>, String, String), String> {
    match typ {
        "reagent" => gen_reagent(w, proj),
        "calibration" => gen_calibration(w, proj),
        "consumable" => gen_consumable(w),
        "quality" => gen_quality(w, proj),
        _ => Err("未知类型".into()),
    }
}

/// 返回页面类型对应的中文名称。
fn label_of(typ: &str) -> &'static str {
    match typ {
        "reagent" => "试剂",
        "calibration" => "校准品",
        "consumable" => "耗材",
        "quality" => "质控品",
        _ => "条码",
    }
}
