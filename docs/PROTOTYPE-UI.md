# BBLBB 高保真原型 · 设计 Token 与组件系统规格

本规格对应需求文档第二、五、七节，与《BBLBB 产品信息架构与页面流程规格》配套使用。所有数值均为最终值，原型师可直接翻译为 CSS Variables 与组件样式，无需再自行决策。配色以需求给定基色为准，延伸色（hover / pressed / 浅底）由其派生并在此固定。原型主展示亮色模式，暗色 Token 一并给出。

# 一、风格红线（实现时必须遵守）

- 不做巨大渐变 Hero：首页直接进入内容区，社区介绍压缩为一行文字 + 发布按钮。

- 不用玻璃拟态、不用大阴影：全站仅下拉菜单 / 弹层 / Toast / Modal / Header 允许使用一级轻阴影，其余一律 1px 边框分隔。

- 圆角收敛：卡片与容器 4px，控件 3px，最大不超过 6px；Tag / Category Badge / 头像保持方角或微圆角，**不再使用全圆角**。

- 图标只用 Lucide 线性图标（stroke-width 2，尺寸 16/20 两档），禁止用 Emoji 代替功能图标。

- 信息密度：论坛列表行高 60–68px，**采用 Discourse 式对齐表格**（主题｜参与者｜回复｜浏览｜活动五列），禁止大留白卡片流。

- 满足 WCAG 2.2 AA：正文对比度 ≥ 4.5:1，所有交互元素有可见 focus 环，全流程可键盘操作。

# 二、设计 Token

## 2.1 色彩 Token（亮色）

> 配色采用 Discourse 默认浅色系的低饱和、中性色风格：页面背景与卡片靠轻微明度差 + 1px 边框分隔，主色用 `#0088CC`（Discourse 默认 tertiary 蓝），错误与警告用与之搭配的橙色。

| Token 名（CSS 变量） | 色值 | 用途 |
|---|---|---|
| --color-bg-page | #F5F5F5 | 页面背景 |
| --color-bg-card | #FFFFFF | 卡片 / 导航 / 列表项背景 |
| --color-bg-subtle | #F0F0F0 | hover 浅底、Tab 选中底、表头 |
| --color-text-primary | #222222 | 标题、正文 |
| --color-text-secondary | #646464 | 摘要、元信息、辅助说明 |
| --color-text-tertiary | #919191 | 占位符、禁用文字、时间戳弱化场景 |
| --color-brand | #0088CC | 主按钮、链接、选中态、品牌标识 |
| --color-brand-hover | #0072AA | 主按钮 hover |
| --color-brand-pressed | #005C88 | 主按钮按下 |
| --color-brand-soft | #E5F3FA | 品牌浅底（Tag hover、焦点底） |
| --color-border | #E9E9E9 | 卡片边框、分割线、表格线 |
| --color-border-strong | #D5D5D5 | 表头下边框、输入框常态边框 |
| --color-success | #009900 | 成功文字 / 图标、置顶标签 |
| --color-success-soft | #E5F2E5 | 成功浅底 |
| --color-warning | #E9A100 | 警告文字 / 图标、精华标签 |
| --color-warning-soft | #FBF3E0 | 警告浅底 |
| --color-danger | #E45735 | 危险按钮、错误文字、封禁标识 |
| --color-danger-soft | #FBEAE6 | 危险浅底、错误提示底 |
| --color-code-bg | #F1F1F1 | 行内代码、代码块背景 |
| --color-focus-ring | rgba(0,136,204,0.35) | focus 外发光环 |
| --color-visited | #6E6E6E | 已访问题目（Discourse 列表特性） |
| --color-highlight-soft | #FFFFCC | 引用高亮底（备用） |

## 2.2 色彩 Token（暗色映射）

暗色模式仅替换变量值，组件结构不变。品牌色提亮以保证深色底上对比度达标。

