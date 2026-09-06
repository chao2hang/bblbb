
ALTER TABLE search_documents ADD FULLTEXT INDEX search_documents_fts_idx (title, body);
