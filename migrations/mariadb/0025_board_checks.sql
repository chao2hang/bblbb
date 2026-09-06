
ALTER TABLE boards ADD CONSTRAINT boards_visibility_ck CHECK (visibility IN ('public', 'members', 'restricted', 'hidden'));
ALTER TABLE boards ADD CONSTRAINT boards_posting_mode_ck CHECK (posting_mode IN ('normal', 'approval', 'readonly', 'closed'));
