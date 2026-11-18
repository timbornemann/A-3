-- Fresh candidate62 only: no existing61 data is upgraded or relabeled.
-- The existing immutable row and bounded proposal now use the strict V2 focus protocol.
ALTER TABLE http_correction_plan_reservations ADD COLUMN protocol_version INTEGER NOT NULL
    DEFAULT 2 CHECK(protocol_version=2);