| Token 名 | 暗色值 | 说明 |
|---|---|---|
| --color-bg-page | #1E1E1E | 页面背景 |
| --color-bg-card | #262626 | 卡片背景 |
| --color-bg-subtle | #303030 | hover 浅底 / Tab 选中底 |
| --color-text-primary | #E8E8E8 | 主文字 |
| --color-text-secondary | #A5A5A5 | 次要文字 |
| --color-text-tertiary | #7C7C7C | 第三级文字 |
| --color-brand | #48B0DD | 品牌色（提亮） |
| --color-brand-hover | #6BC2E8 | hover |
| --color-brand-pressed | #3A96BE | pressed |
| --color-brand-soft | #1C3A48 | 品牌浅底 |
| --color-border | #3A3A3A | 卡片边框、分割线 |
| --color-border-strong | #4E4E4E | 表头下边框、输入框 |
| --color-success | #4CB74C | 成功色提亮 |
| --color-success-soft | #1C2E1C | 成功浅底 |
| --color-warning | #E9A100 | 警告色 |
| --color-warning-soft | #362A0F | 警告浅底 |
| --color-danger | #EF7A5C | 危险色提亮 |
| --color-danger-soft | #3A211B | 危险浅底 |
| --color-code-bg | #2D2D2D | 代码块背景 |
| --color-focus-ring | rgba(72,176,221,0.45) | focus 环 |
| --color-visited | #8A8A8A | 已访问题目 |
| --color-highlight-soft | #4A4A2A | 引用高亮底 |

## 2.3 字体与字号阶梯

| Token 名 | 值 | 用途 |
|---|---|---|
| --font-family-base | -apple-system, "PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif | 全站正文与界面 |
| --font-family-mono | "SF Mono", "JetBrains Mono", Consolas, monospace | 代码、Client ID、数值流水号 |
| --text-xs | 12px / 行高 18px | Badge、Tag、辅助标注、列表次级列 |
| --text-sm | 13px / 行高 20px | 列表元信息、摘要、侧栏内容、按钮小号 |
| --text-base | 14px / 行高 22px | 界面正文、表单、按钮默认 |
| --text-md | 16px / 行高 26px | 列表主标题、文章正文（阅读字号） |
| --text-lg | 18px / 行高 28px | 详情页 H2、卡片组标题 |
| --text-xl | 20px / 行高 30px | 页面标题（板块页、后台页） |
| --text-2xl | 24px / 行高 34px | 文章 / 讨论详情主标题 |
| --text-3xl | 28px / 行高 38px | 仅用户主页昵称、仪表盘大数字 |
|\-\-weight\-regular / medium / semibold|400 / 500 / 600|正文 / 列表标题与按钮 / 页面标题与强调|

## 2\.4 间距阶梯（4px 基准）

|Token|值|典型用途|
|---|---|---|
|\-\-space\-1 / 2 / 3|4 / 8 / 12px|图标与文字间距、Badge 内边距、列表行内元素间距|
| --space-4 / 5 / 6 | 16 / 20 / 24px | 卡片内边距（16px）、区块间距、表单字段间距 |
| --space-8 / 10 / 12 / 16 | 32 / 40 / 48 / 64px | 页面大区块间距、详情页标题区下边距 |

## 2.5 圆角、边框、阴影、动效、层级

| Token | 值 | 用途 |
|---|---|---|
| --radius-sm | 3px | 控件（输入框、按钮、Tab、Tag、Category Badge） |
| --radius-md | 4px | 卡片、面板、列表容器（默认方角） |
| --radius-lg | 6px | Modal、抽屉、浮层 |
| --radius-xl | 8px | 极少使用，预留大尺寸容器 |
| --border-default | 1px solid var(--color-border) | 卡片、表格、分割线 |
| --border-input | 1px solid var(--color-border-strong) | 输入类控件常态 |
| --shadow-pop | 0 2px 6px rgba(0,0,0,0.12) | 仅下拉菜单、Popover、Toast |
| --shadow-modal | 0 4px 14px rgba(0,0,0,0.18) | 仅 Modal、ConfirmDialog、抽屉 |
| --shadow-header | 0 1px 2px rgba(0,0,0,0.08) | 顶部导航阴影（替代 1px 下边框） |
| --duration-fast / base | 120ms / 200ms，ease-out | hover 过渡 / 弹层与抽屉动画 |
| --z-dropdown / sticky / drawer / modal / toast | 100 / 200 / 300 / 400 / 500 | 层级阶梯，禁止自定义其他值 |

## 2\.6 排版细节（文章正文 \.prose）

- 正文 16px / 行高 26px，段落间距 16px；H2 20px 上 32px 下 12px，H3 16px 上 24px 下 8px。

- 引用块：左侧 3px 品牌色竖条 \+ \-\-color\-bg\-subtle 浅底，内边距 12px 16px。

- 行内代码：mono 13px，\-\-color\-code\-bg 底，4px 圆角，2px 6px 内边距。

