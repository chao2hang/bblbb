# BBLBB 原型规格 ①：信息架构与页面流程

本规格对应需求文档第三、四、六节，是原型实现（SvelteKit \+ TypeScript）的信息架构与交互跳转唯一依据：全部路由、每个可点击元素的跳转目标、9 条演示流程的逐步路径、各类状态的触发条件均已定死，实现时不需要再猜任何跳转目标。设计 Token 与组件系统规格见 UI 设计师的配套文档，两份文档对照使用。

# 一、全局信息架构

## 1\.1 桌面端顶部导航（全站所有前台页面共用）

|位置|元素|形态|点击目标 / 行为|
|---|---|---|---|
|最左|BBLBB 文字 Logo|文本 Logo|/（首页）|
|左侧导航项|首页|NavLink|/|
|左侧导航项|文章|NavLink|/articles|
|左侧导航项|板块|NavLink|/boards|
|左侧导航项|标签|NavLink|/tags|
|中部|搜索框|Input \+ 搜索图标|回车或点图标 → /search?q=关键词|
|右侧|发布按钮|Button primary|/publish|
|右侧|通知铃铛|图标 \+ 未读角标|/notifications|
|最右|当前用户头像|Avatar|展开头像菜单（见 1\.2）|

## 1\.2 头像菜单（登录态下拉）

|菜单项|目标|说明|
|---|---|---|
|我的主页|/users/Chaos|跳当前用户主页|
|我的积分|/users/Chaos?tab=points|个人主页「积分明细」tab，仅本人可见|
|收藏|/favorites|收藏列表页|
|通知|/notifications|同导航铃铛|
|账号设置|/settings|默认 profile tab|
|OAuth 授权管理|/settings?tab=oauth|已授权应用列表|
|退出登录|ConfirmDialog → /login|二次确认后清登录态|

## 1\.3 移动端（390px）导航与抽屉

- 顶栏保留四个入口：BBLBB Logo（→ /）、搜索图标（点击展开全宽搜索框，提交 → /search?q=关键词）、发布按钮（→ /publish）、用户头像（点击打开抽屉）。

- 抽屉菜单上半部分：首页 /、文章 /articles、板块 /boards、标签 /tags；分割线后：我的主页 /users/Chaos、收藏 /favorites、通知 /notifications、账号设置 /settings、OAuth 授权管理 /settings?tab=oauth、退出登录（ConfirmDialog → /login）。未登录态显示「登录 /login」「注册 /register」两个按钮。

- 桌面端右侧栏内容（社区动态、板块规则、版主列表等）在移动端折叠到对应页面底部，顺序排在主内容之后。

- 回复的点赞 / 引用 / 举报等操作在移动端收进每条楼层的「更多」菜单。

## 1\.4 管理后台左侧导航（/admin 下所有页面共用）

|导航项|路由|导航项|路由|
|---|---|---|---|
|仪表盘|/admin|等级管理|/admin/levels|
|用户管理|/admin/users|附件管理|/admin/attachments|
|文件存储|/admin/storage|角色与权限|/admin/roles|
|通知与邮件|/admin/notifications|主题管理|/admin/themes|
|板块管理|/admin/boards|插件管理|/admin/plugins|
|帖子与回复|/admin/posts|OAuth 客户端|/admin/oauth|
|举报与审核|/admin/reports|审计日志|/admin/audit|
|标签管理|/admin/tags|系统设置|/admin/settings|
|积分与货币|/admin/points|—|—|

高保真重点页为：仪表盘、文件存储、举报与审核（含详情）、积分与货币、等级管理、主题管理、插件管理、OAuth 客户端；其余导航项做列表占位页（DataTable \+ FilterBar \+ 少量 Mock 行）即可，但导航必须全部可点。

## 1\.5 全站路由表

