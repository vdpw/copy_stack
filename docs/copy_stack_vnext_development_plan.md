# Copy Stack vNext 开发计划

## 目标

Copy Stack 当前版本已经完成：

-   macOS clipboard 监听
-   Rust/Tauri backend
-   SQLite 持久化
-   History UI
-   Restore clipboard
-   Compact mode
-   Clipboard filtering

下一版本目标：

从「可用工具」提升为「可信赖的长期使用产品」。

------------------------------------------------------------------------

# Milestone 0: 基础工程保障

优先级：★★★★★

## 0.1 Release baseline

任务：

-   创建 v0.1.0 tag
-   保存当前数据库 schema
-   保存 migration 测试数据

验收：

-   老数据库可以正常启动
-   migration 测试通过

## 0.2 增加测试覆盖

重点：

-   store
-   classification
-   pasteboard_protocol
-   resource_policy

覆盖：

-   hash 计算
-   compact mode
-   duplicate
-   migration

------------------------------------------------------------------------

# Milestone 1: Clipboard 搜索

优先级：★★★★★

目标：

用户可以快速找回历史复制内容。

## 1.1 SQLite FTS5 搜索索引

新增全文索引：

-   text
-   summary_display
-   compact_display

## 1.2 Search API

新增 Tauri command：

`search_copy_events`

参数：

-   keyword
-   limit
-   cursor

返回：

-   content_hash
-   highlight
-   timestamp

## 1.3 UI

History 页面增加搜索框：

-   debounce 300ms
-   输入立即搜索
-   清空恢复历史

验收：

复制：

docker compose password

之后搜索：

docker

可以找到。

------------------------------------------------------------------------

# Milestone 2: Clipboard 数据加密

优先级：★★★★★

目标：

保护用户 clipboard 隐私。

## 2.1 macOS Keychain

首次启动：

生成随机 256 bit key。

保存：

macOS Keychain。

## 2.2 加密内容

加密：

-   event_data
-   display
-   summary_display
-   compact_display

保留明文：

-   timestamp
-   content_hash
-   byte_count

算法：

AES-256-GCM

## 2.3 Migration

旧数据库：

plaintext -\> encrypted

要求：

-   rollback
-   backup

------------------------------------------------------------------------

# Milestone 3: Dedup 行为优化

优先级：★★★★☆

增加：

-   first_seen_at
-   last_seen_at
-   copy_count

重复复制时：

-   更新 last_seen_at
-   copy_count + 1
-   移动到顶部

UI 可显示：

-   copied count
-   last copied time

------------------------------------------------------------------------

# Milestone 4: Compact Mode 强化

优先级：★★★★☆

## 4.1 Hash namespace

避免：

图片+文字 与 纯文本冲突。

使用：

sha256( "compact-text:v1:" + text )

## 4.2 图文策略

规则：

  类型        处理
  ----------- ----------
  纯文本      保存
  图片        过滤
  图片+文字   保存文字
  HTML        提取文本
  RTF         转换文本

------------------------------------------------------------------------

# Milestone 5: macOS 原生体验

优先级：★★★★☆

## 5.1 首次启动引导

增加：

Clipboard access 权限提示。

## 5.2 Menu Bar

优化：

-   最近复制列表
-   Open History
-   Settings
-   Quit

## 5.3 Global Hotkey

增加：

Command + Shift + V

打开历史。

------------------------------------------------------------------------

# Milestone 6: 数据管理

优先级：★★★☆☆

## 6.1 Retention

支持：

-   Forever
-   30 days
-   90 days
-   1 year

## 6.2 Storage quota

显示：

当前占用空间。

------------------------------------------------------------------------

# Milestone 7: 发布准备

优先级：★★★☆☆

## 7.1 Apple Developer

准备：

-   Developer ID Application
-   notarization
-   signed DMG

## 7.2 Crash reporting

只上传：

-   error code
-   stack trace
-   app version
-   OS version

禁止上传：

-   clipboard content

------------------------------------------------------------------------

# 推荐开发顺序

## v0.2.0

1.  Search
2.  Duplicate tracking
3.  Compact mode hash fix

## v0.3.0

4.  Encryption
5.  Permission UX

## v0.4.0

6.  Global hotkey
7.  Menu bar enhancement
8.  Release pipeline

------------------------------------------------------------------------

# Codex 执行要求

Before coding:

1.  Read docs/index.md
2.  Read architecture/backend/persistence docs
3.  Understand SQLite migration strategy
4.  Do not break existing user database

Requirements:

-   Every schema change requires migration
-   Every feature requires tests
-   Keep Rust backend ownership clear
-   Avoid exposing clipboard content in logs
-   Never upload clipboard data externally

Run:

-   cargo test
-   pnpm test
-   pnpm lint

After implementation:

-   update docs
-   provide migration notes
-   provide manual QA checklist
