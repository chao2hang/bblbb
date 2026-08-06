//! M04-POSTS：草稿、文章与讨论服务。
//!
//! 本模块按里程碑逐步实现：M04-POSTS-01 定义 article/discussion 创建命令与
//! 服务端字段校验（不信任 author、状态或统计值）；后续任务落地草稿 CRUD、
//! 预览、发布、列表/详情投影、修订与治理命令。

pub mod command;

pub use command::{
    validate_draft_patch, CreateDraftCommand, CreateDraftInput, CreatePostCommand, CreatePostInput,
    DraftPatch, DraftPatchInput, PostCreateError,
};
