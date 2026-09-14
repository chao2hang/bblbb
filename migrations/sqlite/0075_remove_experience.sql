-- Retire experience-driven runtime behavior without destructive data loss (SQLite).
-- Historical exp balances, ledger rows, legacy level tables, and users.level
-- remain available for rollback/audit compatibility. New code must not read
-- them for trust, access, quota, or activity rewards.

-- Existing activity rules must not create new exp entries after the switch.
UPDATE activity_rules
   SET currency_id = (SELECT id FROM currencies WHERE code = 'coin')
 WHERE currency_id = (SELECT id FROM currencies WHERE code = 'exp');

-- Do not translate legacy LV1-LV10 values into TL0-TL4: the systems have
-- different semantics. Only normalize invalid trust values to the safe default.
UPDATE users
   SET trust_level = CASE
       WHEN trust_level BETWEEN 0 AND 4 THEN trust_level
       ELSE 0
   END;
