-- BBLBB MariaDB 10.11 FULLTEXT index (M03-SEARCH-STORE-04)
-- Same DDL as MySQL 8 (0031, byte-equal executable SQL): FULLTEXT on
-- search_documents(title, body); InnoDB FULLTEXT updates natively with row
-- changes (no triggers needed, unlike SQLite FTS5 external content).
--
-- Known MariaDB 10.11 vs MySQL 8 differences (docs/SEARCH.md §7.4):
--   - MySQL 8 ships the ngram parser (WITH PARSER ngram) for CJK word
--     segmentation; MariaDB 10.11 does not provide an equivalent ngram
--     parser — CJK runs are tokenized by whitespace/punctuation in both.
--   - Token size limits are the same defaults in both:
--     innodb_ft_min_token_size=3 / innodb_ft_max_token_size=84
--     (server variables, not per-table).
--   - Rebuild: OPTIMIZE TABLE search_documents works in both; MariaDB also
--     accepts ALTER TABLE search_documents FORCE.
--   - Stopword lists may differ between distributions; behaviour is only
--     pinned by deployment verification (M03-SEARCH-STORE-07 / M16).

ALTER TABLE search_documents ADD FULLTEXT INDEX search_documents_fts_idx (title, body);
