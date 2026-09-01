//! SemanticBooster — 语义分数增强器（ScoreBooster 组件）。
//!
//! 对引擎已打分的候选做语义相似度增强：查询与候选（名称+执行目标）经宿主
//! embedding 模型服务向量化，相似度折算为附加分（similarity × semantic_weight）。
//! 候选与查询向量均由宿主缓存（单文本粒度 L1/L2），重复查询免重算。
//! 宿主模型服务不可用（未配置 embedding 模型/调用失败）时静默降级为 no-op。
//!
//! 由宿主按模型档案拼接，本组件只声明 task_type 与候选标题（命名占位符参数）/
//! dimensions；相似度由宿主按模型元数据公式并行计算（host/model.similarity）。
use zerolaunch_plugin_api::services::model::{
    EmbeddingCapability, EmbeddingTemplateArgs, ModelEmbeddingRequest, ModelInfo, ModelKind,
    ModelSimilarityRequest, SemanticTask,
};
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use zerolaunch_plugin_api::config::{
    ComponentCore, ComponentType, ConfigActionDef, ConfigError, Configurable, DataActionBinding,
    FieldAction, FieldUiMetadata, SchemaKind, SchemaNode, SettingDefinition, WidgetHint,
};
use zerolaunch_plugin_api::{
    CachedCandidateData, CandidateId, ScoreBooster, ScoreDetail, ScoreDetailKind, ScoredCandidate,
    SearchCandidate,
};
use zerolaunch_plugin_sdk_rust::{host, t_key};

/// 组件默认设置（设置键名为 snake_case，配置存储类型约定）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSettings {
    /// 宿主 embedding 模型的全局 model id（如 `ollama/nomic-embed-text`）。
    #[serde(default)]
    pub embedding_model_id: String,
    /// 语义分加成系数（相似度 × 系数 累加到候选分数；无上限，默认值约等于
    /// 标准搜索基础分量级，使语义相似度成为总分的显著组成部分）。
    #[serde(default = "default_weight")]
    pub semantic_weight: f64,
}

fn default_weight() -> f64 {
    50.0
}

impl Default for SemanticSettings {
    fn default() -> Self {
        Self {
            embedding_model_id: String::new(),
            semantic_weight: default_weight(),
        }
    }
}

/// 配置动作 id：拉取宿主 embedding 模型清单（设置页下拉数据源）。
const ACTION_LIST_EMBEDDING_MODELS: &str = "list_embedding_models";

/// 从 ModelInfo 提取的 embedding 模型元数据（请求组装与相似度计算依据）。
#[derive(Debug, Clone)]
struct EmbeddingMeta {
    /// 模型声明的请求级能力清单（Title / TaskType / OutputDimensions）。
    capabilities: Vec<EmbeddingCapability>,
    /// 模型原生向量维度；未声明时为 None。
    dimensions: Option<u32>,
}

impl EmbeddingMeta {
    fn from_model_info(info: &ModelInfo) -> Option<Self> {
        match &info.kind {
            ModelKind::Embedding {
                capabilities,
                dimensions,
                ..
            } => Some(Self {
                capabilities: capabilities.clone(),
                dimensions: *dimensions,
            }),
            _ => None,
        }
    }
}

pub struct SemanticBooster {
    /// 组件 ID、名称、类型等基础元数据。
    core: ComponentCore,
    /// 组件设置（仅在 apply_settings 时写入）。
    settings: RwLock<SemanticSettings>,
    /// 模型元数据缓存：(model_id, meta)；模型变更或清单刷新时重建。
    meta: RwLock<Option<(String, EmbeddingMeta)>>,
}

impl SemanticBooster {
    pub fn new() -> Self {
        Self {
            core: ComponentCore::new(
                "semantic-booster".to_string(),
                t_key("booster.name"),
                t_key("booster.description"),
                ComponentType::ScoreBooster,
                60,
            ),
            settings: RwLock::new(SemanticSettings::default()),
            meta: RwLock::new(None),
        }
    }

    /// 当前生效的宿主 embedding 模型 id；未配置时为空。
    fn model_id(&self) -> String {
        self.settings.read().embedding_model_id.clone()
    }

    /// 解析当前模型的元数据（优先读缓存，model_id 变化时经宿主清单重新拉取）。
    async fn ensure_meta(&self) -> Option<EmbeddingMeta> {
        let model_id = self.model_id();
        if model_id.trim().is_empty() {
            return None;
        }
        if let Some((cached_id, meta)) = self.meta.read().as_ref() {
            if *cached_id == model_id {
                return Some(meta.clone());
            }
        }
        let models = host().model_list().await.ok()?;
        let meta = models
            .iter()
            .find(|m| m.model_id == model_id)
            .and_then(EmbeddingMeta::from_model_info);
        if let Some(meta) = &meta {
            *self.meta.write() = Some((model_id, meta.clone()));
        }
        meta
    }

