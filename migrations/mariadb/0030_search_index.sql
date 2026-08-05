-- BBLBB search index storage (M03-SEARCH-STORE-02)
-- search_documents: index document metadata (regular table, structure equivalent
-- across engines; rowid BIGINT AUTO_INCREMENT maps to the SQLite FTS5
-- external-content rowid, doc_id is the logical source id).
-- The FULLTEXT index on (title, body) is added in 0031 (MySQL 8) / 0032
-- (MariaDB 10.11) — docs/SEARCH.md §7. search_documents rows are maintained
-- by the index Jobs (M03-SEARCH-STORE-06); InnoDB FULLTEXT updates natively.

CREATE TABLE search_documents (
    rowid BIGINT AUTO_INCREMENT NOT NULL,
    doc_id VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    entity_type VARCHAR(16) NOT NULL,
    title VARCHAR(240) NOT NULL,
    body MEDIUMTEXT NOT NULL,
    excerpt VARCHAR(200) NOT NULL,
    slug VARCHAR(120) NOT NULL,
    author_id VARCHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    tags_json VARCHAR(2048) NOT NULL DEFAULT '[]',
    source_revision BIGINT NOT NULL,
    policy_revision BIGINT NOT NULL,
    indexed_at BIGINT NOT NULL,
    PRIMARY KEY (rowid),
    UNIQUE KEY search_documents_doc_id_uq (doc_id),
    KEY search_documents_type_idx (entity_type),
    KEY search_documents_slug_idx (slug)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
