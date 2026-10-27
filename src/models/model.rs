use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ModelStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Hash, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum Capability {
    Reasoning,
    Vision,
    Embedding,
    Rerank,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub platform_id: String,
    /// 同一上游目录键（type + base_url 哈希）；同 key 的账号共享这一行。
    #[serde(default)]
    pub catalog_key: Option<String>,
    pub model_id: String,
    pub display_name: String,
    pub max_tokens: u32,
    pub context_window: u32,
    /// 缓存未命中输入价（每 100 万 token）
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    /// 缓存命中输入价；NULL → 回退 input_price
    #[serde(default)]
    pub cache_read_price: Option<f64>,
    /// 缓存写入输入价；NULL → 回退 input_price
    #[serde(default)]
    pub cache_write_price: Option<f64>,
    /// 高峰时段覆盖：{"windows":[{start,end}], "peak":{input,cache_read,cache_write,output}}
    #[serde(default)]
    pub peak_pricing: Option<serde_json::Value>,
    /// 每次成功请求消耗的上游积分；NULL = 未定价，按 0 计。
    /// 与 input/output_price 的“每 100 万 token”口径不同，单位是“次”。
    pub credit_price: Option<f64>,
    /// 积分倍率（如 WorkBuddy 目录给的 "x0.79 credits"）。NULL = 1.0。
    /// 实际消耗 = credit_price × credit_multiplier，所以倍率 0 表示该模型不扣积分。
    #[serde(default)]
    pub credit_multiplier: Option<f64>,
    pub capabilities: Vec<Capability>,
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelRequest {
    pub platform_id: String,
    pub model_id: String,
    pub display_name: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    #[serde(default)]
    pub cache_read_price: Option<f64>,
    #[serde(default)]
    pub cache_write_price: Option<f64>,
    #[serde(default)]
    pub peak_pricing: Option<serde_json::Value>,
    #[serde(default)]
    pub credit_price: Option<f64>,
    /// 积分倍率；NULL = 1.0
    #[serde(default)]
    pub credit_multiplier: Option<f64>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

/// 多选导入的请求体：只带勾选的远端模型 ID，档位仍用 `default_*` 那两档占位。
#[derive(Debug, Clone, Deserialize)]
pub struct ImportModelsRequest {
    #[serde(default)]
    pub model_ids: Vec<String>,
}

/// 定价导入的一行。三态与单行编辑、批量编辑逐字一致：字段不出现＝不改、`null`＝清空、数字＝设值。
/// "CSV 里的空单元格算不算清空"由前端在解析时定（默认算保持原值），到这儿已经是明确的三态。
#[derive(Debug, Clone, Deserialize)]
pub struct ModelPriceRow {
    pub model_id: String,
    #[serde(default = "keep_number")]
    pub input_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub output_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_read_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_write_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub credit_price: NullableNumber,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportPricesRequest {
    #[serde(default)]
    pub prices: Vec<ModelPriceRow>,
}

/// 可空列的三种意图：JSON 里没写＝保持原值，写 `null`＝清空，写数字＝设值。
///
/// 为什么不用 `Option<Option<f64>>`：serde_json 把 JSON `null` 交给外层 Option 的 `visit_none`，
/// "没写"和"写了个空"会塌成同一个值，价格一旦录入就再也退不回"未定价"（成本统计会一直挂着旧价）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NullableNumber {
    #[default]
    Keep,
    Clear,
    Set(f64),
}

struct NullableNumberVisitor;

impl<'de> serde::de::Visitor<'de> for NullableNumberVisitor {
    type Value = NullableNumber;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a number or null")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(NullableNumber::Clear)
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(NullableNumber::Set(v))
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(NullableNumber::Set(v as f64))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(NullableNumber::Set(v as f64))
    }
}

impl<'de> serde::Deserialize<'de> for NullableNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(NullableNumberVisitor)
    }
}

fn keep_number() -> NullableNumber { NullableNumber::Keep }

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateModelRequest {
    pub display_name: Option<String>,
    pub max_tokens: Option<u32>,
    pub context_window: Option<u32>,
    #[serde(default = "keep_number")]
    pub input_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub output_price: NullableNumber,
    /// 三态：没写 = 不改，null = 清空（未定价），数字 = 设值。
    #[serde(default = "keep_number")]
    pub credit_price: NullableNumber,
    /// 三态：没写 = 不改，null = 清空（回落 1.0），数字 = 设值。
    #[serde(default = "keep_number")]
    pub credit_multiplier: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_read_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_write_price: NullableNumber,
    /// 缺省 = 不改，null = 清空，对象 = 设置高峰时段覆盖。
    #[serde(default)]
    pub peak_pricing: Option<serde_json::Value>,
    pub capabilities: Option<Vec<Capability>>,
    pub status: Option<ModelStatus>,
}

/// 批量编辑的请求体：三态与单行编辑完全一致（没写＝不改，`null`＝清空，给值＝设值），
/// 所以"清空价格"在两个入口里是同一个动作，不会一处能清、一处清不掉。
/// 能力只有整组替换一种动作——追加与替换没法用同一个多选框表达，UI 上就写"替换"。
#[derive(Debug, Clone, Deserialize)]
pub struct BatchModelPatch {
    #[serde(default)]
    pub model_ids: Vec<String>,
    #[serde(default = "keep_number")]
    pub input_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub output_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub credit_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub credit_multiplier: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_read_price: NullableNumber,
    #[serde(default = "keep_number")]
    pub cache_write_price: NullableNumber,
    pub capabilities: Option<Vec<Capability>>,
}

impl BatchModelPatch {
    pub fn has_change(&self) -> bool {
        self.input_price != NullableNumber::Keep
            || self.output_price != NullableNumber::Keep
            || self.credit_price != NullableNumber::Keep
            || self.credit_multiplier != NullableNumber::Keep
            || self.cache_read_price != NullableNumber::Keep
            || self.cache_write_price != NullableNumber::Keep
            || self.capabilities.is_some()
    }
}

// 远端模型列表只给 ID，入库时先按这两档占位，等人在"已同步模型"里补。
// 导出给 db::model::import_model_ids 用，免得同步与导入两处各写一套默认值。
pub fn default_max_tokens() -> u32 { 4096 }pub fn default_context_window() -> u32 { 8192 }
