use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};

/// CBC 模式下的 AES-256 加密器。
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
/// CBC 模式下的 AES-256 解密器。
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

const KEY: &[u8; 32] = b"12345678123456781234567812345678";
const IV: &[u8; 16] = &[
    0x1f, 0x32, 0x43, 0x51, 0x56, 0x98, 0xaf, 0xed, 0xab, 0xc8, 0x21, 0x45, 0x63, 0x72, 0xac, 0xfc,
];

/// 对业务明文执行 AES-256-CBC 加密，并返回 Base64 文本。
pub fn encrypt(plaintext: &str) -> Result<String, String> {
    let pt = plaintext.as_bytes();
    let mut buf = vec![0u8; pt.len() + 32]; // extra for padding
    buf[..pt.len()].copy_from_slice(pt);
    let encrypted = Aes256CbcEnc::new(KEY.into(), IV.into())
        .encrypt_padded_mut::<Pkcs7>(&mut buf, pt.len())
        .map_err(|e| format!("AES加密失败: {e}"))?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        encrypted,
    ))
}

/// 对 Base64 密文执行解密，并还原为 UTF-8 文本。
pub fn decrypt(b64_input: &str) -> Result<String, String> {
    let ciphertext =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64_input.trim())
            .map_err(|e| format!("Base64解码失败: {e}"))?;
    // Use exact-sized buffer so decrypt_padded_mut only processes ciphertext
    let mut buf = ciphertext;
    let decrypted = Aes256CbcDec::new(KEY.into(), IV.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| format!("AES解密失败: {e}"))?;
    String::from_utf8(decrypted.to_vec()).map_err(|e| format!("编码错误: {e}"))
}

/// 将名称中的特殊字符替换成设备端约定的编码形式。
fn replace_beta(name: &str) -> &str {
    match name {
        "S100β" => "S100B",
        "Aβ1-42" => "AB1-42",
        "β-HCG" => "B-HCG",
        _ => name,
    }
}

/// 组装试剂条码明文并加密。
pub fn compose_reagent(
    project_name: &str,
    project_id: &str,
    lot: &str,
    prod_date: &str,
    expire_date: &str,
    test_counts: &str,
    open_days: &str,
    reaction_mode: &str,
    serial_number: &str,
    unit: &str,
    top_param: &str,
    hs_param: &str,
    logec_param: &str,
    bottom_param: &str,
    range_low: &str,
    range_upper: &str,
    low_limit: &str,
    upper_limit: &str,
) -> Result<String, String> {
    let s = format!(
        "reagent;{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{};{}",
        replace_beta(project_name),
        project_id,
        lot,
        prod_date,
        expire_date,
        test_counts,
        open_days,
        reaction_mode,
        serial_number,
        unit,
        top_param,
        hs_param,
        logec_param,
        bottom_param,
        range_low,
        range_upper,
        low_limit,
        upper_limit,
    );
    encrypt(&s)
}

/// 组装校准品条码明文并加密。
pub fn compose_calibration(
    project_name: &str,
    project_id: &str,
    lot: &str,
    prod_date: &str,
    expire_date: &str,
    reaction_mode: &str,
    c1: &str,
    c2: &str,
) -> Result<String, String> {
    let s = format!(
        "calibration;{};{};{};{};{};{};{};{}",
        replace_beta(project_name),
        project_id,
        lot,
        prod_date,
        expire_date,
        reaction_mode,
        c1,
        c2,
    );
    encrypt(&s)
}

/// 组装耗材条码明文并加密。
pub fn compose_consumable(
    project_name: &str,
    lot: &str,
    prod_date: &str,
    expire_date: &str,
    test_counts: &str,
    open_days: &str,
) -> Result<String, String> {
    let s = format!(
        "consumable;{};{};{};{};{};{}",
        replace_beta(project_name),
        lot,
        prod_date,
        expire_date,
        test_counts,
        open_days,
    );
    encrypt(&s)
}

/// 组装质控品条码明文并加密。
pub fn compose_quality(
    project_name: &str,
    project_id: &str,
    lot: &str,
    prod_date: &str,
    expire_date: &str,
    reaction_mode: &str,
    q1: &str,
    sd1: &str,
    q2: &str,
    sd2: &str,
) -> Result<String, String> {
    let s = format!(
        "qc;{};{};{};{};{};{};{};{};{};{}",
        replace_beta(project_name),
        project_id,
        lot,
        prod_date,
        expire_date,
        reaction_mode,
        q1,
        sd1,
        q2,
        sd2,
    );
    encrypt(&s)
}
