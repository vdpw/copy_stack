# ClipEcho

用户确认的产品名称：**ClipEcho**。Clip 表达剪贴板，Echo 表达已有内容的再次回应。

## Icon direction

深午夜蓝圆角方形底座、暖象牙白的古典侧影，以及一道淡蓝色回声弧线。
用清晰的平面轮廓呈现宁芙意象，控制装饰细节，使图形保持安静、克制，
适合当前 macOS 风格界面。

Echo 的神话背景见英国国家美术馆的
[Echo and Narcissus](https://www.nationalgallery.org.uk/paintings/glossary/echo-and-narcissus)：
Echo 是只能重复他人话语的宁芙。这里借用“再次回应”作为剪贴板历史的品牌隐喻。
这幅侧影是现代创作，不是对古代 Echo 形象的考据复原。

## Deliverables

- `clipecho-icon.png` — 高清透明 PNG 设计稿。
- `icon-128.png` — 128 px 预览。
- `icon-32.png` — 32 px 预览。
- `prompt.txt` — 内置 imagegen 实际使用的完整提示词。
- `tray-master.png` — 菜单栏专用透明单色母版。
- `tray-prompt.txt` — 菜单栏母版的 imagegen 编辑提示词。
- `tray-preview.png` — 浅色、深色和选中状态下的模板渲染预览。
- `menu-bar-native.png` — macOS 开发版实际菜单栏图标的局部截图。

图稿在名称尚为 Echo 时开始生成；用户随后确定为 ClipEcho。图标没有文字，
因此沿用该图形，并以 ClipEcho 命名交付文件。

使用内置 imagegen 生成，macOS `sips` 导出尺寸预览。128 px 与 32 px 预览
均经过视觉检查。应用名称、窗口、原生菜单和托盘提示已统一为 ClipEcho。
完整应用图标已通过 Tauri icon 命令导出到 `src-tauri/icons/`，网页图标为
`public/clipecho.png`。

菜单栏使用独立的 `src-tauri/icons/tray-template.png`：36 × 36 RGBA，
透明背景，仅保留侧影与回声弧线。macOS 按 18 pt 显示并通过 template 模式
自动着色；没有将深蓝方形底座带入菜单栏。`tray-preview.png` 是同一 alpha
模板在浅色、深色和蓝色选中背景上的渲染预览，不是桌面截图。

## Verification — 2026-09-22

- 前端 type-check、lint、70 项测试、build 与 security-check 通过。
- Rust 格式检查、debug / release cargo check 通过；170 项测试通过，
  1 项手动性能测试按项目设置忽略。
- 使用私有临时数据库及临时 QA identifier 启动 `pnpm desktop:dev`。
  原生辅助功能检查确认主菜单、About / Hide / Quit 和托盘提示均为 ClipEcho。
  菜单栏正常展示新的单色侧影；打开后通过原生接口确认原有菜单与新名称，
  并通过 Quit ClipEcho 退出测试实例。浅色、深色和选中着色另见模板渲染预览。
- 正式版保留原登录项键名，并只刷新原本已启用的登录项路径；这条分支已
  通过单元测试及 release 编译检查，没有在本次 QA 中改写真实登录项。
