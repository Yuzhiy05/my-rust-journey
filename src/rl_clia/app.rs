use crate::barcode::{LabelContent, generate_barcode, generate_pdf, gray_to_slint_image, render_label};
use crate::config;
use crate::encryptor;
use crate::layout::{
    LABEL_HEIGHT, LABEL_WIDTH, LayoutConfig, LayoutElement, LayoutElementKind, PageKind,
    load_layout_config, save_layout_config,
};
use chrono::{Duration, Local};
use image::GrayImage;
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

slint::include_modules!();

type PreviewModel = Rc<VecModel<PreviewElementData>>;

struct EditorState {
    layout: LayoutConfig,
    preview_model: PreviewModel,
    selected: Option<SelectedElement>,
    active_page: PageKind,
}

#[derive(Clone)]
struct SelectedElement {
    page: PageKind,
    id: String,
}

struct GeneratedPage {
    images: Vec<GrayImage>,
    content: LabelContent,
    preview_barcode: GrayImage,
    label: &'static str,
}

enum LayoutUpdate {
    Move { dx: f32, dy: f32 },
}

/// 应用运行入口。
pub fn run() {
    let proj = config::load_project_config();
    let window = RLCLIAWindow::new().expect("创建窗口失败");
    let layout = load_layout_config();
    let _ = save_layout_config(&layout);
    let preview_model = Rc::new(VecModel::from(Vec::<PreviewElementData>::new()));
    let state = Rc::new(RefCell::new(EditorState {
        layout,
        preview_model: preview_model.clone(),
        selected: None,
        active_page: PageKind::Reagent,
    }));

    window.set_preview_elements(preview_model.clone().into());
    init_project_models(&window, &proj);
    init_default_dates(&window);
    bind_compute_expiry_callback(&window);
    bind_generate_preview_callback(&window, proj.clone(), state.clone());
    bind_export_png_callback(&window, proj.clone(), state.clone());
    bind_export_pdf_callback(&window, proj.clone(), state.clone());
    bind_decrypt_callback(&window);
    bind_layout_editor_callbacks(&window, proj.clone(), state.clone());

    refresh_editor_for_page(&window, &proj, &state, PageKind::Reagent);
    window.run().expect("运行失败");
}

fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn compute_expire(date: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d + Duration::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn generate_serials(count: usize) -> Vec<String> {
    let d = Local::now().format("%Y%m%d").to_string();
    (1..=count).map(|i| format!("{d}{:04}", i)).collect()
}

fn require_fields(fields: &[(&str, &str)]) -> Result<(), String> {
    for (name, val) in fields {
        if val.trim().is_empty() {
            return Err(format!("「{name}」不能为空"));
        }
    }
    Ok(())
}

fn init_project_models(window: &RLCLIAWindow, proj: &config::ProjectConfig) {
    let names: Vec<SharedString> = proj
        .project_name_list
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    let ids: Vec<SharedString> = proj
        .project_id_list
        .iter()
        .map(|s| s.as_str().into())
        .collect();
    window.set_project_names(ModelRc::new(VecModel::from(names)));
    window.set_project_ids(ModelRc::new(VecModel::from(ids)));
}

fn init_default_dates(window: &RLCLIAWindow) {
    let today = today_str();
    window.set_reagent_prod_date(today.clone().into());
    window.set_calib_prod_date(today.clone().into());
    window.set_consumable_prod_date(today.clone().into());
    window.set_quality_prod_date(today.into());
}

fn bind_compute_expiry_callback(window: &RLCLIAWindow) {
    window.on_compute_expiry(|pd, vd| {
        compute_expire(&pd.to_string(), vd.to_string().parse().unwrap_or(365)).into()
    });
}

fn bind_generate_preview_callback(
    window: &RLCLIAWindow,
    proj: config::ProjectConfig,
    state: Rc<RefCell<EditorState>>,
) {
    let weak = window.as_weak();
    window.on_generate_preview(move |page| {
        let window = weak.unwrap();
        let page = PageKind::from_ui(&page.to_string());
        let generated = {
            let editor = state.borrow();
            dispatch_generate(page, &window, &proj, &editor.layout)
        };
        match generated {
            Ok(result) => {
                apply_preview_data(
                    &window,
                    &state,
                    page,
                    &result.content,
                    Some(&result.preview_barcode),
                );
                window.set_status(format!("{} 预览已生成", result.label).into());
                window.set_toast_msg("预览成功".into());
                window.set_toast_visible(true);
            }
            Err(err) => window.set_status(format!("错误: {err}").into()),
        }
    });
}

fn bind_export_png_callback(
    window: &RLCLIAWindow,
    proj: config::ProjectConfig,
    state: Rc<RefCell<EditorState>>,
) {
    let weak = window.as_weak();
    window.on_export_png(move |page| {
        let window = weak.unwrap();
        let page = PageKind::from_ui(&page.to_string());
        let generated = {
            let editor = state.borrow();
            dispatch_generate(page, &window, &proj, &editor.layout)
        };
        match generated {
            Ok(result) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("保存PNG图片")
                    .set_file_name(&format!("{}.png", result.label))
                    .add_filter("PNG图片", &["png"])
                    .save_file()
                {
                    match result.images[0].save(&path) {
                        Ok(_) => {
                            window.set_status(format!("已保存: {}", path.display()).into());
                            window.set_toast_msg("导出成功".into());
                            window.set_toast_visible(true);
                        }
                        Err(err) => window.set_status(format!("保存失败: {err}").into()),
                    }
                }
            }
            Err(err) => window.set_status(format!("错误: {err}").into()),
        }
    });
}

