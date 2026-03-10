# LotID-Codec

一个很小的 LotID 编解码工具：

- 输入 `ID(0-255)` 和 `Lot(0-255)`，生成 **3 字符代码**（并可生成 DataMatrix 预览、复制/保存图片）
- 输入 3 字符代码，反推出 `ID` 与 `Lot`

> 说明：这里的“加密/解密”更准确说是**可逆混淆编码**（XOR + base43），并非面向安全场景的密码学加密。

## 技术栈

- 语言：Rust（`edition = "2024"`）
- GUI：Slint（桌面 UI）
- 条码生成：`zxing-cpp`（生成 DataMatrix），`image`（图像处理）
- 交互能力：`arboard`（剪贴板），`rfd`（文件保存对话框）
- 错误处理：`anyhow`

对应实现：`src/LotID-Codec/main.rs` 里的 `encode()` / `decode()`。

## 编解码算法（来自 C 版本）

核心思路：把 `(lot << 8) | id` 组合成 16-bit 值后，与常量 `0xE19A` 做 XOR，然后按 **43 进制**拆成 3 位并映射到字符表。

- **编码（encode）**
  1. `val = ((lot as u32) << 8) | (id as u32)`
  2. `val ^= 0xE19A`
  3. 循环 3 次：`remainder = val % 43`，`val /= 43`，输出字符 `TABLE[remainder]`
- **解码（decode）**
  1. 把 3 个字符（忽略大小写）在 `TABLE` 中找到索引 `i0,i1,i2`（找不到则失败）
  2. `val = i0 + i1*43 + i2*43*43`
  3. `val ^= 0xE19A`
  4. `id = val & 0xFF`，`lot = (val >> 8) & 0xFF`

> 数值范围：`43^3 = 79507`，覆盖 `0..=65535` 的全部组合，因此 3 字符足够表达 `id/lot` 的 16-bit 组合值。

## C 参考实现

```c
#include <stdint.h>
#include <stdio.h>
#include <ctype.h>

static const char TABLE[43] = {
    '0','1','2','3','4','5','6','7','8','9',
    'A','B','C','D','E','F','G','H','I',
    'J','K','L','M','N','O','P','Q','R',
    'S','T','U','V','W','X','Y','Z',
    '-','+','/','$','.','%',' '
};

void encode(uint8_t id, uint8_t lot, char out[3])
{
    uint32_t val = ((uint32_t)lot << 8) | id;
    val ^= 0xE19A;

    for (int i = 0; i < 3; i++) {
        uint32_t remainder = val % 43;
        val /= 43;
        out[i] = TABLE[remainder];
    }
}

int decode(const char chars[3], uint8_t *id, uint8_t *lot)
{
    uint32_t indices[3];

    for (int i = 0; i < 3; i++) {
        char ch = toupper((unsigned char)chars[i]);
        int found = -1;

        for (int j = 0; j < 43; j++) {
            if (TABLE[j] == ch) {
                found = j;
                break;
            }
        }

        if (found < 0)
            return 0; // decode失败

        indices[i] = (uint32_t)found;
    }

    uint32_t val = indices[0] +
                   indices[1] * 43 +
                   indices[2] * 43 * 43;

    val ^= 0xE19A;

    *id = val & 0xFF;
    *lot = (val >> 8) & 0xFF;

    return 1;
}

int main()
{
    uint8_t id = 25;
    uint8_t lot = 103;

    char code[3];

    encode(id, lot, code);

    printf("encode: %c%c%c\n", code[0], code[1], code[2]);

    uint8_t did, dlot;

    if (decode(code, &did, &dlot)) {
        printf("decode: id=%d lot=%d\n", did, dlot);
    }

    return 0;
}
```

## 运行

```bash
cargo run --bin LotID-Codec
```

