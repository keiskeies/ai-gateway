//! 内容（content 字段）形态转换工具。
//!
//! 统一层 `UnifiedMessage.content` 使用 OpenAI 风格的内容形态：
//! - 字符串：`"hello"`
//! - 多模态数组：`[{"type":"text","text":"..."},{"type":"image_url","image_url":{"url":"data:image/png;base64,..."}}]`
//!
//! 上游协议差异：
//! - OpenAI 兼容上游：`content` 可以是字符串或数组；部分实现（如 agnes）要求 user 消息必须带
//!   `content` 字段（`null` 也算缺失），因此转发前需要兜底补空串。
//! - Anthropic 上游：`content` 必须是字符串或内容块数组，图片使用 `{"type":"image","source":...}`。

use serde_json::{json, Value};

fn block_text(block: &Value) -> Option<&str> {
    block.get("text").and_then(|t| t.as_str())
}

/// 提取纯文本内容：字符串直接返回；数组则拼接所有 `text` 块（忽略图片等非文本块）。
pub fn extract_text(content: &Value) -> String {
    match content {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let mut out = String::new();
            for block in blocks {
                if let Some(text) = block_text(block) {
                    out.push_str(text);
                }
            }
            out
        }
        other => other.to_string(),
    }
}

/// Anthropic 内容块数组 → OpenAI 风格 content。
///
/// 单个 `text` 块会折叠成字符串，便于兼容只接受字符串的上游；
/// 含图片等非文本块时输出 OpenAI 多模态数组。
pub fn anthropic_blocks_to_openai(blocks: &[Value]) -> Value {
    let mut out: Vec<Value> = Vec::new();
    for block in blocks {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                let text = block_text(block).unwrap_or("");
                out.push(json!({"type": "text", "text": text}));
            }
            Some("image") => {
                if let Some(url) = anthropic_image_to_url(block) {
                    out.push(json!({"type": "image_url", "image_url": {"url": url}}));
                }
            }
            _ => {}
        }
    }

    if out.len() == 1 && out[0].get("type").and_then(|t| t.as_str()) == Some("text") {
        return Value::String(block_text(&out[0]).unwrap_or("").to_string());
    }
    if out.is_empty() {
        return Value::String(String::new());
    }
    Value::Array(out)
}

/// OpenAI 风格 content → Anthropic 内容块数组。
pub fn openai_content_to_anthropic_blocks(content: &Value) -> Vec<Value> {
    match content {
        Value::Null => Vec::new(),
        Value::String(s) => vec![json!({"type": "text", "text": s})],
        Value::Array(items) => items
            .iter()
            .filter_map(|item| match item.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    let text = block_text(item).unwrap_or("");
                    Some(json!({"type": "text", "text": text}))
                }
                Some("image_url") => {
                    let url = item
                        .get("image_url")
                        .and_then(|v| v.get("url").and_then(|u| u.as_str()).or_else(|| v.as_str()))
                        .or_else(|| item.get("url").and_then(|v| v.as_str()))?;
                    Some(url_to_anthropic_image(url))
                }
                Some("image") => Some(item.clone()),
                _ => None,
            })
            .collect(),
        other => vec![json!({"type": "text", "text": other.to_string()})],
    }
}

/// Anthropic `image` 块 → data URL（或原始 URL）。
fn anthropic_image_to_url(block: &Value) -> Option<String> {
    let source = block.get("source")?;
    match source.get("type").and_then(|t| t.as_str()) {
        Some("base64") => {
            let media_type = source
                .get("media_type")
                .and_then(|v| v.as_str())
                .unwrap_or("image/png");
            let data = source.get("data").and_then(|v| v.as_str())?;
            Some(format!("data:{};base64,{}", media_type, data))
        }
        Some("url") => source.get("url").and_then(|v| v.as_str()).map(String::from),
        _ => None,
    }
}

/// data URL（或普通 URL）→ Anthropic `image` 块。
fn url_to_anthropic_image(url: &str) -> Value {
    if let Some(rest) = url.strip_prefix("data:") {
        if let Some((meta, data)) = rest.split_once(',') {
            let media_type = meta.split(';').next().unwrap_or("image/png");
            if meta.contains("base64") {
                return json!({
                    "type": "image",
                    "source": {"type": "base64", "media_type": media_type, "data": data}
                });
            }
        }
    }
    json!({"type": "image", "source": {"type": "url", "url": url}})
}

// ========== 单通道（灰度）图片规范化 ==========
//
// 部分上游（agnes 的 vLLM 部署）无法处理单通道图片：灰度图会让服务端报
// `mean must have 1 elements if it is an iterable, got 3`（它把 3 通道的归一化参数用在了
// 单通道图上），同一张图转成 RGB 后即可正常处理。这里在转发前把单通道图统一转成 RGB。

fn is_single_channel(color: image::ColorType) -> bool {
    use image::ColorType::*;
    matches!(color, L8 | La8 | L16 | La16)
}

