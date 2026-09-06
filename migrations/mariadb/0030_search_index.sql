
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