    /// 调用宿主 embedding 模型服务；参数按模型能力已由调用方展开。
    /// 失败时返回 None（调用方降级跳过增强，不中断搜索管道）。
    async fn host_embed(
        &self,
        model_id: &str,
        input: Vec<String>,
        template_args: Option<Vec<EmbeddingTemplateArgs>>,
        task_type: SemanticTask,
        dimensions: Option<u32>,
    ) -> Option<Vec<Vec<f32>>> {
        let req = ModelEmbeddingRequest {
            model_id: model_id.to_string(),
            input,
            template_args,
            task_type,
            dimensions,
        };
        let expected = req.input.len();
        let resp = host().model_embedding(req).await.ok()?;
        if resp.vectors.len() != expected {
            tracing::warn!("宿主 embedding 返回向量数量不匹配，跳过语义增强");
            return None;
        }
        Some(resp.vectors)
    }

    /// 候选的语义文本：仅应用名（避免启动路径噪声主导向量、抹平区分度）。
    fn candidate_text(candidate: &SearchCandidate) -> String {
        candidate.name.clone()
    }
}

impl Default for SemanticBooster {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Configurable for SemanticBooster {
    fn core(&self) -> &ComponentCore {
        &self.core
    }

    fn setting_schema(&self) -> Vec<SettingDefinition> {
        vec![
            SettingDefinition {
                key: "semantic_weight".to_string(),
                schema: SchemaNode {
                    kind: SchemaKind::Number {
                        minimum: Some(0.0),
                        maximum: None,
                        multiple_of: None,
                    },
                    default: Some(serde_json::json!(default_weight())),
                },
                ui: FieldUiMetadata {
                    pointer: "/semantic_weight".to_string(),
                    label: t_key("booster.weightLabel"),
                    description: t_key("booster.weightDesc"),
                    group: None,
                    order: 0,
                    visible: true,
                    read_only: false,
                    visible_when: None,
                    widget: None,
                    action: None,
                    detail_action: None,
                },
            },
            SettingDefinition {
                key: "embedding_model_id".to_string(),
                schema: SchemaNode {
                    kind: SchemaKind::String {
                        enum_values: Vec::new(),
                        enum_labels: Vec::new(),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                    },
                    default: Some(serde_json::json!("")),
                },
                ui: FieldUiMetadata {
                    pointer: "/embedding_model_id".to_string(),
                    label: t_key("booster.modelIdLabel"),
                    description: t_key("booster.modelIdDesc"),
                    group: None,
                    order: 1,
                    visible: true,
                    read_only: false,
                    visible_when: None,
                    // 下拉选择宿主 embedding 模型；选项经 data action
                    // 声明的 list_embedding_models config action 动态加载。
                    widget: Some(WidgetHint::Select),
                    action: Some(FieldAction::Data(DataActionBinding {
                        action: ACTION_LIST_EMBEDDING_MODELS.to_string(),
                        component: None,
                        label_field: "name".to_string(),
                        label_field_label: String::new(),
                        value_field: "modelId".to_string(),
                        merge_key: None,
                        field_mapping: Vec::new(),
                    })),
                    detail_action: None,
                },
            },
        ]
    }

    fn get_settings(&self) -> serde_json::Value {
        serde_json::to_value(self.settings.read().clone()).unwrap_or_default()
    }

    async fn apply_settings(&self, settings: serde_json::Value) -> Result<(), ConfigError> {
        let parsed: SemanticSettings = serde_json::from_value(settings).unwrap_or_default();
        // 模型变更时清空元数据缓存（不同模型向量空间不一致，宿主嵌入缓存按请求键隔离）。
        let model_changed = parsed.embedding_model_id != self.settings.read().embedding_model_id;
        *self.settings.write() = parsed;
        if model_changed {
            *self.meta.write() = None;
        }
        Ok(())
    }

    fn config_actions(&self) -> Vec<ConfigActionDef> {
        vec![ConfigActionDef {
            action: ACTION_LIST_EMBEDDING_MODELS.to_string(),
            label: t_key("booster.listModelsLabel"),
            description: t_key("booster.listModelsDesc"),
        }]
    }

    async fn execute_config_action(
        &self,
        action: &str,
        _params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match action {
            ACTION_LIST_EMBEDDING_MODELS => {
                let models = host()
                    .model_list()
                    .await
                    .map_err(|e| format!("获取宿主模型清单失败: {e}"))?;
                let embedding_models: Vec<_> = models
                    .into_iter()
                    .filter(|m| matches!(m.kind, ModelKind::Embedding { .. }))
                    .collect();
                if embedding_models.is_empty() {
                    tracing::warn!("宿主未配置 embedding 模型，语义搜索下拉将为空");
                }
                serde_json::to_value(embedding_models)
                    .map_err(|e| format!("序列化模型清单失败: {e}"))
            }
            _ => Err(format!("Unknown config action: {action}")),
        }
    }
}

#[async_trait]
impl ScoreBooster for SemanticBooster {
    async fn record(&self, _candidate_id: CandidateId, _data: &CachedCandidateData, _query: &str) {
        // 纯静态语义增强：无查询-候选关联学习
    }