|路由|页面|备注|
|---|---|---|
|/|首页|无营销 Hero|
|/articles|文章列表页|顶部导航「文章」，仅文章类型 PostCard 列表|
|/boards|板块总览页|顶部导航「板块」，BoardCard 网格|
|/boards/\[slug\]|板块页|slug：tech\-essay、rust、web\-dev、opensource、chat、meta|
|/tags|标签总览页|顶部导航「标签」，Tag 列表 \+ 使用次数|
|/tags/\[name\]|标签聚合页|该标签下文章与讨论列表|
|/topics/\[id\]|文章/讨论详情页|统一路由，按数据 type 区分文章与讨论；示例 id：101、201、202|
|/publish|发布与编辑页|?type=article 或 ?type=topic，可选 ?board=slug 预选板块|
|/users/\[name\]|用户主页|示例 /users/Chaos；本人视图多出管理入口|
|/login|登录页|含 5 种演示状态|
|/register|注册页|含邮箱验证状态|
|/forgot\-password|忘记密码页|简单表单 \+ 提交成功态|
|/search|搜索结果页|?q=关键词；无结果走空状态|
|/notifications|通知页|NotificationItem 列表，含未读态|
|/favorites|收藏页|收藏的文章/讨论列表|
|/settings|账号设置页|?tab=profile / security / devices / notifications / oauth；登录设备在 ?tab=devices|
|/admin|管理后台仪表盘|左侧导航布局|
|/admin/reports|举报与审核列表|4 个状态 tab \+ 筛选|
|/admin/reports/\[id\]|举报处理详情页|示例 /admin/reports/R\-1024|
|/admin/points|积分与货币管理|含调整积分流程|
|/admin/levels|等级管理|含晋升路径可视化|
|/admin/themes|主题管理|含预览与切换流程|
|/admin/plugins|插件管理|配置型 / 预编译 UI 插件分区|
|/admin/oauth|OAuth 客户端管理|含创建客户端流程|
|/admin/users、/admin/roles、/admin/boards、/admin/posts、/admin/tags、/admin/attachments、/admin/notifications、/admin/audit、/admin/settings|后台占位列表页|DataTable \+ FilterBar \+ 3–5 行 Mock 数据|
|/admin/storage|文件存储配置|本地/S3 后端切换、连接测试、脱敏凭证、上传限制与迁移提示|
|/403|403 页|无权限访问时渲染|
|任意未匹配路由|404 页|catch\-all|

# 二、页面清单与跳转关系（11 类核心页面）

## 2\.1 首页（/）

**区块自上而下：**顶部导航；社区简介条（一句话介绍 \+ 「发布内容」按钮）；精选文章区（ArticleCard × 2）；主列最新讨论列表（PostCard × 6–8）；主列底部活跃板块（BoardCard × 6）与热门标签（Tag 云）；右侧栏：当前用户等级与积分摘要卡片、社区动态。

|可点击元素|目标|
|---|---|
|「发布内容」按钮|/publish|
|精选文章卡片（标题/封面）|/topics/101|
|讨论项标题《使用 SvelteKit 构建博客与轻量论坛是否合理？》|/topics/201|
|讨论项标题《OAuth Provider 应该自研到什么程度？》|/topics/202|
|帖子项作者头像/昵称|/users/\{作者名\}|
|帖子项板块名|/boards/\{slug\}|
|帖子项标签|/tags/\{标签名\}|
|BoardCard（技术随笔、Rust、Web 开发、开源项目、闲聊、站务）|/boards/tech\-essay、/boards/rust、/boards/web\-dev、/boards/opensource、/boards/chat、/boards/meta|
|热门标签（Rust、SvelteKit、SQLite、OAuth、自托管、性能优化）|/tags/\{标签名\}|
|用户摘要卡片「查看主页」|/users/Chaos|
|社区动态条目（如「XX 回复了你的帖子」）|对应 /topics/\[id\]|

## 2\.2 板块页（/boards/\[slug\]）

**区块：**面包屑；板块头（名称、说明、规则摘要、版主头像组、「关注板块」「发布讨论」按钮）；Tabs（最新 / 热门 / 精华 / 未回复）；FilterBar（标签 Select、作者 Input、时间 Select）；PostCard 紧凑列表；底部分页 Pagination；右侧栏：子板块、版主列表、热门标签、板块规则。

