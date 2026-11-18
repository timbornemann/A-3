-- Fresh63 initialization only. Existing62 data is rejected before this script can run.
-- Retain the one immutable reservation table and bounded proposal blob, now strict V3.
ALTER TABLE http_correction_plan_reservations DROP COLUMN protocol_version;
ALTER TABLE http_correction_plan_reservations ADD COLUMN protocol_version INTEGER NOT NULL
    DEFAULT 3 CHECK(protocol_version=3);