fn bind_export_pdf_callback(
    window: &RLCLIAWindow,
    proj: config::ProjectConfig,
    state: Rc<RefCell<EditorState>>,
) {
    let weak = window.as_weak();
    window.on_export_pdf(move |page| {
        let window = weak.unwrap();
        let page = PageKind::from_ui(&page.to_string());
        let generated = {
            let editor = state.borrow();
            dispatch_generate(page, &window, &proj, &editor.layout)
        };
        match generated {
            Ok(result) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("保存PDF")
                    .set_file_name(&format!("{}.pdf", result.label))
                    .add_filter("PDF文件", &["pdf"])
                    .save_file()
                {
                    match generate_pdf(&result.images, path.to_str().unwrap_or("")) {
                        Ok(_) => {
                            window.set_status(format!("已保存: {}", path.display()).into());
                            window.set_toast_msg("导出成功".into());
                            window.set_toast_visible(true);
                        }
                        Err(err) => window.set_status(format!("PDF失败: {err}").into()),
                    }
                }
            }
            Err(err) => window.set_status(format!("错误: {err}").into()),
        }
    });
}

fn bind_decrypt_callback(window: &RLCLIAWindow) {
    let weak = window.as_weak();
    window.on_decrypt_data(move || {
        let window = weak.unwrap();
        let input = window.get_decrypt_input().to_string();
        match encryptor::decrypt(&input) {
            Ok(plain) => window.set_decrypt_output(plain.into()),
            Err(err) => window.set_decrypt_output(format!("错误: {err}").into()),
        }
    });
}

fn bind_layout_editor_callbacks(
    window: &RLCLIAWindow,
    proj: config::ProjectConfig,
    state: Rc<RefCell<EditorState>>,
) {
    let weak = window.as_weak();
    let proj_load = proj.clone();
    let state_load = state.clone();
    window.on_load_layout_page(move |page| {
        let window = weak.unwrap();
        state_load.borrow_mut().layout = load_layout_config();
        refresh_editor_for_page(
            &window,
            &proj_load,
            &state_load,
            PageKind::from_ui(&page.to_string()),
        );
    });

    let weak = window.as_weak();
    let state_select = state.clone();
    window.on_select_layout_element(move |page, id| {
        let window = weak.unwrap();
        select_layout_element(
            &window,
            &state_select,
            PageKind::from_ui(&page.to_string()),
            &id.to_string(),
        );
    });

    let weak = window.as_weak();
    let state_drag = state.clone();
    window.on_drag_layout_element(move |page, id, dx, dy| {
        let window = weak.unwrap();
        adjust_layout_element(
            &window,
            &state_drag,
            PageKind::from_ui(&page.to_string()),
            &id.to_string(),
            LayoutUpdate::Move { dx, dy },
        );
    });

    let weak = window.as_weak();
    let state_field = state.clone();
    window.on_update_selected_layout(move |page, field, value| {
        let window = weak.unwrap();
        adjust_selected_field(
            &window,
            &state_field,
            PageKind::from_ui(&page.to_string()),
            &field.to_string(),
            &value.to_string(),
        );
    });

    let weak = window.as_weak();
    let state_bold = state.clone();
    window.on_toggle_selected_bold(move |page, value| {
        let window = weak.unwrap();
        adjust_selected_bold(
            &window,
            &state_bold,
            PageKind::from_ui(&page.to_string()),
            value,
        );
    });

    let weak = window.as_weak();
    window.on_reset_layout_page(move |page| {
        let window = weak.unwrap();
        let page = PageKind::from_ui(&page.to_string());
        {
            let mut editor = state.borrow_mut();
            editor.layout.reset_page(page);
            if let Err(err) = save_layout_config(&editor.layout) {
                window.set_status(format!("布局保存失败: {err}").into());
                return;
            }
        }
        refresh_editor_for_page(&window, &proj, &state, page);
        window.set_status(format!("{} 页面布局已重置", page.label()).into());
    });
}