|可点击元素|目标 / 行为|
|---|---|
|面包屑「首页」/「板块」|/ 、/boards|
|「关注板块」按钮|原地切换为「已关注」\+ Toast；再次点击取消关注|
|「发布讨论」按钮|/publish?type=topic\&board=\{slug\}|
|Tabs 最新/热门/精华/未回复|原地切换列表数据（Mock 过滤），未回复 tab 可能为空状态|
|筛选器（标签/作者/时间）|原地过滤列表；无匹配时空状态 \+ 「清除筛选」按钮|
|帖子项标题|/topics/\{id\}（Rust 板块示例帖 → /topics/201）|
|帖子项作者 / 标签|/users/\{name\} 、/tags/\{name\}|
|版主头像|/users/\{版主名\}|
|分页页码 / 下一页|原地换页（Mock 数据翻页）|
|侧栏子板块、热门标签|/boards/\{slug\} 、/tags/\{name\}|

## 2\.3 文章/讨论详情页（/topics/\[id\]）

**区块：**面包屑；标题区（类型徽章、标题、作者信息含等级与角色徽章、发布/更新时间、阅读量、板块、标签、收藏/点赞/分享/举报按钮）；Markdown 正文（720–760px 宽，代码块带语言标识与复制按钮）；受限内容卡（RestrictedContentCard，正文内容绝不进 DOM）；回复区（总数、排序 Tabs、楼层列表）；底部 Markdown 回复编辑器。

|可点击元素|目标 / 行为|
|---|---|
|面包屑「首页」「板块名」|/ 、/boards/\{slug\}|
|作者头像/昵称|/users/\{name\}|
|板块徽章、标签|/boards/\{slug\} 、/tags/\{name\}|
|收藏按钮|切换已收藏态 \+ Toast「已加入收藏」，数据出现在 /favorites|
|点赞按钮|点赞数 \+1，按钮高亮|
|分享按钮|Modal 展示链接 \+ 「复制链接」→ Toast「链接已复制」|
|举报按钮|Modal：原因单选 \+ 说明 Textarea \+ 提交 → Toast「已提交举报，等待处理」|
|受限卡「去回复」按钮（回复可见/LV\.5 可见）|页内锚点滚动到底部回复编辑器并聚焦|
|受限卡「立即解锁」按钮（支付 10 B币）|ConfirmDialog：显示当前余额 328 B币、本次扣除 10 → 确认后余额变 318、受限卡解锁、Toast「解锁成功」|
|回复排序 Tabs（最新/最早/只看作者）|原地重排楼层|
|楼层点赞 / 引用 / 回复|点赞 \+1；引用与回复把引用块填入底部编辑器并锚点聚焦|
|楼层举报|同帖子举报 Modal|
|版主操作（隐藏/删除/编辑/禁言，仅当前用户对 rust 板块帖子可见）|均弹 ConfirmDialog → Toast 演示；「禁言」弹 Modal（时长 Select \+ 原因）|
|代码块「复制」按钮|Toast「代码已复制」|
|底部编辑器「发表回复」|校验非空 → 楼层列表尾部插入新楼层、回复数 \+1、Toast「回复已发布」；若本帖含「回复后可见」受限卡，同时解锁|

**受限块分布（Mock 定死）：**/topics/201 含「回复本主题后可见」块（默认锁定）；/topics/101 含「达到 LV\.5 或回复后可见」块（用 Mock flag 强制锁定用于演示）；/topics/202 含「支付 10 B币永久解锁」块（默认锁定）。

## 2\.4 发布与编辑页（/publish）

**区块：**类型切换（发布文章 / 发布讨论，与 ?type 对应）；表单：标题 Input、板块 Select、标签多选、Markdown 编辑器（编辑/预览分栏切换）、摘要 Textarea、封面图/附件上传（显示当前等级单附件上限、已用/总容量和有效期 Select，Mock 本地预览）、草稿自动保存状态文案（如「草稿已于 21:05 自动保存」）、定时发布（日期时间选择，默认关闭）、允许回复 Switch（默认开）、内容可见性 Radio 组（所有人可见 / 回复后可见 / 指定等级或回复后可见 \+ 等级 Select / 支付指定金币后可见 \+ 金额 Input）；底部操作条。

