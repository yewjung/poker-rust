CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE room_info (
   room_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
   player_count INT DEFAULT 0
);

-- Inserting 10 rows in one query
INSERT INTO room_info (player_count)
VALUES
(0),
(0),
(0),
(0),
(0),
(0),
(0),
(0),
(0),
(0);