fn refresh_editor_for_page(
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
) {
    let preview = dispatch_generate(page, window, proj, &state.borrow().layout)
        .map(|generated| (generated.content, Some(generated.preview_barcode)))
        .unwrap_or_else(|_| (fallback_preview_content(page, window, proj), None));
    apply_preview_data(window, state, page, &preview.0, preview.1.as_ref());
}

fn apply_preview_data(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    content: &LabelContent,
    barcode: Option<&GrayImage>,
) {
    let mut editor = state.borrow_mut();
    editor.active_page = page;
    let elements = build_preview_elements(editor.layout.page(page), content);
    editor.preview_model.set_vec(elements);
    if let Some(image) = barcode {
        window.set_preview_barcode(gray_to_slint_image(image));
        window.set_preview_barcode_visible(true);
    } else {
        window.set_preview_barcode(slint::Image::default());
        window.set_preview_barcode_visible(false);
    }
    window.set_layout_editor_ready(true);

    let keep_selected = editor.selected.as_ref().is_some_and(|selected| {
        selected.page == page && editor.layout.page(page).element(&selected.id).is_some()
    });
    if !keep_selected {
        editor.selected = None;
    }
    drop(editor);
    sync_selected_fields(window, state);
}

fn select_layout_element(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    id: &str,
) {
    let mut editor = state.borrow_mut();
    if editor.layout.page(page).element(id).is_some() {
        editor.selected = Some(SelectedElement {
            page,
            id: id.to_string(),
        });
    }
    drop(editor);
    sync_selected_fields(window, state);
}

fn adjust_selected_field(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    field: &str,
    value: &str,
) {
    let Ok(value) = value.trim().parse::<f32>() else {
        return;
    };
    adjust_current_element(window, state, page, |element| match field {
        "x" => element.x = value,
        "y" => element.y = value,
        "width" => element.width = value,
        "height" => element.height = value,
        "font_size" => element.font_size = value,
        _ => {}
    });
}

fn adjust_selected_bold(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    value: bool,
) {
    adjust_current_element(window, state, page, |element| {
        if element.kind == LayoutElementKind::Text {
            element.bold = value;
        }
    });
}

fn adjust_layout_element(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    id: &str,
    update: LayoutUpdate,
) {
    let mut editor = state.borrow_mut();
    let (x, y, width, height, font_size, bold) = {
        let Some(element) = editor.layout.page_mut(page).element_mut(id) else {
            return;
        };
        match update {
            LayoutUpdate::Move { dx, dy } => {
                element.x += dx;
                element.y += dy;
            }
        }
        normalize_element(element);
        (
            element.x,
            element.y,
            element.width,
            element.height,
            element.font_size,
            element.bold,
        )
    };
    if let Err(err) = save_layout_config(&editor.layout) {
        window.set_status(format!("布局保存失败: {err}").into());
        return;
    }
    if let Some(row) = preview_row_index(&editor.preview_model, id) {
        if let Some(mut item) = editor.preview_model.row_data(row) {
            item.x = x;
            item.y = y;
            item.width = width;
            item.height = height;
            item.font_size = font_size;
            item.bold = bold;
            editor.preview_model.set_row_data(row, item);
        }
    }
    editor.selected = Some(SelectedElement {
        page,
        id: id.to_string(),
    });
    drop(editor);
    sync_selected_fields(window, state);
    window.set_status(format!("{} 布局已保存到配置文件", page.label()).into());
}