- 提示框（info / warning / danger 三型）：对应语义色浅底 \+ 同色系 1px 左边框（4px），8px 圆角。

- 表格：表头 \-\-color\-bg\-subtle 底，行高 40px，1px 边框，单元格内边距 8px 12px。

- 图片：最大宽度 100%，8px 圆角，1px 边框，图注 12px 次要文字居中。

# 三、布局与响应式规格

|项目|桌面（1440 基准）|手机（390 基准）|
|---|---|---|
|断点|≥1024px 桌面布局；1200px 容器居中|\<768px 单列；768–1023px 单列 \+ 侧栏下移|
|页面容器|max\-width 1200px，左右 padding 24px|左右 padding 16px|
|内容 \+ 侧栏|内容 flex:1（max 840px）\+ 侧栏 300px（区间 280–320），间距 24px|侧栏内容折叠进抽屉或移至页面底部卡片|
|阅读正文|详情页正文列 max\-width 740px（区间 720–760）|全宽|
|顶部导航|高 60px，sticky，白底 + 浅阴影（替代 1px 下边框）|高 52px，汉堡菜单开抽屉（宽 280px）|
|后台布局|左侧导航 220px 固定 \+ 内容区 max 1080px|左侧导航收进抽屉，顶部显示当前模块名|
|表格|DataTable 原生表格|转卡片列表（每行一卡，主字段 \+ 次要字段两行）或横向滚动容器|
|列表元信息|一行展示全部统计|精简为浏览 / 回复两项，其余收进详情|
|回复操作|行尾平铺 点赞 / 引用 / 回复 / 举报|收进「更多」（Lucide more\-horizontal）弹出菜单|
|发布按钮|导航右侧 primary 按钮|右下角 56px 悬浮圆形按钮（Lucide pen\-line）|

# 四、组件系统规格

所有组件统一：focus 态 = 2px 品牌色描边或 \-\-color\-focus\-ring 外环；disabled = 透明度 0\.5 \+ 禁止指针；loading = 内置 16px spinner（Lucide loader\-2 旋转）替换原图标，宽度保持不抖动。

## 4\.1 Button（四型 × 三尺寸，六态全定义）

|变体|normal|hover|pressed|disabled / loading / error|
|---|---|---|---|---|
|primary|底 \-\-color\-brand，白字|底 \-\-color\-brand\-hover|底 \-\-color\-brand\-pressed|disabled 底不变透明度 0\.5；loading spinner 白；error 配合表单 Shake 120ms|
|secondary|白底，\-\-border\-input 边框，主文字|底 \-\-color\-bg\-subtle|底 \-\-color\-border|同左规则|
|ghost|无底无边框，次要文字|底 \-\-color\-bg\-subtle，主文字|底 \-\-color\-border|同左规则|
|danger|底 \-\-color\-danger，白字|底 \#B91F1F|底 \#9E1B1B|同左规则；破坏性操作必须搭配 ConfirmDialog|

尺寸：sm 高 28px（13px 字，padding 0 10px）/ md 高 36px（14px 字，padding 0 14px）/ lg 高 44px（16px 字，padding 0 20px）。圆角 6px。带图标时图标 16px 与文字间距 6px。图标按钮（纯 icon）36×36px、圆角 6px、ghost 样式。

## 4\.2 表单控件（Input / Select / Textarea / Checkbox / Switch）

|组件|结构与尺寸|六态说明|
|---|---|---|
|Input|高 36px，padding 0 12px，14px 字，3px 圆角；可带左图标 / 前缀|normal 边框 --border-input；hover 边框 #919191；focus 品牌色边框 + focus 环；disabled 灰底；error 边框 --color-danger + 下方 12px 红字提示；loading 右侧 spinner|
|Select|同 Input 尺寸，右侧 chevron\-down；下拉面板白底 8px 圆角 \-\-shadow\-pop，选项高 32px，选中项 \-\-color\-brand\-soft 底|同 Input；键盘上下选择、Enter 确认、Esc 关闭|
|Textarea|最小高 96px，padding 10px 12px，可拖拽右下角调高|同 Input|
|Checkbox|16×16px，2px 圆角，选中品牌底 \+ 白色 check 图标|hover 边框加深；focus 环；disabled 灰底；indeterminate 横杠态用于全选|
|Switch|36×20px 全圆角，滑块 16px；开启品牌底|切换 200ms 过渡；loading 时滑块内 spinner（用于启停插件等异步开关）|

