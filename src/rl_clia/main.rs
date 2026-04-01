#![windows_subsystem = "windows"]

/// 化学发光条码工具的桌面应用入口。
///
/// 该二进制只负责组装模块并启动 UI，具体业务逻辑集中在 `app` 模块。
mod app;
mod barcode;
mod config;
mod encryptor;
mod layout;

fn main() {
    app::run();
}