fn adjust_current_element<F>(
    window: &RLCLIAWindow,
    state: &Rc<RefCell<EditorState>>,
    page: PageKind,
    mut update: F,
) where
    F: FnMut(&mut LayoutElement),
{
    let mut editor = state.borrow_mut();
    let Some(selected) = editor.selected.clone() else {
        return;
    };
    if selected.page != page {
        return;
    }
    let (x, y, width, height, font_size, bold) = {
        let Some(element) = editor.layout.page_mut(page).element_mut(&selected.id) else {
            return;
        };
        update(element);
        normalize_element(element);
        (
            element.x,
            element.y,
            element.width,
            element.height,
            element.font_size,
            element.bold,
        )
    };
    if let Err(err) = save_layout_config(&editor.layout) {
        window.set_status(format!("布局保存失败: {err}").into());
        return;
    }
    if let Some(row) = preview_row_index(&editor.preview_model, &selected.id) {
        if let Some(mut item) = editor.preview_model.row_data(row) {
            item.x = x;
            item.y = y;
            item.width = width;
            item.height = height;
            item.font_size = font_size;
            item.bold = bold;
            editor.preview_model.set_row_data(row, item);
        }
    }
    drop(editor);
    sync_selected_fields(window, state);
    window.set_status(format!("{} 布局已保存到配置文件", page.label()).into());
}

fn preview_row_index(model: &PreviewModel, id: &str) -> Option<usize> {
    (0..model.row_count()).find(|index| {
        model
            .row_data(*index)
            .is_some_and(|row| row.id.as_str() == id)
    })
}

fn sync_selected_fields(window: &RLCLIAWindow, state: &Rc<RefCell<EditorState>>) {
    let editor = state.borrow();
    let Some(selected) = editor.selected.as_ref() else {
        clear_selected_fields(window);
        return;
    };
    let Some(element) = editor.layout.page(selected.page).element(&selected.id) else {
        clear_selected_fields(window);
        return;
    };
    window.set_selected_has_element(true);
    window.set_selected_element_id(selected.id.clone().into());
    window.set_selected_element_name(element_name(&selected.id).into());
    window.set_selected_element_kind(match element.kind {
        LayoutElementKind::Text => "text".into(),
        LayoutElementKind::Barcode => "barcode".into(),
    });
    window.set_selected_layout_x(format_number(element.x).into());
    window.set_selected_layout_y(format_number(element.y).into());
    window.set_selected_layout_width(format_number(element.width).into());
    window.set_selected_layout_height(format_number(element.height).into());
    window.set_selected_font_size(format_number(element.font_size).into());
    window.set_selected_bold(element.bold);
}

fn clear_selected_fields(window: &RLCLIAWindow) {
    window.set_selected_has_element(false);
    window.set_selected_element_id("".into());
    window.set_selected_element_name("".into());
    window.set_selected_element_kind("".into());
    window.set_selected_layout_x("".into());
    window.set_selected_layout_y("".into());
    window.set_selected_layout_width("".into());
    window.set_selected_layout_height("".into());
    window.set_selected_font_size("".into());
    window.set_selected_bold(false);
}

fn build_preview_elements(
    layout: &crate::layout::PageLayout,
    content: &LabelContent,
) -> Vec<PreviewElementData> {
    layout
        .elements
        .iter()
        .map(|element| PreviewElementData {
            id: element.id.clone().into(),
            name: element_name(&element.id).into(),
            kind: match element.kind {
                LayoutElementKind::Text => "text".into(),
                LayoutElementKind::Barcode => "barcode".into(),
            },
            text: preview_text(&element.id, content).into(),
            x: element.x,
            y: element.y,
            width: element.width,
            height: element.height,
            font_size: element.font_size,
            bold: element.bold,
            visible: true,
        })
        .collect()
}

fn preview_text(id: &str, content: &LabelContent) -> String {
    let actual = match id {
        "title" => content.title.clone(),
        "subtitle1" => content.subtitle1.clone().unwrap_or_default(),
        "subtitle2" => content.subtitle2.clone().unwrap_or_default(),
        "lot" => format!("产品批号: {}", content.lot_number),
        "prod_date" => format!("生产日期: {}", content.prod_date),
        "expire_date" => format!("失效日期: {}", content.expire_date),
        "barcode" => String::new(),
        _ => String::new(),
    };
    if actual.trim().is_empty() {
        format!("[{}]", element_name(id))
    } else {
        actual
    }
}

