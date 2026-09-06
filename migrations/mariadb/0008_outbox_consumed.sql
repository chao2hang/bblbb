
CREATE TABLE outbox_consumed (
    event_id VARCHAR(36) NOT NULL,
    consumer VARCHAR(64) NOT NULL,
    consumed_at BIGINT NOT NULL,
    PRIMARY KEY (event_id, consumer)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
