-- Fresh65 initialization only. Existing64 data is rejected before this script can run.
-- Keep the immutable reservation table; canonical V4 proposals retain unverified edit intent.
ALTER TABLE http_correction_plan_reservations DROP COLUMN protocol_version;
ALTER TABLE http_correction_plan_reservations ADD COLUMN protocol_version INTEGER NOT NULL
    DEFAULT 4 CHECK(protocol_version=4);
