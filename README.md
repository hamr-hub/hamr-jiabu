# JiaBu 决策系统 (jiabu.hamr.store)

> HamR 智能决策引擎 - 数据分析、幸福感评估、智能建议

[![Status](https://img.shields.io/badge/status-开发中-yellow)](https://github.com/hamr-hub/hamr-jiabu)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Backend](https://img.shields.io/badge/backend-Rust+Axum-orange)](https://github.com/tokio-rs/axum)
[![Frontend](https://img.shields.io/badge/frontend-React+TypeScript-61dafb)](https://react.dev)

## 📋 项目概述

**项目编号**: PROJ-012  
**域名**: jiabu.hamr.store  
**优先级**: ⭐⭐ 中  
**状态**: 待开发（Phase 2）

JiaBu（家步）决策系统是 HamR 的智能大脑，基于五维家庭数据进行深度分析，提供幸福感评估、智能建议和决策辅助功能。

## 🎯 核心功能

### 1. 数据分析
对 HamR 管家的五维数据进行深度分析：
- **人**: 成员互动频率、关系亲密度
- **时**: 时间分配均衡度、日程冲突检测
- **事**: 任务完成率、延期分析
- **物**: 资产管理完善度、消耗品预警
- **境**: 环境舒适度、异常检测

### 2. 幸福感评估

**五维幸福指数** (0-100 分):

| 维度 | 指标 | 权重 |
|-----|-----|------|
| **人** | 人际互动频率、关系质量 | 25% |
| **时** | 时间分配均衡度、自由时间占比 | 20% |
| **事** | 任务完成率、压力指数 | 20% |
| **物** | 资产管理完善度、物品整洁度 | 15% |
| **境** | 环境舒适度、空间利用率 | 20% |

**输出内容**:
- 综合评分 (加权平均)
- 趋势对比 (周/月/年)
- 预警机制 (分数低于 60 分提醒)
- 雷达图可视化

### 3. 智能建议

**规则引擎**:
- 基于数据分析的提醒
  - "已连续 X 天未全家活动，建议安排家庭聚餐"
  - "本月任务完成率仅 40%，建议调整优先级"
  - "客厅温度偏高，建议调整空调设置"
  
- 个性化活动推荐
  - 周末家庭活动建议
  - 休闲娱乐推荐
  - 健康生活建议

- 消耗品补货提醒
  - 基于历史消耗速度预测
  - 库存低于阈值自动提醒

### 4. 决策辅助

**场景模拟**:
- 多维度对比 (成本/时间/影响)
- 风险量化评估
- 最佳/最差/中位情景分析
- 决策跟踪与复盘

**示例场景**:
- 购车决策: 价格/油耗/空间/维护成本对比
- 旅行规划: 预算/时间/景点/交通方式分析
- 教育投资: 学费/效果/时间投入/长期回报

### 5. 幸福报告

**周报/月报/年报**:
- 幸福指数趋势图
- 五维得分雷达图
- 家庭活动热力图
- 时间分配饼图
- 重要事件回顾
- 改进建议清单

## 🏗️ 系统架构

```
┌─────────────────┐
│   Frontend      │  React SPA
│(jiabu.hamr.*)   │  可视化展示
└────────┬────────┘
         │ HTTPS
┌────────▼────────┐
│  JiaBu Backend  │  Rust + Axum
│   分析引擎       │  规则引擎
└────────┬────────┘
         │
    ┌────┴─────┬────────────┬──────────┐
    │          │            │          │
┌───▼───┐  ┌──▼───┐  ┌────▼────┐  ┌──▼───┐
│ HamR  │  │Redis │  │Machine  │  │Time  │
│  App  │  │Cache │  │Learning │  │Series│
│ Data  │  │      │  │(Future) │  │  DB  │
└────────┘  └──────┘  └─────────┘  └──────┘
```

## 🛠️ 技术栈

### 后端 (backend/)
| 技术 | 用途 | 备注 |
|-----|------|------|
| **Rust** | 编程语言 | 高性能 |
| **Axum** | Web 框架 | 异步 |
| **SQLx** | 数据访问 | 查询 HamR 数据 |
| **Redis** | 缓存 | 分析结果缓存 |
| **InfluxDB** | 时序数据库 | 历史趋势分析 |
| **规则引擎** | 业务规则 | MVP 阶段 |

### 前端 (frontend/)
| 技术 | 用途 | 备注 |
|-----|------|------|
| **React 18** | UI 框架 | TypeScript |
| **Recharts** | 数据可视化 | 雷达图/趋势图 |
| **TanStack Query** | 数据管理 | 缓存 |
| **Tailwind CSS** | 样式框架 | 响应式 |

## 🚀 快速开始

### 后端启动

```bash
cd backend

# 配置环境变量
cp .env.example .env

# 开发模式
cargo run

# 生产构建
cargo build --release
```

### 前端启动

```bash
cd frontend

npm install
npm run dev
```

## 📦 项目结构

```
hamr-jiabu/
├── backend/
│   ├── src/
│   │   ├── analysis/         # 数据分析
│   │   ├── scoring/          # 评分算法
│   │   ├── rules/            # 规则引擎
│   │   ├── ml/               # 机器学习 (Phase 3)
│   │   └── reports/          # 报告生成
│   ├── Cargo.toml
│   └── .env.example
├── frontend/
│   ├── src/
│   │   ├── pages/
│   │   │   ├── Dashboard.tsx   # 幸福指数看板
│   │   │   ├── Insights.tsx    # 智能建议
│   │   │   ├── Decisions.tsx   # 决策辅助
│   │   │   └── Reports.tsx     # 幸福报告
│   │   ├── components/
│   │   └── charts/
│   └── package.json
└── README.md
```

## 📊 评分算法

### 幸福指数计算

```rust
// 五维得分
let person_score = calculate_person_score(family_data);
let time_score = calculate_time_score(family_data);
let task_score = calculate_task_score(family_data);
let item_score = calculate_item_score(family_data);
let environment_score = calculate_environment_score(family_data);

// 加权平均
let happiness_score = 
    person_score * 0.25 +
    time_score * 0.20 +
    task_score * 0.20 +
    item_score * 0.15 +
    environment_score * 0.20;
```

### 人际互动得分

```rust
fn calculate_person_score(data: &FamilyData) -> f64 {
    // 全家活动频率
    let family_activity_freq = data.family_events_last_30_days / 30.0;
    
    // 一对一互动
    let one_on_one_freq = data.one_on_one_interactions / 30.0;
    
    // 关系质量（基于互动质量评分）
    let relationship_quality = data.avg_interaction_rating;
    
    // 综合得分
    (family_activity_freq * 0.4 + 
     one_on_one_freq * 0.3 + 
     relationship_quality * 0.3) * 100.0
}
```

## 🔌 API 端点

```
GET    /api/insights/happiness-score    # 幸福指数
GET    /api/insights/trends              # 趋势分析
GET    /api/insights/suggestions         # 智能建议
POST   /api/decisions/simulate           # 决策模拟
GET    /api/reports/weekly               # 周报
GET    /api/reports/monthly              # 月报
GET    /api/reports/yearly               # 年报
```

## 📊 里程碑

- [ ] **2026-06-15**: 需求确认
- [ ] **2026-07-15**: 评分模型开发
- [ ] **2026-08-05**: 后端开发
- [ ] **2026-08-20**: 前端开发
- [ ] **2026-08-30**: 测试上线

## 🔒 隐私保护

- **本地计算优先**: 评分和分析在本地完成
- **数据不上传**: 除非用户主动分享报告
- **透明算法**: 评分逻辑完全开源
- **用户可控**: 可关闭任意分析维度

## 🔗 相关服务

- [HamR 管家](https://app.hamr.store) - 数据来源
- [技术文档](https://docs.hamr.top) - API 文档

## 📄 许可证

MIT License

---

**最后更新**: 2026-03-05  
**部署环境**: https://jiabu.hamr.store (Phase 2 上线)
