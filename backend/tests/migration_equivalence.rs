//! M01-DB-09：SQLite/MySQL/MariaDB 三份不可变迁移的结构等价断言。
//!
//! 断言目标：
//! 1. 每个逻辑迁移版本在三个目录中存在同名、同版本号的不可变 SQL 文件；
//! 2. MySQL 与 MariaDB 的迁移内容除版本头注释外逐字节一致；
//! 3. 每个 `CREATE TABLE` 的列集合、列名、归一化类型与可空性三库等价
//!    （类型归一化遵循 docs/SCHEMA.md §2.7 的类型映射表）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::read_migration_files;

/// 迁移目录相对 backend 的位置
const MIGRATIONS_ROOT: &str = "../migrations";

#[derive(Debug, PartialEq, Eq)]
struct Column {
    name: String,
    normalized_type: String,
    nullable: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Table {
    name: String,
    columns: Vec<Column>,
}

/// 归一化列类型：把三库的物理类型映射到逻辑类型（SCHEMA §2.7）。
fn normalize_type(raw: &str) -> String {
    let base = raw
        .split(['(', ' '])
        .next()
        .unwrap_or(raw)
        .trim()
        .to_lowercase();
    match base.as_str() {
        // UUID/哈希/文本/JSON：sqlite TEXT ↔ mysql CHAR(36)/VARCHAR(n)/MEDIUMTEXT/JSON
        "char" | "varchar" | "text" | "mediumtext" | "longtext" | "tinytext" | "json" | "blob" => {
            "text".to_string()
        }
        // 整数语义（毫秒时间戳/计数/布尔/序号）：sqlite INTEGER ↔ mysql INT/BIGINT/TINYINT
        "integer" | "int" | "bigint" | "tinyint" | "smallint" => "int".to_string(),
        "real" | "double" | "float" | "numeric" | "decimal" => "real".to_string(),
        other => other.to_string(),
    }
}

/// 解析 `CREATE TABLE name (...)` 块；不处理 `IF NOT EXISTS`/反引号（本仓库迁移不使用）。
fn parse_create_tables(sql: &str) -> Vec<Table> {
    let upper = sql.to_uppercase();
    let mut tables = Vec::new();
    let mut idx = 0;
    while let Some(rel) = upper[idx..].find("CREATE TABLE") {
        let start = idx + rel;
        let name_start = start + "CREATE TABLE".len();
        let body_open_rel = upper[name_start..].find('(').unwrap_or_else(|| {
            panic!("CREATE TABLE 缺少括号: {}", &sql[start..start + 60]);
        });
        let body_open = name_start + body_open_rel;
        let name = sql[name_start..name_start + body_open_rel]
            .trim()
            .to_string();

        // 匹配括号深度找到表体结束
        let mut depth = 1usize;
        let mut body_end = body_open;
        for (i, c) in sql[body_open + 1..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        body_end = body_open + 1 + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "未匹配的括号: {name}");
        tables.push(parse_table_body(&name, &sql[body_open + 1..body_end]));
        idx = body_end + 1;
    }
    tables
}

/// 按顶层逗号切分表体，识别列与约束。
fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts
}

fn parse_table_body(name: &str, body: &str) -> Table {
    let mut columns = Vec::new();
    // 表级 PRIMARY KEY (col, ...) 中的列不可空
    let mut table_pk: Vec<String> = Vec::new();

    for item in split_top_level(body) {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item_upper = trimmed.to_uppercase();
        // 约束：PRIMARY/UNIQUE/KEY/CONSTRAINT/FOREIGN/CHECK
        if item_upper.starts_with("PRIMARY KEY")
            || item_upper.starts_with("UNIQUE")
            || item_upper.starts_with("KEY")
            || item_upper.starts_with("CONSTRAINT")
            || item_upper.starts_with("FOREIGN")
            || item_upper.starts_with("CHECK")
        {
            if item_upper.starts_with("PRIMARY KEY") {
                // PRIMARY KEY (col1, col2)
                if let Some(open) = item.find('(') {
                    if let Some(close) = item[open..].find(')') {
                        for pk in item[open + 1..open + close].split(',') {
                            table_pk.push(pk.trim().trim_matches('`').to_string());
                        }
                    }
                }
            }
            continue;
        }

        // 列定义：`name type ...`
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let col_name = parts
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('`')
            .to_string();
        let col_rest = parts.next().unwrap_or("");
        assert!(!col_name.is_empty(), "无法解析列名: {trimmed}");
        let col_type = col_rest
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(',');
        let nullable = !(trimmed.to_uppercase().contains("NOT NULL")
            || trimmed.to_uppercase().contains("PRIMARY KEY")
            || table_pk.contains(&col_name));
        columns.push(Column {
            name: col_name,
            normalized_type: normalize_type(col_type),
            nullable,
        });
    }

    Table {
        name: name.trim().to_string(),
        columns,
    }
}

/// 读取某目录迁移文件，返回 {版本: (文件名, 内容)}。
fn load_migration_dir(dir: &Path) -> BTreeMap<u64, (String, String)> {
    let files = read_migration_files(dir).expect("读取迁移目录失败");
    files
        .into_iter()
        .map(|f| (f.version, (f.name, f.sql)))
        .collect()
}

/// 解析某目录全部迁移的建表结构：{版本: Vec<Table>}。
fn parse_dir_tables(dir: &Path) -> BTreeMap<u64, Vec<Table>> {
    let mut result = BTreeMap::new();
    for (version, (_, sql)) in load_migration_dir(dir) {
        result.insert(version, parse_create_tables(&sql));
    }
    result
}

