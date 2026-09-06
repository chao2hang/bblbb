
CREATE TABLE preauth_csrf_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    csrf_secret_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY preauth_csrf_tokens_token_hash_uq (token_hash),
    KEY preauth_csrf_tokens_expires_at_idx (expires_at)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;
