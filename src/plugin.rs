//! AI 语义搜索插件实现——**业务文件**：不参与模板同步（`.templatesyncignore` 排除 `src/plugin.rs`）。
//!
//! 插件 = 组件壳（AiSearchPlugin，纯 Plugin 主组件）+ SemanticBooster（ScoreBooster，语义重排）。
//! 启动骨架（`src/main.rs`）只负责 `init()` + `app().run()`；组装（含挂载 booster）在下面的 `app()` 里。

//! AI 语义搜索插件主入口。
//!
//! 插件 = 组件壳（AiSearchPlugin，纯 Plugin 主组件）+ SemanticBooster
//! （ScoreBooster，语义重排能力）。语义能力挂进宿主默认搜索管道：
//! 引擎打分后按嵌入余弦相似度增强，无需触发词路由与面板。

mod booster;

use std::sync::Arc;

use async_trait::async_trait;
use zerolaunch_plugin_api::config::{
    ComponentCore, ComponentType, Configurable, SettingDefinition,
};
use zerolaunch_plugin_api::{Plugin, PluginContext, PluginError, PluginHandle, Query, QueryResponse};
use zerolaunch_plugin_sdk_rust::{t_key, PluginApp};

use booster::SemanticBooster;

/// 插件主组件：组件清单的载体（插件级元数据由宿主读 `manifest.toml` 构造，代码不再声明）。
/// 不参与触发词路由（清单 `triggerKeywords` 为空），语义能力全部由 SemanticBooster 提供。
struct AiSearchPlugin {
    /// 组件 ID、名称、类型等基础元数据。
    core: ComponentCore,
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

/// 装配插件应用（骨架 `main()` 调用 `.run()`）：主组件 + 语义重排组件。
pub fn app() -> PluginApp {
    PluginApp::new(AiSearchPlugin::new()).with_score_booster(SemanticBooster::new())
}