## 4\.3 导航与指示（Tabs / Breadcrumb / Pagination / Tag / StatusBadge）

|组件|结构与尺寸|状态说明|
|---|---|---|
|Tabs|下划线式：项高 40px，14px 字，间距 24px；选中项品牌色 2px 下划线 \+ 主文字，未选中次要文字|hover 文字转主文字色；可带数量徽标（12px 灰底圆角 999px）；手机端可横向滚动|
|Breadcrumb|13px 次要文字，分隔符 Lucide chevron\-right 14px，末级主文字不可点|hover 品牌色；层级超过 3 级中间折叠为「…」|
|Pagination|页码按钮 32×32px 6px 圆角，当前页品牌底白字；首尾 \+ 省略号；旁附「共 N 页」13px 次要文字|hover 浅底；边界页禁用箭头；板块页与后台列表统一使用，移动端替换为「加载更多」按钮|
|Tag|高 22px，12px 字，padding 0 8px，全圆角，\-\-color\-bg\-subtle 底 \+ 次要文字；可带 14px hash 图标|hover 品牌浅底 \+ 品牌字；选中态品牌底白字（发布页选标签）；可删除型右侧带 x 图标|
|StatusBadge|高 20px，12px 字，padding 0 6px，4px 圆角，语义浅底 \+ 同色系文字|枚举固定：置顶（品牌）、精华（warning）、回复可见（次要灰）、待审核（warning）、已处理（success）、已驳回 / 封禁（danger）、草稿（灰）、已锁定（danger）|

## 4\.4 身份与等级（Avatar / UserBadge / RoleBadge / LevelBadge / LevelProgress / CurrencyBalance）

|组件|结构与尺寸|说明|
|---|---|---|
|Avatar|24 / 32 / 40 / 64 / 96px 五档，全圆角，1px 边框；无图时昵称首字符 \+ 由用户名哈希出的 8 色之一浅底|可叠 8px 在线绿点（右下，2px 白描边）|
|UserBadge|Avatar 24px \+ 昵称 13px 的组合单元，hover 昵称品牌色|列表、楼层、通知统一复用|
|RoleBadge|高 20px 12px 字 4px 圆角：版主（品牌浅底）、管理员（danger 浅底）、社区成员（灰浅底）|跟随昵称右侧出现|
|LevelBadge|高 20px，12px 字 mono，「LV\.6」格式，等级色由等级管理配置（LV\.1–3 灰、LV\.4–5 品牌、LV\.6–7 warning 橙、LV\.8\+ danger 红），浅底 \+ 同色字|所有出现用户名的位置跟随展示|
|LevelProgress|进度条高 6px 全圆角，轨道 \-\-color\-border，填充品牌渐变纯色 \-\-color\-brand；上方 12px 文字「LV\.6 · 2680 / 3000 经验」|用于用户主页、侧栏积分摘要|
|CurrencyBalance|三项横排：Lucide 图标 16px（经验 star / B币 coins / 贡献 award）\+ 数值 16px semibold \+ 名称 12px 次要文字，项间距 20px|用户主页三账户、解锁弹窗内余额展示复用；数值变动时 200ms 数字滚动动画|

## 4\.5 内容卡片（PostCard / ArticleCard / BoardCard）

|组件|结构（自上而下）|尺寸与状态|
|---|---|---|
|PostCard（讨论列表项，Discourse 式对齐表格）|五列：① 主列 = 标题 16px regular（最多两行，超出截断）+ 第二行 CategoryBadge（彩色方块 + 板块名）+ Tag×≤3；② 参与者列 = 作者头像 + 最近回复者头像（最多 3 个 24px 圆形，叠放）；③ 回复列 = 数字 14px（≥20 切主色加粗）；④ 浏览列 = 数字 14px（紧凑格式 1.2k / 3.4k）；⑤ 活动列 = 最后回复相对时间 12px 次要文字|行高 62–68px，1px 下边框（无圆角、无阴影）；hover 整行换 --color-bg-subtle；表格上方有列头行（主题｜参与者｜回复｜浏览｜活动），列头 12px 次要文字高 36px + 1px 强下边框；置顶行标题前加置顶 Badge（成功色描边）、精华行加精华 Badge（警告色描边）；已访问题目标题用 --color-visited；移动端 ≤640px 收起列头与数值列、≤900px 收起参与者列|
|ArticleCard（精选文章）|封面 16:9（无封面时品牌浅底 \+ 标题首字）在上；类型 Badge「文章」\+ 标题 16px semibold（两行截断）；摘要 13px 两行；底部 UserBadge \+ 时间 \+ 阅读数|首页三列网格（间距 16px），卡高固定；hover 边框加深 \+ 标题品牌色，无位移无阴影|
|BoardCard（活跃板块）|板块图标（Lucide，32px 品牌浅底圆角方块）\+ 板块名 14px semibold \+ 说明 13px 两行截断；底部「帖子 N · 今日 N」12px 次要文字|首页 / 板块页网格复用；hover 同 PostCard 规则|