|可点击元素|目标 / 行为|
|---|---|
|类型切换 文章/讨论|原地切换表单（文章显示摘要与封面，讨论隐藏）|
|编辑器「编辑/预览」切换|原地分栏或单栏预览渲染 Markdown|
|保存草稿|Toast「草稿已保存」\+ 更新自动保存时间文案|
|预览|打开全屏预览 Modal（模拟详情页渲染）|
|提交审核|ConfirmDialog → Toast「已提交审核」→ 跳 /topics/201 并显示「审核中」StatusBadge|
|立即发布|校验标题/板块必填（缺失则字段红字 \+ 顶部错误摘要）→ ConfirmDialog → 跳 /topics/201，Toast「发布成功」|
|附件选择|文件超过当前等级/站点实际单附件上限时立即清空并提示；页面展示已用容量与总容量|
|附件有效期|只能选择不超过当前等级、站点、用途和板块最长限制的天数，明确提示到期后不可访问|
|可见性 Radio|选中「指定等级」时出现等级 Select；选中「支付金币」时出现金额 Input|

## 2\.5 用户主页（/users/\[name\]，示例 Chaos）

**区块：**头部（头像、昵称、签名、角色徽章「社区成员」「Rust 板块版主」、等级徽章 LV\.6）；等级进度条（经验 2680 / 3000）；三个账户卡片（经验 2680、金币 328、贡献 146）；Tabs（文章 / 讨论 / 回复 / 收藏 / 动态）；徽章墙；「管理板块」卡片（Rust 板块）；注册时间与最近活跃时间。本人视图额外一排管理入口：编辑资料、账号安全、登录设备、通知设置、OAuth 授权应用。

|可点击元素|目标 / 行为|
|---|---|
|Tabs 文章/讨论/回复/收藏/动态|原地切换列表；列表项标题 → 对应 /topics/\{id\}|
|积分明细（头像菜单「我的积分」进入时 tab=points）|展示经验/B币/贡献流水表（Mock 5 行）|
|徽章墙徽章|Tooltip 或 Modal 显示徽章名称与获得条件|
|「管理板块」卡片中的 Rust 板块|/boards/rust|
|编辑资料|/settings?tab=profile|
|账号安全|/settings?tab=security|
|登录设备|/settings?tab=devices（登录设备列表，含「下线」按钮 → ConfirmDialog → Toast「设备已下线」）|
|通知设置|/settings?tab=notifications（Switch 组）|
|OAuth 授权应用|/settings?tab=oauth（已授权应用列表，「撤销授权」→ ConfirmDialog → Toast）|

## 2\.6 登录与注册页（/login、/register）

**登录页区块：**居中卡片：用户名或邮箱 Input、密码 Input、「记住登录状态」Checkbox、忘记密码链接、登录按钮、创建账号入口、第三方登录占位（GitHub / Google）。**注册页区块：**用户名、邮箱、密码 Input、「同意社区规则」Checkbox（未勾选时注册按钮 disabled）、注册按钮、邮箱验证状态提示条。

|可点击元素|目标 / 行为|
|---|---|
|登录按钮|loading 1 秒 → 成功跳 /；密码错误（Mock 口令错误）显示错误横幅「用户名或密码不正确」；连续失败 5 次触发账号锁定 \+ 429 提示|
|忘记密码|/forgot\-password，提交后显示「重置邮件已发送」成功态|
|创建账号|/register|
|注册按钮|校验（邮箱格式、密码长度）→ loading → Toast「注册成功，验证邮件已发送」→ /login|
|第三方登录按钮|Toast「原型演示：未接入真实 OAuth」|

## 2\.7 管理后台仪表盘（/admin）

**区块：**左侧导航（见 1\.4）；指标卡片区：用户总数与今日新增、帖子数与待审核数、回复数、待处理举报数；邮件/后台任务状态卡；存储使用情况（进度条）；最近管理员操作列表；社区活跃趋势折线图（Mock）。整体保留 BBLBB 社区风格，不做成通用企业 SaaS。

