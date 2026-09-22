use crate::model::{AppResult, ModelConfig};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_LINK_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportPayload {
    pub models: Vec<ModelConfig>,
}

/// Parse only the versioned model-import route, never paths, scripts or auto-apply flags.
pub fn parse_link(raw: &str) -> AppResult<Vec<ModelConfig>> {
    if raw.len() > MAX_LINK_BYTES {
        return Err("导入链接超过 64 KiB，请分批导入".into());
    }
    let url = Url::parse(raw.trim()).map_err(|_| "导入链接格式无效")?;
    if url.scheme() != "power-switch"
        || url.host_str() != Some("model")
        || url.path() != "/import"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err("仅支持 power-switch://model/import 链接".into());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if pairs.len() != 2
        || pairs.iter().filter(|(k, _)| k == "v").count() != 1
        || pairs.iter().filter(|(k, _)| k == "data").count() != 1
    {
        return Err("链接只允许一个 v 和一个 data 参数".into());
    }
    if pairs.iter().find(|(k, _)| k == "v").unwrap().1 != "1" {
        return Err("不支持的导入协议版本".into());
    }
    let data = URL_SAFE_NO_PAD
        .decode(
            pairs
                .iter()
                .find(|(k, _)| k == "data")
                .unwrap()
                .1
                .as_bytes(),
        )
        .map_err(|_| "配置清单不是有效 Base64URL")?;
    let mut payload: ImportPayload =
        serde_json::from_slice(&data).map_err(|_| "模型清单格式错误或包含不支持的字段")?;
    if payload.models.is_empty() || payload.models.len() > 50 {
        return Err("每次可导入 1 至 50 个模型".into());
    }
    for model in &mut payload.models {
        model.id.clear();
        model.validate()?;
    }
    Ok(payload.models)
}

/// Create a shareable link with credentials excluded unless explicitly requested.
pub fn share_link(model: &ModelConfig, include_secret: bool) -> AppResult<String> {
    let mut model = model.clone();
    model.id.clear();
    if !include_secret {
        model.api_key.clear();
    }
    let bytes = serde_json::to_vec(&ImportPayload {
        models: vec![model],
    })
    .map_err(|_| "无法生成链接")?;
    let link = format!(
        "power-switch://model/import?v=1&data={}",
        URL_SAFE_NO_PAD.encode(bytes)
    );
    if link.len() > MAX_LINK_BYTES {
        return Err("模型信息太长，无法生成链接".into());
    }
    Ok(link)
}
