-- BBLBB MySQL 8 FULLTEXT index (M03-SEARCH-STORE-03)
-- FULLTEXT on search_documents(title, body); InnoDB FULLTEXT updates natively
-- with row changes (no triggers needed, unlike SQLite FTS5 external content).
-- Tokenization limits (InnoDB FULLTEXT defaults, server variables):
--   innodb_ft_min_token_size = 3  → tokens shorter than 3 chars are NOT indexed
--   innodb_ft_max_token_size = 84 → tokens longer than 84 chars are NOT indexed
--   (ngram parser not used; CJK text is split by whitespace/punctuation, long
--    CJK runs become up-to-84-char tokens — known limitation, docs/SEARCH.md §7)
-- Rebuild command: OPTIMIZE TABLE search_documents (backend/src/search/fts.rs).

ALTER TABLE search_documents ADD FULLTEXT INDEX search_documents_fts_idx (title, body);