|可点击元素|目标|
|---|---|
|「待处理举报数」卡片|/admin/reports|
|「待审核帖子」卡片|/admin/posts|
|「用户总数」卡片|/admin/users|
|最近管理员操作「查看全部」|/admin/audit|
|左侧导航各项|见 1\.4 路由表|

## 2.8 文件存储配置页（/admin/storage）

页面必须明确当前后端为本地磁盘或 S3 兼容对象存储。S3 表单包含 Endpoint、Region、Bucket、Access Key ID、Secret（仅输入，不回显）、Path-style、预签名上传、公开基础 URL、单文件上限和私有 Bucket 开关；页面只展示“已配置/未配置”和最近测试时间等脱敏状态。

|可点击元素|目标 / 行为|
|---|---|
|本地 / S3 切换|切换配置表单；未保存内容需提示确认|
|保存配置|校验必填项，Secret 只提交至后端并显示脱敏状态，不写入浏览器持久化状态|
|测试连接|执行后端连接测试，验证 Endpoint、Bucket、权限和签名模式；显示成功/失败 Toast|
|迁移提示|明确本地与 S3 切换不会自动搬运已有对象，必须先完成迁移、hash 校验和回滚准备|
|上传限制与私有 Bucket|展示站点硬上限、默认/最长附件有效期、预签名 TTL 和私有访问策略；业务规则取更小限制|
|等级管理中的附件配额|每级可配置单附件大小、附件总容量和最长有效期，列表直接展示三项额度|
|附件管理列表|展示上传者等级、容量占用、准确到期时间和即将到期/已过期状态|

## 2.9 举报与审核页（/admin/reports \+ /admin/reports/\[id\]）

**列表页区块：**Tabs（待处理 / 处理中 / 已处理 / 已驳回）；FilterBar（板块、原因、优先级、负责人）；举报列表（ReportCard：被举报内容摘要、原因、举报人、时间、状态）。**详情页区块：**原内容预览（折叠/展开）、举报原因与证据、被举报用户历史处罚记录、内容操作（隐藏内容 / 恢复 / 移动 / 关闭主题）、处罚操作（警告 / 限流 / 禁言 / 板块禁言 / 封禁）、处理原因 Textarea（必填）、申诉处理状态卡、完整审计时间线（ModerationTimeline）。

|可点击元素|目标 / 行为|
|---|---|
|Tabs / 筛选器|原地过滤列表；空结果走空状态|
|举报单 R\-1024|/admin/reports/R\-1024|
|原内容预览「查看原帖」|/topics/\{id\}（新演示中跳对应详情页）|
|内容操作（隐藏/恢复/移动/关闭主题）|各弹 ConfirmDialog → 原内容预览区状态变更 \+ 审计时间线追加一条 \+ Toast|
|处罚操作（如「禁言 7 天」）|处罚 Modal：类型 Select \+ 时长 \+ 处理原因（必填，为空禁用提交）→ ConfirmDialog → 状态变「已处理」\+ 时间线追加 \+ Toast「处理完成」|
|「驳回举报」|ConfirmDialog（填原因）→ 状态变「已驳回」|
|申诉状态卡「处理申诉」|Modal：维持/撤销处罚 → ConfirmDialog → Toast|

## 2\.10 积分与等级管理页（/admin/points、/admin/levels）

**积分页区块：**三种货币卡片（经验、B币、贡献：流通量 / 今日产生 / 今日消费）；积分规则表（行为 → 增减数值）；用户账户查询（搜索框 → 账户行）；手动调整入口；流水查询表（含「补偿记录」类型标识）。**等级页区块：**等级表（名称、图标、颜色、经验门槛、权益、用户数量）；可视化晋升路径（LV\.1 → LV\.10 横向节点图）；等级预览卡。

