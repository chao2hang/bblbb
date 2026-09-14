-- Retire experience-driven runtime behavior without destructive data loss (MariaDB).
-- Historical exp balances, ledger rows, legacy level tables, and users.level
-- remain available for rollback/audit compatibility. New code must not read
-- them for trust, access, quota, or activity rewards.

UPDATE activity_rules ar
JOIN currencies c ON c.id = ar.currency_id
JOIN currencies coin ON coin.code = 'coin'
   SET ar.currency_id = coin.id
 WHERE c.code = 'exp';

-- Do not translate legacy LV1-LV10 values into TL0-TL4: the systems have
-- different semantics. Only normalize invalid trust values to the safe default.
UPDATE users
   SET trust_level = CASE
       WHEN trust_level BETWEEN 0 AND 4 THEN trust_level
       ELSE 0
   END;
