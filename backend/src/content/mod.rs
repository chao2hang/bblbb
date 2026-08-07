//! M04-SCHEMA：内容领域模型与仓储契约（帖子/正文/评论/可见性）。
//!
//! 本模块按里程碑逐任务扩展；M04-SCHEMA-01 提供帖子元数据模型（类型、状态、
//! 板块内 slug、乐观并发版本与发布时间），M04-SCHEMA-02 起加入 post_contents/
//! revisions，M04-SCHEMA-04 加入 comments 楼层模型。

pub mod markdown;
pub mod model;
pub mod posts;
pub mod repository;

// M04-COMMENTS / M04-VISIBILITY：模块骨架由主代理预注册，各域 agent 只填充
// 各自目录下的实现文件，避免并行编辑本文件产生冲突。
pub mod comments;
pub mod visibility;
