-- BBLBB 搜索索引存储（M03-SEARCH-STORE-02）
-- 1) search_documents：索引文档元数据表（常规表，三库结构等价；SQLite rowid
--    INTEGER PRIMARY KEY AUTOINCREMENT 供 FTS5 external content 映射）；
-- 2) search_fts：SQLite FTS5 external content 虚拟表（title/body 全文索引，
--    unicode61 分词；content='search_documents'）；
-- 3) 同步触发器：search_documents INSERT/UPDATE/DELETE 维护 search_fts；
-- 4) 重建命令：INSERT INTO search_fts(search_fts) VALUES('rebuild');
-- 更新策略（docs/SEARCH.md §7）：search_documents 由索引 Job
-- （M03-SEARCH-STORE-06）维护；FTS5 由触发器同步，Job 不直接写 FTS 表。
-- MySQL/MariaDB 的 FULLTEXT 索引在 0031/0032 加入，由 InnoDB 原生随行更新。

CREATE TABLE search_documents (
    rowid INTEGER PRIMARY KEY AUTOINCREMENT,
    doc_id TEXT NOT NULL UNIQUE,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('post','user','board','tag')),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    excerpt TEXT NOT NULL,
    slug TEXT NOT NULL,
    author_id TEXT,
    tags_json TEXT NOT NULL DEFAULT '[]',
    source_revision INTEGER NOT NULL,
    policy_revision INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL
);

CREATE INDEX search_documents_type_idx ON search_documents (entity_type);
CREATE INDEX search_documents_slug_idx ON search_documents (slug);

CREATE VIRTUAL TABLE search_fts USING fts5(
    title,
    body,
    content='search_documents',
    content_rowid='rowid',
    tokenize = 'unicode61'
);

CREATE TRIGGER search_fts_ai AFTER INSERT ON search_documents BEGIN
    INSERT INTO search_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;

CREATE TRIGGER search_fts_ad AFTER DELETE ON search_documents BEGIN
    INSERT INTO search_fts(search_fts, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
END;

CREATE TRIGGER search_fts_au AFTER UPDATE ON search_documents BEGIN
    INSERT INTO search_fts(search_fts, rowid, title, body) VALUES('delete', old.rowid, old.title, old.body);
    INSERT INTO search_fts(rowid, title, body) VALUES (new.rowid, new.title, new.body);
END;