/// 把单通道图片编码成 RGB；不需要转换时返回 None。
fn convert_gray_to_rgb(bytes: &[u8]) -> Option<(Vec<u8>, String)> {
    let img = image::load_from_memory(bytes).ok()?;
    if !is_single_channel(img.color()) {
        return None;
    }
    let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
    let mut out = Vec::new();
    let media_type = match image::guess_format(bytes) {
        Ok(image::ImageFormat::Jpeg) => {
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92);
            rgb.write_with_encoder(encoder).ok()?;
            "image/jpeg"
        }
        Ok(image::ImageFormat::WebP) => {
            rgb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::WebP).ok()?;
            "image/webp"
        }
        _ => {
            rgb.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
            "image/png"
        }
    };
    Some((out, media_type.to_string()))
}

/// data URL → 单通道时转成 RGB 的 data URL；无需转换时返回 None。
pub fn normalize_image_data_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.contains("base64") || !meta.starts_with("image/") {
        return None;
    }
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine.decode(data).ok()?;
    let (rgb_bytes, media_type) = convert_gray_to_rgb(&bytes)?;
    let encoded = engine.encode(&rgb_bytes);
    Some(format!("data:{};base64,{}", media_type, encoded))
}

/// 就地规范化请求体中的图片，返回转换张数。
///
/// 同时支持两种块形态：
/// - OpenAI：`{"type":"image_url","image_url":{"url":"data:..."}}`
/// - Anthropic：`{"type":"image","source":{"type":"base64","media_type":...,"data":...}}`
pub fn normalize_images_in_body(body: &mut Value) -> usize {
    let mut converted = 0;
    let Some(messages) = body.get_mut("messages").and_then(|m| m.as_array_mut()) else {
        return 0;
    };
    for message in messages {
        let Some(blocks) = message.get_mut("content").and_then(|c| c.as_array_mut()) else {
            continue;
        };
        for block in blocks {
            match block.get("type").and_then(|t| t.as_str()) {
                Some("image_url") => {
                    let Some(url) = block.get("image_url").and_then(|u| u.get("url")).and_then(|v| v.as_str()) else {
                        continue;
                    };
                    if let Some(new_url) = normalize_image_data_url(url) {
                        block["image_url"]["url"] = Value::String(new_url);
                        converted += 1;
                    }
                }
                Some("image") => {
                    let is_base64 = block
                        .get("source")
                        .and_then(|s| s.get("type"))
                        .and_then(|t| t.as_str())
                        == Some("base64");
                    if !is_base64 {
                        continue;
                    }
                    let media_type = block
                        .get("source")
                        .and_then(|s| s.get("media_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("image/png")
                        .to_string();
                    let data = block
                        .get("source")
                        .and_then(|s| s.get("data"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let data_url = format!("data:{};base64,{}", media_type, data);
                    let Some(new_url) = normalize_image_data_url(&data_url) else {
                        continue;
                    };
                    let Some((meta, new_data)) = new_url.strip_prefix("data:").and_then(|r| r.split_once(',')) else {
                        continue;
                    };
                    let new_media_type = meta.split(';').next().unwrap_or("image/png").to_string();
                    if let Some(source) = block.get_mut("source") {
                        source["media_type"] = Value::String(new_media_type);
                        source["data"] = Value::String(new_data.to_string());
                        converted += 1;
                    }
                }
                _ => {}
            }
        }
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_handles_string_and_array() {
        assert_eq!(extract_text(&json!("hi")), "hi");
        assert_eq!(extract_text(&json!([{"type": "text", "text": "a"}, {"type": "image_url"}])), "a");
        assert_eq!(extract_text(&Value::Null), "");
    }

    #[test]
    fn anthropic_image_roundtrips_to_openai_and_back() {
        let blocks = vec![
            json!({"type": "text", "text": "look"}),
            json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}),
        ];
        let openai = anthropic_blocks_to_openai(&blocks);
        let back = openai_content_to_anthropic_blocks(&openai);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0]["text"], "look");
        assert_eq!(back[1]["source"]["data"], "AAAA");
        assert_eq!(back[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn single_text_block_folds_to_string() {
        assert_eq!(anthropic_blocks_to_openai(&[json!({"type": "text", "text": "x"})]), json!("x"));
    }

    fn data_url_of(img: image::DynamicImage) -> String {
        use base64::Engine;
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
        format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf))
    }

    #[test]
    fn gray_image_is_converted_to_rgb() {
        let gray = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(8, 8, image::Luma([120])));
        let converted = normalize_image_data_url(&data_url_of(gray)).expect("gray image should be converted");
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(converted.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().color(), image::ColorType::Rgb8);
    }

    #[test]
    fn rgb_image_is_left_untouched() {
        let rgb = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3])));
        assert!(normalize_image_data_url(&data_url_of(rgb)).is_none());
    }

    #[test]
    fn normalize_images_in_body_handles_both_block_shapes() {
        let gray = image::DynamicImage::ImageLuma8(image::GrayImage::from_pixel(4, 4, image::Luma([7])));
        let url = data_url_of(gray);
        let mut body = json!({
            "model": "agnes",
            "messages": [
                {"role": "user", "content": [{"type": "image_url", "image_url": {"url": url}}]},
                {"role": "user", "content": [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AA=="}}]}
            ]
        });
        // 第二块的 base64 不是合法图片，只应转换第一块
        assert_eq!(normalize_images_in_body(&mut body), 1);
        let new_url = body["messages"][0]["content"][0]["image_url"]["url"].as_str().unwrap();
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(new_url.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().color(), image::ColorType::Rgb8);
        assert_eq!(body["messages"][1]["content"][0]["source"]["data"], "AA==");
    }
}