fn element_name(id: &str) -> &'static str {
    match id {
        "title" => "一级标题",
        "subtitle1" => "二级标题",
        "subtitle2" => "三级标题",
        "barcode" => "条码区域",
        "lot" => "产品批号",
        "prod_date" => "生产日期",
        "expire_date" => "失效日期",
        _ => "元素",
    }
}

fn normalize_element(element: &mut LayoutElement) {
    element.width = element.width.clamp(1.0, LABEL_WIDTH);
    element.height = element.height.clamp(1.0, LABEL_HEIGHT);
    if element.kind == LayoutElementKind::Text {
        element.font_size = element.font_size.clamp(1.0, 200.0);
    } else {
        element.font_size = 0.0;
        element.bold = false;
    }
    element.x = element.x.clamp(0.0, (LABEL_WIDTH - element.width).max(0.0));
    element.y = element.y.clamp(0.0, (LABEL_HEIGHT - element.height).max(0.0));
}

fn format_number(value: f32) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{}", value.round() as i32)
    } else {
        format!("{value:.1}")
    }
}

fn project_name_at(proj: &config::ProjectConfig, index: usize) -> String {
    proj.project_name_list.get(index).cloned().unwrap_or_default()
}

fn project_id_at(proj: &config::ProjectConfig, index: usize) -> String {
    proj.project_id_list.get(index).cloned().unwrap_or_default()
}

fn fallback_preview_content(
    page: PageKind,
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
) -> LabelContent {
    match page {
        PageKind::Reagent => {
            let index = window.get_reagent_project_index() as usize;
            let name = project_name_at(proj, index);
            LabelContent {
                title: "试剂二维码信息".into(),
                subtitle1: Some(format!(
                    "{} 测定试剂盒",
                    if name.is_empty() { "项目名称" } else { &name }
                )),
                subtitle2: Some(format!(
                    "(化学发光免疫分析法)  {} 测试/盒",
                    blank_or(window.get_reagent_test_counts().as_str(), "测试数")
                )),
                lot_number: window.get_reagent_lot().to_string(),
                prod_date: window.get_reagent_prod_date().to_string(),
                expire_date: compute_expire(
                    &window.get_reagent_prod_date().to_string(),
                    window.get_reagent_valid_days().to_string().parse().unwrap_or(365),
                ),
            }
        }
        PageKind::Calibration => {
            let index = window.get_calib_project_index() as usize;
            LabelContent {
                title: "校准品二维码".into(),
                subtitle1: Some(blank_or(&project_name_at(proj, index), "项目名称")),
                subtitle2: None,
                lot_number: window.get_calib_lot().to_string(),
                prod_date: window.get_calib_prod_date().to_string(),
                expire_date: compute_expire(
                    &window.get_calib_prod_date().to_string(),
                    window.get_calib_valid_days().to_string().parse().unwrap_or(365),
                ),
            }
        }
        PageKind::Consumable => {
            let title = if window.get_consumable_type_index() == 0 {
                "激发液A二维码"
            } else {
                "激发液B二维码"
            };
            LabelContent {
                title: title.into(),
                subtitle1: None,
                subtitle2: None,
                lot_number: window.get_consumable_lot().to_string(),
                prod_date: window.get_consumable_prod_date().to_string(),
                expire_date: compute_expire(
                    &window.get_consumable_prod_date().to_string(),
                    window.get_consumable_valid_days().to_string().parse().unwrap_or(365),
                ),
            }
        }
        PageKind::Quality => {
            let index = window.get_quality_project_index() as usize;
            LabelContent {
                title: "质控品二维码".into(),
                subtitle1: Some(blank_or(&project_name_at(proj, index), "项目名称")),
                subtitle2: None,
                lot_number: window.get_quality_lot().to_string(),
                prod_date: window.get_quality_prod_date().to_string(),
                expire_date: compute_expire(
                    &window.get_quality_prod_date().to_string(),
                    window.get_quality_valid_days().to_string().parse().unwrap_or(365),
                ),
            }
        }
    }
}

fn blank_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn dispatch_generate(
    page: PageKind,
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
    layout: &LayoutConfig,
) -> Result<GeneratedPage, String> {
    match page {
        PageKind::Reagent => gen_reagent(window, proj, layout),
        PageKind::Calibration => gen_calibration(window, proj, layout),
        PageKind::Consumable => gen_consumable(window, layout),
        PageKind::Quality => gen_quality(window, proj, layout),
    }
}