    async fn boost(
        &self,
        scored: &mut Vec<ScoredCandidate>,
        data: &CachedCandidateData,
        query: &str,
    ) {
        if scored.is_empty() {
            return;
        }
        let Some(meta) = self.ensure_meta().await else {
            tracing::warn!("语义增强跳过：未配置 embedding 模型或模型元数据不可用");
            return;
        };
        let model_id = self.model_id();
        let settings = self.settings.read().clone();

        // 按模型能力展开请求参数：task_type 宿主必填（档案驱动模板），维度按能力裁剪。
        let supports_dimensions = meta
            .capabilities
            .contains(&EmbeddingCapability::OutputDimensions);
        // 固定到模型原生维度，保证跨查询输出维度一致（利于缓存命中）。
        let dimensions = if supports_dimensions {
            meta.dimensions
        } else {
            None
        };
        // 候选侧任务类型：检索文档；查询侧为检索查询（见下方 q_task_type）。
        let task_type = SemanticTask::RetrievalDocument;

        // 候选文本批量向量化（宿主按单文本粒度缓存，重复查询命中 L1/L2 免重算）。
        // 去重 payload，避免同一启动目标重复嵌入；结果按候选顺序对齐。
        let candidates: Vec<&SearchCandidate> = scored
            .iter()
            .filter_map(|sc| data.get_candidate(sc.candidate_id))
            .collect();
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<(String, String)> = candidates
            .iter()
            .filter_map(|candidate| {
                let key = candidate.target.payload().to_string();
                if seen.insert(key.clone()) {
                    Some((key, Self::candidate_text(candidate)))
                } else {
                    None
                }
            })
            .collect();
        if unique.is_empty() {
            return;
        }
        let texts: Vec<String> = unique.iter().map(|(_, text)| text.clone()).collect();
        // 模板额外参数：候选应用名作为命名占位符 title（gemma 类 `title: {title:none} | text: {0}`
        // 模板用；qwen3 类只用 {0} 的模板忽略该字段）。顺序与 input 一一对应。
        let template_args: Vec<EmbeddingTemplateArgs> = unique
            .iter()
            .map(|(_, text)| EmbeddingTemplateArgs {
                title: Some(text.clone()),
            })
            .collect();
        let Some(vectors) = self
            .host_embed(&model_id, texts, Some(template_args), task_type, dimensions)
            .await
        else {
            return;
        };
        let by_payload: HashMap<String, Arc<Vec<f32>>> = unique
            .into_iter()
            .zip(vectors)
            .map(|((key, _), emb)| (key, Arc::new(emb)))
            .collect();

        // 查询嵌入（每查询一次）；查询侧无标题，仅按能力传 dimensions。
        let Some(q_emb) = self
            .host_embed(
                &model_id,
                vec![query.to_string()],
                None,
                SemanticTask::RetrievalQuery,
                dimensions,
            )
            .await
            .and_then(|mut v| v.pop())
        else {
            tracing::warn!("查询嵌入计算失败，跳过语义增强");
            return;
        };

        // 候选嵌入与查询向量一并交宿主并行计算相似度；缺失嵌入的候选保留原分。
        let weight = settings.semantic_weight as f32;
        let mut sims = Vec::with_capacity(candidates.len());
        let mut candidate_embeds: Vec<Vec<f32>> = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            match by_payload.get(candidate.target.payload()) {
                Some(emb) => {
                    sims.push(f32::MAX); // 占位：由宿主 similarity 返回覆盖
                    candidate_embeds.push(emb.as_ref().clone());
                }
                None => sims.push(0.0),
            }
        }
        if !candidate_embeds.is_empty() {
            let resp = host()
                .model_similarity(ModelSimilarityRequest {
                    model_id: model_id.clone(),
                    query: q_emb,
                    targets: candidate_embeds,
                })
                .await
                .ok();
            if let Some(resp) = resp {
                if resp.similarities.len() == sims.iter().filter(|s| **s == f32::MAX).count() {
                    let mut sim_iter = resp.similarities.into_iter();
                    for sim_slot in sims.iter_mut() {
                        if *sim_slot == f32::MAX {
                            *sim_slot = sim_iter.next().unwrap_or(0.0);
                        }
                    }
                } else {
                    tracing::warn!("宿主 similarity 返回数量不匹配，跳过语义增强");
                }
            }
        }
        // 应用相似度加成
        for (sc, sim) in scored.iter_mut().zip(sims.iter()) {
            if *sim <= 0.0 {
                continue;
            }
            let contribution = *sim * weight;
            sc.score += contribution as f64;
            sc.detailed_score.push(ScoreDetail {
                score: *sim as f64,
                weight: weight as f64,
                description: t_key("booster.semanticScore"),
                kind: ScoreDetailKind::Add,
            });
        }
    }
}
