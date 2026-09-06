
CREATE TABLE outbox_consumed (
    event_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    consumer VARCHAR(64) NOT NULL,
    consumed_at BIGINT NOT NULL,
    PRIMARY KEY (event_id, consumer)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