fn gen_reagent(
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
    layout: &LayoutConfig,
) -> Result<GeneratedPage, String> {
    let idx = window.get_reagent_project_index() as usize;
    let name = project_name_at(proj, idx);
    let id = project_id_at(proj, idx);
    let lot = window.get_reagent_lot().to_string();
    let prod = window.get_reagent_prod_date().to_string();
    let days: i64 = window.get_reagent_valid_days().to_string().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let counts = window.get_reagent_test_counts().to_string();
    let open = window.get_reagent_open_days().to_string();
    let n: usize = window.get_reagent_serial_count().to_string().parse().unwrap_or(1);
    let units = ["pg/mL", "ng/mL", "mg/L", "ng/L", "IU/L"];
    let unit = units
        .get(window.get_reagent_unit_index() as usize)
        .unwrap_or(&"pg/mL");
    let pa = window.get_reagent_param_a().to_string();
    let pb = window.get_reagent_param_b().to_string();
    let pc = window.get_reagent_param_c().to_string();
    let pd = window.get_reagent_param_d().to_string();
    let rl = window.get_reagent_range_low().to_string();
    let ru = window.get_reagent_range_upper().to_string();
    let ll = window.get_reagent_limit_low().to_string();
    let lu = window.get_reagent_limit_upper().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("试剂批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &window.get_reagent_valid_days().to_string()),
        ("测试/盒", &counts),
        ("开瓶天数", &open),
        ("数量", &window.get_reagent_serial_count().to_string()),
        ("曲线参数a", &pa),
        ("曲线参数b", &pb),
        ("曲线参数c", &pc),
        ("曲线参数d", &pd),
        ("范围下限", &rl),
        ("范围上限", &ru),
        ("限值下限", &ll),
        ("限值上限", &lu),
    ])?;

    let content = LabelContent {
        title: "试剂二维码信息".into(),
        subtitle1: Some(format!("{name} 测定试剂盒")),
        subtitle2: Some(format!("(化学发光免疫分析法)  {counts} 测试/盒")),
        lot_number: lot.clone(),
        prod_date: prod.clone(),
        expire_date: exp.clone(),
    };

    let mut preview_barcode = None;
    let mut images = Vec::new();
    for serial in generate_serials(n) {
        let enc = encryptor::compose_reagent(
            &name, &id, &lot, &prod, &exp, &counts, &open, "direct", &serial, unit, &pa, &pb,
            &pc, &pd, &rl, &ru, &ll, &lu,
        )?;
        let barcode = generate_barcode(&enc)?;
        if preview_barcode.is_none() {
            preview_barcode = Some(barcode.clone());
        }
        images.push(render_label(&barcode, layout.page(PageKind::Reagent), &content));
    }

    Ok(GeneratedPage {
        images,
        content,
        preview_barcode: preview_barcode.unwrap_or_else(|| GrayImage::new(1, 1)),
        label: "试剂",
    })
}

fn gen_calibration(
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
    layout: &LayoutConfig,
) -> Result<GeneratedPage, String> {
    let idx = window.get_calib_project_index() as usize;
    let name = project_name_at(proj, idx);
    let id = project_id_at(proj, idx);
    let lot = window.get_calib_lot().to_string();
    let prod = window.get_calib_prod_date().to_string();
    let days: i64 = window.get_calib_valid_days().to_string().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let n: usize = window.get_calib_quantity().to_string().parse().unwrap_or(1);
    let c1 = window.get_calib_c1().to_string();
    let c2 = window.get_calib_c2().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("校准批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &window.get_calib_valid_days().to_string()),
        ("数量", &window.get_calib_quantity().to_string()),
        ("C1发光值", &c1),
        ("C2发光值", &c2),
    ])?;

    let content = LabelContent {
        title: "校准品二维码".into(),
        subtitle1: Some(name.clone()),
        subtitle2: None,
        lot_number: lot.clone(),
        prod_date: prod.clone(),
        expire_date: exp.clone(),
    };

    let mut preview_barcode = None;
    let mut images = Vec::new();
    for _ in 0..n {
        let enc =
            encryptor::compose_calibration(&name, &id, &lot, &prod, &exp, "direct", &c1, &c2)?;
        let barcode = generate_barcode(&enc)?;
        if preview_barcode.is_none() {
            preview_barcode = Some(barcode.clone());
        }
        images.push(render_label(
            &barcode,
            layout.page(PageKind::Calibration),
            &content,
        ));
    }

    Ok(GeneratedPage {
        images,
        content,
        preview_barcode: preview_barcode.unwrap_or_else(|| GrayImage::new(1, 1)),
        label: "校准品",
    })
}

