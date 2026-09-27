# ZeroLaunch AI 语义搜索插件

ZeroLaunch 第三方插件：宿主 embedding 模型对搜索结果做语义相似度重排（`score_booster` 组件挂在宿主默认搜索管道上，无触发词路由、无面板）。

插件只依赖 `zerolaunch-plugin-api`（trait/类型）与 `zerolaunch-plugin-sdk-rust`（`run()`、`host()`），不依赖 Tauri 与宿主源码。

## 目录结构

```
├── Cargo.toml          # 依赖 zerolaunch-plugin-sdk-rust / plugin-api（0.2）
├── manifest.toml       # 插件清单（[plugin] 元数据、运行时命令、能力与图标），打包时位于 zip 根
├── icon.svg            # 插件图标（清单 [icon] 声明，随包分发并作为 Release 附件供市场卡片展示）
├── package.py          # 打包脚本（cargo build --release + 生成安装 zip）
├── src/main.rs         # 启动骨架：init() + plugin::app().run()（随模板同步，尽量别改）
├── src/plugin.rs       # 插件实现（你自己的代码，不参与同步）
├── src/plugin/booster.rs  # 语义重排组件（ScoreBooster，由 plugin.rs 的 app() 装配挂载）
├── i18n/               # 语言包（可选）：宿主加载时并入翻译目录，t_key() 自动带插件 id 前缀
└── .github/workflows/  # CI（check/build/打包）、推 tag 自动发 Release、模板同步入口
```

`src/main.rs` 只做启动（`init()` 预置插件 id → `plugin::app().run()`），十来行、随模板同步；插件实现与装配都在 `src/plugin.rs`——照常实现 SDK 的 `Plugin` / `Configurable` trait，并导出 `pub fn app() -> PluginApp`。

## 快速开始

1. **改标识**——三处必须一致，漏改会导致宿主侧路由与显示错位：

   | 位置 | 改什么 |
   |---|---|
   | `manifest.toml` | `[plugin]` 的 `id`（全小写反向域名，如 `com.ghost-him.ai-search`）、`version`，以及必填元数据 `mode` / `triggerKeywords` / `supportedOs` / `priority` |
   | `src/plugin.rs` | `ComponentCore::new` 的组件 ID（须与清单 `[plugin].id` 一致；插件级元数据全部在清单里，代码不再声明） |
   | `Cargo.toml` | `package.version`（与 manifest 保持一致） |

2. **实现 `Plugin` trait**（`src/plugin.rs` 是完整可跑示例，写法与 SDK 文档一致）：
   - `query()`：返回 `QueryResponse::List`（标准搜索结果，走搜索栏/CLI）或 `QueryResponse::CustomPanel`（面板自渲染，`data` 可承载任意 JSON，`keep_search_bar` 决定是否保留搜索栏）。
   - `execute_action()`：动作执行（打开文件等经 `host()` 平台 API）；`interaction_policy()` 声明面板按键绑定。
   - 插件形态（`inline` / `panel`）与触发词不在代码里写：宿主读 `manifest.toml` 的 `[plugin]` 段构造插件级元数据。
3. **（可选）设置项**：实现 `Configurable`——`setting_schema()` 声明后宿主设置页自动渲染，`apply_settings()` / `get_settings()` 应用与回读。
4. **（可选）i18n**：面向用户的文本用 `t_key("key")`，译文放 `i18n/zh-Hans.json`、`en.json`；面板侧用 `host.t(key)`（同键）。
5. **（可选）面板**：`[ui].panelEntry` 指向 `ui/panel.mjs`，导出 `mount(rootEl, host)`，在宿主 Shadow DOM 内执行。
6. **装配与额外组件**：`src/plugin.rs` 导出 `pub fn app() -> PluginApp`（骨架调它）；需要额外组件就在这里挂载，如 `PluginApp::new(MyPlugin::new()).with_score_booster(Booster::new())`。

## 开发要点

