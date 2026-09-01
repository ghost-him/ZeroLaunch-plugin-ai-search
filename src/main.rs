//! AI 语义搜索插件主入口。
//!
//! 插件 = 进程元数据壳（AiSearchPlugin，纯 Plugin 主组件）+ SemanticBooster
//! （ScoreBooster，语义重排能力）。语义能力挂进宿主默认搜索管道：
//! 引擎打分后按嵌入余弦相似度增强，无需触发词路由与面板。

mod booster;

use std::sync::Arc;

use async_trait::async_trait;
use zerolaunch_plugin_api::config::{
    ComponentCore, ComponentType, Configurable, SettingDefinition,
};
use zerolaunch_plugin_api::{
    Plugin, PluginContext, PluginError, PluginHandle, PluginKind, PluginMetadata, PluginMode,
    Query, QueryResponse,
};
use zerolaunch_plugin_sdk_rust::{t_key, PluginApp};

use booster::SemanticBooster;

/// 插件主组件：进程级 metadata / 组件清单的载体。
/// 不参与触发词路由（trigger_keywords 为空），语义能力全部由 SemanticBooster 提供。
struct AiSearchPlugin {
    /// 组件 ID、名称、类型等基础元数据。
    core: ComponentCore,
    /// 插件静态元数据：id、名称、优先级等。
    metadata: PluginMetadata,
}

impl AiSearchPlugin {
    fn new() -> Self {
        Self {
            core: ComponentCore::new(
                "com.ghost-him.ai-search".to_string(),
                t_key("plugin.name"),
                t_key("plugin.description"),
                ComponentType::Plugin,
                100,
            ),
            metadata: PluginMetadata {
                id: "com.ghost-him.ai-search".to_string(),
                name: t_key("plugin.name"),
                version: "0.1.0".to_string(),
                description: t_key("plugin.description"),
                author: "ghost-him".to_string(),
                trigger_keywords: vec![],
                supported_os: vec!["windows".to_string()],
                priority: 100,
                // 第三方插件种类（宿主加载时强制覆盖为 ThirdParty，此处显式声明保持语义一致）
                kind: PluginKind::ThirdParty,
                hotkey: None,
                icon: None,
                mode: PluginMode::Inline,
            },
        }
    }
}

#[async_trait]
impl Configurable for AiSearchPlugin {
    fn core(&self) -> &ComponentCore {
        &self.core
    }

    fn setting_schema(&self) -> Vec<SettingDefinition> {
        vec![]
    }
}

#[async_trait]
impl Plugin for AiSearchPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    async fn init(
        &self,
        _ctx: &PluginContext,
        _handle: Option<Arc<PluginHandle>>,
    ) -> Result<(), PluginError> {
        Ok(())
    }

    async fn query(
        &self,
        _ctx: &PluginContext,
        _query: &Query,
    ) -> Result<QueryResponse, PluginError> {
        // 无触发词路由，查询不应到达；防御式返回空结果（宿主用不到）。
        Ok(QueryResponse::Empty)
    }

    async fn execute_action(
        &self,
        _ctx: &PluginContext,
        _action_id: &str,
        _payload: serde_json::Value,
    ) -> Result<(), PluginError> {
        Ok(())
    }
}

fn main() {
    // 预置插件 id（宿主注入 ZEROLAUNCH_PLUGIN_ID），使组件构造阶段的
    // t_key() 可用（组件元数据含翻译键，构造早于 run() 握手）。
    zerolaunch_plugin_sdk_rust::init();
    PluginApp::new(AiSearchPlugin::new())
        .with_score_booster(SemanticBooster::new())
        .run();
}