## 4\.6 编辑与受限内容（MarkdownEditor / CodeBlock / RestrictedContentCard）

|组件|结构|规格与状态|
|---|---|---|
|MarkdownEditor|工具栏（高 40px，图标按钮 28×28px：粗体 / 斜体 / 标题 / 引用 / 代码 / 链接 / 图片 / 列表 / 表格）\+ 编辑区 \+ 「编辑 / 预览」分段切换；底部状态栏 28px：字数统计 \+ 草稿保存状态（「草稿已保存 12:03」12px 次要文字）|编辑区 mono 14px / 行高 24px；预览区渲染 \.prose 排版；发布页支持左右分栏（编辑 \| 预览各 50%）；工具栏按钮 hover 浅底、激活品牌浅底|
|CodeBlock|头部条 32px：左语言标识 12px mono 次要文字，右「复制」ghost 按钮（Lucide copy 14px）；代码区 mono 13px / 行高 22px，\-\-color\-code\-bg 底，左右 padding 16px|8px 圆角 1px 边框；复制成功按钮变 check 图标 \+ 「已复制」1200ms 后还原；超 20 行折叠并显示「展开全部」|
|RestrictedContentCard|锁定卡：高自适应，\-\-color\-bg\-subtle 底 8px 圆角 1px 虚线边框；中心 Lucide lock 24px 图标 \+ 条件文案 14px \+ 操作按钮|三型固定：①「回复本主题后可见」→ primary「回复后查看」；②「达到 LV\.5 或回复后可见」→ 文案带当前等级提示 \+ primary 按钮；③「支付 10 B币永久解锁」→ 显示 CurrencyBalance 当前余额 \+ primary「支付 10 B币解锁」（余额不足时按钮 disabled \+ 红字提示）。受限正文绝不渲染进 DOM|

## 4\.7 通知与审核（NotificationItem / ReportCard / ModerationTimeline）

|组件|结构|规格与状态|
|---|---|---|
|NotificationItem|左类型图标 32px 浅底圆角方块（回复 message\-square / 点赞 heart / 系统 bell）\+ 内容两行（首行 13px 主文字、次行 12px 次要时间与来源）\+ 右侧未读 6px 品牌色圆点|行高 64px；未读整行 \-\-color\-brand\-soft 浅底；hover 浅底加深；整行可点跳转来源|
|ReportCard（举报列表项）|首行：举报单号（mono 13px）\+ 原因 StatusBadge \+ 优先级 StatusBadge（高 danger / 中 warning / 低 灰）\+ 状态；次行：被举报内容摘要两行截断（引用块样式）；末行：举报人 · 被举报人 · 板块 · 时间 \+ 右侧负责人 Avatar 与「处理」secondary 按钮|白卡 8px 圆角；hover 边框加深；已处理 / 已驳回行整体透明度 0\.7|
|ModerationTimeline|竖向时间线：左侧 2px 竖线 \+ 节点 8px 圆点（按动作语义着色）；每节点：动作文字 13px（操作人 \+ 动作 \+ 对象）\+ 时间 12px 次要文字 \+ 附言（处理原因，引用块样式）|动作枚举：提交举报（灰）、受理（品牌）、隐藏 / 删除（danger）、恢复（success）、警告（warning）、禁言（warning）、封禁（danger）、申诉 / 申诉结果（品牌）。用于举报详情与处罚记录页|

## 4\.8 后台通用（DataTable / FilterBar / EmptyState）

