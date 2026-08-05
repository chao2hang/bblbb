//! M03-BOARDS-01：板块层级读取——限制最大深度并检测循环父级。
//!
//! `boards.parent_id`（迁移 0022）是软自引用层级：ALTER ADD COLUMN 不能带 FK，
//! 层级完整性与环路校验在服务层裁决（SCHEMA.md §6）。
//!
//! 规则：
//! - **深度**：根板块（`parent_id IS NULL`）= 第 1 级，子板块深度 = 父级深度 + 1；
//!   `MAX_BOARD_DEPTH = 4`（最深 4 级 = 3 层子板块），超出 → [`HierarchyError::DepthExceeded`]；
//! - **环路**：父链构成环路（含自引用）→ [`HierarchyError::Cycle`] / [`HierarchyError::SelfParent`]，
//!   读取与写入均硬错误（防无限展开）；环路检测为迭代式（栈安全，不随链长递归）；
//! - **悬空父级**：`parent_id` 指向活动投影外的板块（如父板块已软删/停用）——
//!   读取时提升为根并记录（软删父板块是合法操作，不应破坏子板块读取）；
//!   写入时拒绝（不能引用不存在的父板块，[`HierarchyError::DanglingParent`]）；
//! - **硬删除裁决**：存在子板块时禁止物理删除（SCHEMA.md §6），
//!   [`BoardHierarchy::has_children`] / [`BoardHierarchy::descendant_ids`] 供服务层裁决。
//!
//! 同级顺序 = 输入顺序（调用方按 `sort_order, created_at` 排序，DB 加载器已排）。

use std::collections::{HashMap, HashSet};

use sqlx::Either;

use crate::db::DatabasePool;

/// 板块层级最大深度（根 = 第 1 级；v1 策略常量，最深 4 级 = 3 层子板块）。
pub const MAX_BOARD_DEPTH: usize = 4;

/// 层级构建所需的最小板块视图（id + parent_id）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardRef {
    pub id: String,
    pub parent_id: Option<String>,
}

/// 层级完整性错误（服务层裁决）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HierarchyError {
    /// `parent_id` 指向自身。
    SelfParent { board_id: String },
    /// 父链构成环路：`board_id` 沿 `path` 又回到自身（path = 环上的节点链）。
    Cycle { board_id: String, path: Vec<String> },
    /// 深度超出 [`MAX_BOARD_DEPTH`]。
    DepthExceeded { board_id: String, depth: usize },
    /// 引用了不存在的父板块（写入侧拒绝；读取侧容忍并提升为根）。
    DanglingParent { board_id: String, parent_id: String },
}

impl std::fmt::Display for HierarchyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HierarchyError::SelfParent { board_id } => {
                write!(f, "board {board_id} has itself as parent")
            }
            HierarchyError::Cycle { board_id, path } => {
                write!(f, "board hierarchy cycle at {board_id}: {:?}", path)
            }
            HierarchyError::DepthExceeded { board_id, depth } => {
                write!(
                    f,
                    "board {board_id} depth {depth} exceeds MAX_BOARD_DEPTH {MAX_BOARD_DEPTH}"
                )
            }
            HierarchyError::DanglingParent {
                board_id,
                parent_id,
            } => write!(f, "board {board_id} references missing parent {parent_id}"),
        }
    }
}

impl std::error::Error for HierarchyError {}

/// 构建后的层级只读视图。
#[derive(Debug, Clone, Default)]
pub struct BoardHierarchy {
    /// board_id → 深度（根 = 1）。
    depth: HashMap<String, usize>,
    /// board_id → 父板块 id（悬空父级不出现，子级被提升为根）。
    parent: HashMap<String, String>,
    /// parent_id → 有序子板块 id（同级顺序 = 输入顺序）。
    children: HashMap<String, Vec<String>>,
    /// 根板块 id（有序；含被悬空父级提升的根）。
    roots: Vec<String>,
    /// (子板块, 缺失父板块) 悬空引用记录。
    dangling: Vec<(String, String)>,
}

impl BoardHierarchy {
    /// 板块深度（根 = 1；不在层级内返回 None）。
    pub fn depth_of(&self, board_id: &str) -> Option<usize> {
        self.depth.get(board_id).copied()
    }