| 主题 | 要点 |
|---|---|
| 骨架 vs 业务 | `src/main.rs`（十来行启动骨架）在同步集合里，**尽量别改**；确需改动就照常改，同步 PR 人工合并。业务代码一律写 `src/plugin.rs`，追加模块放 `src/plugin/`（同时把 `src/plugin/` 加进 `.github/.templatesyncignore`）。 |
| manifest 键名 | 宿主 schema 用 camelCase（`panelEntry` / `settingsEntry` / `resultItemEntry`）。写成 snake_case **不报错但被静默忽略**；`package.py` 对这三个键做了预检告警。 |
| 插件 id | 必须匹配 `[a-z][a-z0-9]*(\.[a-z][a-z0-9_-]*)+\z`（全小写反向域名），且与 `src/plugin.rs` 里构造的 id 一致。 |
| 插件元数据 | 唯一声明处是 `manifest.toml` 的 `[plugin]` 段（必填 `id`/`name`/`version`/`description`/`author`/`mode`/`triggerKeywords`/`supportedOs`/`priority`，可选 `hotkey`），宿主读清单构造后不再向插件进程索取。 |
| SDK 版本 | 依赖 `zerolaunch-plugin-* = "0.2"`：清单必填元数据与「插件级元数据只由清单声明」都是 0.2 起的契约，不要降级到 0.1（0.1 的 SDK 上报协议 1.0，会被宿主协议 2.0 闸门拒载）。 |
| `t_key` 时机 | `t_key()` 依赖握手注入的插件 id；在 `run()` 之前构造元数据时调用**必须先 `zerolaunch_plugin_sdk_rust::init()`**——骨架 `main()` 已含，业务文件里无需再调。 |
| 语言包结构 | 键与 `t_key` 路径一致、可带点号（`"booster.name"` ↔ `t_key("booster.name")`）；值只能是字符串或嵌套对象，出现数字/布尔会让宿主拒绝加载整包。 |
| 面板样式 | 用宿主 CSS 变量（`--bg-primary`、`--text-primary` 等）即自动跟随宿主主题，不要写死颜色。 |
| 面板销毁 | `mount` 内创建的定时器 / window 级监听器必须清理：`host.onDestroy(cb)` 注册回调，或让 `mount` 返回 cleanup 函数（二选一）。否则反复开关面板会线性累积泄漏。 |
| 打包布局 | `manifest.toml` 必须在 zip 根；`bin/` 内 exe 的文件名取自 `[runtime].command`；`extra/` 内容并入 zip 根，放必须与 exe 同目录的运行时文件（如 `Everything64.dll`）。 |
| 宿主兼容 | 加载期只校验协议 major 版本；manifest 没有宿主版本下限字段（旧模板的 `minHostVersion` 已废弃，写了也会被忽略）。 |

## 调试与验证

- **插件日志**：`%USERPROFILE%/.ZeroLaunch-rs/plugin-logs/<plugin-id>.log`（与宿主日志分离）。加载失败时日志保留供排查，插件正常卸载时被删除。
- **CLI**（宿主运行时可直查）——`zerolaunch-cli.exe`（别名 `zl`，`--json` 是全局开关，必须写在子命令**前面**）：
  - `query "<触发词> <文本>"` 查插件响应形状（`--json query "..."` 输出原始 JSON，验证 `CustomPanel` 载荷最直接）；
  - `plugins list` / `plugins info <id>` / `plugins logs <id> --tail 50`：已装插件、清单详情、日志；
  - `session`：当前会话模式。
- **改完冒烟**：`cargo check` 零错误、`python package.py` 能出包；面板改动在宿主设置页重新加载插件后即可预览。

## 发布

推 tag 自动发布（`.github/workflows/release.yml`，随模板同步下发）：`git tag v0.1.0 && git push origin v0.1.0` —— GitHub Actions 构建 + 打包，把 `dist/zerolaunch-plugin-<短id>-v<版本>.zip` 作为附件发到 Release。要求 tag 形如 `v<major>.<minor>.<patch>`，且与 `manifest.toml [plugin].version`、`Cargo.toml version` 三处一致（不一致直接失败）。补发：Actions → 「发布插件」→ Run workflow，填该 tag。

本地打包（同一套流程）：

```bash
python package.py              # 等价于 cargo build --release 后打包（无 Python 时用 uv run --python 3.12 python package.py）
python package.py --no-build    # 复用现有产物直接打包
```

产物 `zerolaunch-plugin-<插件短id>-v<版本号>.zip`（短 id = `[plugin].id` 末段，`com.ghost-him.ai-search` → `ai-search`）。
插件市场按 `/releases/latest` 的这个 zip 附件自动安装，别改产物名与 zip 布局。

安装：设置 → 插件管理 → 安装本地插件，选择该 zip；或手动解压到 `%USERPROFILE%/.ZeroLaunch-rs/plugins/<plugin-id>/` 后重新加载。

## 模板同步（框架文件自动更新）

同步跑在插件仓库这一侧（`.github/workflows/sync-template.yml`，模板与各插件仓库逐字节一致，改动在模板仓库做）。业务文件（`src/plugin.rs`、`ui/`、`i18n/`、`Cargo.*`、`manifest.toml`、`README.md`）按 `.github/.templatesyncignore` 保留本仓库内容，其余框架文件（`src/main.rs` 骨架、`package.py`、`ci.yml`、`sync-template.yml` 等）由 [actions-template-sync](https://github.com/AndreasAugustin/actions-template-sync) 合入，有差异时自动提 PR（无差异不提）——**宿主/SDK API 变更靠这一步送进插件仓库**。

- **触发**：每周一 02:23 UTC 定时跑一次（GitHub 高峰时段可能延迟）；模板刚改完想立刻同步，到本仓库 Actions 页手动运行 `sync-template`。
- **首次配置**：配 secret `TEMPLATE_SYNC_TOKEN`（PAT，`contents: write` + `workflow`）——同步会推 `.github/workflows/` 下的文件，默认 `GITHUB_TOKEN` 没有 `workflow` 权限会失败；不配则需在 Settings → Actions → General 勾选 *Allow GitHub Actions to create and approve pull requests*，且同步 PR 的 CI 需人工批准。