fn migrations_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 断言三个目录的版本集合与文件名一致。
#[test]
fn migration_sets_are_parallel() {
    let root = migrations_root();
    for engine in ["sqlite", "mysql", "mariadb"] {
        let dir = root.join(engine);
        assert!(dir.is_dir(), "迁移目录缺失: {dir:?}");
    }
    let sqlite = load_migration_dir(&root.join("sqlite"));
    let mysql = load_migration_dir(&root.join("mysql"));
    let mariadb = load_migration_dir(&root.join("mariadb"));

    let names = |map: &BTreeMap<u64, (String, String)>| {
        map.iter()
            .map(|(v, (n, _))| format!("{v}_{n}.sql"))
            .collect::<Vec<_>>()
    };
    let sqlite_names = names(&sqlite);
    let mysql_names = names(&mysql);
    let mariadb_names = names(&mariadb);

    assert_eq!(
        sqlite_names, mysql_names,
        "sqlite 与 mysql 的迁移文件集合不一致"
    );
    assert_eq!(
        mysql_names, mariadb_names,
        "mysql 与 mariadb 的迁移文件集合不一致"
    );
    assert!(!sqlite.is_empty(), "迁移目录不应为空");
}

/// mysql 与 mariadb 迁移内容逐版本一致（允许注释行差异，可执行 SQL 必须一致）。
#[test]
fn mysql_and_mariadb_contents_match() {
    let root = migrations_root();
    let mysql = load_migration_dir(&root.join("mysql"));
    let mariadb = load_migration_dir(&root.join("mariadb"));
    assert_eq!(
        mysql.keys().collect::<Vec<_>>(),
        mariadb.keys().collect::<Vec<_>>()
    );

    // 去掉以 -- 开头的注释行后比较可执行 SQL
    let strip_comments = |s: &str| -> String {
        s.lines()
            .filter(|line| {
                let trimmed = line.trim_start();
                !trimmed.starts_with("--")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    for (version, (mysql_name, mysql_sql)) in &mysql {
        let (mariadb_name, mariadb_sql) = &mariadb[version];
        assert_eq!(mysql_name, mariadb_name, "v{version} 文件名不一致");
        assert_eq!(
            strip_comments(mysql_sql),
            strip_comments(mariadb_sql),
            "v{version} mysql/mariadb 可执行 SQL 必须一致（仅允许注释差异）"
        );
    }
}

/// 逐版本逐表结构等价：列名集合、归一化类型与可空性一致。
#[test]
fn table_structure_is_equivalent_across_engines() {
    let root = migrations_root();
    let sqlite_tables = parse_dir_tables(&root.join("sqlite"));
    let mysql_tables = parse_dir_tables(&root.join("mysql"));
    let mariadb_tables = parse_dir_tables(&root.join("mariadb"));

    assert_eq!(
        sqlite_tables.keys().collect::<Vec<_>>(),
        mysql_tables.keys().collect::<Vec<_>>(),
        "sqlite 与 mysql 版本集合不一致"
    );

    for (version, sqlite_schema) in &sqlite_tables {
        let mysql_schema = &mysql_tables[version];
        let mariadb_schema = &mariadb_tables[version];

        let table_names = |schema: &[Table]| -> Vec<String> {
            let mut names: Vec<String> = schema.iter().map(|t| t.name.clone()).collect();
            names.sort();
            names
        };
        let sqlite_names = table_names(sqlite_schema);
        assert_eq!(
            sqlite_names,
            table_names(mysql_schema),
            "v{version} 建表集合 sqlite/mysql 不一致"
        );
        assert_eq!(
            table_names(mysql_schema),
            table_names(mariadb_schema),
            "v{version} 建表集合 mysql/mariadb 不一致"
        );

        for table in sqlite_schema {
            let mysql_table = mysql_schema
                .iter()
                .find(|t| t.name == table.name)
                .unwrap_or_else(|| panic!("v{version} 缺少表 {}", table.name));
            let mariadb_table = mariadb_schema
                .iter()
                .find(|t| t.name == table.name)
                .unwrap_or_else(|| panic!("v{version} 缺少表 {}", table.name));

            assert_eq!(
                table.columns, mysql_table.columns,
                "v{version} 表 {} 结构 sqlite/mysql 不一致",
                table.name
            );
            assert_eq!(
                mysql_table.columns, mariadb_table.columns,
                "v{version} 表 {} 结构 mysql/mariadb 不一致",
                table.name
            );
        }
    }
}

/// 类型映射表本身的自我校验：sqlite TEXT 与 mysql CHAR(36) 归一化后一致。
#[test]
fn type_normalization_mapping_is_consistent() {
    assert_eq!(normalize_type("TEXT"), normalize_type("CHAR(36)"));
    assert_eq!(normalize_type("TEXT"), normalize_type("VARCHAR(255)"));
    assert_eq!(normalize_type("TEXT"), normalize_type("MEDIUMTEXT"));
    assert_eq!(normalize_type("INTEGER"), normalize_type("BIGINT"));
    assert_eq!(normalize_type("INTEGER"), normalize_type("TINYINT"));
    assert_eq!(normalize_type("INTEGER"), normalize_type("INT"));
    assert_eq!(normalize_type("TEXT"), normalize_type("JSON"));
}
