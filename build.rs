fn main() {
    embed_resource::compile_for("./icon.rc", ["AbbottBarcodeGeneration"]);
    embed_resource::compile_for("./lotid-icon.rc", ["LotID-Codec"]);
    embed_resource::compile_for("./LiteCrypt-icon.rc", ["LiteCrypt"]);
    embed_resource::compile_for("./RL-CLIA-icon.rc", ["RL-CLIA"]);

    // ui/main.slint 作为统一入口，引入导出 了 barcode.slint 和 lotid.slint，
    // 一次编译即可将所有组件（BarcodeWindow、LotIdWindow）都写入生成代码，
    // 各 binary 的 include_modules!() 均能引用各自所需的组件。
    slint_build::compile_with_config(
        "ui/main.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-light".to_string()),
    )
    .unwrap();
}