|可点击元素|目标 / 行为|
|---|---|
|用户搜索（输入 Chaos）|下方展示账户行：经验 2680、B币 328、贡献 146|
|账户行「调整」按钮|调整 Modal：币种 Select \+ 增减数额 Input \+ 原因 Textarea（必填）→ 「下一步」→ 二次确认 ConfirmDialog（明示「流水不可删除，仅可创建补偿记录」）→ 确认后流水表顶部新增记录 \+ Toast「调整已生效」|
|流水表筛选（币种/类型）|原地过滤|
|等级表行「编辑」|编辑 Modal（名称/颜色/经验门槛/权益）→ 保存 Toast|
|晋升路径节点（LV\.1–LV\.10）|点击节点在右侧「等级预览卡」展示该等级样式|

## 2\.11 主题、插件与 OAuth 管理页（/admin/themes、/admin/plugins、/admin/oauth）

**主题页区块：**当前主题卡（预览图、亮色/暗色模式标识）；主题列表（含预览图缩略）；外观设置（颜色、字号、圆角、密度）；「上传数据型主题」按钮；代码型主题提示条（「代码型主题需要重新构建部署后生效」）。**插件页区块：**配置型插件区与预编译 UI 插件区（明确标识类型）；插件表格（名称、版本、状态、所需能力、操作）；运行错误与任务状态卡；页面顶部说明「不提供上传并执行任意代码的能力」。**OAuth 页区块：**客户端表格（名称、Client ID、类型 Public/Confidential、状态、Redirect URI、Post logout redirect URI、Scopes、最近授权用户数、安全事件）；创建客户端按钮。

|可点击元素|目标 / 行为|
|---|---|
|主题卡「预览」|预览 Modal（大图 \+ 亮/暗切换） |
|主题卡「应用」|ConfirmDialog → 全站切换对应 Token（演示暗色模式）\+ Toast「主题已切换」|
|「上传数据型主题」|Modal 选择文件（Mock）→ Toast「主题已导入」|
|插件「启用/停用」|ConfirmDialog → 状态列切换 \+ Toast|
|插件「设置」|设置 Modal（表单 Mock）→ 保存 Toast|
|插件「卸载」|ConfirmDialog（危险样式）→ 列表移除该行 \+ Toast|
|「创建客户端」|创建 Modal：名称 \+ 类型 Radio（Public/Confidential）\+ Redirect URI \+ Post logout redirect URI \+ Scopes Checkbox（openid/profile/email）→ 提交 → 成功 Modal 显示 Client Secret（仅此一次，带复制按钮，关闭后不再可查）→ 表格新增一行|
|客户端行「禁用」|ConfirmDialog → 状态变「已禁用」|
|客户端行「重置 Secret」|ConfirmDialog → 成功 Modal 显示新 Secret（同样仅一次）|
|客户端行「撤销授权」|ConfirmDialog → 最近授权用户数清零 \+ Toast|

# 三、可点击交互流程（9 条，逐步实现路径）

## 流程 1：首页 → 板块 → 讨论详情 → 回复

1. /（首页）：点击「活跃板块」中的「Web 开发」BoardCard → 跳 /boards/web\-dev。

2. /boards/web\-dev：点击帖子列表中《使用 SvelteKit 构建博客与轻量论坛是否合理？》标题 → 跳 /topics/201。

3. /topics/201：滚动到底部 Markdown 回复编辑器，输入任意文本。

4. 点击「发表回复」→ 楼层列表尾部新增一层（楼层号递增、作者为 Chaos）、回复总数 \+1、Toast「回复已发布」；若该帖「回复本主题后可见」受限卡处于锁定态则同时解锁（即流程 3 的汇合点）。

## 流程 2：首页 → 文章详情 → 收藏

1. /：点击精选文章区《Rust 小机器上的 SQLite 并发实践》卡片 → 跳 /topics/101。

2. /topics/101：点击标题区「收藏」按钮 → 按钮变为已收藏高亮态、Toast「已加入收藏」。

3. （验证）点击右上角头像 → 头像菜单「收藏」→ /favorites，列表第一条即为该文章。

## 流程 3：讨论详情 → 回复后解锁隐藏内容

1. /topics/201：正文中的受限卡显示「回复本主题后可见」，正文内容不在 DOM 中。

2. 点击受限卡上「去回复」按钮 → 页内锚点滚动到底部回复编辑器并聚焦。