|组件|结构|规格与状态|
|---|---|---|
|DataTable|表头：\-\-color\-bg\-subtle 底，13px medium 次要文字，行高 40px，可排序列带 sort 图标；数据行高 48px，14px，行间 1px 边框；首列可带 Checkbox；末列操作区 ghost 图标按钮|整表 8px 圆角 1px 边框白卡；行 hover \-\-color\-bg\-subtle；选中行 \-\-color\-brand\-soft；空数据时表格区域渲染 EmptyState；手机端转卡片（首字段为标题行，次字段两行，操作收更多菜单）|
|FilterBar|横排：搜索 Input（宽 220px 带 search 图标）\+ 若干 Select（宽 140px）\+ 右侧「重置」ghost 按钮；高 36px，间距 8px|空间不足自动换行；手机端收进「筛选」按钮弹出的抽屉面板|
|EmptyState|居中：Lucide 图标 40px（次要色 50% 透明度）\+ 标题 14px medium \+ 说明 13px 次要文字 \+ 可选 primary 按钮|枚举文案固定：无帖子「还没有内容，来发第一帖」；无通知「暂无新通知」；无举报「队列已清空」；无搜索结果「没有找到相关内容，换个关键词试试」|

## 4\.9 反馈与状态（Modal / ConfirmDialog / Toast / Skeleton / 状态页）

|组件|结构|规格与状态|
|---|---|---|
|Modal|遮罩 rgba\(15,19,26,0\.45\) \+ 面板：宽 480px（表单类）/ 640px（内容类），10px 圆角，\-\-shadow\-modal；头部 16px semibold 标题 \+ 关闭图标按钮；内容区 padding 20px；底部右对齐按钮组（间距 8px）|打开 200ms 缩放 0\.96→1 淡入；Esc 关闭；遮罩点击关闭（破坏性内容除外）；焦点锁定在面板内|
|ConfirmDialog|Modal 变体，宽 400px；语义图标 20px（warning 橙 / danger 红）\+ 标题 16px \+ 说明 14px；按钮组：secondary 取消 \+ danger/primary 确认|二次确认型（积分调整、重置 Secret、封禁）：确认按钮 3 秒倒计时后可点，或要求输入指定文本（如用户名）激活；不可逆操作必须在说明中明示后果（如「历史流水不可删除，仅可创建补偿记录」）|
|Toast|顶部居中浮层：高 40px，白底 1px 边框 8px 圆角 \-\-shadow\-pop，左语义图标 16px \+ 13px 文字，宽度自适应内容|success / warning / danger / info 四型对应语义色图标；停留 3000ms 上滑淡出；多条纵向堆叠间距 8px，最多 3 条|
|Skeleton|占位块：\-\-color\-bg\-subtle 底 4px 圆角，1200ms 呼吸透明度动画（1→0\.5→1）|列表页按 PostCard 形状渲染 5 行骨架；详情页标题条 \+ 3 段文字条 \+ 侧栏卡骨架；禁止用整页 spinner 替代|
|状态页（Loading / Error / 403 / 404 / 429）|页面级居中：大图标 48px \+ 状态码 28px semibold（错误类）\+ 标题 16px \+ 说明 14px 次要文字 \+ 操作按钮|403「没有访问权限」→ secondary「返回首页」\+ ghost「登录」；404「页面不存在」→ primary「返回首页」；429「操作过于频繁，请稍后再试」→ 显示倒计时秒数，结束自动恢复；Error「出错了」→ primary「重试」。触发条件见产品规格第 5 节|

# 五、图标与无障碍

- 图标库 Lucide，stroke\-width 2；导航与操作 16px，空态与状态页 40/48px，列表元信息 14px。图标与文字基线对齐，间距 6px。

- 所有可点元素最小命中区 32×32px（手机端 44×44px，视觉尺寸可小、热区不可小）。

- focus 顺序与视觉顺序一致；Modal / 抽屉打开时焦点进入内部，关闭后还原到触发元素。

- 正文 #222222 对 #FFFFFF 对比度 15.9:1，次要文字 #646464 对 #FFFFFF 为 5.9:1（满足 AA ≥4.5:1），品牌色 #0088CC 对白底 3.9:1 仅用于大号文字 / 图标 / 控件底色，不用于 14px 以下正文。

- 暗色模式经 html\.dark 类切换，所有组件只引用变量、不写死色值。

落地方式：原型师将本文 Token 表翻译为 :root 与 html\.dark 两组 CSS Variables（变量名即表中 Token 名），组件样式全部引用变量。组件六态、尺寸、文案枚举均已定死，实现时不需要再猜任何样式决策。