fn gen_consumable(
    window: &RLCLIAWindow,
    layout: &LayoutConfig,
) -> Result<GeneratedPage, String> {
    let types = ["激发液A", "激发液B"];
    let type_index = window.get_consumable_type_index() as usize;
    let type_name = types.get(type_index).unwrap_or(&"激发液A");
    let lot = window.get_consumable_lot().to_string();
    let prod = window.get_consumable_prod_date().to_string();
    let days: i64 = window
        .get_consumable_valid_days()
        .to_string()
        .parse()
        .unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let freq = window.get_consumable_freq().to_string();
    let open = window.get_consumable_open_days().to_string();
    let n: usize = window.get_consumable_quantity().to_string().parse().unwrap_or(1);

    require_fields(&[
        ("耗材批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &window.get_consumable_valid_days().to_string()),
        ("可用频次", &freq),
        ("开瓶天数", &open),
        ("数量", &window.get_consumable_quantity().to_string()),
    ])?;

    let content = LabelContent {
        title: format!("{type_name}二维码"),
        subtitle1: None,
        subtitle2: None,
        lot_number: lot.clone(),
        prod_date: prod.clone(),
        expire_date: exp.clone(),
    };

    let mut preview_barcode = None;
    let mut images = Vec::new();
    for _ in 0..n {
        let enc = encryptor::compose_consumable(type_name, &lot, &prod, &exp, &freq, &open)?;
        let barcode = generate_barcode(&enc)?;
        if preview_barcode.is_none() {
            preview_barcode = Some(barcode.clone());
        }
        images.push(render_label(
            &barcode,
            layout.page(PageKind::Consumable),
            &content,
        ));
    }

    Ok(GeneratedPage {
        images,
        content,
        preview_barcode: preview_barcode.unwrap_or_else(|| GrayImage::new(1, 1)),
        label: "耗材",
    })
}

fn gen_quality(
    window: &RLCLIAWindow,
    proj: &config::ProjectConfig,
    layout: &LayoutConfig,
) -> Result<GeneratedPage, String> {
    let idx = window.get_quality_project_index() as usize;
    let name = project_name_at(proj, idx);
    let id = project_id_at(proj, idx);
    let lot = window.get_quality_lot().to_string();
    let prod = window.get_quality_prod_date().to_string();
    let days: i64 = window.get_quality_valid_days().to_string().parse().unwrap_or(365);
    let exp = compute_expire(&prod, days);
    let n: usize = window.get_quality_quantity().to_string().parse().unwrap_or(1);
    let q1 = window.get_quality_q1().to_string();
    let sd1 = window.get_quality_sd1().to_string();
    let q2 = window.get_quality_q2().to_string();
    let sd2 = window.get_quality_sd2().to_string();

    require_fields(&[
        ("项目名称", &name),
        ("质控批号", &lot),
        ("生产日期", &prod),
        ("有效天数", &window.get_quality_valid_days().to_string()),
        ("数量", &window.get_quality_quantity().to_string()),
        ("Q1", &q1),
        ("SD1", &sd1),
        ("Q2", &q2),
        ("SD2", &sd2),
    ])?;

    let content = LabelContent {
        title: "质控品二维码".into(),
        subtitle1: Some(name.clone()),
        subtitle2: None,
        lot_number: lot.clone(),
        prod_date: prod.clone(),
        expire_date: exp.clone(),
    };

    let mut preview_barcode = None;
    let mut images = Vec::new();
    for _ in 0..n {
        let enc = encryptor::compose_quality(
            &name, &id, &lot, &prod, &exp, "direct", &q1, &sd1, &q2, &sd2,
        )?;
        let barcode = generate_barcode(&enc)?;
        if preview_barcode.is_none() {
            preview_barcode = Some(barcode.clone());
        }
        images.push(render_label(&barcode, layout.page(PageKind::Quality), &content));
    }

    Ok(GeneratedPage {
        images,
        content,
        preview_barcode: preview_barcode.unwrap_or_else(|| GrayImage::new(1, 1)),
        label: "质控品",
    })
}