3. 输入内容并点击「发表回复」→ 新楼层插入、受限卡解锁渲染正文、Toast「回复已发布，隐藏内容已解锁」。

## 流程 4：讨论详情 → 支付 10 B币解锁

1. /topics/202：受限卡显示「支付 10 B币永久解锁」。

2. 点击「立即解锁」→ ConfirmDialog：当前余额 328 B币、本次扣除 10 B币、解锁后余额 318 B币。

3. 点击「确认支付」→ 受限卡解锁渲染正文、顶部/用户卡余额同步变为 318、Toast「解锁成功」。

## 流程 5：用户头像 → 个人主页 → 登录设备

1. 任意前台页面：点击右上角头像 → 头像菜单点「我的主页」→ 跳 /users/Chaos。

2. /users/Chaos（本人视图）：点击管理入口「登录设备」→ 跳 /settings?tab=devices。

3. /settings?tab=devices：展示登录设备列表（当前设备 \+ 另外 2 台 Mock），点击某设备「下线」→ ConfirmDialog → 该行移除、Toast「设备已下线」。

## 流程 6：管理后台 → 举报队列 → 处理举报 → 创建处罚

1. /admin：点击「待处理举报数」指标卡或左侧导航「举报与审核」→ 跳 /admin/reports。

2. /admin/reports（默认「待处理」tab）：点击举报单 R\-1024 → 跳 /admin/reports/R\-1024。

3. /admin/reports/R\-1024：查看原内容预览、举报原因与证据、被举报用户历史处罚记录。

4. 处罚操作选择「禁言 7 天」，处理原因填入「发布广告内容」（不填时提交按钮 disabled）→ 点击「提交处理」。

5. ConfirmDialog 二次确认 → 状态变为「已处理」、审计时间线追加「禁言 7 天」记录、Toast「处理完成」。

## 流程 7：管理后台 → 积分管理 → 调整用户积分 → 二次确认

1. /admin：点击左侧导航「积分与货币」→ 跳 /admin/points。

2. /admin/points：用户查询框输入「Chaos」→ 展示账户行（经验 2680 / B币 328 / 贡献 146）。

3. 点击账户行「调整」→ Modal 选择币种「B币」、数额 \+50、原因填「优质内容奖励」→ 点击「下一步」。

4. 二次确认 ConfirmDialog（明示「历史流水不可删除，仅可创建补偿记录」）→ 点击「确认调整」→ 流水表顶部新增一条 \+50 记录、账户余额变 378、Toast「调整已生效」。

## 流程 8：管理后台 → OAuth 客户端 → 创建客户端

1. /admin：点击左侧导航「OAuth 客户端」→ 跳 /admin/oauth。

2. /admin/oauth：点击「创建客户端」→ Modal 表单：名称「Demo App」、类型选 Confidential、Redirect URI 填示例地址、Post logout redirect URI 填示例地址、Scopes 勾选 openid \+ profile \+ email。

3. 点击「创建」→ 成功 Modal 展示 Client ID 与 Client Secret（带复制按钮，文案「Secret 仅本次显示，请妥善保存」）。

4. 点击「我已保存」关闭 Modal → 客户端表格新增一行（状态：启用中）。

## 流程 9：管理后台 → 主题管理 → 预览并切换主题

1. /admin：点击左侧导航「主题管理」→ 跳 /admin/themes。

2. /admin/themes：在主题列表中点击「暗色主题」卡的「预览」→ 预览 Modal 展示暗色效果。

3. 关闭预览，点击「应用」→ ConfirmDialog「切换后全站立即生效」→ 确认。

4. 全站切换为暗色 Token、当前主题卡变为「暗色主题」、Toast「主题已切换」；点击「默认主题」可同样流程切回亮色。代码型主题行始终显示「代码型主题需要重新构建部署后生效」提示，不提供「应用」按钮。

# 四、状态设计（空 / 加载 / 错误 / 403 / 404 / 429）

