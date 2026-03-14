# My Rust Journey

Rust 学习仓库，当前分为三大类内容：

1. `examples/`：Rust 语法学习示例（变量、流程控制、函数、结构体等）
2. `src/AbbottBarcodeGeneration`：条码生成项目
3. `src/LotID-Codec`：Lot ID 编解码项目

另外，`docs/` 用于编写和整理项目文档。

## 目录

```txt
my-rust-journey/
├── examples/
├── docs/
├── src/
│   ├── AbbottBarcodeGeneration/
│   ├── LotID-Codec/
│   └── lib.rs
└── Cargo.toml
```

## 运行与构建

### examples（语法示例）

```bash
# 运行某个示例
cargo run --example hello

# 构建全部示例
cargo build --examples
```

### AbbottBarcodeGeneration（src 下项目）

```bash
cargo run --bin AbbottBarcodeGeneration
cargo build --bin AbbottBarcodeGeneration --release
```

### LotID-Codec（src 下项目）

```bash
cargo run --bin LotID-Codec
cargo build --bin LotID-Codec --release
```

## 常用命令

```bash
cargo test
cargo check
cargo fmt
cargo clippy
```
