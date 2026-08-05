-- BBLBB outbox 消费者去重表（M01-JOBS-06）
-- 消费者处理事件前，在业务事务内先原子插入 (event_id, consumer) 去重标记；
-- 唯一约束保证“至少一次投递”不会重复产生业务副作用。
-- 同一消费者对同一事件只领取一次；不同消费者（consumer）各自独立去重。

CREATE TABLE outbox_consumed (
    event_id VARCHAR(36) NOT NULL,
    consumer VARCHAR(64) NOT NULL,
    consumed_at BIGINT NOT NULL,
    PRIMARY KEY (event_id, consumer)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