    /// 板块父级 id（悬空父级 → None）。
    pub fn parent_of(&self, board_id: &str) -> Option<&str> {
        self.parent.get(board_id).map(String::as_str)
    }

    /// 有序子板块（无子级 → 空切片）。
    pub fn children_of(&self, board_id: &str) -> &[String] {
        self.children
            .get(board_id)
            .map_or(&[], Vec::<String>::as_slice)
    }

    /// 有序根板块。
    pub fn roots(&self) -> &[String] {
        &self.roots
    }

    /// 悬空父级引用记录（读取侧容忍项）。
    pub fn dangling(&self) -> &[(String, String)] {
        &self.dangling
    }

    /// 是否存在子板块（硬删除裁决，SCHEMA.md §6）。
    pub fn has_children(&self, board_id: &str) -> bool {
        self.children
            .get(board_id)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// 全部后代（含间接），BFS 展开；同级按构建顺序。
    pub fn descendant_ids(&self, board_id: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut queue: Vec<&str> = self
            .children_of(board_id)
            .iter()
            .map(String::as_str)
            .collect();
        let mut head = 0;
        while head < queue.len() {
            let id = queue[head];
            head += 1;
            out.push(id);
            queue.extend(self.children_of(id).iter().map(String::as_str));
        }
        out
    }
}

/// 从 `boards`（最小视图）构建层级；检测自引用/环路/超深（硬错误），
/// 悬空父级提升为根并记录。
///
/// - 环路/自引用/超深属于数据完整性故障，返回错误（防无限展开）；
/// - 悬空父级（父板块被软删/停用）是合法状态（SCHEMA.md §6：`deleted_at`
///   非空仍保留行），读取时提升为根并在 [`BoardHierarchy::dangling`] 记录；
/// - 同级顺序 = 输入顺序。
pub fn build_hierarchy(boards: &[BoardRef]) -> Result<BoardHierarchy, HierarchyError> {
    let mut depth: HashMap<String, usize> = HashMap::with_capacity(boards.len());
    let mut parent: HashMap<String, String> = HashMap::with_capacity(boards.len());
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots: Vec<String> = Vec::new();
    let mut dangling: Vec<(String, String)> = Vec::new();

    let index: HashMap<&str, &BoardRef> = boards.iter().map(|b| (b.id.as_str(), b)).collect();

    // 迭代式深度计算 + 环路/自引用/悬空检测（不随链长递归，栈安全）。
    for board in boards {
        if depth.contains_key(&board.id) {
            continue;
        }
        let mut path: Vec<&BoardRef> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut cur = board;
        let mut base = 1usize;
        loop {
            if let Some(&d) = depth.get(&cur.id) {
                // 已计算祖先：break 节点不在 path 中，第一个 path 节点是它的
                // 直接子级，深度 = 祖先深度 + 1
                base = d + 1;
                break;
            }
            if !seen.insert(cur.id.as_str()) {
                let start = path.iter().position(|b| b.id == cur.id).unwrap_or(0);
                let cycle_path: Vec<String> = path[start..].iter().map(|b| b.id.clone()).collect();
                return Err(HierarchyError::Cycle {
                    board_id: cur.id.clone(),
                    path: cycle_path,
                });
            }
            path.push(cur);
            match &cur.parent_id {
                None => break, // 根：base = 1
                Some(pid) if pid == &cur.id => {
                    return Err(HierarchyError::SelfParent {
                        board_id: cur.id.clone(),
                    });
                }
                Some(pid) => match index.get(pid.as_str()) {
                    None => {
                        dangling.push((cur.id.clone(), pid.clone()));
                        break; // 悬空父级 → 提升为根：base = 1
                    }
                    Some(next) => cur = next,
                },
            }
        }
        // 逆序回填深度（根=1；超出 MAX_BOARD_DEPTH → 错误）
        for (d, node) in (base..).zip(path.iter().rev()) {
            if d > MAX_BOARD_DEPTH {
                return Err(HierarchyError::DepthExceeded {
                    board_id: node.id.clone(),
                    depth: d,
                });
            }
            depth.insert(node.id.clone(), d);
        }
    }

    // 组装父子/根/悬空（同级顺序 = 输入顺序）。
    for board in boards {
        match &board.parent_id {
            None => roots.push(board.id.clone()),
            Some(pid) => {
                if index.contains_key(pid.as_str()) {
                    parent.insert(board.id.clone(), pid.clone());
                    children
                        .entry(pid.clone())
                        .or_default()
                        .push(board.id.clone());
                } else {
                    roots.push(board.id.clone());
                }
            }
        }
    }

    Ok(BoardHierarchy {
        depth,
        parent,
        children,
        roots,
        dangling,
    })
}

/// 写入侧父级校验（M03-BOARDS-01/02/05 共用）：新父级必须存在于 `all`、
/// 非自身、不构成环路、不超出最大深度。
///
/// - `all` 应包含全部候选父板块（活跃且未软删）；`child_id` 可不在 `all`
///   （新建板块），此时只校验新挂载点；
/// - 复用 [`build_hierarchy`] 的深度/环路检测（合成树 = 把 child 的父级替换为
///   `new_parent_id`）。
pub fn validate_parent(
    all: &[BoardRef],
    child_id: &str,
    new_parent_id: Option<&str>,
) -> Result<(), HierarchyError> {
    if new_parent_id == Some(child_id) {
        return Err(HierarchyError::SelfParent {
            board_id: child_id.to_string(),
        });
    }
    if let Some(pid) = new_parent_id {
        if !all.iter().any(|b| b.id == pid) {
            return Err(HierarchyError::DanglingParent {
                board_id: child_id.to_string(),
                parent_id: pid.to_string(),
            });
        }
    }
    let mut synthetic: Vec<BoardRef> = all.to_vec();
    if let Some(child) = synthetic.iter_mut().find(|b| b.id == child_id) {
        child.parent_id = new_parent_id.map(str::to_string);
    } else {
        synthetic.push(BoardRef {
            id: child_id.to_string(),
            parent_id: new_parent_id.map(str::to_string),
        });
    }
    build_hierarchy(&synthetic).map(|_| ())
}

/// 从数据库加载活动板块层级（`is_active = 1 AND deleted_at IS NULL`，
/// SCHEMA.md §6 活跃投影），同级按 `sort_order, created_at, id` 稳定排序。
///
/// 环路/超深数据故障 → Err；悬空父级（父板块软删/停用）提升为根并记录。
pub async fn load_hierarchy(pool: &DatabasePool) -> Result<BoardHierarchy, String> {
    let rows: Vec<(String, Option<String>)> = match pool {
        Either::Left(db) => sqlx::query_as(
            "SELECT id, parent_id FROM boards
                 WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_as(
            "SELECT id, parent_id FROM boards
                 WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?,
    };
    let refs: Vec<BoardRef> = rows
        .into_iter()
        .map(|(id, parent_id)| BoardRef { id, parent_id })
        .collect();
    build_hierarchy(&refs).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(id: &str, parent: Option<&str>) -> BoardRef {
        BoardRef {
            id: id.to_string(),
            parent_id: parent.map(str::to_string),
        }
    }

    #[test]
    fn flat_boards_are_roots_at_depth_one() {
        let h = build_hierarchy(&[board("a", None), board("b", None)]).unwrap();
        assert_eq!(h.roots(), &["a".to_string(), "b".to_string()]);
        assert_eq!(h.depth_of("a"), Some(1));
        assert!(h.dangling().is_empty());
        assert!(!h.has_children("a"));
    }

    #[test]
    fn nested_tree_depths_children_and_descendants() {
        let h = build_hierarchy(&[
            board("root", None),
            board("mid", Some("root")),
            board("leaf", Some("mid")),
        ])
        .unwrap();
        assert_eq!(h.roots(), &["root".to_string()]);
        assert_eq!(h.depth_of("root"), Some(1));
        assert_eq!(h.depth_of("mid"), Some(2));
        assert_eq!(h.depth_of("leaf"), Some(3));
        assert_eq!(h.parent_of("leaf"), Some("mid"));
        assert_eq!(h.children_of("root"), &["mid".to_string()]);
        assert_eq!(h.children_of("mid"), &["leaf".to_string()]);
        assert_eq!(
            h.descendant_ids("root"),
            vec!["mid", "leaf"],
            "BFS：同级按构建顺序"
        );
        assert!(h.has_children("root"));
        assert!(h.has_children("mid"));
        assert!(!h.has_children("leaf"));
    }

    #[test]
    fn depth_at_limit_ok_and_beyond_errors() {
        // 链 root→c2→c3→c4：深度 4 = MAX，允许
        let ok = build_hierarchy(&[
            board("r", None),
            board("c2", Some("r")),
            board("c3", Some("c2")),
            board("c4", Some("c3")),
        ]);
        assert!(ok.is_ok(), "深度 4 必须在限制内");
        assert_eq!(ok.unwrap().depth_of("c4"), Some(4));

        // 再加一级 → 深度 5 超限
        let deep = build_hierarchy(&[
            board("r", None),
            board("c2", Some("r")),
            board("c3", Some("c2")),
            board("c4", Some("c3")),
            board("c5", Some("c4")),
        ]);
        assert_eq!(
            deep.unwrap_err(),
            HierarchyError::DepthExceeded {
                board_id: "c5".to_string(),
                depth: 5
            }
        );
    }

    #[test]
    fn self_parent_is_rejected() {
        assert_eq!(
            build_hierarchy(&[board("a", Some("a"))]).unwrap_err(),
            HierarchyError::SelfParent {
                board_id: "a".to_string()
            }
        );
    }

    #[test]
    fn two_node_cycle_is_detected() {
        let err = build_hierarchy(&[board("a", Some("b")), board("b", Some("a"))]).unwrap_err();
        match err {
            HierarchyError::Cycle { board_id, path } => {
                assert_eq!(board_id, "a");
                assert!(path.contains(&"a".to_string()) && path.contains(&"b".to_string()));
            }
            other => panic!("必须是 Cycle 错误: {other}"),
        }
    }

    #[test]
    fn three_node_cycle_is_detected() {
        let err = build_hierarchy(&[
            board("a", Some("c")),
            board("b", Some("a")),
            board("c", Some("b")),
        ])
        .unwrap_err();
        assert!(
            matches!(err, HierarchyError::Cycle { board_id, .. } if board_id == "a" || board_id == "b" || board_id == "c")
        );
    }

    #[test]
    fn dangling_parent_promoted_to_root_and_recorded() {
        let h = build_hierarchy(&[board("child", Some("missing")), board("other", None)]).unwrap();
        // child 被提升为根（读取容忍），悬空引用被记录
        assert_eq!(h.roots(), &["child".to_string(), "other".to_string()]);
        assert_eq!(h.parent_of("child"), None);
        assert_eq!(h.depth_of("child"), Some(1));
        assert_eq!(
            h.dangling(),
            &[("child".to_string(), "missing".to_string())]
        );
    }

    #[test]
    fn validate_parent_accepts_valid_reparent() {
        let all = [board("root", None), board("child", None)];
        assert!(validate_parent(&all, "child", Some("root")).is_ok());
        assert!(
            validate_parent(&all, "new", Some("root")).is_ok(),
            "新建板块"
        );
        assert!(validate_parent(&all, "new", None).is_ok(), "新建根板块");
    }

    #[test]
    fn validate_parent_rejects_self_missing_cycle_and_depth() {
        let all = [board("root", None), board("child", None)];
        // 自引用
        assert_eq!(
            validate_parent(&all, "child", Some("child")),
            Err(HierarchyError::SelfParent {
                board_id: "child".to_string()
            })
        );
        // 父级不存在
        assert_eq!(
            validate_parent(&all, "child", Some("nope")),
            Err(HierarchyError::DanglingParent {
                board_id: "child".to_string(),
                parent_id: "nope".to_string()
            })
        );
        // 深度超限：root → c2 → c3 → c4 → child = 深度 5
        let deep = [
            board("root", None),
            board("c2", Some("root")),
            board("c3", Some("c2")),
            board("c4", Some("c3")),
            board("child", None),
        ];
        assert!(matches!(
            validate_parent(&deep, "child", Some("c4")),
            Err(HierarchyError::DepthExceeded { depth: 5, .. })
        ));
        // 环路：把 a 挂到 b 下，而 b 是 a 的后代
        let cyc = [board("a", None), board("b", Some("a"))];
        assert!(matches!(
            validate_parent(&cyc, "a", Some("b")),
            Err(HierarchyError::Cycle { .. })
        ));
    }
}