|状态|出现页面 / 位置|触发条件（Mock 定死）|展示形式|
|---|---|---|---|
|空状态|/boards/\[slug\] 列表区|筛选组合无匹配，或「未回复」tab 下帖子全部已有回复|EmptyState「没有符合条件的帖子」\+「清除筛选」按钮|
|空状态|/search|关键词无匹配（Mock：搜索「不存在的内容」触发）|EmptyState「未找到相关内容」\+ 热门搜索建议|
|空状态|/favorites、/notifications|无收藏 / 无通知|EmptyState \+ 引导按钮（去首页逛逛）|
|空状态|/topics/\[id\] 回复区|0 回复的讨论|「还没有回复，来抢沙发」\+ 锚点跳编辑器|
|空状态|/admin/reports「待处理」tab|队列为空|EmptyState「举报队列已清空」|
|空状态|/users/\[name\] 各 tab|该分类无内容|EmptyState 对应文案|
|加载|所有列表页与详情页路由切换时|进入路由即展示（Mock 300ms）|Skeleton：列表骨架行、详情页标题\+正文骨架|
|加载|登录、发表回复、发布、举报提交、后台各表单提交按钮|点击提交|按钮 loading（禁用 \+ 转圈，Mock 1 秒）|
|错误|/publish、/register 表单|必填缺失 / 格式错误时提交|字段下方红字 \+ 页面顶部错误摘要条|
|错误|任意提交操作|Mock 开关开启「模拟接口失败」|Toast 错误「操作失败，请重试」\+ 保留表单内容|
|403|/admin 及其全部子路由|Mock 用户切换为非管理员后直接访问 /admin|403 页「需要管理员权限」\+「返回首页」按钮|
|404|任意未匹配路由、/topics/999 等不存在资源|直接输入地址或点击失效链接|404 页「页面不存在或已被删除」\+「返回首页」「去搜索」|
|429|/login|连续登录失败 5 次|锁定横幅「尝试次数过多，账号已临时锁定」\+ 429 提示「请求过于频繁，请 10 分钟后再试」|
|429|/topics/\[id\] 回复编辑器|1 分钟内第 2 次提交回复（Mock 频率限制）|Toast 429「发言过于频繁，请稍后再试」|

# 五、Mock 数据锚点（保证跳转闭合，实现时原样使用）

- 当前登录用户：Chaos，LV\.6，经验 2680/3000，B币 328，贡献 146；角色：社区成员、Rust 板块版主（因此在 rust 板块帖子 /topics/\[id\] 的楼层可见版主操作）。

- 文章 \#101：《Rust 小机器上的 SQLite 并发实践》，板块 tech\-essay，标签 Rust / SQLite / 性能优化，含「达到 LV\.5 或回复后可见」受限块（Mock flag 强制锁定演示）。

- 讨论 \#201：《使用 SvelteKit 构建博客与轻量论坛是否合理？》，板块 web\-dev，标签 SvelteKit / 自托管，含「回复本主题后可见」受限块（默认锁定）。

- 讨论 \#202：《OAuth Provider 应该自研到什么程度？》，板块 opensource，标签 OAuth / 自托管，含「支付 10 B币永久解锁」受限块（默认锁定）。

- 板块 6 个：tech\-essay 技术随笔、rust Rust、web\-dev Web 开发、opensource 开源项目、chat 闲聊、meta 站务；Chaos 是 rust 板块版主。

- 标签 6 个：Rust、SvelteKit、SQLite、OAuth、自托管、性能优化。

- 举报单：R\-1024，待处理，板块 rust，原因「广告 / 垃圾信息」，被举报人 Alice（历史处罚：警告 1 次）。

- 货币：经验、B币、贡献；等级：LV\.1–LV\.10，LV\.5 经验门槛 1500、LV\.6 门槛 3000（Chaos 当前 2680，故 LV\.5 块按规则本应可见，演示锁定态由 Mock flag 控制）。

- 登录设备 3 台（当前设备 \+ MacBook \+ iPhone）；OAuth 客户端已有 2 条（1 个 Public 启用中、1 个 Confidential 已禁用）；插件 4 个（2 配置型、2 预编译 UI 型，其中 1 个有运行错误用于展示）。

